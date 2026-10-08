// SPDX-License-Identifier: MPL-2.0
//! Tech2 Memory Bus, PCMCIA Card Banking, and Intel CFI Flash State Machine.

use crate::lcd::{Sed1335, Tech2Ui, UiScreen};
use m68k::AddressBus;
use std::collections::BTreeSet;
use std::env;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::LazyLock;

pub const A24: u32 = 0x00FF_FFFF;
pub const FLASH_SIZE: u32 = 0x0004_0000;
pub const RAM_BASE: u32 = 0x0010_0000;
// CS1 is 1 MB at $100000 (CSBAR1 = $1007). At 2 MB this range swallowed the card
// window at $200000, and since the RAM test runs first every card read returned the
// POST fill pattern ($AA) instead of the CIS -- so the card always read as absent.
pub const RAM_SIZE: u32 = 0x0010_0000;
pub const ERAM_BASE: u32 = 0x0030_0000;
pub const ERAM_SIZE: u32 = 0x0010_0000;
pub const CARD_BASE: u32 = 0x0020_0000;
pub const BANK_SIZE: u32 = 0x0010_0000;
pub const CARD_WIN: u32 = 0x0010_0000;
#[allow(dead_code)]
pub const LCD0: u32 = 0x0040_0000;
#[allow(dead_code)]
pub const LCD1: u32 = 0x0040_0800;
#[allow(dead_code)]
pub const LCD2: u32 = 0x0040_1000;
pub const RTC_BASE: u32 = 0x0050_0000;
pub const RTC_SIZE: u32 = 0x0000_0800;
// $700000 is not the keypad.  Nothing in the ROM references it -- the previous
// mailbox here was invented and could never be read by the guest.  The real
// keypad is a self-contained encoder at $600500 (see KEYPAD_REG below).  The
// window is kept only so stray accesses stay harmless.
pub const KEY_BASE: u32 = 0x0070_0000;
pub const KEY_SIZE: u32 = 0x0001_0000;

// ---------------------------------------------------------------------------
// Keypad encoder at $600500 (CS6).
//
// A byte read returns `(code << 3) | (active << 2)`; the low two bits are not
// decoded by any ROM path.  Idle is $00 -- no key, no active bit.
//
// Note the polarity of the keypad POST at $0014AC, which is easy to read
// backwards: all-ones is the *stuck line* case, not the healthy one.
//
//     $0014AC  move.w $600500.l, d0
//     $0014B2  andi.w #$fc00, d0
//     $0014B6  cmpi.w #$fc00, d0
//     $0014BA  bne.b  $14de           ; not all-ones -> $14DE prints $DEE "Pass"
//     $0014BC  move.l #$df3,-(a7)     ; all-ones     -> prints $DF3 "Fail"
//
// Idle $00 is also what the rest of the ROM assumes: the "press any key" waits
// at $000F6E and $004FC6 spin on `cmpi.b #$0,d0 / beq`, so a nonzero idle would
// make every error prompt fall straight through.
//
// The read is destructive.  The ROM's own IRQ1 stub at $000D96 is nothing but
// `move.b $600500.l,d7` followed by RTE, which is how a key event is
// acknowledged when no driver is installed yet.
//
// Writes are *not* acknowledgements.  They are pulses to an external mod-64
// menu counter: the menu-down path at $0168FC emits one, and the menu-up path
// at $016918 emits 63 to step the same counter backwards around the ring.
// They have no effect on the input side, so they are counted and discarded.
pub const KEYPAD_REG: u32 = 0x0060_0500;
pub const KEYPAD_IDLE: u8 = 0x00;
pub const KEYPAD_ACTIVE_BIT: u8 = 0x04;

// Hardware key codes (5 bits, presented in bits 7..3).
//
// $09 (menu up) and $0C (menu down) are hardcoded in the keypad ISR at $0168B8.
// Every other code is translated through the RAM tables at $100CD0 (released) /
// $100CEA (pressed) -- 26 entries each, codes $00..$19 -- and latched at $1011B4
// for the GETKEY service at $017336.  Dumped from a live run they are:
//
//   code  released  pressed        code  released  pressed
//   $01   $09       $09            $10   $03       $50 'P'
//   $02   '7'       $F7            $11   '8'       $F8
//   $03   '4'       $F4            $12   '5'       $F5
//   $04   '1'       $F1            $13   '2'       $F2
//   $06   $A2       $A6            $14   $11       $00
//   $07   $A3       $A7            $15   '9'       $F9
//   $08   $A4       $A8            $16   '6'       $F6
//   $09   $02       $00  (up)      $17   '3'       $F3
//   $0A   $A1       $A5            $18   '0'       $F0
//   $0B   $06       $FB            $19   $04       $00
//   $0C   $07       $00  (down)
//   $0D   $08       $FD
//   $0E   $01       $00
//
// So digits are ASCII on release and ASCII|$C0 on press, $06/$07/$08/$0A are the
// four soft keys, and $09/$0C/$0E/$14/$19 report on release only.  ENTER is one
// of the release-only keys, but which of $0E/$14/$19 is not decidable from the
// EPROM alone -- override with TECH2_KEY_ENTER=<n> to pin it.
// Original emulator.exe's VK_UP / VK_DOWN branches at 0x446172 / 0x446186
// emit encoder bytes 0x48 / 0x60 (+4 on press), confirming the directions.
pub const KEY_DOWN: u8 = 0x0C;
pub const KEY_UP: u8 = 0x09;
pub const KEY_ENTER_DEFAULT: u8 = 0x0E;
pub const KEY_EXIT: u8 = 0x01;
#[allow(dead_code)]
pub const KEY_SELECT: u8 = 0x10;
#[allow(dead_code)]
pub const KEY_MORE: u8 = 0x0D;
#[allow(dead_code)]
pub const KEY_PAGE_UP: u8 = 0x0B;
/// Key-translation tables the ISR indexes, for TECH2_DUMP_KEYTBL.
pub const KEYTBL_UP: u32 = 0x0010_0CD0;
pub const KEYTBL_DOWN: u32 = 0x0010_0CEA;
/// Latched translated key consumed by GETKEY ($017336).
pub const KEY_LATCH: u32 = 0x0010_11B4;
/// Menu selection index the ISR steps directly (mod 64).
pub const KEY_MENU_IDX: u32 = 0x0010_0CBC;
pub const CS6_BASE: u32 = 0x0060_0000;
pub const CS6_SIZE: u32 = 0x0010_0000;
#[allow(dead_code)]
pub const SEED_BASE: u32 = 0x005C_0000;
#[allow(dead_code)]
pub const SEED_LEN: u32 = 0x0001_0000;
// The internal register window starts at $YFF000.  The SIM itself begins at
// +$A00, QSM at +$C00, and TPU at +$E00.  Using $YFE000 shifted all of those
// offsets by $1000: HSRR writes landed at $1E18 while the clear-on-service
// rule watched $0E18, causing the $16EB6 busy loop in native boot.
pub const SIM7: u32 = 0x007F_F000;
pub const SIM_HI: u32 = 0x00FF_F000;
pub const SIM_MASK: u32 = 0x0FFF;
pub const OFF_PICR: usize = 0x0A22;
#[allow(dead_code)]
pub const OFF_PITR: usize = 0x0A24;
#[allow(dead_code)]
pub const OFF_CSOR2: usize = 0x0A56;
#[allow(dead_code)]
pub const OFF_BANK: usize = 0x0A80;
pub const OFF_SCSR: usize = 0x0C0C;
pub const OFF_SPCR0: usize = 0x0C18;
pub const OFF_SPCR1: usize = 0x0C1A;
pub const OFF_SPCR2: usize = 0x0C1C;
pub const OFF_SPCR3: usize = 0x0C1E;
pub const OFF_SPSR: usize = 0x0C1F;
pub const OFF_TPU: usize = 0x0E00;
pub const OFF_HSRR0: usize = 0x0E18;
pub const OFF_HSRR1: usize = 0x0E1A;
pub const PCMCIA_BANK_REG: u32 = 0x0060_0600;
pub const UNUSED: u8 = 0xFF;

pub const MAX_INSNS_DEFAULT: u64 = 80_000_000;

/// Bounded, opt-in trace for the external-RAM POST.  It is intentionally
/// observational: it records the accesses around the $300000 chip-select
/// range without altering their values or timing.
static ERAM_TRACE: LazyLock<bool> = LazyLock::new(|| env::var_os("ERAM_TRACE").is_some());
static ERAM_TRACE_COUNT: AtomicUsize = AtomicUsize::new(0);
const ERAM_TRACE_LIMIT: usize = 512;
static CFI_TRACE: LazyLock<bool> = LazyLock::new(|| env::var_os("CFI_TRACE").is_some());
static CFI_TRACE_COUNT: AtomicUsize = AtomicUsize::new(0);
const CFI_TRACE_LIMIT: usize = 256;

/// Controls whether the emulator is allowed to alter guest software in order
/// to explore a later boot stage.
///
/// `Fidelity` is deliberately the default: it executes the supplied EPROM and
/// card image without PC-specific return stubs, forced branch instructions, or
/// fabricated UI state.  `ResearchHarness` retains the legacy experiments for
/// diagnosis only; a result from that mode is never a native-boot result.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ExecutionMode {
    Fidelity,
    ResearchHarness,
}

impl ExecutionMode {
    pub const fn is_research_harness(self) -> bool {
        matches!(self, Self::ResearchHarness)
    }
}

pub fn sim_off(a: u32) -> Option<usize> {
    if (SIM7..SIM7 + 0x1000).contains(&a) || a >= SIM_HI {
        Some((a & SIM_MASK) as usize)
    } else {
        None
    }
}

pub fn lcd_kind(addr: u32) -> Option<bool> {
    let a = addr & A24;
    if a == 0x0070_0102 {
        Some(true)
    } else if a == 0x0070_0100 {
        Some(false)
    } else if (0x0040_0000..0x0040_1800).contains(&a) {
        Some((a & 1) != 0)
    } else {
        None
    }
}

#[derive(Clone, Debug)]
pub struct Ata {
    pub err: u8,
    pub nsec: u8,
    pub lba: [u8; 3],
    pub dev: u8,
    pub status: u8,
    pub lba_addr: usize,
    pub remaining: usize,
}

impl Ata {
    pub fn new() -> Self {
        Self {
            err: 0,
            nsec: 0,
            lba: [0; 3],
            dev: 0,
            status: 0x50,
            lba_addr: 0,
            remaining: 0,
        }
    }
    pub fn lba_off(&self) -> usize {
        let v =
            self.lba[0] as usize | ((self.lba[1] as usize) << 8) | ((self.lba[2] as usize) << 16);
        v.saturating_mul(512)
    }
}

pub struct Tech2Bus {
    pub candi_link: Option<crate::candi::native_link::NativeLink>,
    pub pending_candi: Option<crate::candi::demand::Pending>,
    pub demand_boot_complete: bool,
    pub demand_guest_init: Option<crate::candi::demand::GuestInit>,
    pub execution_mode: ExecutionMode,
    pub flash: Vec<u8>,
    pub ram: Vec<u8>,
    pub eram: Vec<u8>,
    pub rtc: Vec<u8>,
    pub iram: Vec<u8>,
    pub cs6: Vec<u8>,
    pub card: Vec<u8>,
    pub bank: u32,
    pub ata: Ata,
    pub lcd: Sed1335,
    pub sim: [u8; 0x1000],
    pub mmio_pages: BTreeSet<u32>,
    pub first_w: Vec<(u32, u8)>,
    pub seed_hits: u64,
    pub pcmcia_log: Vec<String>,
    pub qspi_xfers: u64,
    pub t2_latched: bool,
    pub iack_vec: u16,
    pub pit_ticks: u64,
    pub pit_pending: bool,
    pit_next_tick: Option<u64>,
    tpu_countdown: crate::tpu::Countdown,
    pub research_in_app: bool,
    pub research_struct_buf: u32,
    pub research_path_depth: u32,
    pub bank_raw: bool,
    pub card_writes: bool,
    pub ssa_flash: Option<crate::ssa_flash::SsaFlash>,
    pub card_wr_dropped: u64,
    pub cfi_status: bool,
    pub cfi_cmds: u64,
    pub cfi_erase_armed: bool,
    pub card_wr_pages: [u64; 16],
    pub card_rd_pages: [u64; 16],
    pub cs2_ram: Option<Vec<u8>>,
    pub attr_mem: Vec<u8>,
    pub attr_reads: u64,
    pub eprom_cfi_status: bool,
    pub eprom_cfi_erase_armed: bool,
    /// A $4040/$1010 program command has been received for the next flash word.
    pub eprom_cfi_program_armed: bool,
    /// The bus trait exposes byte writes, while the Intel flash is a 16-bit
    /// device.  Hold the first lane so commands/data are interpreted as words.
    pub eprom_cfi_pending_byte: Option<(u32, u8)>,
    pub eprom_writes: u64,
    pub eprom_erases: u64,
    /// Bytes accepted by the Intel program command in each 64 KiB sector.
    /// This is diagnostic accounting only; it makes partial flash updates
    /// visible without changing the guest-visible device behavior.
    pub eprom_program_sector_bytes: [u64; 4],
    pub eprom_cfi_program_bytes: u8,
    pub current_pc: u32,
    pub current_insns: u64,
    pub failure_reason: Option<String>,
    pub host_cancelled_operations: u64,
    pub recent_pcs: [u32; 64],
    pub steps_recorded: u64,
    pub trace: crate::trace::Trace,
    pub card_hdr_wr: Option<(u32, u32, u8)>,
    pub bank0_at: Option<u64>,
    pub wr_7ffa56: u64,
    pub opsys_loaded: usize,
    pub opsys_img: Vec<u8>,
    pub opsys_restored: bool,
    /// Background colour (0x00RRGGBB), from -CB in tech2win.conf
    pub bg_color: u32,
    /// Foreground/text colour (0x00RRGGBB), from -CT in tech2win.conf
    pub fg_color: u32,
    pub ram_test_ret: u32,
    /// Key codes waiting to be presented at $600500, oldest first.
    pub key_queue: std::collections::VecDeque<u8>,
    /// Code currently presented at $600500; cleared by the guest's read.
    pub key_active: Option<u8>,
    /// IRQ1 asserted because a key event is waiting to be read.
    pub key_irq: bool,
    pub key_events: u64,
    pub key_reads: u64,
    pub key_consumed: u64,
    pub key_pulses: u64,
    pub key_hold_count: u32,
    pub qspi_pending: bool,
    /// Last-resort boot presentation used only after the real guest reports
    /// that its unimplemented pSOS task services have failed.
    pub compatibility_splash: bool,
    pub ui: Tech2Ui,
}

impl Tech2Bus {
    /// Complete guest state; the live host trace is intentionally not copied.
    pub fn recovery_snapshot(&self) -> Self {
        Self {
            pending_candi: self.pending_candi.clone(),
            demand_boot_complete: self.demand_boot_complete,
            demand_guest_init: self.demand_guest_init.clone(),
            candi_link: None, // Checkpoint::capture copies the virtual link separately and handles errors.
            execution_mode: self.execution_mode,
            flash: self.flash.clone(),
            ram: self.ram.clone(),
            eram: self.eram.clone(),
            rtc: self.rtc.clone(),
            iram: self.iram.clone(),
            cs6: self.cs6.clone(),
            card: self.card.clone(),
            bank: self.bank,
            ata: self.ata.clone(),
            lcd: self.lcd.clone(),
            sim: self.sim,
            mmio_pages: self.mmio_pages.clone(),
            first_w: self.first_w.clone(),
            seed_hits: self.seed_hits,
            pcmcia_log: self.pcmcia_log.clone(),
            qspi_xfers: self.qspi_xfers,
            t2_latched: self.t2_latched,
            iack_vec: self.iack_vec,
            pit_ticks: self.pit_ticks,
            pit_pending: self.pit_pending,
            pit_next_tick: self.pit_next_tick,
            tpu_countdown: self.tpu_countdown.clone(),
            research_in_app: self.research_in_app,
            research_struct_buf: self.research_struct_buf,
            research_path_depth: self.research_path_depth,
            bank_raw: self.bank_raw,
            card_writes: self.card_writes,
            ssa_flash: self.ssa_flash.clone(),
            card_wr_dropped: self.card_wr_dropped,
            cfi_status: self.cfi_status,
            cfi_cmds: self.cfi_cmds,
            cfi_erase_armed: self.cfi_erase_armed,
            card_wr_pages: self.card_wr_pages,
            card_rd_pages: self.card_rd_pages,
            cs2_ram: self.cs2_ram.clone(),
            attr_mem: self.attr_mem.clone(),
            attr_reads: self.attr_reads,
            eprom_cfi_status: self.eprom_cfi_status,
            eprom_cfi_erase_armed: self.eprom_cfi_erase_armed,
            eprom_cfi_program_armed: self.eprom_cfi_program_armed,
            eprom_cfi_pending_byte: self.eprom_cfi_pending_byte,
            eprom_writes: self.eprom_writes,
            eprom_erases: self.eprom_erases,
            eprom_program_sector_bytes: self.eprom_program_sector_bytes,
            eprom_cfi_program_bytes: self.eprom_cfi_program_bytes,
            current_pc: self.current_pc,
            current_insns: self.current_insns,
            failure_reason: self.failure_reason.clone(),
            host_cancelled_operations: self.host_cancelled_operations,
            recent_pcs: self.recent_pcs,
            steps_recorded: self.steps_recorded,
            trace: crate::trace::Trace::default(),
            card_hdr_wr: self.card_hdr_wr,
            bank0_at: self.bank0_at,
            wr_7ffa56: self.wr_7ffa56,
            opsys_loaded: self.opsys_loaded,
            opsys_img: self.opsys_img.clone(),
            opsys_restored: self.opsys_restored,
            bg_color: self.bg_color,
            fg_color: self.fg_color,
            ram_test_ret: self.ram_test_ret,
            key_queue: self.key_queue.clone(),
            key_active: self.key_active,
            key_irq: self.key_irq,
            key_events: self.key_events,
            key_reads: self.key_reads,
            key_consumed: self.key_consumed,
            key_pulses: self.key_pulses,
            key_hold_count: self.key_hold_count,
            qspi_pending: self.qspi_pending,
            compatibility_splash: self.compatibility_splash,
            ui: self.ui.clone(),
        }
    }

    pub fn restore_recovery_snapshot(&mut self, mut saved: Self) {
        let now = self.current_insns;
        let captured = saved.current_insns;
        saved.tpu_countdown.rebase(captured, now);
        saved.pit_next_tick = saved
            .pit_next_tick
            .map(|at| now.saturating_add(at.saturating_sub(captured)));
        saved.current_insns = now;
        // Preserve cumulative host input accounting and the append-only trace.
        saved.host_cancelled_operations = self.host_cancelled_operations.saturating_add(1);
        saved.key_events = self.key_events;
        saved.key_reads = self.key_reads;
        saved.key_consumed = self.key_consumed;
        saved.key_pulses = self.key_pulses;
        saved.trace = std::mem::take(&mut self.trace);
        *self = saved;
        self.trace_event(
            "menu_restore",
            &format!("checkpoint_insns={captured} operation_cancelled=true"),
        );
        self.trace_screen();
    }

    fn trace_eram_access(&self, operation: char, address: u32, value: Option<u8>) {
        // Include the last card page as well as ERAM so a mistaken chip-select
        // boundary is visible in one trace.  LCD traffic starts at $400000 and
        // is deliberately excluded.
        if !*ERAM_TRACE || !(0x002F_0000..0x0040_0000).contains(&address) {
            return;
        }
        let n = ERAM_TRACE_COUNT.fetch_add(1, Ordering::Relaxed);
        if n < ERAM_TRACE_LIMIT {
            match value {
                Some(value) => eprintln!(
                    "ERAM_TRACE #{n:03} {operation} pc={:#010x} addr={address:#010x} value={value:#04x}",
                    self.current_pc
                ),
                None => eprintln!(
                    "ERAM_TRACE #{n:03} {operation} pc={:#010x} addr={address:#010x}",
                    self.current_pc
                ),
            }
        } else if n == ERAM_TRACE_LIMIT {
            eprintln!("ERAM_TRACE: limit ({ERAM_TRACE_LIMIT}) reached");
        }
    }

    /// Apply a 16-bit Intel flash transaction.  The guest uses `MOVE.W` for
    /// both commands and payload words; `AddressBus` presents that as two
    /// byte writes, which are joined by `write_eprom_byte` below.
    fn write_eprom_word(&mut self, address: u32, high: u8, low: u8) {
        let word = u16::from_be_bytes([high, low]);
        if *CFI_TRACE && self.current_pc >= RAM_BASE {
            let n = CFI_TRACE_COUNT.fetch_add(1, Ordering::Relaxed);
            if n < CFI_TRACE_LIMIT {
                eprintln!(
                    "CFI_TRACE #{n:03} pc={:#010x} addr={address:#010x} word={word:#06x} program_word={} erase_armed={}",
                    self.current_pc,
                    self.eprom_cfi_program_armed,
                    self.eprom_cfi_erase_armed,
                );
            } else if n == CFI_TRACE_LIMIT {
                eprintln!("CFI_TRACE: limit ({CFI_TRACE_LIMIT}) reached");
            }
        }

        // A programmed value is data, including values which happen to equal
        // an Intel command byte.  Consume it before looking for a new command.
        if self.eprom_cfi_program_armed {
            self.eprom_cfi_program_armed = false;
            self.eprom_cfi_status = true;
            self.eprom_writes += 2;
            if address < FLASH_SIZE {
                self.eprom_program_sector_bytes[(address >> 16) as usize] += 2;
            }
            if address >= 0x8018 && crate::options::env_flag("EPROM_WRITABLE") {
                self.flash[address as usize] = high;
                if (address as usize) + 1 < self.flash.len() {
                    self.flash[address as usize + 1] = low;
                }
            }
            return;
        }

        match word {
            0x5050 => self.eprom_cfi_status = false,
            0x2020 => self.eprom_cfi_erase_armed = true,
            0xD0D0 if self.eprom_cfi_erase_armed => {
                self.eprom_cfi_erase_armed = false;
                self.eprom_cfi_status = true;
                self.eprom_erases += 1;
                // The boot block ends just before $8018.  The ROM explicitly
                // selects its next update block at $8000, so preserve that
                // boot area while erasing the requested 64 KiB block.
                if crate::options::env_flag("EPROM_WRITABLE") {
                    let block_start = address & !0xFFFF;
                    let start = block_start.max(0x8018);
                    let end = (block_start + 0x1_0000).min(FLASH_SIZE);
                    if start < end {
                        self.flash[start as usize..end as usize].fill(0xFF);
                    }
                }
            }
            0x4040 | 0x1010 => self.eprom_cfi_program_armed = true,
            0x7070 => self.eprom_cfi_status = true,
            0xFFFF => self.eprom_cfi_status = false,
            _ => {}
        }
    }

    fn write_eprom_byte(&mut self, address: u32, value: u8) {
        if let Some((first_address, first_value)) = self.eprom_cfi_pending_byte.take() {
            if first_address.wrapping_add(1) == address && first_address & 1 == 0 {
                self.write_eprom_word(first_address, first_value, value);
                return;
            }

            // All observed CSBOOT writes are aligned words.  If a future
            // guest path uses an isolated byte write, model it on both data
            // lanes rather than silently losing the pending byte.
            self.write_eprom_word(first_address & !1, first_value, first_value);
        }

        if address & 1 == 0 {
            self.eprom_cfi_pending_byte = Some((address, value));
        } else {
            self.write_eprom_word(address & !1, value, value);
        }
    }

    pub fn new(flash: Vec<u8>, card: Vec<u8>, execution_mode: ExecutionMode) -> Self {
        let mut f = vec![UNUSED; FLASH_SIZE as usize];
        let n = flash.len().min(f.len());
        f[..n].copy_from_slice(&flash[..n]);
        let mut s = Self {
            candi_link: None,
            pending_candi: None,
            demand_boot_complete: false,
            demand_guest_init: None,
            execution_mode,
            flash: f,
            ram: vec![0; RAM_SIZE as usize],
            eram: vec![0; ERAM_SIZE as usize],
            rtc: vec![0; RTC_SIZE as usize],
            iram: vec![0; KEY_SIZE as usize],
            cs6: vec![0; CS6_SIZE as usize],
            card,
            bank: 0,
            ata: Ata::new(),
            lcd: Sed1335::new(),
            // Internal peripherals reset as registers, not as erased flash.
            // Returning $FF for every unimplemented SIM/QSM/TPU register made
            // status-bit POST checks pass merely because every bit was high.
            sim: [0; 0x1000],
            mmio_pages: BTreeSet::new(),
            first_w: Vec::new(),
            seed_hits: 0,
            pcmcia_log: Vec::new(),
            qspi_xfers: 0,
            t2_latched: false,
            iack_vec: 0,
            pit_ticks: 0,
            pit_pending: false,
            pit_next_tick: None,
            tpu_countdown: crate::tpu::Countdown::default(),
            research_in_app: false,
            research_struct_buf: 0x0030_0000,
            research_path_depth: 0,
            bank_raw: crate::options::env_flag("BANK_RAW"),
            card_writes: crate::options::env_flag("CARD_WRITES"),
            ssa_flash: None,
            card_wr_dropped: 0,
            cfi_status: false,
            cfi_cmds: 0,
            cfi_erase_armed: false,
            card_wr_pages: [0; 16],
            card_rd_pages: [0; 16],
            attr_mem: Vec::new(),
            attr_reads: 0,
            eprom_cfi_status: false,
            eprom_cfi_erase_armed: false,
            eprom_cfi_program_armed: false,
            eprom_cfi_pending_byte: None,
            eprom_writes: 0,
            eprom_erases: 0,
            eprom_program_sector_bytes: [0; 4],
            eprom_cfi_program_bytes: 0,
            current_pc: 0,
            current_insns: 0,
            failure_reason: None,
            host_cancelled_operations: 0,
            recent_pcs: [0; 64],
            steps_recorded: 0,
            trace: crate::trace::Trace::default(),
            cs2_ram: if crate::options::env_flag("CS2_RAM") {
                Some(vec![0; CARD_WIN as usize])
            } else {
                None
            },
            card_hdr_wr: None,
            bank0_at: None,
            wr_7ffa56: 0,
            opsys_loaded: 0,
            opsys_img: Vec::new(),
            opsys_restored: false,
            bg_color: 0x0000_00FF, // default blue (overridden by config)
            fg_color: 0x00FF_FFFF, // default white (overridden by config)
            ram_test_ret: 0,
            key_queue: std::collections::VecDeque::new(),
            key_active: None,
            key_irq: false,
            key_events: 0,
            key_reads: 0,
            key_consumed: 0,
            key_pulses: 0,
            key_hold_count: 0,
            qspi_pending: false,
            compatibility_splash: false,
            ui: Tech2Ui::new(),
        };
        // Baseline QSM register reset state used by the peripheral model.  A
        // transfer starts with SPIF clear and raises it only after a queue has
        // actually been started and subsequently polled.
        s.sim[OFF_SPCR0] = 0x00;
        s.sim[OFF_SPCR0 + 1] = 0x04;
        s.sim[OFF_SPCR1] = 0x04;
        s.sim[OFF_SPCR1 + 1] = 0x04;
        s.sim[OFF_SPCR2] = 0x00;
        s.sim[OFF_SPCR2 + 1] = 0x00;
        s.sim[OFF_SPCR3] = 0x00;
        s.sim[OFF_SPSR] = 0x00;

        if execution_mode.is_research_harness() {
            // The following mutations are retained for controlled research of
            // post-POST code only.  They are not emulation of the original
            // hardware and must never be used to claim a native boot.
            s.sim[0x0A04] = 0x10;
            s.sim[0x0A05] = 0x10;
            s.sim[OFF_TPU] = 0x04;
            s.sim[OFF_TPU + 1] = 0x00;

            // Rewriting the ROM's "Fail" string to "Pass" makes every POST line
            // report success whatever the hardware did.  It is diagnostic-only.
            if env::var("TECH2_FAKE_POST")
                .map(|v| v != "0")
                .unwrap_or(false)
            {
                for i in 0x0500..0x6000 {
                    if s.flash.len() >= i + 4 && &s.flash[i..i + 4] == b"Fail" {
                        s.flash[i..i + 4].copy_from_slice(b"Pass");
                    }
                }
            }

            // Fast POST RAM test patch: bypass 32k loop so payload boots instantly.
            if s.flash.len() > 0x07CD {
                s.flash[0x07CC] = 0x60;
                s.flash[0x07CD] = 0x02; // BRA +2
            }
            // ROM TRAP #11, #12, #13 exception vectors.
            if s.flash.len() >= 0xB8 {
                s.flash[0xAC..0xB0].copy_from_slice(&0x0001_2A4Eu32.to_be_bytes());
                s.flash[0xB0..0xB4].copy_from_slice(&0x0001_2A52u32.to_be_bytes());
                s.flash[0xB4..0xB8].copy_from_slice(&0x0001_2A56u32.to_be_bytes());
            }
            // Force POST handoff to JMP $9018.
            if s.flash.len() > 0x1571 {
                s.flash[0x156A..0x1572]
                    .copy_from_slice(&[0x4E, 0xF9, 0x00, 0x00, 0x90, 0x18, 0x4E, 0x71]);
            }
            if s.flash.len() > 0x1203B {
                s.flash[0x12038..0x1203C].copy_from_slice(&0x0010_11BAu32.to_be_bytes());
            }
            // Force POST pause and skip the keypad wait loop.
            if s.flash.len() > 0x0F79 {
                s.flash[0x0F54] = 0x60;
                s.flash[0x0F55] = 0x04;
                s.flash[0x0F78] = 0x4E;
                s.flash[0x0F79] = 0x71;
            }
            // NOP out hardware memory-controller busy spin loops.
            for off in [0x16ED2usize, 0x16EEE] {
                if s.flash.len() > off + 1 && s.flash[off] == 0x66 && s.flash[off + 1] == 0xF8 {
                    s.flash[off] = 0x4E;
                    s.flash[off + 1] = 0x71;
                }
            }
        }
        let mut attr = vec![0xFF; 256];
        attr[0x00] = 0x18;
        attr[0x02] = 0x06;
        attr[0x04] = b'S';
        attr[0x06] = 0x7E;
        attr[0x08] = b'R';
        attr[0x0A] = 0x7E;
        attr[0x1C] = 0x1D;
        attr[0x1E] = 0x02;
        attr[0x20] = 0x00;
        attr[0x22] = 0x00;
        attr[0x24] = 0xFF;
        s.attr_mem = attr;
        s
    }

    pub fn card_off(&self, offset_in_window: u32) -> Option<usize> {
        if self.card.is_empty() {
            return None;
        }
        #[cfg(feature = "load-test")]
        let small_bank = { static VALUE: LazyLock<bool> = LazyLock::new(|| crate::options::env_flag("BANK_SIZE_128")); *VALUE };
        #[cfg(not(feature = "load-test"))]
        let small_bank = crate::options::env_flag("BANK_SIZE_128");
        let bsz = if small_bank {
            0x0002_0000
        } else {
            BANK_SIZE
        };
        let base = if self.bank_raw {
            self.bank as usize * bsz as usize
        } else {
            ((self.bank >> 2) as usize) * bsz as usize
        };
        let off = base + (offset_in_window as usize);
        if off < self.card.len() {
            Some(off)
        } else {
            None
        }
    }

    pub fn load_opsys_ram(&mut self, opsys: &[u8]) {
        let n = opsys.len().min(self.ram.len());
        self.ram[..n].copy_from_slice(&opsys[..n]);
        self.opsys_loaded = n;
        self.opsys_img = opsys.to_vec();
        if !self.execution_mode.is_research_harness() {
            return;
        }

        // Legacy research setup follows.  This is intentionally not part of a
        // fidelity load because it edits guest state and code.
        // Plant PCMCIA Card Present status flags in RAM ($00101884, $00101888, $00101894)
        self.ram_write_long(0x0010_1884, 0x0001_0001);
        self.ram_write_long(0x0010_1888, 0x0001_0001);
        self.ram_write_long(0x0010_1894, 0x0001_0001);
        // Patch 0x064FC (Flash 0x0E514) in opsys RAM to return Card Present (MOVEQ #1, D0; RTS)
        if self.ram.len() > 0x064FF {
            self.ram[0x064FC] = 0x70; // MOVEQ #1, D0
            self.ram[0x064FD] = 0x01;
            self.ram[0x064FE] = 0x4E; // RTS
            self.ram[0x064FF] = 0x75;
        }
        // NOP out hardware memory controller busy spin loops at 0x0EEBA and 0x0EED6 in opsys RAM
        for off in [0x0EEBAusize, 0x0EED6] {
            if self.ram.len() > off + 1 && self.ram[off] == 0x66 && self.ram[off + 1] == 0xF8 {
                self.ram[off] = 0x4E;
                self.ram[off + 1] = 0x71; // NOP
            }
        }
    }

    pub fn plant_rtc(&mut self) {
        let rtc_time = 0x1000_0000u32;
        self.rtc[0x10] = (rtc_time >> 24) as u8;
        self.rtc[0x11] = (rtc_time >> 16) as u8;
        self.rtc[0x12] = (rtc_time >> 8) as u8;
        self.rtc[0x13] = rtc_time as u8;
    }

    pub fn plant_a5_mmobj(&mut self) {
        let a5 = 0x0010_0CB4u32;
        self.ram_write_long(a5.wrapping_add(0x0F0C), 0x0010_0F0C);
        self.ram_write_long(a5.wrapping_add(0x0F00), 0x0010_0F00);
        self.ram_write_long(a5.wrapping_add(0x0F30), 0x0030_0000);
        self.ram_write_long(0x0010_0F30, 0x0030_0000);
        self.write_word(0x0030_0008, 0x12A4);
        self.write_word(0x0030_000C, 0x4E75);
        // Plant Card Present flags in RAM
        self.ram_write_long(0x0010_1884, 0x0001_0001);
        self.ram_write_long(0x0010_1888, 0x0001_0001);
        self.ram_write_long(0x0010_1894, 0x0001_0001);
        // Vector 0x42 stub: RTE ($4E73) so PIT interrupt returns cleanly
        self.write_word(0x0010_0712, 0x4E73);
    }

    pub fn ram_word(&self, addr: u32) -> u16 {
        let Some(off) = addr.checked_sub(RAM_BASE).map(|v| v as usize) else {
            return 0;
        };
        if off + 2 <= self.ram.len() {
            u16::from_be_bytes(self.ram[off..off + 2].try_into().unwrap())
        } else {
            0
        }
    }

    pub fn ram_long(&self, addr: u32) -> u32 {
        let Some(off) = addr.checked_sub(RAM_BASE).map(|v| v as usize) else {
            return 0;
        };
        if off + 4 <= self.ram.len() {
            u32::from_be_bytes(self.ram[off..off + 4].try_into().unwrap())
        } else {
            0
        }
    }

    pub fn ram_write_long(&mut self, addr: u32, val: u32) {
        let Some(off) = addr.checked_sub(RAM_BASE).map(|v| v as usize) else {
            return;
        };
        if off + 4 <= self.ram.len() {
            self.ram[off..off + 4].copy_from_slice(&val.to_be_bytes());
        }
    }

    pub fn pit_period(&self) -> Option<u64> {
        let pitr = u16::from_be_bytes([self.sim[0x0A24], self.sim[0x0A25]]);
        if pitr > 0 {
            Some(10_000)
        } else {
            None
        }
    }

    pub fn picr_level(&self) -> u8 {
        let picr = u16::from_be_bytes([self.sim[OFF_PICR], self.sim[OFF_PICR + 1]]);
        ((picr >> 8) & 7) as u8
    }

    pub(crate) fn ensure_candi(&mut self, trigger: &str) {
        let Some(pending) = self.pending_candi.take() else { return; };
        let began = std::time::Instant::now();
        println!("CANDI_STARTING: trigger={trigger} pc={:#x} insns={} live_adapter={}", self.current_pc, self.current_insns, pending.has_adapter());
        // Called before the initiating bus operation. Guest execution is held
        // here; that operation is neither dropped nor replayed after starting.
        let result = pending.start();
        let status = match result {
            Ok(link) => { self.candi_link = Some(link); "ready" }
            Err(error) => { self.failure_reason = Some(format!("CANdi initialization failed: {error}")); "failed" }
        };
        let report = serde_json::json!({"status":status,"trigger":trigger,"pc":self.current_pc,"instructions":self.current_insns,"initialization_ms":began.elapsed().as_millis(),"live_adapter":pending.has_adapter(),"error":self.failure_reason});
        let _ = std::fs::write(pending.output.join("candi-startup.json"), report.to_string());
        println!("CANDI_INITIALIZED: {report}");
        if pending.transport_trace && !self.trace.has_writer() {
            match crate::trace::Trace::open(&pending.output.join("trace.jsonl"), false) {
                Ok(mut trace) => { trace.concise_transport(); self.trace = trace; }
                Err(error) => self.failure_reason = Some(format!("CANdi trace failed: {error}")),
            }
        }
    }

    /// Offline load-test scheduling only: never skip a native CANdi poll.
    #[cfg(feature = "load-test")]
    pub fn load_test_deadline(&self) -> Option<u64> {
        if self.candi_link.is_some() { return None; }
        self.pit_next_tick.into_iter()
            .chain(self.tpu_countdown.next_instruction_deadline())
            .min()
    }

    /// Timer deadlines advance while the CPU is masked or stopped. PIT clears
    /// on IACK; TPU status remains pending until the guest clears CISR.
    pub fn poll_interrupt(&mut self, insns: u64) -> u8 {
        let mut link_events = Vec::new();
        if let Some(link) = &mut self.candi_link {
            if insns % 128 == 0 {
                link.advance();
                link_events = link.events();
            }
            // Connected virtual CANdi drives the presence input. The native
            // TPU channel-15 handler performs normal guest discovery.
            if self.sim[0xe0a] & 0x80 == 0 {
                link.presence_announced = false;
            }
            if link.connected() {
                self.sim[0xff2] |= 0x80;
                if !link.presence_announced && self.sim[0xe0a] & 0x80 != 0 {
                    self.sim[0xe20] |= 0x80;
                    link.presence_announced = true;
                    link_events.push("virtual CANdi presence edge channel=15".into());
                }
            } else {
                self.sim[0xff2] &= !0x80;
            }
        }
        for event in link_events {
            self.trace_event("candi_native_link", &event);
        }

        // Supported Tech2 downloaded timer function. Do not mistake an arbitrary
        // firmware's function 15 for this custom countdown personality.
        if self.execution_mode.is_research_harness()
            && self.peek_bytes(0x1d9c7e, 8)
                == Some(&[0x48, 0xe7, 0x01, 0x04, 0x3f, 0x3c, 0x00, 0x88])
        {
            let live = self.candi_link.as_ref().is_some_and(|link| link.has_live_clock());
            let expired = if live {
                // Sample with the native-link poll, rather than calling the host
                // clock on every main-CPU instruction. The countdown still raises
                // the original TPU interrupt; guest code sends every request.
                if insns % 128 == 0 {
                    let elapsed = self.candi_link.as_ref().unwrap().live_elapsed().unwrap();
                    self.tpu_countdown.advance_wall(&mut self.sim, elapsed)
                } else {
                    0
                }
            } else {
                self.tpu_countdown.advance(&mut self.sim, insns)
            };
            if expired != 0 {
                self.trace_event(
                    "tpu_timer_expired",
                    &format!(
                        "channels={expired:#06x} backend=tech2-countdown-model clock={} tpumcr={:#06x} external_tx=false",
                        if live { "monotonic-tcr1" } else { "instructions" },
                        u16::from_be_bytes([self.sim[0xe00], self.sim[0xe01]])
                    ),
                );
            }
            if let Some(link) = &self.candi_link {
                if self
                    .tpu_countdown
                    .advance_uart(&mut self.sim, insns, link.uart_receive_idle())
                {
                    self.trace_event("candi_uart_timer", "expired channel=5 function=13 clock=instruction-approximation external_tx=false");
                }
            }
        }
        if let Some(pending) = &self.pending_candi {
            self.tpu_countdown.advance_uart(&mut self.sim, insns, pending.uart.receive_idle());
        }
        if let Some(period) = self.pit_period() {
            let next = *self
                .pit_next_tick
                .get_or_insert_with(|| insns.saturating_add(period));
            if insns >= next {
                self.pit_pending = true;
                self.pit_next_tick = Some(insns.saturating_add(period));
            }
        } else {
            self.pit_next_tick = None;
            self.pit_pending = false;
        }
        let timer = if self.pit_pending {
            self.picr_level()
        } else {
            0
        };
        timer
            .max(if self.candi_link.as_ref().is_some_and(|link| link.irq()) {
                5
            } else {
                0
            })
            .max(if self.pending_candi.as_ref().is_some_and(|pending| pending.uart.irq()) { 5 } else { 0 })
            .max(u8::from(self.key_irq_pending()))
            .max(crate::tpu::interrupt(&self.sim).map_or(0, |(level, _)| level))
    }

    /// Visible text cells at the programmed font height and address pitch.
    /// Excludes graphics bytes and inactive rows from milestone detection.
    pub fn screen_text(&self) -> String {
        if !self.lcd.text_enabled() {
            return String::new();
        }
        let (cols, rows) = self.lcd.text_dimensions();
        let mut text = String::with_capacity((cols + 1) * rows);
        for row in 0..rows {
            for col in 0..cols {
                let b =
                    self.lcd.vram[(self.lcd.sad1 + row * self.lcd.ap + col) % self.lcd.vram.len()];
                text.push(if (0x20..0x7f).contains(&b) {
                    b as char
                } else {
                    ' '
                });
            }
            text.push('\n');
        }
        text
    }

    pub fn guest_splash_reached(&self) -> bool {
        let text = self.screen_text();
        (text.contains("Press [ENTER]") || text.contains("Press ENTER"))
            && text.contains("Software Version")
            && text.contains("North American Operations")
    }

    pub fn guest_boot_failed(&self) -> bool {
        let text = self.screen_text();
        text.contains("ADDRESS ERROR - PROCESSOR HALT") || text.contains("Nav task creation error")
    }

    /// Per-test POST results scraped from the LCD text layer.
    ///
    /// Returns (label, passed) in screen order.  The ROM prints each result as
    /// "NNN  NAME.....Pass" / "...Fail" from the string table at $0DEE/$0DF3, so
    /// the screen is the authoritative record of what the guest concluded.
    pub fn post_results(&self) -> Vec<(String, bool)> {
        let text = self.screen_text();
        let mut out = Vec::new();
        for name in [
            "IRAM", "ERAM", "UART", "MCU", "QSPI", "SCI", "TPU", "RTC", "CLKMEM", "KEYPAD",
        ] {
            if let Some(i) = text.find(name) {
                let tail: String = text[i..].chars().take(24).collect();
                if tail.contains("Pass") {
                    out.push((name.to_string(), true));
                } else if tail.contains("Fail") {
                    out.push((name.to_string(), false));
                }
            }
        }
        out
    }

    /// True only when every POST line the guest printed says Pass.
    ///
    /// The old form -- `contains("KEYPAD") && contains("Pass")` -- was true as
    /// soon as any single test passed anywhere on screen, so it reported
    /// "all-pass" over a screen with seven failures.
    pub fn post_complete(&self) -> bool {
        let r = self.post_results();
        r.len() == 10 && r.iter().all(|(_, ok)| *ok)
    }

    // ARMv7 load-test build: ordinary RAM/ROM words need one address decode.
    // Every overlaid byte, device window and cross-region access keeps the
    // existing byte bus, including CFI status and research compatibility reads.
    #[cfg(feature = "load-test")]
    #[inline]
    fn load_test_plain_read<const N: usize>(&self, address: u32) -> Option<[u8; N]> {
        let a = address & A24;
        let end = a + N as u32;
        if end <= FLASH_SIZE {
            if self.eprom_cfi_status && (a < 2 || (a < 0x8002 && end > 0x8000)) { return None; }
            if self.execution_mode.is_research_harness()
                && ((a <= 0x5ffc && end > 0x5ffc)
                    || (a < 0x16ef0 && end > 0x16ed2)) { return None; }
            return self.flash.get(a as usize..end as usize)?.try_into().ok();
        }
        if a >= RAM_BASE && end <= RAM_BASE + RAM_SIZE {
            if self.execution_mode.is_research_harness()
                && ((a < 0x101282 && end > 0x10127f)
                    || (a < 0x10eed8 && end > 0x10eeba)) { return None; }
            return self.ram.get((a-RAM_BASE) as usize..(end-RAM_BASE) as usize)?.try_into().ok();
        }
        None
    }

    /// Observe backing memory only: never poll MMIO, consume keys, or clear status.
    pub fn peek_bytes(&self, address: u32, len: usize) -> Option<&[u8]> {
        let (data, offset) = if address < FLASH_SIZE {
            (&self.flash, address as usize)
        } else if (RAM_BASE..RAM_BASE + RAM_SIZE).contains(&address) {
            (&self.ram, (address - RAM_BASE) as usize)
        } else if (ERAM_BASE + 0x1000..ERAM_BASE + ERAM_SIZE).contains(&address) {
            (&self.eram, (address - ERAM_BASE) as usize)
        } else {
            return None;
        };
        data.get(offset..offset.checked_add(len)?)
    }

    pub fn recent_pc_history(&self) -> Vec<u32> {
        let len = self.steps_recorded.min(64);
        (self.steps_recorded - len..self.steps_recorded)
            .map(|i| self.recent_pcs[i as usize & 63])
            .collect()
    }

    pub fn at_assertion_halt(&self, pc: u32) -> bool {
        // Supplied opsys assertion epilogue: BRA self; LEA $20(SP),SP.
        // Matching bytes as well as the known address avoids treating unrelated
        // firmware code or blank EPROM content as an assertion.
        matches!(pc, 0x0001_0cbe | 0x0011_0cbe)
            && self.peek_bytes(pc, 6) == Some(&[0x60, 0xfe, 0x4f, 0xef, 0x00, 0x20])
    }

    pub fn trace_event(&mut self, kind: &str, detail: &str) {
        self.trace
            .event(self.current_insns, self.current_pc, kind, detail);
    }

    pub fn trace_screen(&mut self) {
        if self.trace.enabled() {
            let screen = self.screen_text();
            self.trace
                .screen(self.current_insns, self.current_pc, screen);
            self.trace
                .highlight(self.current_insns, self.current_pc, self.highlighted_text());
            if let (Some(menu), Some(latch)) = (
                self.peek_bytes(KEY_MENU_IDX, 2),
                self.peek_bytes(KEY_LATCH, 1),
            ) {
                let menu = u16::from_be_bytes(menu.try_into().unwrap());
                let latch = latch[0];
                self.trace
                    .navigation_state(self.current_insns, self.current_pc, menu, latch);
            }
        }
    }

    /// Read the guest's broad XOR selection bar; never infer selection from
    /// the host key or the internal keypad queue index. Skip header rows.
    pub fn highlighted_text(&self) -> Option<String> {
        let lcd = &self.lcd;
        if !lcd.text_enabled() || lcd.overlay_mode & 3 != 1 || lcd.display_mode & 0x30 == 0 {
            return None;
        }
        let (_, rows) = lcd.text_dimensions();
        let text = self.screen_text();
        for (row, line) in text.lines().enumerate().take(rows).skip(3) {
            if line.trim().is_empty() {
                continue;
            }
            let y = row * lcd.fy as usize + lcd.fy as usize / 2;
            if y == 0 || y + 1 >= 240 {
                continue;
            }
            let bar = (y - 1..=y + 1).all(|y| {
                (8..312)
                    .filter(|x| {
                        lcd.vram[(lcd.sad2 + y * lcd.ap + x / 8) % lcd.vram.len()]
                            & (0x80 >> (x % 8))
                            != 0
                    })
                    .count()
                    > 280
            });
            if bar {
                return Some(line.trim().to_owned());
            }
        }
        None
    }

    pub fn trace_memory(&mut self, kind: &str, pointer: Option<u32>, len: usize) {
        if !self.trace.enabled() {
            return;
        }
        let bytes = pointer
            .and_then(|p| self.peek_bytes(p, len))
            .map(hex_bytes)
            .unwrap_or_else(|| "unavailable".into());
        let address = pointer
            .map(|p| format!("{p:#010x}"))
            .unwrap_or_else(|| "unavailable".into());
        self.trace_event(kind, &format!("address={address} prefix_length={len} bytes={bytes} interpretation=unclassified external_tx=false"));
    }

    pub fn trace_stack(&mut self, kind: &str, target: u32, sp: u32) {
        if !self.trace.enabled() {
            return;
        }
        let stack = self
            .peek_bytes(sp, 24)
            .map(hex_bytes)
            .unwrap_or_else(|| "unavailable".into());
        self.trace_event(
            kind,
            &format!("target={target:#010x} sp={sp:#010x} stack24={stack}"),
        );
    }

    /// Queue a hardware key code for the encoder at $600500 and raise IRQ1.
    ///
    /// `code` is the raw 5-bit encoder code (KEY_UP / KEY_DOWN / ...), not an
    /// ASCII value: the ROM does its own translation through the tables at
    /// $100CD0 / $100CEA.
    pub fn press_key(&mut self, code: u8) {
        self.trace_screen();
        self.trace.navigation += 1;
        let code5 = code & 0x1F;
        if self.trace.enabled() {
            self.trace_event(
                "key_queued",
                &format!(
                    "code={code5:#04x} name={} source=host encoder_down={:#04x} encoder_up={:#04x}",
                    key_name(code5),
                    (code5 << 3) | KEYPAD_ACTIVE_BIT,
                    code5 << 3
                ),
            );
        }
        let down_byte = (code5 << 3) | KEYPAD_ACTIVE_BIT;
        let up_byte = code5 << 3;
        self.key_queue.push_back(down_byte);
        self.key_queue.push_back(up_byte);
        self.key_events += 1;
        crate::log_info!(
            "KEY",
            self.current_insns,
            "{} encoder={:#04x} (events={} depth={})",
            key_name(code5),
            code5,
            self.key_events,
            self.key_queue.len()
        );
        if crate::options::env_flag("TECH2_KEYLOG") {
            println!(
                "KEYLOG queue code {:#04x} (events={} depth={})",
                code5,
                self.key_events,
                self.key_queue.len()
            );
        }
        if self.key_active.is_none() {
            self.latch_next_key();
        }

        if !self.execution_mode.is_research_harness() || !self.compatibility_splash {
            return;
        }

        let prev_screen = self.ui.screen;

        // Synthetic UI state exists only for the explicit research harness.
        // Normal key delivery above is always hardware-only.
        if code5 == KEY_ENTER_DEFAULT || code5 == 0x14 || code5 == 0x19 {
            match self.ui.screen {
                UiScreen::Splash => {
                    self.ui.screen = UiScreen::MainMenu;
                    self.ui.menu_selected = 0;
                }
                UiScreen::MainMenu => match self.ui.menu_selected {
                    0 => {
                        self.ui.screen = UiScreen::Diagnostics;
                        self.ui.diag_selected = 0;
                    }
                    1 => {
                        self.ui.screen = UiScreen::ServiceProgramming;
                        self.ui.sps_selected = 0;
                    }
                    2 => {
                        self.ui.screen = UiScreen::ViewStoredData;
                        self.ui.data_selected = 0;
                    }
                    3 => {
                        self.ui.screen = UiScreen::ToolOptions;
                        self.ui.tool_selected = 0;
                    }
                    4 => {
                        self.ui.screen = UiScreen::Download;
                        self.ui.dl_selected = 0;
                    }
                    _ => {}
                },
                _ => {}
            }
        } else if code5 == KEY_DOWN {
            match self.ui.screen {
                UiScreen::Splash => {
                    self.ui.screen = UiScreen::MainMenu;
                    self.ui.menu_selected = 0;
                }
                UiScreen::MainMenu => {
                    self.ui.menu_selected = (self.ui.menu_selected + 1) % 5;
                    println!("TECH2: Main Menu -> item {}", self.ui.menu_selected);
                }
                UiScreen::Diagnostics => {
                    self.ui.diag_selected = (self.ui.diag_selected + 1) % 6;
                }
                UiScreen::ServiceProgramming => {
                    self.ui.sps_selected = (self.ui.sps_selected + 1) % 4;
                }
                UiScreen::ViewStoredData => {
                    self.ui.data_selected = (self.ui.data_selected + 1) % 4;
                }
                UiScreen::ToolOptions => {
                    self.ui.tool_selected = (self.ui.tool_selected + 1) % 4;
                }
                UiScreen::Download => {
                    self.ui.dl_selected = (self.ui.dl_selected + 1) % 3;
                }
            }
        } else if code5 == KEY_UP {
            match self.ui.screen {
                UiScreen::Splash => {
                    self.ui.screen = UiScreen::MainMenu;
                    self.ui.menu_selected = 0;
                }
                UiScreen::MainMenu => {
                    self.ui.menu_selected = if self.ui.menu_selected == 0 {
                        4
                    } else {
                        self.ui.menu_selected - 1
                    };
                    println!("TECH2: Main Menu -> item {}", self.ui.menu_selected);
                }
                UiScreen::Diagnostics => {
                    self.ui.diag_selected = if self.ui.diag_selected == 0 {
                        5
                    } else {
                        self.ui.diag_selected - 1
                    };
                }
                UiScreen::ServiceProgramming => {
                    self.ui.sps_selected = if self.ui.sps_selected == 0 {
                        3
                    } else {
                        self.ui.sps_selected - 1
                    };
                }
                UiScreen::ViewStoredData => {
                    self.ui.data_selected = if self.ui.data_selected == 0 {
                        3
                    } else {
                        self.ui.data_selected - 1
                    };
                }
                UiScreen::ToolOptions => {
                    self.ui.tool_selected = if self.ui.tool_selected == 0 {
                        3
                    } else {
                        self.ui.tool_selected - 1
                    };
                }
                UiScreen::Download => {
                    self.ui.dl_selected = if self.ui.dl_selected == 0 {
                        2
                    } else {
                        self.ui.dl_selected - 1
                    };
                }
            }
        }

        if prev_screen != self.ui.screen {
            println!(
                "TECH2: Screen transition: {:?} -> {:?}",
                prev_screen, self.ui.screen
            );
            crate::log_info!(
                "UI",
                0,
                "Screen transition: {:?} -> {:?}",
                prev_screen,
                self.ui.screen
            );
        }
    }

    /// Exit/Back key handler to step backwards through screens.
    pub fn exit_key(&mut self) {
        // Native CANdi sessions must consume the real keypad EXIT themselves.
        // Never inject the legacy abort flags into an original diagnostic flow.
        if self.execution_mode.is_research_harness() && self.candi_link.is_none() && self.pending_candi.is_none() {
            if self.compatibility_splash {
                crate::log_info!(
                    "KEY",
                    0,
                    "ESC/EXIT pressed - navigating back from {:?}",
                    self.ui.screen
                );
                match self.ui.screen {
                    UiScreen::Diagnostics
                    | UiScreen::ServiceProgramming
                    | UiScreen::ViewStoredData
                    | UiScreen::ToolOptions
                    | UiScreen::Download => {
                        self.ui.screen = UiScreen::MainMenu;
                    }
                    UiScreen::MainMenu => {
                        self.ui.screen = UiScreen::Splash;
                    }
                    UiScreen::Splash => {}
                }
            }
            // Application-abort flags are guest-memory edits, so they are
            // deliberately a harness capability rather than normal ESC input.
            self.trace_event("research_override", "EXIT writes abort flags: 0x001fbe6e=0001 0x001fbc72=0001; no transport cancellation implemented");
            self.ram_write_word(0x001F_BE6E, 1);
            self.ram_write_word(0x001F_BC72, 1);
        }
        self.press_key(KEY_EXIT);
    }

    /// Write 16-bit word to guest RAM.
    pub fn ram_write_word(&mut self, addr: u32, val: u16) {
        if (RAM_BASE..RAM_BASE + RAM_SIZE).contains(&addr) {
            let Some(off) = addr.checked_sub(RAM_BASE).map(|v| v as usize) else {
                return;
            };
            let bytes = val.to_be_bytes();
            if let Some(dst) = self.ram.get_mut(off..off + 2) {
                dst.copy_from_slice(&bytes);
            }
        }
    }

    /// Write 8-bit byte to guest RAM.
    pub fn ram_write_byte(&mut self, addr: u32, val: u8) {
        if (RAM_BASE..RAM_BASE + RAM_SIZE).contains(&addr) {
            let Some(off) = addr.checked_sub(RAM_BASE).map(|v| v as usize) else {
                return;
            };
            self.ram[off] = val;
        }
    }

    /// Execute QSPI queue transfer initiated by setting SPE (bit 15) in SPCR1.
    ///
    /// Implements authentic 68332 QSM transfer and peripheral dispatch matching
    /// Tech2Win emulator.exe:
    /// - Clears SPIF upon starting transfer.
    /// - Iterates queue slots from NEWQP to ENDQP (read from SPCR2).
    /// - If SPCR3 bit 2 (LOOPQ) is set: internal loopback (TX -> RX).
    /// - If Command RAM selects RTC (PCS0 active-low / 0x0A): decodes DS1302 SPI
    ///   commands, returning valid date/time BCD bytes and CLKMEM signature 0xA5.
    /// - Sets SPIF (bit 7) and CPTQP = ENDQP in SPSR upon completion.
    /// - Clears SPE in SPCR1.
    pub fn qspi_transfer(&mut self) {
        // Clear SPIF and mark transfer pending (as in emulator.exe 0x41C76D)
        self.qspi_pending = true;
        self.sim[OFF_SPSR] &= 0x7F;

        // SPCR2: High byte (0x0C1C) = ENDQP (bits 3..0), Low byte (0x0C1D) = NEWQP (bits 3..0)
        let endqp = (self.sim[OFF_SPCR2] & 0x0F) as usize;
        let newqp = (self.sim[OFF_SPCR2 + 1] & 0x0F) as usize;
        let loopq = (self.sim[OFF_SPCR3] & 0x04) != 0;

        let mut qp = newqp;
        loop {
            let slot = qp & 0x0F;
            let cmd = self.sim[0x0D40 + slot];
            let tx_hi = self.sim[0x0D20 + slot * 2];
            let tx_lo = self.sim[0x0D20 + slot * 2 + 1];

            if loopq {
                // Loop Mode: internal loopback TX -> RX (used by QSPI POST test at $0016A2)
                self.sim[0x0D00 + slot * 2] = tx_hi;
                self.sim[0x0D00 + slot * 2 + 1] = tx_lo;
            } else {
                // Peripheral dispatch via Command RAM PCS (active low)
                let pcs = cmd & 0x0F;
                if pcs == 0x0E || pcs == 0x0A {
                    // RTC DS1302 device:
                    // Values bit-reversed in hardware, unpacked by ROM $001AA6 and routed via $001B70:
                    // Entry 4 (Idx 0): Seconds -> $120000 (valid range 0..59)
                    // Entry 5 (Idx 1): Day     -> $120008 (valid range 1..7)   - BCD 0x03 -> rev8 = 0xC0
                    // Entry 6 (Idx 2): Month   -> $120004 (valid range 1..12)  - BCD 0x09 -> rev8 = 0x90
                    // Entry 7 (Idx 3): Date    -> $12000C (valid range 1..31)  - BCD 0x04 -> rev8 = 0x20
                    // Entry 8 (Idx 4): Hours   -> $120010 (valid range 0..23)  - BCD 0x12 -> rev8 = 0x48
                    // Entry 9 (Idx 5): Minutes -> $120014 (valid range 0..59)  - BCD 0x30 -> rev8 = 0x0C
                    // Entry 10 (Idx 6): Sec2   -> $120018 (valid range 0..59)  - BCD 0x00 -> rev8 = 0x00
                    // Entry 11 (Idx 7): Sig    -> $12001C (CLKMEM signature byte 0xA5)
                    let rx_val = match slot {
                        4 => 0x00,  // Seconds: 0
                        5 => 0xC0,  // Day of week: 3 (BCD 0x03, rev8 = 0xC0)
                        6 => 0x90,  // Month: 9 (BCD 0x09, rev8 = 0x90)
                        7 => 0x20,  // Date: 4 (BCD 0x04, rev8 = 0x20)
                        8 => 0x48,  // Hours: 12 (BCD 0x12, rev8 = 0x48)
                        9 => 0x0C,  // Minutes: 30 (BCD 0x30, rev8 = 0x0C)
                        10 => 0x00, // Seconds2: 0
                        11 => 0xA5, // CLKMEM signature byte (0xA5 reversed = 0xA5)
                        _ => 0x00,
                    };
                    self.sim[0x0D00 + slot * 2] = 0x00;
                    self.sim[0x0D00 + slot * 2 + 1] = rx_val;
                } else if pcs & 0x0e == 2 && (self.candi_link.is_some() || self.pending_candi.is_some()) {
                    let rx = if let Some(pending) = &mut self.pending_candi {
                        crate::candi::demand::cable_adc(&mut pending.adc_channel, self.cs6[0], u16::from_be_bytes([tx_hi,tx_lo]))
                    } else { self.candi_link.as_mut().unwrap().adc_transfer(self.cs6[0], u16::from_be_bytes([tx_hi,tx_lo])) };
                    let [hi, lo] = rx.to_be_bytes();
                    self.sim[0x0d00 + slot * 2] = hi;
                    self.sim[0x0d01 + slot * 2] = lo;
                } else {
                    // Default / fallback: echo TX to RX
                    self.sim[0x0D00 + slot * 2] = tx_hi;
                    self.sim[0x0D00 + slot * 2 + 1] = tx_lo;
                }
            }

            if self.trace.enabled() {
                let backend = if loopq {
                    "internal-loopback"
                } else if matches!(cmd & 15, 0x0e | 0x0a) {
                    "rtc-fixed-model"
                } else if cmd & 0x0e == 2 && self.candi_link.is_some() {
                    "virtual-cable-adc-oem-default"
                } else {
                    "fallback-echo"
                };
                self.trace_event("qspi_transfer", &format!("slot={slot} command={cmd:#04x} pcs={:#x} tx={tx_hi:02x}{tx_lo:02x} rx={:02x}{:02x} backend={backend} external_tx=false", cmd & 15, self.sim[0xd00 + slot * 2], self.sim[0xd01 + slot * 2]));
            }
            if qp == endqp {
                break;
            }
            qp = (qp + 1) & 0x0F;
        }

        // Completion: record CPTQP = ENDQP in SPSR (SPIF will be set when polled)
        self.sim[OFF_SPSR] = endqp as u8 & 0x0F;
        self.qspi_pending = true;

        // Clear SPE in SPCR1
        self.sim[OFF_SPCR1] &= 0x7F;
    }

    /// Direct digit/number key selection.
    pub fn select_digit(&mut self, digit: u8) {
        if self.execution_mode.is_research_harness() && self.compatibility_splash {
            crate::log_info!(
                "KEY",
                0,
                "Digit key {} pressed (active screen: {:?})",
                digit,
                self.ui.screen
            );
            match self.ui.screen {
                UiScreen::MainMenu if (digit as usize) < 5 => {
                    self.ui.menu_selected = digit as usize;
                    self.press_key(KEY_ENTER_DEFAULT);
                }
                UiScreen::Diagnostics if (1..=6).contains(&digit) => {
                    self.ui.diag_selected = (digit - 1) as usize;
                }
                UiScreen::ServiceProgramming if (1..=4).contains(&digit) => {
                    self.ui.sps_selected = (digit - 1) as usize;
                }
                UiScreen::ViewStoredData if (1..=4).contains(&digit) => {
                    self.ui.data_selected = (digit - 1) as usize;
                }
                UiScreen::ToolOptions if (1..=4).contains(&digit) => {
                    self.ui.tool_selected = (digit - 1) as usize;
                }
                UiScreen::Download if (1..=3).contains(&digit) => {
                    self.ui.dl_selected = (digit - 1) as usize;
                }
                _ => {}
            }
        }
        // Send genuine hardware matrix key code to guest
        let code = match digit {
            0 => 0x18,
            1 => 0x04,
            2 => 0x13,
            3 => 0x17,
            4 => 0x03,
            5 => 0x12,
            6 => 0x16,
            7 => 0x02,
            8 => 0x11,
            9 => 0x15,
            _ => return,
        };
        self.press_key(code);
    }

    fn latch_next_key(&mut self) {
        self.key_active = self.key_queue.pop_front();
        self.key_hold_count = 6;
        self.key_irq = self.key_active.is_some();
    }

    /// Destructive read of the encoder. Holds pending code across 6 reads
    /// before latching next code, ensuring multi-read debounce filters sample reliably.
    fn keypad_read(&mut self) -> u8 {
        self.key_reads += 1;
        match self.key_active {
            Some(raw_code) => {
                if self.key_hold_count > 0 {
                    self.key_hold_count -= 1;
                }
                if self.key_hold_count == 0 {
                    if self.trace.enabled() {
                        self.trace_event(
                            "key_consumed",
                            &format!(
                                "encoder={raw_code:#04x} code={:#04x} phase={}",
                                raw_code >> 3,
                                if raw_code & KEYPAD_ACTIVE_BIT != 0 {
                                    "down"
                                } else {
                                    "up"
                                }
                            ),
                        );
                    }
                    self.key_consumed += 1;
                    self.key_irq = false;
                    self.latch_next_key();
                }
                raw_code
            }
            None => KEYPAD_IDLE,
        }
    }

    /// True while a key event is waiting for the guest to read $600500.
    pub fn key_irq_pending(&self) -> bool {
        self.key_irq
    }

    fn ata_command(&mut self, cmd: u8) {
        if self.trace.enabled() {
            self.trace_event("ata_command", &format!("command={cmd:#04x} device={:#04x} lba_registers={:02x?} count={} backend=card-image-model", self.ata.dev, self.ata.lba, self.ata.nsec));
        }
        match cmd {
            0x20 | 0x21 => {
                // Read Sectors
                let is_lba = (self.ata.dev & 0x40) != 0;
                let lba = if is_lba {
                    ((self.ata.dev as usize & 0x0F) << 24)
                        | ((self.ata.lba[2] as usize) << 16)
                        | ((self.ata.lba[1] as usize) << 8)
                        | (self.ata.lba[0] as usize)
                } else {
                    let cyl = ((self.ata.lba[2] as usize) << 8) | (self.ata.lba[1] as usize);
                    let head = (self.ata.dev as usize) & 0x0F;
                    let sec = self.ata.lba[0] as usize;

                    let spt = u16::from_be_bytes([
                        self.ram[0x0010_127E - RAM_BASE as usize],
                        self.ram[0x0010_127F - RAM_BASE as usize],
                    ]) as usize;
                    let hpc = u16::from_be_bytes([
                        self.ram[0x0010_1280 - RAM_BASE as usize],
                        self.ram[0x0010_1281 - RAM_BASE as usize],
                    ]) as usize;
                    if spt > 0 && hpc > 0 {
                        (cyl * hpc + head) * spt + sec.saturating_sub(1)
                    } else {
                        sec.saturating_sub(1)
                    }
                };
                let count = if self.ata.nsec == 0 {
                    256
                } else {
                    self.ata.nsec as usize
                };
                self.ata.lba_addr = lba * 512;
                self.ata.remaining = count * 512;
                self.ata.status = 0x58; // DRDY | DSC | DRQ
                self.ata.err = 0;
                static CALLS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
                let c = CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if c < 15 {
                    println!(
                        "ATA: READ #{} from PC={:#010x} lba={} count={} (cyl={:#x} head={:#x} sec={:#x} spt={} hpc={})",
                        c, self.current_pc, lba, count,
                        ((self.ata.lba[2] as usize) << 8) | (self.ata.lba[1] as usize),
                        self.ata.dev & 0x0F,
                        self.ata.lba[0],
                        u16::from_be_bytes([self.ram[0x0010_127E - RAM_BASE as usize], self.ram[0x0010_127F - RAM_BASE as usize]]),
                        u16::from_be_bytes([self.ram[0x0010_1280 - RAM_BASE as usize], self.ram[0x0010_1281 - RAM_BASE as usize]])
                    );
                }
            }
            0x30 | 0x31 => {
                // Write Sectors
                let is_lba = (self.ata.dev & 0x40) != 0;
                let lba = if is_lba {
                    ((self.ata.dev as usize & 0x0F) << 24)
                        | ((self.ata.lba[2] as usize) << 16)
                        | ((self.ata.lba[1] as usize) << 8)
                        | (self.ata.lba[0] as usize)
                } else {
                    let cyl = ((self.ata.lba[2] as usize) << 8) | (self.ata.lba[1] as usize);
                    let head = (self.ata.dev as usize) & 0x0F;
                    let sec = self.ata.lba[0] as usize;
                    let spt = u16::from_be_bytes([
                        self.ram[0x0010_127E - RAM_BASE as usize],
                        self.ram[0x0010_127F - RAM_BASE as usize],
                    ]) as usize;
                    let hpc = u16::from_be_bytes([
                        self.ram[0x0010_1280 - RAM_BASE as usize],
                        self.ram[0x0010_1281 - RAM_BASE as usize],
                    ]) as usize;
                    if spt > 0 && hpc > 0 {
                        (cyl * hpc + head) * spt + sec.saturating_sub(1)
                    } else {
                        sec.saturating_sub(1)
                    }
                };
                let count = if self.ata.nsec == 0 {
                    256
                } else {
                    self.ata.nsec as usize
                };
                self.ata.lba_addr = lba * 512;
                self.ata.remaining = count * 512;
                self.ata.status = 0x58; // DRDY | DSC | DRQ
                self.ata.err = 0;
            }
            0x90 => {
                self.ata.status = 0x50;
                self.ata.err = 0x01; // Diagnostics passed
            }
            0x91 => {
                self.ata.status = 0x50;
                self.ata.err = 0;
            }
            _ => {
                self.ata.status = 0x50;
                self.ata.err = 0;
            }
        }
    }

    pub fn render_lcd_pixels(&self, buf: &mut [u32]) {
        self.render_lcd_pixels_stride(buf, 320);
    }

    pub fn render_lcd_pixels_stride(&self, buf: &mut [u32], stride: usize) {
        let w = 320usize;
        let h = 240usize;
        if stride < w || buf.len() / stride < h {
            return;
        }
        for y in 0..h {
            for x in 0..w {
                buf[y * stride + x] = if self.lcd.pixel(x, y) {
                    self.fg_color
                } else {
                    self.bg_color
                };
            }
        }
    }
}

impl AddressBus for Tech2Bus {
    fn interrupt_acknowledge(&mut self, level: u8) -> u32 {
        if level == 5 && (self.candi_link.as_ref().is_some_and(|link| link.irq()) || self.pending_candi.as_ref().is_some_and(|pending| pending.uart.irq())) {
            return 0x1d;
        }

        if let Some((tpu_level, vector)) = crate::tpu::interrupt(&self.sim) {
            if level == tpu_level {
                return vector;
            }
        }
        let picr = u16::from_be_bytes([self.sim[OFF_PICR], self.sim[OFF_PICR + 1]]);
        let picr_level = ((picr >> 8) & 7) as u8;
        if self.pit_pending && level == picr_level {
            self.pit_pending = false;
            self.pit_ticks += 1;
            (picr & 0xFF) as u32
        } else {
            0xFFFF_FFFF
        }
    }

    fn read_byte(&mut self, address: u32) -> u8 {
        let a = address & A24;
        self.trace_eram_access('R', a, None);
        if (0x400800..0x400810).contains(&a) {
            if let Some(pending) = &mut self.pending_candi {
                return pending.uart.read(a, &mut Vec::new());
            }
            if let Some(link) = &mut self.candi_link {
                return link.read(a);
            }
        }

        if let Some(is_cmd) = lcd_kind(a) {
            if self.lcd.cmd == 0x47 || self.lcd.cmd == 0x43 {
                return self.lcd.read_data();
            }
            return if is_cmd {
                self.lcd.status
            } else {
                self.lcd.read_data()
            };
        }

        if self.execution_mode.is_research_harness() {
            // Legacy dynamic equivalents of the research ROM patches.  Keep
            // them out of the fidelity read path: returning altered opcode
            // bytes makes a raw checksum or disassembly misleading.
            if a == 0x0001_6ED2 || a == 0x0010_EEBA {
                return 0x4E;
            }
            if a == 0x0001_6ED3 || a == 0x0010_EEBB {
                return 0x71;
            }
            if a == 0x0001_6EEE || a == 0x0010_EED6 {
                return 0x4E;
            }
            if a == 0x0001_6EEF || a == 0x0010_EED7 {
                return 0x71;
            }

            if a == 0x0000_5FFC {
                return 0x01; // Boot payload bypass flag
            }
        }

        if a < FLASH_SIZE {
            if self.eprom_cfi_status && (a < 2 || a == 0x8000 || a == 0x8001) {
                return 0x80;
            }
            return self.flash[a as usize];
        }

        if (RAM_BASE..RAM_BASE + RAM_SIZE).contains(&a) {
            let off = (a - RAM_BASE) as usize;
            // The legacy ATA screen harness fabricated disk geometry in two
            // bytes of guest RAM.  Those bytes are inside the ROM's external
            // RAM walking-pattern test, so exposing the fabrication during a
            // native boot guarantees a false ERAM failure.
            if self.execution_mode.is_research_harness() && (off == 0x127F || off == 0x1281) {
                let v = self.ram[off];
                return if v == 0 && self.ram[off - 1] == 0 {
                    8
                } else {
                    v
                };
            }
            return self.ram[off];
        }

        if (0x007C_0000..0x007F_F000).contains(&a) {
            return self.card.get(a as usize).copied().unwrap_or(UNUSED);
        }

        if (CARD_BASE..CARD_BASE + CARD_WIN).contains(&a) {
            let win_off = a - CARD_BASE;
            // Attribute memory (CIS) vs common memory is selected by $600600, NOT by
            // $500008. ROM $16468 builds a common-memory value as (page << 2) | 3 and
            // $164A0 reads it back with ASR.W #2 before writing 0; $0064AE writes the
            // same shape. So the low two bits are a common-memory enable: clear => the
            // window shows attribute memory. ($500008 bit 0 is set once at eprom $812C
            // and never cleared, so gating on it pinned one space permanently.)
            if (self.bank & 3) == 0 {
                let idx = (win_off as usize) % self.attr_mem.len();
                self.attr_reads += 1;
                return self.attr_mem[idx];
            }
            if let Some(ref ram) = self.cs2_ram {
                return ram[(win_off as usize) % ram.len()];
            }
            if let Some(off) = self.card_off(win_off) {
                if crate::ssa_flash::SsaFlash::handles(off) {
                    if let Some(flash) = &self.ssa_flash {
                        return flash.status().unwrap_or(self.card[off]);
                    }
                }
            }
            if self.cfi_status {
                return 0x80;
            }
            if let Some(off) = self.card_off(win_off) {
                let seg = (off >> 20) & 0xF;
                self.card_rd_pages[seg] += 1;
                return self.card[off];
            }
            return UNUSED;
        }

        // A prior menu-test harness overlaid an ATA/COR device on the first
        // bytes of external RAM.  The boot ROM's ERAM POST explicitly writes
        // test patterns to $300000, so this alias makes a healthy RAM model
        // fail its own test.  Keep the shim only for that old research path;
        // a fidelity run exposes the whole configured ERAM range as RAM.
        if self.execution_mode.is_research_harness()
            && ((0x0030_0000..=0x0030_000F).contains(&a) || a == 0x0030_0200)
        {
            if a == 0x0030_0200 {
                return 0x01; // PCMCIA Configuration Option Register (COR)
            }
            match a & 0x0F {
                0x0 => {
                    if self.ata.remaining > 0 {
                        let b = if self.ata.lba_addr < self.card.len() {
                            self.card[self.ata.lba_addr]
                        } else {
                            0
                        };
                        self.ata.lba_addr += 1;
                        self.ata.remaining -= 1;
                        if self.ata.remaining == 0 {
                            self.ata.status = 0x50; // DRQ cleared, DRDY | DSC set
                        }
                        return b;
                    }
                    return self.eram[(a - ERAM_BASE) as usize];
                }
                0x1 => {
                    if (self.ata.status & 0x08) != 0 && self.ata.remaining > 0 {
                        let b = if self.ata.lba_addr < self.card.len() {
                            self.card[self.ata.lba_addr]
                        } else {
                            0
                        };
                        self.ata.lba_addr += 1;
                        self.ata.remaining -= 1;
                        if self.ata.remaining == 0 {
                            self.ata.status = 0x50;
                        }
                        return b;
                    }
                    return self.ata.err;
                }
                0x2 => return self.ata.nsec,
                0x3 => return self.ata.lba[0],
                0x4 => return self.ata.lba[1],
                0x5 => return self.ata.lba[2],
                0x6 => return self.ata.dev,
                0x7 | 0xE => return self.ata.status,
                _ => return self.eram[(a - ERAM_BASE) as usize],
            }
        }

        if (ERAM_BASE..ERAM_BASE + ERAM_SIZE).contains(&a) {
            return self.eram[(a - ERAM_BASE) as usize];
        }

        if (RTC_BASE..RTC_BASE + RTC_SIZE).contains(&a) {
            let off = (a - RTC_BASE) as usize;
            let mut v = self.rtc[off];
            if off == 0x0A {
                v |= 0x20; // PCMCIA Card Inserted status bit
            }
            return v;
        }

        if (KEY_BASE..KEY_BASE + KEY_SIZE).contains(&a) {
            return self.iram[(a - KEY_BASE) as usize];
        }

        if a == KEYPAD_REG {
            return self.keypad_read();
        }

        if (CS6_BASE..CS6_BASE + CS6_SIZE).contains(&a) {
            return self.cs6[(a - CS6_BASE) as usize];
        }

        if let Some(off) = sim_off(a) {
            if off == OFF_SCSR {
                return 0xC0; // QSM SCI status: TDRE (0x80) | TC (0x40) ready
            }
            if off == OFF_SPSR {
                if self.qspi_pending {
                    self.qspi_pending = false;
                    self.sim[OFF_SPSR] |= 0x80;
                }
                return self.sim[OFF_SPSR]; // QSM SPI status: SPIF (0x80) | CPTQP (bits 3:0)
            }
            if self.execution_mode.is_research_harness() && off == OFF_TPU {
                return self.sim[OFF_TPU] | 0x04; // Ensure TPUMCR bit 10 is set (TPU initialized)
            }
            if self.execution_mode.is_research_harness() && (off == 0x0A04 || off == 0x0A05) {
                return 0x10; // UART/SCI POST pass status bit 4 (0x10)
            }
            return self.sim[off];
        }

        UNUSED
    }

    fn write_byte(&mut self, address: u32, value: u8) {
        let a = address & A24;
        self.trace_eram_access('W', a, Some(value));
        if (0x400800..0x400810).contains(&a) {
            if let Some(pending) = &mut self.pending_candi {
                if !pending.uart.needs_processor(a, value) {
                    pending.uart.write(a, value, &mut Vec::new());
                    return;
                }
            }
            self.ensure_candi("serial-transmit");
            if let Some(link) = &mut self.candi_link {
                link.write(a, value);
                return;
            }
        }
        if self.trace.enabled() && lcd_kind(a).is_none() {
            let peripheral = if sim_off(a).is_some_and(|off| (0xc0e..=0xc0f).contains(&off)) {
                Some(("sci_data_write", "register-storage-only external_tx=false"))
            } else if sim_off(a).is_some_and(|off| (0xc00..=0xc1f).contains(&off)) {
                Some(("qsm_register_write", "qsm-model"))
            } else if (CS6_BASE..CS6_BASE + CS6_SIZE).contains(&a) {
                Some(("cs6_write", "raw-register-write"))
            } else if (RTC_BASE..RTC_BASE + RTC_SIZE).contains(&a) {
                Some(("rtc_write", "rtc-register-model"))
            } else {
                None
            };
            if let Some((kind, backend)) = peripheral {
                self.trace_event(
                    kind,
                    &format!("address={a:#010x} value={value:02x} width=8 backend={backend}"),
                );
            }
        }

        if let Some(is_cmd) = lcd_kind(a) {
            if is_cmd {
                self.lcd.write_cmd(value);
            } else {
                #[cfg(feature = "load-test")]
                let post_trace = { static VALUE: LazyLock<bool> = LazyLock::new(|| crate::options::env_flag("POST_TRACE")); *VALUE };
                #[cfg(not(feature = "load-test"))]
                let post_trace = crate::options::env_flag("POST_TRACE");
                if post_trace && (0x21..0x7f).contains(&value) {
                    static LCD_TRACE_COUNT: std::sync::atomic::AtomicUsize =
                        std::sync::atomic::AtomicUsize::new(0);
                    let n = LCD_TRACE_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    if n < 320 {
                        println!(
                            "POST_LCD pc={:#010x} vram={:#06x} char={:?}",
                            self.current_pc, self.lcd.cursor, value as char
                        );
                    }
                }
                self.lcd.write_data(value);
            }
            return;
        }

        if a < FLASH_SIZE {
            // The update driver uses the status pointer at $0000 and selects
            // update blocks at $8000+.  Treat the chip as a 16-bit device: a
            // CPU word store reaches us as two byte stores, so byte-level
            // command decoding would mistake the second command lane for data.
            if !(2..0x8000).contains(&a) {
                self.write_eprom_byte(a, value);
            }
            return;
        }

        if (RAM_BASE..RAM_BASE + RAM_SIZE).contains(&a) {
            self.ram[(a - RAM_BASE) as usize] = value;
            return;
        }

        if (CARD_BASE..CARD_BASE + CARD_WIN).contains(&a) {
            let win_off = a - CARD_BASE;
            if self.candi_link.is_some() {
                self.trace_event("card_window_store", &format!("address={a:#x} card_offset={:?} value={value:02x} card_writes_enabled={} origin=guest raw_store_not_decoded_flash_program", self.card_off(win_off), self.card_writes));
                if let Some(off) = self
                    .card_off(win_off)
                    .filter(|off| (0xfe0000..0xfe0000 + 714).contains(off))
                {
                    self.trace_event("ssa_card_store", &format!("card_offset={off:#x} value={value:02x} card_writes_enabled={} origin=guest raw_store_not_decoded_flash_program", self.card_writes));
                }
            }
            if let Some(ref mut ram) = self.cs2_ram {
                let idx = (win_off as usize) % ram.len();
                ram[idx] = value;
                return;
            }
            if let Some(off) = self.card_off(win_off) {
                if crate::ssa_flash::SsaFlash::handles(off) {
                    if let Some(flash) = &mut self.ssa_flash {
                        flash.write_byte(off, value, &mut self.card);
                        return;
                    }
                }
            }
            if value == 0x50 {
                self.cfi_status = false;
            } else if value == 0x20 {
                self.cfi_erase_armed = true;
            } else if value == 0xD0 && self.cfi_erase_armed {
                self.cfi_erase_armed = false;
                self.cfi_status = true;
            } else if value == 0x70 {
                self.cfi_status = true;
            } else if value == 0xFF {
                self.cfi_status = false;
            }
            self.cfi_cmds += 1;
            if self.card_writes {
                if let Some(off) = self.card_off(win_off) {
                    if off < self.card.len() {
                        let seg = (off >> 20) & 0xF;
                        self.card_wr_pages[seg] += 1;
                        self.card[off] = value;
                        return;
                    }
                }
            }
            self.card_wr_dropped += 1;
            return;
        }

        if self.execution_mode.is_research_harness()
            && ((0x0030_0000..=0x0030_000F).contains(&a) || a == 0x0030_0200)
        {
            if a != 0x0030_0200 {
                match a & 0x0F {
                    0x0 => {
                        if self.ata.remaining > 0 {
                            if self.ata.lba_addr < self.card.len() {
                                self.card[self.ata.lba_addr] = value;
                            }
                            self.ata.lba_addr += 1;
                            self.ata.remaining -= 1;
                            if self.ata.remaining == 0 {
                                self.ata.status = 0x50;
                            }
                        }
                    }
                    0x1 => self.ata.err = value,
                    0x2 => self.ata.nsec = value,
                    0x3 => self.ata.lba[0] = value,
                    0x4 => self.ata.lba[1] = value,
                    0x5 => self.ata.lba[2] = value,
                    0x6 => self.ata.dev = value,
                    0x7 => self.ata_command(value),
                    0xE => {
                        if (value & 0x04) != 0 {
                            self.ata.status = 0x80; // BSY (reset)
                        } else {
                            self.ata.status = 0x50; // DRDY
                        }
                    }
                    _ => {}
                }
            }
            self.eram[(a - ERAM_BASE) as usize] = value;
            return;
        }

        if (ERAM_BASE..ERAM_BASE + ERAM_SIZE).contains(&a) {
            self.eram[(a - ERAM_BASE) as usize] = value;
            return;
        }

        if (RTC_BASE..RTC_BASE + RTC_SIZE).contains(&a) {
            self.rtc[(a - RTC_BASE) as usize] = value;
            return;
        }

        if (KEY_BASE..KEY_BASE + KEY_SIZE).contains(&a) {
            self.iram[(a - KEY_BASE) as usize] = value;
            return;
        }

        if (CS6_BASE..CS6_BASE + CS6_SIZE).contains(&a) {
            self.cs6[(a - CS6_BASE) as usize] = value;
            if a == PCMCIA_BANK_REG {
                self.bank = value as u32;
            }
            if a == KEYPAD_REG {
                // Menu-counter pulse, not an acknowledgement -- see KEYPAD_REG.
                self.key_pulses += 1;
            }
            return;
        }

        if let Some(off) = sim_off(a) {
            // TPU CISR is cleared by writing zero; software cannot manufacture
            // a hardware event by writing one to a currently clear flag.
            if matches!(off, 0xe20 | 0xe21) {
                self.sim[off] &= value;
            } else {
                self.sim[off] = value;
            }
            if off == OFF_HSRR0 && value & 3 == 1 {
                if self.demand_boot_complete { self.ensure_candi("presence-probe"); }
                if let Some(link) = &mut self.candi_link {
                    link.presence_announced = false;
                    self.sim[0xff2] = 0xff;
                    self.sim[0xff3] = 0xff;
                    self.trace_event("candi_native_link", "Tech2 requested CANdi presence probe channel=12 host_service=1 external_tx=false");
                }
            }
            if (OFF_HSRR0..=OFF_HSRR1 + 1).contains(&off) {
                if (self.candi_link.is_some() || self.pending_candi.is_some()) && off == OFF_HSRR1 {
                    self.tpu_countdown.uart_host_service(value);
                    self.trace_event("candi_uart_timer", &format!(
                        "host_service={value:#04x} ch4_words={:02x?} ch5_words={:02x?} functions={:02x?} sequence={:02x?} priority={:02x?} external_tx=false",
                        &self.sim[0xf40..0xf4a], &self.sim[0xf50..0xf5a],
                        &self.sim[0xe10..0xe12], &self.sim[0xe16..0xe18], &self.sim[0xe1e..0xe20]
                    ));
                }
                self.sim[off] = 0;
            }
            if off == OFF_SPCR1 && (value & 0x80) != 0 {
                self.qspi_transfer();
            }
            return;
        }

        if self.trace.enabled() {
            self.trace_event("unmapped_write", &format!("address={a:#010x} value={value:02x} width=8 backend=discarded external_tx=false"));
        }
        if crate::options::env_flag("LOG_UNMAPPED") {
            println!("UNMAPPED WRITE at {a:#010x}: {value:#04x}");
        }
    }

    fn read_word(&mut self, address: u32) -> u16 {
        #[cfg(feature = "load-test")]
        if let Some(bytes) = self.load_test_plain_read(address) { return u16::from_be_bytes(bytes); }
        let hi = self.read_byte(address) as u16;
        let lo = self.read_byte(address.wrapping_add(1)) as u16;
        (hi << 8) | lo
    }

    fn read_long(&mut self, address: u32) -> u32 {
        #[cfg(feature = "load-test")]
        if let Some(bytes) = self.load_test_plain_read(address) { return u32::from_be_bytes(bytes); }
        let w0 = self.read_word(address) as u32;
        let w1 = self.read_word(address.wrapping_add(2)) as u32;
        (w0 << 16) | w1
    }

    fn write_word(&mut self, address: u32, value: u16) {
        #[cfg(feature = "load-test")]
        {
            let a = address & A24;
            if (RAM_BASE..=RAM_BASE + RAM_SIZE - 2).contains(&a) {
                self.ram[(a-RAM_BASE) as usize..(a-RAM_BASE+2) as usize].copy_from_slice(&value.to_be_bytes());
                return;
            }
        }
        self.write_byte(address, (value >> 8) as u8);
        self.write_byte(address.wrapping_add(1), value as u8);
    }

    fn write_long(&mut self, address: u32, value: u32) {
        #[cfg(feature = "load-test")]
        {
            let a = address & A24;
            if (RAM_BASE..=RAM_BASE + RAM_SIZE - 4).contains(&a) {
                self.ram[(a-RAM_BASE) as usize..(a-RAM_BASE+4) as usize].copy_from_slice(&value.to_be_bytes());
                return;
            }
        }
        self.write_word(address, (value >> 16) as u16);
        self.write_word(address.wrapping_add(2), value as u16);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "load-test")]
    #[test]
    fn load_test_wide_bus_matches_byte_bus_at_overlays_and_boundaries() {
        for mode in [ExecutionMode::ResearchHarness, ExecutionMode::Fidelity] {
            for cfi in [false, true] {
                let mut bus = Tech2Bus::new(vec![0xa5; 0x40000], vec![0x5a; 0x100000], mode);
                bus.eprom_cfi_status = cfi;
                for (i, v) in bus.ram.iter_mut().enumerate() { *v = (i % 251) as u8; }
                bus.ram[0x127e..0x1282].fill(0);
                for base in [0, 0x5ffc, 0x8000, 0x16ed2, 0x16eef, 0x40000,
                    RAM_BASE, 0x10127f, 0x101281, 0x10eeba, 0x10eed6,
                    RAM_BASE + RAM_SIZE, 0x1000000] {
                    for offset in -5i64..=5 {
                        let a = (base as i64 + offset) as u32;
                        let expected = u32::from_be_bytes(std::array::from_fn(|i| bus.read_byte(a.wrapping_add(i as u32))));
                        assert_eq!(bus.read_long(a), expected, "read long {a:x}, cfi={cfi}");
                        assert_eq!(bus.read_word(a), (expected >> 16) as u16, "read word {a:x}");
                    }
                }
                for a in [RAM_BASE, RAM_BASE+1, RAM_BASE+0x127e, RAM_BASE+RAM_SIZE-4] {
                    bus.write_long(a, 0x9876abcd);
                    for (i,b) in [0x98,0x76,0xab,0xcd].iter().enumerate() {
                        assert_eq!(bus.ram[(a-RAM_BASE) as usize+i], *b);
                    }
                    bus.write_word(a, 0x1234);
                    assert_eq!(&bus.ram[(a-RAM_BASE) as usize..(a-RAM_BASE+2) as usize], &[0x12,0x34]);
                }
            }
        }
    }

    #[test]
    fn cleared_ssa_initialization_requires_flash_readback_without_raw_writes() {
        for enabled in [false, true] {
            let mut card = vec![0x55; 0x1000000];
            card[crate::ssa_flash::OFFSET..crate::ssa_flash::OFFSET + crate::ssa_flash::SIZE].fill(0xff);
            let mut bus = Tech2Bus::new(vec![], card, ExecutionMode::ResearchHarness);
            if enabled { bus.ssa_flash = Some(crate::ssa_flash::SsaFlash::default()); }
            bus.bank_raw = false;
            bus.bank = 0x3f;
            let ssa = CARD_BASE + 0xe0000;
            // A cleared card still needs the guest's program/status/read-array sequence.
            bus.write_word(ssa, 0x4040);
            bus.write_word(ssa, 0xb1ff);
            bus.write_word(CARD_BASE, 0x7070);
            if enabled { assert_eq!(bus.read_word(CARD_BASE), 0x8080); }
            bus.write_word(CARD_BASE, 0xffff);
            assert_eq!(bus.read_word(ssa), if enabled { 0xb1ff } else { 0xffff });
            assert_eq!(bus.card[0xfdffff], 0x55);
            assert_eq!(bus.card[0xfe02ca], 0x55);
            assert!(bus.card[0xfe0002..0xfe02ca].iter().all(|b| *b == 0xff));
            assert!(!bus.card_writes);
        }
    }

    #[test]
    fn ssa_flash_readback_exposes_filled_and_next_free_seed_slots() {
        let mut bus = Tech2Bus::new(
            vec![],
            vec![0x55; 0x1000000],
            ExecutionMode::ResearchHarness,
        );
        bus.ssa_flash = Some(crate::ssa_flash::SsaFlash::default());
        bus.bank_raw = false;
        bus.bank = 0x3f; // Common memory, physical card bank F.
        let ssa = CARD_BASE + 0xe0000;
        bus.write_word(CARD_BASE, 0x5050);
        bus.write_word(ssa, 0x2020);
        bus.write_word(ssa, 0xd0d0);
        bus.write_word(CARD_BASE, 0x7070);
        assert_eq!(bus.read_word(CARD_BASE), 0x8080);
        bus.write_word(CARD_BASE, 0xffff);
        assert_eq!(bus.read_word(ssa + 0x132), 0xffff);
        for (i, value) in [0x0000, 0x0361, 0x72bc, 0xffff].into_iter().enumerate() {
            let destination = ssa + 0x132 + i as u32 * 2;
            bus.write_word(destination, 0x4040);
            bus.write_word(destination, value);
            bus.write_word(CARD_BASE, 0x7070);
            assert_eq!(bus.read_word(CARD_BASE), 0x8080);
        }
        bus.write_word(CARD_BASE, 0x5050);
        bus.write_word(CARD_BASE, 0xffff);
        assert_eq!(bus.read_word(ssa + 0x132), 0x0000);
        assert_eq!(bus.read_word(ssa + 0x134), 0x0361);
        assert_eq!(bus.read_word(ssa + 0x136), 0x72bc);
        assert_eq!(bus.read_word(ssa + 0x13a), 0xffff);
        assert_eq!(bus.card[0xfdffff], 0x55);
        assert_eq!(bus.card[0xfe02ca], 0x55);
        assert!(!bus.card_writes); // Raw-write mode is not used.
    }

    #[test]
    fn tpu_countdown_requires_supported_firmware_and_cisr_cannot_set_events() {
        let mut bus = Tech2Bus::new(vec![0; 0x40000], vec![], ExecutionMode::ResearchHarness);
        bus.write_word(SIM7 + 0xe08, 0x0450);
        bus.write_word(SIM7 + 0xe0a, 8);
        bus.write_word(SIM7 + 0xe12, 0xf000);
        bus.write_word(SIM7 + 0xe16, 0x80);
        bus.write_word(SIM7 + 0xe1e, 0x40);
        bus.write_word(SIM7 + 0xf32, 1);
        bus.write_word(SIM7 + 0xe20, 0xffff);
        assert_eq!(bus.read_word(SIM7 + 0xe20), 0);
        assert_eq!(bus.poll_interrupt(100_000), 0);
        assert_eq!(bus.read_word(SIM7 + 0xf32), 1);
        for (offset, value) in [0x48, 0xe7, 0x01, 0x04, 0x3f, 0x3c, 0x00, 0x88]
            .into_iter()
            .enumerate()
        {
            bus.write_byte(0x1d9c7e + offset as u32, value);
        }
        assert_eq!(bus.poll_interrupt(100_000), 0);
        assert_eq!(bus.poll_interrupt(110_000), 4);
        assert_eq!(bus.read_word(SIM7 + 0xf32), 0);
        assert_eq!(bus.interrupt_acknowledge(4), 0x53);
        assert_eq!(bus.poll_interrupt(110_001), 4);
        bus.write_word(SIM7 + 0xe20, 0xfff7);
        assert_eq!(bus.poll_interrupt(110_002), 0);
    }

    #[test]
    fn test_ui_enter_transitions_to_main_menu() {
        let mut bus = Tech2Bus::new(
            vec![0; 0x40000],
            vec![0; 0x10000],
            ExecutionMode::ResearchHarness,
        );
        bus.compatibility_splash = true;
        assert_eq!(bus.ui.screen, UiScreen::Splash);

        // Pressing Enter on splash transitions to Main Menu
        bus.press_key(KEY_ENTER_DEFAULT);
        assert_eq!(bus.ui.screen, UiScreen::MainMenu);
        assert_eq!(bus.ui.menu_selected, 0);

        // Up/Down navigation on Main Menu
        bus.press_key(KEY_DOWN);
        assert_eq!(bus.ui.menu_selected, 1);
        bus.press_key(KEY_DOWN);
        assert_eq!(bus.ui.menu_selected, 2);
        bus.press_key(KEY_UP);
        assert_eq!(bus.ui.menu_selected, 1);

        // Enter on Service Programming (item 1)
        bus.press_key(KEY_ENTER_DEFAULT);
        assert_eq!(bus.ui.screen, UiScreen::ServiceProgramming);

        // Exit back to Main Menu
        bus.exit_key();
        assert_eq!(bus.ui.screen, UiScreen::MainMenu);

        // Direct select item 0 (Diagnostics)
        bus.select_digit(0);
        assert_eq!(bus.ui.screen, UiScreen::Diagnostics);

        // Exit back to Main Menu
        bus.exit_key();
        assert_eq!(bus.ui.screen, UiScreen::MainMenu);

        // Exit back to Splash
        bus.exit_key();
        assert_eq!(bus.ui.screen, UiScreen::Splash);
    }

    #[test]
    fn test_ui_rendering_all_screens() {
        let mut bus = Tech2Bus::new(
            vec![0; 0x40000],
            vec![0; 0x10000],
            ExecutionMode::ResearchHarness,
        );
        bus.compatibility_splash = true;
        let mut buf = vec![0u32; 320 * 240];

        let screens = [
            UiScreen::Splash,
            UiScreen::MainMenu,
            UiScreen::Diagnostics,
            UiScreen::ServiceProgramming,
            UiScreen::ViewStoredData,
            UiScreen::ToolOptions,
            UiScreen::Download,
        ];

        for screen in screens {
            bus.ui.screen = screen;
            bus.render_lcd_pixels(&mut buf);
            // Ensure pixels were drawn (not all zeros)
            let nonzero = buf.iter().any(|&p| p != 0);
            assert!(nonzero, "Screen {:?} produced empty buffer", screen);

            if crate::options::env_flag("DUMP_UI_SCREENS") {
                let name = format!("screen_{:?}.ppm", screen).to_lowercase();
                let mut ppm = b"P6\n320 240\n255\n".to_vec();
                for px in &buf {
                    ppm.push((px >> 16) as u8);
                    ppm.push((px >> 8) as u8);
                    ppm.push(*px as u8);
                }
                let _ = std::fs::write(&name, ppm);
            }
        }
    }

    #[test]
    fn test_qspi_loopback_post() {
        let mut bus = Tech2Bus::new(vec![0; 0x40000], vec![0; 0x10000], ExecutionMode::Fidelity);

        // 1. Enable Loop Mode (LOOPQ = bit 2 of SPCR3 at $7FFC1E)
        bus.write_byte(0x007F_FC1E, 0x04);

        // 2. Set queue bounds in SPCR2 ($7FFC1C): NEWQP=0, ENDQP=0
        bus.write_word(0x007F_FC1C, 0x0000);

        // 3. Write test pattern $5555 to TX RAM 0 ($7FFD20)
        bus.write_word(0x007F_FD20, 0x5555);

        // 4. Start transfer by writing SPCR1 with SPE=1 ($7FFC1A)
        bus.write_word(0x007F_FC1A, 0x8000);

        // 5. Verify transfer completed:
        // SPSR should have SPIF=1 ($7FFC1F)
        let spsr = bus.read_byte(0x007F_FC1F);
        assert_eq!(spsr & 0x80, 0x80, "SPIF should be set upon completion");

        // Receive RAM 0 ($7FFD00) should match Transmit RAM 0 ($5555)
        let rx0 = bus.read_word(0x007F_FD00);
        assert_eq!(
            rx0, 0x5555,
            "Receive RAM 0 must match Transmit RAM in loopback mode"
        );
    }

    #[test]
    fn test_qspi_rtc_clkmem_post() {
        let mut bus = Tech2Bus::new(vec![0; 0x40000], vec![0; 0x10000], ExecutionMode::Fidelity);

        // 1. Set queue bounds in SPCR2: NEWQP=4, ENDQP=11 ($0B04)
        bus.write_word(0x007F_FC1C, 0x0B04);

        // 2. Set Command RAM for slots 4..11 to $4E (PCS0 RTC select)
        for slot in 4..=11 {
            bus.write_byte(0x007F_FD40 + slot, 0x4E);
        }

        // 3. Write TX RAM with DS1302 command stream
        let tx_words = [
            0xB1FF, 0xD1FF, 0x91FF, 0xE1FF, 0xA1FF, 0xC1FF, 0x81FF, 0xF7FF,
        ];
        for (i, &w) in tx_words.iter().enumerate() {
            bus.write_word(0x007F_FD20 + (4 + i as u32) * 2, w);
        }

        // 4. Trigger transfer via SPCR1 write with SPE=1
        bus.write_word(0x007F_FC1A, 0x9201);

        // 5. Verify SPSR has SPIF set and CPTQP = 11 (0x0B)
        let spsr = bus.read_byte(0x007F_FC1F);
        assert_eq!(spsr, 0x8B, "SPSR must have SPIF (0x80) and CPTQP=11 (0x0B)");

        // 6. Verify Receive RAM entries 4..11:
        // When ROM $001AA6 bit-reverses these values, they must yield:
        // Entry 4: 0x00 (Seconds <= 59)
        // Entry 5: 0x09 (Month: 1 <= 9 <= 12)
        // Entry 6: 0x05 (Day: 1 <= 5 <= 7)
        // Entry 7: 0x04 (Date: 1 <= 4 <= 31)
        // Entry 8: 0x0C (Hours: 0 <= 12 <= 23)
        // Entry 9: 0x1E (Minutes: 0 <= 30 <= 59)
        // Entry 10: 0x00 (Seconds 2 <= 59)
        // Entry 11: 0xA5 (CLKMEM signature)
        let sig_rx = bus.read_word(0x007F_FD16);
        assert_eq!(
            sig_rx & 0xFF,
            0xA5,
            "CLKMEM signature at slot 11 must be 0xA5"
        );

        let raw_rx = [
            bus.read_word(0x007F_FD08) as u8,
            bus.read_word(0x007F_FD0A) as u8,
            bus.read_word(0x007F_FD0C) as u8,
            bus.read_word(0x007F_FD0E) as u8,
            bus.read_word(0x007F_FD10) as u8,
            bus.read_word(0x007F_FD12) as u8,
            bus.read_word(0x007F_FD14) as u8,
            bus.read_word(0x007F_FD16) as u8,
        ];
        let unpacked: Vec<u8> = raw_rx.iter().map(|b| b.reverse_bits()).collect();
        assert_eq!(
            unpacked,
            vec![0x00, 0x03, 0x09, 0x04, 0x12, 0x30, 0x00, 0xA5]
        );
    }
}

#[cfg(test)]
mod robustness_tests {
    use super::*;
    #[test]
    fn trace_peek_never_consumes_device_state_or_crosses_memory_bounds() {
        let mut bus = Tech2Bus::new(vec![0; 0x40000], vec![], ExecutionMode::Fidelity);
        bus.press_key(KEY_ENTER_DEFAULT);
        bus.qspi_transfer();
        assert!(bus.peek_bytes(KEYPAD_REG, 1).is_none());
        assert!(bus.peek_bytes(SIM7 + OFF_SPSR as u32, 1).is_none());
        assert!(bus.peek_bytes(RAM_BASE + RAM_SIZE - 1, 2).is_none());
        assert!(bus.peek_bytes(RAM_BASE, usize::MAX).is_none());
        assert!(bus.peek_bytes(RAM_BASE, 32).is_some());
        assert_eq!(bus.key_consumed, 0);
        assert!(bus.key_irq_pending());
        assert!(bus.qspi_pending);
    }

    #[test]
    fn inactive_vram_text_cannot_verify_a_splash() {
        let mut bus = Tech2Bus::new(vec![0; 0x40000], vec![0; 0x100000], ExecutionMode::Fidelity);
        let splash = b"Press [ENTER] Software Version North American Operations";
        bus.lcd.vram[0x3000..0x3000 + splash.len()].copy_from_slice(splash);
        assert!(!bus.guest_splash_reached());
        bus.lcd.sad1 = 0x3000;
        // Split phrases across rows exactly as displayed rather than arbitrary bytes.
        bus.lcd.vram[0x3000..0x3100].fill(0);
        for (row, text) in [
            (0, "Press [ENTER]"),
            (1, "Software Version"),
            (2, "North American Operations"),
        ] {
            let start = 0x3000 + row * 40;
            bus.lcd.vram[start..start + text.len()].copy_from_slice(text.as_bytes());
        }
        assert!(bus.guest_splash_reached());
    }
    #[test]
    fn invalid_ram_addresses_are_safe() {
        let mut bus = Tech2Bus::new(vec![0; 0x40000], vec![], ExecutionMode::Fidelity);
        for addr in [0, 0xffff_ffff, RAM_BASE - 1] {
            assert_eq!(bus.ram_word(addr), 0);
            assert_eq!(bus.ram_long(addr), 0);
            bus.ram_write_long(addr, 0xffff_ffff);
            bus.ram_write_word(addr, 0xffff);
            bus.ram_write_byte(addr, 0xff);
        }
        bus.ram_write_word(RAM_BASE + RAM_SIZE - 1, 0xffff);
        assert!(bus.ram.iter().all(|b| *b == 0));
    }
}

fn hex_bytes(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(out, "{b:02x}");
    }
    out
}

fn key_name(code: u8) -> &'static str {
    match code {
        KEY_UP => "UP",
        KEY_DOWN => "DOWN",
        KEY_ENTER_DEFAULT => "ENTER",
        KEY_EXIT => "EXIT",
        KEY_SELECT => "SELECT",
        KEY_MORE => "MORE",
        KEY_PAGE_UP => "PAGE_UP",
        0x18 => "0",
        0x04 => "1",
        0x13 => "2",
        0x17 => "3",
        0x03 => "4",
        0x12 => "5",
        0x16 => "6",
        0x02 => "7",
        0x11 => "8",
        0x15 => "9",
        _ => "RAW",
    }
}
