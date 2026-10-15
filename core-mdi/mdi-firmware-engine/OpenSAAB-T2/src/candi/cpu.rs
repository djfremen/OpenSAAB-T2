// SPDX-License-Identifier: MPL-2.0
//! Bounded execution of the locally supplied MSI CANDi image.
//!
//! The MSI file contains addressed download records, not a flat executable ROM.
//! Start at its application header entry; hardware reset remains unmodeled.

use m68k::{AddressBus, CpuCore, CpuType, StepResult};
use std::collections::VecDeque;
use std::io::Read;
use std::path::Path;

#[path = "peripherals.rs"]
mod peripherals;
use peripherals::Peripherals;

pub const ROM_BASE: u32 = 0x10000;
pub const ROM_SIZE: usize = 0x10000;
pub const RAM_BASE: u32 = 0x100000;
pub const RAM_SIZE: usize = 0x40000;
pub const APPLICATION_ENTRY: u32 = 0x10040;
pub const DEFAULT_BUDGET: u64 = 100_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Access {
    pub address: u32,
    pub width: u8,
    pub write: bool,
    pub value: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StopReason {
    UnsupportedAccess(Access),
    Cpu(String),
    Budget,
    Cancelled,
}

impl std::fmt::Display for StopReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedAccess(a) => write!(
                f,
                "unsupported {} address={:#010x} width={} value={:#010x}",
                if a.write { "write" } else { "read" },
                a.address,
                a.width,
                a.value
            ),
            Self::Cpu(reason) => write!(f, "CPU stop: {reason}"),
            Self::Budget => f.write_str("instruction budget reached"),
            Self::Cancelled => f.write_str("host cancelled"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Instruction {
    pub pc: u32,
    pub opcode: u16,
}

#[derive(Clone, Debug)]
pub struct Snapshot {
    /// Attempted instructions include the instruction that hit a missing device.
    pub attempted: u64,
    pub completed: u64,
    pub cycles: u64,
    pub pc: u32,
    pub sp: u32,
    pub vbr: u32,
    pub reason: Option<StopReason>,
    pub recent: Vec<Instruction>,
    pub peripheral_writes: Vec<Access>,
    pub peripheral_write_count: u64,
    pub ignored_rom_write_count: u64,
    pub serial_rx_count: u64,
    pub serial_break_count: u64,
    pub serial_tx: Vec<u8>,
    pub serial_tx_count: u64,
}

pub struct Firmware {
    rom: Vec<u8>,
}

impl Firmware {
    pub fn load(path: &Path) -> Result<Self, String> {
        let metadata = std::fs::metadata(path)
            .map_err(|e| format!("CANDi firmware {}: {e}", path.display()))?;
        if !metadata.is_file() {
            return Err("CANDi firmware must be a regular file".into());
        }
        let file = std::fs::File::open(path)
            .map_err(|e| format!("CANDi firmware {}: {e}", path.display()))?;
        let mut bytes = Vec::new();
        file.take((ROM_SIZE + 9) as u64)
            .read_to_end(&mut bytes)
            .map_err(|e| format!("CANDi firmware {}: {e}", path.display()))?;
        Self::parse(&bytes).map_err(|e| format!("{}: {e}", path.display()))
    }

    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() != ROM_SIZE + 8 {
            return Err("CANDi research profile requires a 65544-byte MSI ROM".into());
        }
        let stored = &bytes[8..];
        // Refuse card modules and unknown layouts instead of guessing an entry.
        if bytes[..8] != [0; 8]
            || stored[4..8] != APPLICATION_ENTRY.to_be_bytes()
            || &stored[0x48..0x59] != b"CANdi Application"
        {
            return Err("CANDi firmware layout is not the supported MSI download profile".into());
        }
        let rom = decode_records(bytes)?;
        // Independent absolute branches now land on resident startup, rather
        // than in instruction operands shifted by intervening record headers.
        if rom[0x72..0x78] != [0x4e, 0xf9, 0, 1, 0x8c, 0x94]
            || rom[0x8c94..0x8c98] != [0x60, 0, 0, 0x32]
            || &rom[0x8c9c..0x8caa] != b"CANdi RES V0.1"
            || rom[0x8cc8..0x8cd2] != [0x46, 0xfc, 0x27, 0, 0x2e, 0x7c, 0, 0x13, 0xff, 0xfc]
        {
            return Err("CANDi decoded startup signatures do not match the MSI profile".into());
        }
        Ok(Self { rom })
    }
}

/// The known MSI profile has an implicit first 512-byte block at ROM_BASE,
/// followed by little-endian address/length pairs and variable-size payloads.
/// A zero tail terminates the stream. Reject overlaps, overflow and bad tails.
fn decode_records(bytes: &[u8]) -> Result<Vec<u8>, String> {
    if bytes.len() < 520 || bytes[..8] != [0; 8] {
        return Err("CANDi download lacks its implicit first block".into());
    }
    let mut rom = vec![0; ROM_SIZE];
    rom[..512].copy_from_slice(&bytes[8..520]);
    let mut position = 520;
    let mut previous_end = 512;
    while position < bytes.len() {
        if bytes[position..].iter().all(|&b| b == 0) {
            return Ok(rom);
        }
        let header = bytes
            .get(position..position + 8)
            .ok_or("CANDi truncated record header")?;
        let address = u32::from_le_bytes(header[..4].try_into().unwrap());
        let length = u32::from_le_bytes(header[4..].try_into().unwrap()) as usize;
        let offset = address
            .checked_sub(ROM_BASE)
            .ok_or("CANDi record below ROM")? as usize;
        if length == 0
            || length > 512
            || offset < previous_end
            || offset.checked_add(length).is_none_or(|end| end > ROM_SIZE)
        {
            return Err(format!(
                "CANDi invalid record at {position:#x}: address={address:#x} length={length}"
            ));
        }
        position += 8;
        let payload = bytes
            .get(position..position + length)
            .ok_or("CANDi truncated record payload")?;
        rom[offset..offset + length].copy_from_slice(payload);
        previous_end = offset + length;
        position += length;
    }
    Err("CANDi download lacks a zero terminator".into())
}

#[derive(Clone)]
struct Bus {
    rom: Vec<u8>,
    inventory: Option<Vec<u8>>,
    ram: Vec<u8>,
    fault: Option<Access>,
    writes: VecDeque<Access>,
    write_count: u64,
    peripherals: Peripherals,
    ignored_rom_write_count: u64,
    can: Option<[sja1000::Controller; 3]>,
    can_events: Vec<String>,
}

#[path = "sja1000.rs"]
mod sja1000;
pub use sja1000::{
    ElectricalState as CanElectricalState, Frame as CanFrame, ReceiveState as CanReceiveState,
    Transmission as CanTransmission,
};

fn can_register(address: u32, width: u8) -> Option<(usize, u8)> {
    let offset = address.checked_sub(0x200000)?;
    (width == 1 && offset / 0x1000 < 3 && offset % 0x1000 < 128)
        .then_some(((offset / 0x1000) as usize, (offset % 0x1000) as u8))
}

impl Bus {
    fn reserve_can_trace(&mut self, count: usize, address: u32, width: u8, value: u32) -> bool {
        if self.can_events.len() + count > 1023 {
            self.can_events
                .push("CAN register trace overflow; CPU stopped".into());
            self.fault(Access {
                address,
                width,
                write: true,
                value,
            });
            false
        } else {
            true
        }
    }
    fn can_interrupt(&self) -> Option<(u8, u32)> {
        // Native 0x1366c: channels 0/2/4 use vectors 26/27/28 and PFPAR IRQ pins.
        self.can
            .as_ref()?
            .iter()
            .enumerate()
            .filter_map(|(chip, c)| {
                let route = crate::candi_transport::ROUTES[chip];
                (c.interrupt() && self.peripherals.external_irq_enabled(route.interrupt_level))
                    .then_some((route.interrupt_level, route.vector))
            })
            .max_by_key(|(level, _)| *level)
    }
    fn interrupt(&self) -> Option<(u8, u32)> {
        self.peripherals
            .interrupt()
            .into_iter()
            .chain(self.can_interrupt())
            .max_by_key(|(level, _)| *level)
    }

    fn range(address: u32, width: u8, base: u32, len: usize) -> Option<usize> {
        let offset = address.checked_sub(base)? as usize;
        (offset.checked_add(width as usize)? <= len).then_some(offset)
    }

    fn fault(&mut self, access: Access) {
        if self.fault.is_none() {
            self.fault = Some(access);
        }
    }

    fn read(&mut self, address: u32, width: u8) -> u32 {
        if let Some((chip, register)) = can_register(address, width) {
            if let Some(value) = self
                .can
                .as_mut()
                .and_then(|chips| chips[chip].read(register))
            {
                return u32::from(value);
            }
        }
        if Self::range(address, width, 0xfff900, 0x700).is_some() {
            let mut value = 0;
            let mut supported = true;
            for byte in 0..u32::from(width) {
                if let Some(v) = self.peripherals.read8(address + byte) {
                    value = (value << 8) | u32::from(v);
                } else {
                    supported = false;
                    break;
                }
            }
            if supported {
                return value;
            }
        }
        if let Some(inventory) = &self.inventory {
            if let Some(i) = Self::range(address, width, 0x4000, inventory.len()) {
                return inventory[i..i + width as usize]
                    .iter()
                    .fold(0, |v, b| (v << 8) | u32::from(*b));
            }
        }
        let bytes = if let Some(i) = Self::range(address, width, ROM_BASE, self.rom.len()) {
            &self.rom[i..i + width as usize]
        } else if let Some(i) = Self::range(address, width, RAM_BASE, self.ram.len()) {
            &self.ram[i..i + width as usize]
        } else {
            self.fault(Access {
                address,
                width,
                write: false,
                value: 0,
            });
            // The core's infallible bus interface needs a value to finish the
            // current step. This value is never treated as a device reply;
            // Machine halts immediately and does not count that step completed.
            return 0;
        };
        bytes.iter().fold(0, |v, byte| (v << 8) | u32::from(*byte))
    }

    fn write(&mut self, address: u32, width: u8, value: u32) {
        if self.fault.is_some() {
            return;
        }
        if let Some((chip, register)) = can_register(address, width) {
            if self.can.is_some() && !self.reserve_can_trace(3, address, width, value) {
                return;
            }
            let electrical = if chip == 2 {
                self.peripherals.single_wire_gpio()
            } else {
                CanElectricalState::StandardCan
            };
            if let Some(chips) = self.can.as_mut() {
                chips[chip].set_electrical(electrical);
                if register == 1 && value & 1 != 0 {
                    let route = crate::candi_transport::ROUTES[chip];
                    self.can_events.push(format!(
                        "CAN TX intent controller={chip} native_channel={} bus={:?} electrical={electrical} {}",
                        route.native_channel, route.bus, chips[chip].tx_description()));
                }
                let result = chips[chip].write(register, value as u8);
                self.can_events.push(format!("CAN register write address={address:#010x} value={value:#04x} model=SJA1000-compatible external_tx=false"));
                if let Err(error) = result {
                    self.can_events
                        .push(format!("CAN controller {chip}: {error}"));
                    self.fault(Access {
                        address,
                        width,
                        write: true,
                        value,
                    });
                }
                return;
            }
        }
        if address <= 0xfffc0f && address + u32::from(width) > 0xfffc0e {
            if !matches!((address, width), (0xfffc0f, 1) | (0xfffc0e, 2))
                || !self.peripherals.transmit(value as u8)
            {
                self.fault(Access {
                    address,
                    width,
                    write: true,
                    value,
                });
            }
            return;
        }
        if let Some(i) = Self::range(address, width, RAM_BASE, self.ram.len()) {
            let bytes = value.to_be_bytes();
            self.ram[i..i + width as usize].copy_from_slice(&bytes[4 - width as usize..]);
        } else if Self::range(address, width, 0xfff900, 0x700).is_some() {
            let trace_gpio =
                self.can.is_some() && address < 0xfffc18 && address + u32::from(width) > 0xfffc15;
            if trace_gpio && !self.reserve_can_trace(1, address, width, value) {
                return;
            }
            let before = self.peripherals.single_wire_gpio();
            // Observed CPU32 peripheral writes only. No readback, interrupts,
            // register semantics, CAN windows or external transmissions implied.
            self.write_count += 1;
            if self.writes.len() == 64 {
                self.writes.pop_front();
            }
            self.writes.push_back(Access {
                address,
                width,
                write: true,
                value,
            });
            let bytes = value.to_be_bytes();
            for (i, &byte) in bytes[4 - width as usize..].iter().enumerate() {
                self.peripherals.write8(address + i as u32, byte);
            }
            if trace_gpio {
                self.can_events.push(format!(
                    "CAN transceiver GPIO write address={address:#010x} width={width} value={value:#x} before=[{before}] after=[{}] source=native-registers external_tx=false",
                    self.peripherals.single_wire_gpio()));
            }
        } else if self.peripherals.boot_rom_selected(address, width) {
            // ROM does not change on ordinary CPU stores. The application
            // passes null output pointers to its serial-buffer flush routine;
            // these bus writes are acknowledged in the configured CSBOOT
            // aperture. No missing boot-ROM read data is manufactured here.
            // Flash erase/program commands are outside this read-only model.
            self.ignored_rom_write_count += 1;
        } else {
            self.fault(Access {
                address,
                width,
                write: true,
                value,
            });
        }
    }
}

impl AddressBus for Bus {
    fn interrupt_acknowledge(&mut self, level: u8) -> u32 {
        if self
            .can_interrupt()
            .is_some_and(|(priority, _)| priority == level)
        {
            return 24 + u32::from(level);
        }
        self.peripherals.acknowledge(level)
    }

    fn read_byte(&mut self, a: u32) -> u8 {
        self.read(a, 1) as u8
    }
    fn read_word(&mut self, a: u32) -> u16 {
        self.read(a, 2) as u16
    }
    fn read_long(&mut self, a: u32) -> u32 {
        self.read(a, 4)
    }
    fn write_byte(&mut self, a: u32, v: u8) {
        self.write(a, 1, v.into());
    }
    fn write_word(&mut self, a: u32, v: u16) {
        self.write(a, 2, v.into());
    }
    fn write_long(&mut self, a: u32, v: u32) {
        self.write(a, 4, v);
    }
}

pub struct Machine {
    cpu: CpuCore,
    bus: Bus,
    attempted: u64,
    completed: u64,
    cycles: u64,
    recent: VecDeque<Instruction>,
    reason: Option<StopReason>,
}

impl Machine {
    pub fn checkpoint(&self) -> Result<Self, serde_json::Error> {
        if self
            .bus
            .can
            .as_ref()
            .is_some_and(|chips| chips.iter().any(|c| c.transport_enabled()))
        {
            return Err(serde_json::Error::io(std::io::Error::other(
                "Cannot checkpoint an enabled CAN transport; external adapter state cannot be rewound",
            )));
        }
        Ok(Self {
            cpu: serde_json::from_slice(&serde_json::to_vec(&self.cpu)?)?,
            bus: self.bus.clone(),
            attempted: self.attempted,
            completed: self.completed,
            cycles: self.cycles,
            recent: self.recent.clone(),
            reason: self.reason.clone(),
        })
    }

    /// Virtual module inventory, derived from the loaded application header.
    /// Layout: native helper 0x13258; record metadata: OEM host 0x4548d0.
    /// This is NOT a dump of physical flash. No absent boot/resident slot is advertised.
    pub fn install_application_inventory(&mut self) -> Result<(), String> {
        let rom = &self.bus.rom;
        if rom.get(0x48..0x59) != Some(b"CANdi Application") || rom[8] != 0x55 {
            return Err("application inventory requires the known MSI header".into());
        }
        let digits = [
            0x18, 0x19, 0x0c, 0x0d, 0x0f, 0x10, 0x12, 0x13, 0x30, 0x31, 0x33, 0x34, 0x36, 0x37,
        ];
        if digits.iter().any(|&i| !rom[i].is_ascii_digit()) {
            return Err("application inventory header has invalid version/date/time".into());
        }
        let mut inventory = vec![0; 3 + 5 * 40];
        inventory[0] = 0xa5;
        inventory[2] = 40;
        let record = &mut inventory[43..83]; // Application slot 1; slot 0 boot is absent.
        record[0] = 1;
        record[1..18].copy_from_slice(&rom[0x48..0x59]);
        // Preserve the OEM application-record attributes as opaque metadata.
        record[21..25].copy_from_slice(&[0x40, 1, 0, 0]);
        record[25] = rom[8];
        for (i, &offset) in digits.iter().enumerate() {
            record[26 + i] = rom[offset];
        }
        self.bus.inventory = Some(inventory);
        Ok(())
    }

    /// Local virtual-link experiment only; never an injected ECU response.
    /// Frames end with an SCI break, as handled by the native link ISR.
    pub fn receive_serial_frame(&mut self, bytes: &[u8]) -> Result<(), String> {
        if self.reason.is_some() {
            return Err("CANdi CPU has stopped".into());
        }
        self.bus.peripherals.serial.queue(bytes)
    }

    pub fn take_serial_events(&mut self) -> Vec<Option<u8>> {
        self.bus.peripherals.serial.take_events()
    }

    pub fn enable_can_register_probe(&mut self) {
        self.bus.can = Some(std::array::from_fn(|_| sja1000::Controller::default()));
    }

    /// Controller 0 = native channel 4 (HS); 1 = channel 2; 2 = channel 0 (SW).
    pub fn enable_can_transport(&mut self, controller: usize) -> Result<(), String> {
        self.bus
            .can
            .as_mut()
            .and_then(|chips| chips.get_mut(controller))
            .ok_or("CAN controller is not mapped")?
            .enable_transport();
        Ok(())
    }
    pub fn take_can_transmissions(&mut self) -> Vec<(usize, CanTransmission)> {
        self.bus
            .can
            .as_mut()
            .map(|chips| {
                chips
                    .iter_mut()
                    .enumerate()
                    .filter_map(|(i, c)| c.take_transmission().map(|tx| (i, tx)))
                    .collect()
            })
            .unwrap_or_default()
    }
    /// Acceptance into a driver queue is insufficient: caller must have TX confirmation.
    pub fn complete_can_transmission(
        &mut self,
        controller: usize,
        ticket: u64,
    ) -> Result<bool, String> {
        if self.bus.can_events.len() >= 1023 {
            return Err("CAN trace full; drain events before applying completion".into());
        }
        let applied = self
            .bus
            .can
            .as_mut()
            .and_then(|chips| chips.get_mut(controller))
            .ok_or("CAN controller is not mapped")?
            .complete(ticket)?;
        self.bus.can_events.push(format!(
            "CAN TX confirmation controller={controller} ticket={ticket} source=transport; emulated_completion_applied={applied} cancelled_guest_ticket={}", !applied));
        Ok(applied)
    }
    pub fn receive_can_frame(
        &mut self,
        controller: usize,
        frame: &CanFrame,
    ) -> Result<bool, String> {
        self.bus
            .can
            .as_mut()
            .and_then(|chips| chips.get_mut(controller))
            .ok_or("CAN controller is not mapped")?
            .receive(frame)
    }

    /// An attached fixed-rate physical bus cannot deliver a frame while the
    /// emulated controller is configured for another rate (including startup).
    pub fn receive_can_frame_at_timing(
        &mut self,
        controller: usize,
        frame: &CanFrame,
        btr0: u8,
        btr1: u8,
    ) -> Result<bool, String> {
        let chip = self
            .bus
            .can
            .as_mut()
            .and_then(|chips| chips.get_mut(controller))
            .ok_or("CAN controller is not mapped")?;
        if chip.bit_timing() != (btr0, btr1) {
            return Ok(false);
        }
        let overrun_before = chip.read(2).unwrap_or(0) & 2 != 0;
        let received = chip.receive(frame)?;
        if !overrun_before && chip.read(2).unwrap_or(0) & 2 != 0 {
            if self.bus.can_events.len() >= 1023 {
                return Err("CAN trace full at RX overrun".into());
            }
            self.bus.can_events.push(format!("CAN RX FIFO overrun controller={controller}; new frame dropped; native status/interrupt signalled"));
        }
        Ok(received)
    }
    pub fn can_receive_would_overrun(
        &self,
        controller: usize,
        frame: &CanFrame,
        btr0: u8,
        btr1: u8,
    ) -> Result<bool, String> {
        let chip = self
            .bus
            .can
            .as_ref()
            .and_then(|chips| chips.get(controller))
            .ok_or("CAN controller is not mapped")?;
        Ok(chip.bit_timing() == (btr0, btr1) && chip.receive_would_overrun(frame)?)
    }
    /// USB batches may briefly wait for the active receive ISR to drain its
    /// FIFO. Once the guest disables that interrupt or its CPU pin route,
    /// waiting cannot help. Deliver to the normal hardware model instead:
    /// receive still runs with IRQs disabled, including DOS on FIFO overflow.
    /// This does not infer completion from a screen, stop CAN reception, or
    /// discard the FIFO contents that a polling guest may still read.
    pub fn should_defer_can_receive(
        &self,
        controller: usize,
        frame: &CanFrame,
        btr0: u8,
        btr1: u8,
    ) -> Result<bool, String> {
        let overrun = self.can_receive_would_overrun(controller, frame, btr0, btr1)?;
        Ok(overrun
            && self.can_interrupt_routed(controller)?
            && self.can_receive_state(controller)?.interrupt_enable & 1 != 0)
    }
    pub fn take_can_events(&mut self) -> Vec<String> {
        std::mem::take(&mut self.bus.can_events)
    }

    pub fn can_receive_state(&self, controller: usize) -> Result<CanReceiveState, String> {
        self.bus
            .can
            .as_ref()
            .and_then(|chips| chips.get(controller))
            .map(|chip| chip.receive_state())
            .ok_or_else(|| "CAN controller is not mapped".into())
    }

    pub fn can_interrupt_routed(&self, controller: usize) -> Result<bool, String> {
        let route = crate::candi_transport::ROUTES
            .get(controller)
            .ok_or("CAN controller is not mapped")?;
        Ok(self
            .bus
            .peripherals
            .external_irq_enabled(route.interrupt_level))
    }

    pub fn attempted(&self) -> u64 {
        self.attempted
    }

    pub fn new(firmware: Firmware) -> Self {
        let mut cpu = CpuCore::new();
        cpu.set_cpu_type(CpuType::M68EC020);
        cpu.sr_mask = 0xa71f; // CPU32 has no master-stack mode.
        cpu.reset_soft();
        // No invented reset vectors or HLE: the initializer sets SP and VBR.
        cpu.pc = APPLICATION_ENTRY;
        Self {
            cpu,
            bus: Bus {
                rom: firmware.rom,
                inventory: None,
                ram: vec![0; RAM_SIZE],
                fault: None,
                writes: VecDeque::new(),
                write_count: 0,
                peripherals: Peripherals::default(),
                ignored_rom_write_count: 0,
                can: None,
                can_events: Vec::new(),
            },
            attempted: 0,
            completed: 0,
            cycles: 0,
            recent: VecDeque::new(),
            reason: None,
        }
    }

    /// The same approximate system cycles used by native SCI and PIT timers.
    pub fn elapsed_cycles(&self) -> u64 {
        self.cycles
    }

    /// Execute at most one instruction. False means a terminal research stop.
    pub fn step(&mut self, budget: u64) -> bool {
        if self.reason.is_some() {
            return false;
        }
        if self.attempted >= budget {
            self.reason = Some(StopReason::Budget);
            return false;
        }
        self.bus.peripherals.advance(self.cycles);
        let level = self.bus.interrupt().map_or(0, |(level, _)| level);
        self.cpu.set_irq(level);
        let pc = self.cpu.pc;
        let opcode = self.bus.read_word(pc);
        if let Some(access) = self.bus.fault.clone() {
            self.reason = Some(StopReason::UnsupportedAccess(access));
            return false;
        }
        if self.recent.len() == 16 {
            self.recent.pop_front();
        }
        self.recent.push_back(Instruction { pc, opcode });
        self.attempted += 1;
        let result = self.cpu.step(&mut self.bus);
        // TRAP is a normal firmware OS call. Let the guest's own vector table
        // and handler run; no host syscall emulation or invented completion.
        let result = if self.bus.fault.is_none() {
            if let StepResult::TrapInstruction { trap_num } = result {
                let cycles = self.cpu.take_trap_exception(&mut self.bus, trap_num);
                StepResult::Ok { cycles }
            } else {
                result
            }
        } else {
            result
        };
        if let Some(access) = self.bus.fault.clone() {
            self.reason = Some(StopReason::UnsupportedAccess(access));
        } else if let StepResult::Ok { cycles } = result {
            self.completed += 1;
            self.cycles += cycles.max(0) as u64;
        } else {
            self.reason = Some(StopReason::Cpu(format!("{result:?}")));
        }
        self.reason.is_none()
    }

    pub fn cancel(&mut self) {
        if self.reason.is_none() {
            self.reason = Some(StopReason::Cancelled);
        }
    }

    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            attempted: self.attempted,
            completed: self.completed,
            cycles: self.cycles,
            pc: self.cpu.pc,
            sp: self.cpu.dar[15],
            vbr: self.cpu.vbr,
            reason: self.reason.clone(),
            recent: self.recent.iter().cloned().collect(),
            peripheral_writes: self.bus.writes.iter().cloned().collect(),
            peripheral_write_count: self.bus.write_count,
            ignored_rom_write_count: self.bus.ignored_rom_write_count,
            serial_rx_count: self.bus.peripherals.serial.received,
            serial_break_count: self.bus.peripherals.serial.breaks,
            serial_tx: self.bus.peripherals.serial.output().to_vec(),
            serial_tx_count: self.bus.peripherals.serial.transmitted,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(feature = "gui")]
    fn live_transport_cannot_be_rewound_with_a_gui_checkpoint() {
        let mut m = machine(&[0x4e, 0x71]);
        m.enable_can_register_probe();
        assert!(m.checkpoint().is_ok());
        m.enable_can_transport(0).unwrap();
        assert!(m
            .checkpoint()
            .err()
            .unwrap()
            .to_string()
            .contains("external adapter"));
    }

    fn machine(code: &[u8]) -> Machine {
        let mut rom = vec![0; ROM_SIZE];
        let i = (APPLICATION_ENTRY - ROM_BASE) as usize;
        rom[i..i + code.len()].copy_from_slice(code);
        Machine::new(Firmware { rom })
    }

    #[test]
    fn electrical_context_is_captured_at_request_not_dispatch() {
        for (latch, assignment, direction, wake) in [
            (2, 0, 3, true),
            (3, 0, 3, false),
            (2, 1, 3, false),
            (2, 0, 1, false),
        ] {
            let mut m = machine(&[0x60, 0xfe]);
            m.enable_can_register_probe();
            m.enable_can_transport(2).unwrap();
            m.bus.write_byte(0x20201f, 0xc7);
            m.bus.write_byte(0x202006, 0xdd);
            m.bus.write_byte(0x202007, 0x36);
            m.bus.write_byte(0x202000, 8);
            // Every case uses the same ID 0x100, empty data frame.
            m.bus.write_byte(0x202010, 0);
            m.bus.write_byte(0x202011, 0x20);
            m.bus.write_byte(0x202012, 0);
            m.bus.write_byte(0xfffc15, latch);
            m.bus.write_byte(0xfffc16, assignment);
            m.bus.write_byte(0xfffc17, direction);
            m.bus.write_byte(0x202001, 1);
            m.bus.write_byte(0xfffc15, 0xff); // Restore/change before backend drains.
            let (controller, tx) = m.take_can_transmissions().pop().unwrap();
            assert_eq!(controller, 2);
            assert_eq!(tx.frame.id, 0x100);
            assert_eq!(
                tx.electrical,
                CanElectricalState::SingleWireGpio {
                    latch,
                    assignment,
                    direction
                }
            );
            assert_eq!(tx.electrical.wake_control_selected(), wake);
            let events = m.take_can_events();
            assert!(events
                .iter()
                .any(|e| e.contains("CAN TX intent") && e.contains("board_mapping=unverified")));
            assert!(!events.iter().any(|e| e.contains("CAN TX confirmation")));
            assert!(m.complete_can_transmission(2, tx.ticket + 1).is_err());
            assert!(m.take_can_events().is_empty());
            m.complete_can_transmission(2, tx.ticket).unwrap();
            assert_eq!(
                m.take_can_events()
                    .iter()
                    .filter(|e| e.contains("CAN TX confirmation"))
                    .count(),
                1
            );
            assert!(m.complete_can_transmission(2, tx.ticket).is_err());
            assert!(m.take_can_events().is_empty());
            assert!(m.bus.fault.is_none());
        }
    }

    #[test]
    fn physical_frames_require_matching_native_bit_timing() {
        let mut m = machine(&[0x60, 0xfe]);
        m.enable_can_register_probe();
        m.enable_can_transport(2).unwrap();
        let f = CanFrame {
            id: 0x541,
            extended: false,
            rtr: false,
            dlc: 1,
            data: vec![1],
        };
        assert!(!m.receive_can_frame_at_timing(2, &f, 0xdd, 0x36).unwrap());
        m.bus.write_byte(0x20201f, 0xc7);
        m.bus.write_byte(0x202006, 0xdd);
        m.bus.write_byte(0x202007, 0x36);
        for i in 20..24 {
            m.bus.write_byte(0x202000 + i, 0xff);
        }
        m.bus.write_byte(0x202000, 8);
        assert!(!m.receive_can_frame_at_timing(2, &f, 0xc1, 0x36).unwrap());
        assert_eq!(m.bus.read_byte(0x20201d), 0);
        assert!(m.receive_can_frame_at_timing(2, &f, 0xdd, 0x36).unwrap());
        assert_eq!(m.bus.read_byte(0x20201d), 1);
    }

    #[test]
    fn gpio_trace_overflow_stops_before_unlogged_mutation() {
        let mut m = machine(&[0x60, 0xfe]);
        m.enable_can_register_probe();
        // Word writes cover latch and assignment in one recorded operation.
        for _ in 0..1023 {
            m.bus.write_word(0xfffc15, 0x0200);
        }
        m.bus.write_byte(0xfffc15, 3);
        assert!(m.bus.fault.is_some());
        assert!(matches!(
            m.bus.peripherals.single_wire_gpio(),
            CanElectricalState::SingleWireGpio { latch: 2, .. }
        ));
        let events = m.take_can_events();
        assert_eq!(events.len(), 1024);
        assert!(events.last().unwrap().contains("overflow"));
    }

    #[test]
    fn raw_driver_confirmation_and_receive_reach_controller_interrupts() {
        use crate::candi_transport::{Event, NanoHsSession};
        let mut m = machine(&[0x60, 0xfe]);
        m.enable_can_register_probe();
        m.enable_can_transport(0).unwrap();
        m.bus.write_byte(0x20001f, 0xc7);
        m.bus.write_byte(0x20101f, 0xc7);
        m.bus.write_byte(0x20201f, 0xc7);
        m.bus.write_byte(0x200006, 0xc1);
        m.bus.write_byte(0x200007, 0x36);
        for reg in 20..24 {
            m.bus.write_byte(0x200000 + reg, 0xff);
        }
        m.bus.write_byte(0x200004, 3);
        m.bus.write_byte(0x200000, 8);
        m.bus.write_byte(0xfffa1f, 0x10);
        for (i, b) in [8, 0xfc, 0, 2, 0x1a, 0x90, 0, 0, 0, 0, 0]
            .into_iter()
            .enumerate()
        {
            m.bus.write_byte(0x200010 + i as u32, b);
        }
        m.bus.write_byte(0x200001, 1);
        let (controller, tx) = m.take_can_transmissions().pop().unwrap();
        assert_eq!(tx.electrical, CanElectricalState::StandardCan);
        let now = std::time::Instant::now();
        let mut backend = NanoHsSession::new();
        let data = backend.prepare(controller, tx, now).unwrap();
        backend.driver_accepted(1, now).unwrap();
        assert_eq!(m.bus.can_interrupt(), None);
        assert_eq!(m.bus.read_byte(0x200002) & 0x2c, 0x20);
        let Event::Transmitted { controller, ticket } = backend.receive(5, 1, &data, now).unwrap()
        else {
            panic!("expected TX confirmation")
        };
        m.complete_can_transmission(controller, ticket).unwrap();
        assert_eq!(m.bus.can_interrupt(), Some((4, 28)));
        assert_eq!(m.bus.read_byte(0x200003), 2);
        assert_eq!(m.bus.can_interrupt(), None);
        let Event::Received { controller, frame } = backend
            .receive(
                5,
                0,
                &[
                    0, 0, 7, 0xe8, 0x10, 0x13, 0x5a, 0x90, 0x59, 0x53, 0x33, 0x46,
                ],
                now,
            )
            .unwrap()
        else {
            panic!("expected RX frame")
        };
        assert!(m.receive_can_frame(controller, &frame).unwrap());
        assert_eq!(m.bus.can_interrupt(), Some((4, 28)));
        assert_eq!(m.bus.read_byte(0x200003), 1);
        assert_eq!(m.bus.read_byte(0x200013), 0x10); // ISO-TP PCI reaches native RX window unchanged.
        assert_eq!(m.bus.read_byte(0x20101d), 0);
        assert_eq!(m.bus.read_byte(0x20201d), 0);
        assert!(m.bus.fault.is_none());
    }

    fn receive_fixture() -> Machine {
        let mut m = machine(&[0x60, 0xfe]);
        m.enable_can_register_probe();
        for c in [0, 2] {
            m.enable_can_transport(c).unwrap();
            let base = 0x200000 + c as u32 * 0x1000;
            m.bus.write_byte(base + 31, 0xc7);
            m.bus.write_byte(base + 6, if c == 0 { 0xc1 } else { 0xdd });
            m.bus.write_byte(base + 7, 0x36);
            for reg in 20..24 {
                m.bus.write_byte(base + reg, 0xff);
            }
            m.bus.write_byte(base + 4, 0xff);
            m.bus.write_byte(base, 8);
        }
        m.bus.write_byte(0xfffa1f, 0x14);
        m.take_can_events();
        m
    }

    fn receive_fixture_frame(value: u8) -> CanFrame {
        CanFrame {
            id: 0x541,
            extended: false,
            rtr: false,
            dlc: 8,
            data: vec![value; 8],
        }
    }

    #[test]
    fn completed_guest_disabled_irq_routes_do_not_build_an_unbounded_receive_queue() {
        use crate::can_adapter::ReceiveStaging;
        let mut m = receive_fixture();
        // Recorded CANdi finish state: mode=08, IER=ff, PFPAR IRQ routes off.
        m.bus.write_byte(0xfffa1f, 0);
        let mut staging = ReceiveStaging::default();
        let mut overruns = 0;
        for n in 0..5000 {
            staging.push(2, receive_fixture_frame(n as u8)).unwrap();
            staging
                .drain(|c, frame| {
                    if m.should_defer_can_receive(c, frame, 0xdd, 0x36)? {
                        return Ok(false);
                    }
                    overruns += usize::from(m.can_receive_would_overrun(c, frame, 0xdd, 0x36)?);
                    m.receive_can_frame_at_timing(c, frame, 0xdd, 0x36)?;
                    Ok(true)
                })
                .unwrap();
            assert!(staging.is_empty());
        }
        assert_eq!(overruns, 4995);
        let state = m.can_receive_state(2).unwrap();
        assert_eq!(state.queued_messages, 5);
        assert_eq!(state.queued_bytes, 55);
        assert_ne!(state.status & 2, 0); // Hardware DOS, not hidden loss.
        assert!(m.bus.can_interrupt().is_none());
        assert!(m.step(u64::MAX)); // Overflow is not a native CPU failure.
        assert!(m
            .take_can_events()
            .iter()
            .any(|e| e.contains("FIFO overrun")));
        // A polling guest can still read the original oldest frame.
        assert_eq!(m.bus.read_byte(0x202013), 0);
        for _ in 0..5 {
            m.bus.write_byte(0x202001, 4);
        }
        m.bus.write_byte(0x202001, 8); // Explicit guest clear DOS.
        m.bus.write_byte(0xfffa1f, 0x14);
        let next = receive_fixture_frame(0xa5);
        assert!(m.receive_can_frame_at_timing(2, &next, 0xdd, 0x36).unwrap());
        assert_eq!(m.bus.read_byte(0x202013), 0xa5);
        assert_eq!(m.can_receive_state(2).unwrap().status & 2, 0);
        assert_eq!(m.bus.can_interrupt(), Some((2, 26)));
    }

    #[test]
    fn masking_receive_irq_keeps_fifo_readable_and_allows_hardware_overrun() {
        let mut m = receive_fixture();
        let frame = receive_fixture_frame(1);
        for _ in 0..5 {
            m.receive_can_frame_at_timing(0, &frame, 0xc1, 0x36)
                .unwrap();
        }
        assert!(m.should_defer_can_receive(0, &frame, 0xc1, 0x36).unwrap());
        m.bus.write_byte(0x200004, 0);
        assert!(!m.should_defer_can_receive(0, &frame, 0xc1, 0x36).unwrap());
        let state = m.can_receive_state(0).unwrap();
        assert_eq!(state.queued_messages, 5);
        assert!(!m
            .receive_can_frame_at_timing(0, &frame, 0xc1, 0x36)
            .unwrap());
        assert_ne!(m.can_receive_state(0).unwrap().status & 2, 0);
        // Wrong bit rate / reset frames must never be deferred as overruns.
        assert!(!m.should_defer_can_receive(0, &frame, 0xdd, 0x36).unwrap());
        m.bus.write_byte(0x200000, 9);
        assert!(!m.can_receive_would_overrun(0, &frame, 0xc1, 0x36).unwrap());
    }

    #[test]
    fn active_batch_preserves_bus_order_and_one_full_fifo_does_not_block_another() {
        use crate::can_adapter::ReceiveStaging;
        let mut m = receive_fixture();
        let mut staging = ReceiveStaging::default();
        for n in 0..32 {
            staging.push(0, receive_fixture_frame(n)).unwrap();
        }
        staging.push(2, receive_fixture_frame(99)).unwrap();
        let mut received: [Vec<u8>; 3] = Default::default();
        let mut first = true;
        while !staging.is_empty() {
            staging
                .drain(|c, frame| {
                    let btr0 = if c == 0 { 0xc1 } else { 0xdd };
                    if m.should_defer_can_receive(c, frame, btr0, 0x36)? {
                        return Ok(false);
                    }
                    assert!(m.receive_can_frame_at_timing(c, frame, btr0, 0x36)?);
                    received[c].push(frame.data[0]);
                    Ok(true)
                })
                .unwrap();
            if first {
                assert_eq!(received[0].len(), 5);
                assert_eq!(received[2], [99]); // HS remains blocked; SW progressed.
                assert_eq!(staging.depths(), [27, 0, 0]);
                first = false;
            }
            for c in [0, 2] {
                let state = m.can_receive_state(c).unwrap();
                assert_eq!(state.status & 2, 0);
                for _ in 0..state.queued_messages {
                    m.bus.write_byte(0x200001 + c as u32 * 0x1000, 4);
                }
            }
            m.take_can_events();
        }
        assert_eq!(received[0], (0..32).collect::<Vec<u8>>());
    }

    #[test]
    fn addressed_records_preserve_boundaries_and_sparse_tail() {
        let mut bytes = vec![0; 520];
        bytes[8 + 511] = 0x12;
        bytes.extend_from_slice(&0x10200u32.to_le_bytes());
        bytes.extend_from_slice(&3u32.to_le_bytes());
        bytes.extend_from_slice(&[0x34, 0x56, 0x78]);
        bytes.extend_from_slice(&0x10206u32.to_le_bytes());
        bytes.extend_from_slice(&2u32.to_le_bytes());
        bytes.extend_from_slice(&[0xab, 0xcd]);
        bytes.extend_from_slice(&[0; 8]);
        let rom = decode_records(&bytes).unwrap();
        assert_eq!(
            &rom[511..520],
            &[0x12, 0x34, 0x56, 0x78, 0, 0, 0, 0xab, 0xcd]
        );
        // Byte-swapped, overlapping, oversized, truncated and unterminated
        // streams must not turn metadata into executable instructions.
        for (address, length) in [
            (0x101ffu32, 3u32),
            (0x20001, 3),
            (0x10200, 513),
            (0x10200, 0),
        ] {
            let mut bad = bytes.clone();
            bad[520..524].copy_from_slice(&address.to_le_bytes());
            bad[524..528].copy_from_slice(&length.to_le_bytes());
            assert!(decode_records(&bad).is_err());
        }
        assert!(decode_records(&bytes[..529]).is_err());
        assert!(decode_records(&bytes[..bytes.len() - 8]).is_err());
    }

    #[test]
    fn controller_receive_irq_uses_native_vector_and_port_assignment() {
        for (index, level, vector) in [(0, 4, 28), (1, 3, 27), (2, 2, 26)] {
            let mut m = machine(&[0x4e, 0x71, 0x60, 0xfc]);
            m.enable_can_register_probe();
            m.enable_can_transport(index).unwrap();
            let base = 0x200000 + index as u32 * 0x1000;
            m.bus.write_byte(base + 31, 0xc7);
            for reg in 20..24 {
                m.bus.write_byte(base + reg, 0xff);
            }
            m.bus.write_byte(base + 4, 1);
            m.bus.write_byte(base, 8);
            m.receive_can_frame(
                index,
                &CanFrame {
                    id: 0x7e8,
                    extended: false,
                    rtr: false,
                    dlc: 1,
                    data: vec![0x5a],
                },
            )
            .unwrap();
            assert_eq!(m.bus.can_interrupt(), None); // Pin still GPIO.
            m.bus.write_byte(0xfffa1f, 1 << level);
            assert_eq!(m.bus.can_interrupt(), Some((level, vector)));
            m.cpu.vbr = RAM_BASE;
            m.cpu.set_sr(0x2000);
            m.cpu.set_sp(RAM_BASE + RAM_SIZE as u32);
            let handler = RAM_BASE + 0x400;
            m.bus.write_long(RAM_BASE + vector * 4, handler);
            // MOVE.B #4,CMR releases RX; MOVEQ #42,D0; RTE.
            let mut code = vec![0x13, 0xfc, 0, 4];
            code.extend_from_slice(&(base + 1).to_be_bytes());
            code.extend_from_slice(&[0x70, 42, 0x4e, 0x73]);
            m.bus.ram[0x400..0x400 + code.len()].copy_from_slice(&code);
            for _ in 0..8 {
                assert!(m.step(20), "{:?}", m.snapshot().reason);
            }
            assert_eq!(m.cpu.dar[0], 42);
            assert_eq!(m.bus.can_interrupt(), None);
            assert_eq!(m.bus.read_byte(base + 29), 0);
            assert_eq!(m.cpu.sp(), RAM_BASE + RAM_SIZE as u32);
        }
    }

    #[test]
    #[ignore = "requires the untracked local MSI CANdi firmware"]
    fn local_firmware_answers_device_information_over_serial() {
        let mut m = Machine::new(Firmware::load(Path::new("dumps/candi/candi.bin")).unwrap());
        for _ in 0..100_000 {
            assert!(m.step(2_000_000));
        }
        assert!(m.snapshot().serial_tx.is_empty());
        m.receive_serial_frame(&[0x90, 0x03, 0x6d]).unwrap();
        while m.step(2_000_000) {}
        let snapshot = m.snapshot();
        assert_eq!(snapshot.reason, Some(StopReason::Budget));
        assert_eq!(snapshot.serial_rx_count, 3);
        assert_eq!(snapshot.serial_break_count, 1);
        let mut expected = vec![0x91, 0x03, 0x00];
        expected.extend_from_slice(b"CANdi Application\0");
        expected.extend_from_slice(b"45");
        expected.push(0xd0);
        assert_eq!(snapshot.serial_tx, expected);
        assert_eq!(
            snapshot
                .serial_tx
                .iter()
                .fold(0u8, |sum, b| sum.wrapping_add(*b)),
            0
        );
        assert!(snapshot.ignored_rom_write_count > 0);
    }

    #[test]
    fn native_trap_handler_returns_to_following_instruction() {
        // TRAP #15; NOP. Handler MOVEQ #42,D0; RTE in writable test RAM.
        let mut m = machine(&[0x4e, 0x4f, 0x4e, 0x71]);
        m.cpu.vbr = RAM_BASE;
        m.cpu.set_sp(RAM_BASE + RAM_SIZE as u32);
        m.bus.write_long(RAM_BASE + 47 * 4, RAM_BASE + 0x400);
        m.bus.write_long(RAM_BASE + 0x400, 0x702a4e73);
        assert!(m.step(10));
        assert_eq!(m.cpu.pc, RAM_BASE + 0x400);
        assert!(m.step(10));
        assert!(m.step(10));
        assert_eq!(m.cpu.dar[0], 42);
        assert_eq!(m.cpu.pc, APPLICATION_ENTRY + 2);
        assert_eq!(m.cpu.sp(), RAM_BASE + RAM_SIZE as u32);
    }

    #[test]
    fn inventory_requires_metadata_and_does_not_advertise_absent_modules() {
        let mut m = machine(&[0x4e, 0x71]);
        assert!(m.install_application_inventory().is_err());
        m.bus.rom[8] = 0x55;
        m.bus.rom[0x48..0x59].copy_from_slice(b"CANdi Application");
        m.bus.rom[0x18..0x1a].copy_from_slice(b"45");
        m.bus.rom[0x0c..0x14].copy_from_slice(b"04/19/05");
        m.bus.rom[0x30..0x38].copy_from_slice(b"16:17:00");
        let before = m.bus.rom.clone();
        m.install_application_inventory().unwrap();
        assert_eq!(m.bus.rom, before);
        assert_eq!(m.bus.read_byte(0x4000), 0xa5);
        assert_eq!(m.bus.read_byte(0x4002), 40);
        assert_eq!(m.bus.read_byte(0x4003 + 25), 0); // No BOOT.
        assert_eq!(m.bus.read_byte(0x402b + 25), 0x55);
        let table = m.bus.inventory.as_ref().unwrap();
        assert_eq!(&table[43 + 26..83], b"45041905161700");
        assert!(table[83..].iter().all(|&b| b == 0));
        m.bus.read_word(0x4000 + 202);
        assert!(m.bus.fault.is_some()); // No read beyond inventory mapping.
        m.bus.rom[0x18] = b'?';
        assert!(m.install_application_inventory().is_err());
    }

    #[test]
    fn controller_probe_windows_are_isolated_and_trace_is_bounded() {
        let mut m = machine(&[0x4e, 0x71]);
        m.enable_can_register_probe();
        m.bus.write_byte(0x20101f, 0xc7);
        assert_eq!(m.bus.read_byte(0x20101f), 0xc7);
        assert_eq!(m.bus.read_byte(0x20001f), 5);
        assert_eq!(m.bus.read_byte(0x20201f), 5);
        assert_eq!(m.take_can_events().len(), 1);
        for _ in 0..2000 {
            m.bus.write_byte(0x201004, 0);
        }
        assert!(m.bus.fault.is_some());
        let events = m.take_can_events();
        assert!(events.len() <= 1024);
        assert!(events.last().unwrap().contains("overflow"));
        assert!(can_register(0x203000, 1).is_none());
        assert!(can_register(0x202080, 1).is_none());
        assert!(can_register(0x202000, 2).is_none());
    }

    #[test]
    #[ignore = "requires the untracked local MSI CANdi firmware"]
    fn local_application_restart_serial_timing() {
        for delay in [3_200, 200_000] {
            let mut m = Machine::new(Firmware::load(Path::new("dumps/candi/candi.bin")).unwrap());
            m.install_application_inventory().unwrap();
            for _ in 0..100_000 {
                assert!(m.step(u64::MAX));
            }
            m.receive_serial_frame(&[0x80, 0x08, 0x01, 0x77]).unwrap();
            let mut ack_at = None;
            let mut sent = false;
            let mut entries = 0;
            for _ in 0..500_000 {
                if m.cpu.pc == APPLICATION_ENTRY {
                    entries += 1;
                    eprintln!(
                        "restart delay={delay} entry insns={} rx={} breaks={}",
                        m.attempted,
                        m.bus.peripherals.serial.received,
                        m.bus.peripherals.serial.breaks
                    );
                }
                assert!(m.step(u64::MAX), "{:?}", m.snapshot().reason);
                if m.take_serial_events().contains(&None) && ack_at.is_none() {
                    ack_at = Some(m.attempted);
                }
                if !sent && ack_at.is_some_and(|at| m.attempted >= at + delay) {
                    eprintln!(
                        "restart delay={delay} baud request insns={} pc={:#x}",
                        m.attempted, m.cpu.pc
                    );
                    m.receive_serial_frame(&[0x80, 0x09, 0x01, 0x45, 0x85, 0xac])
                        .unwrap();
                    sent = true;
                }
            }
            eprintln!(
                "restart delay={delay} entries={entries} pc={:#x} output={:02x?}",
                m.cpu.pc,
                m.bus.peripherals.serial.output()
            );
            assert!(sent);
            assert_eq!(entries, 1);
            assert!(m
                .bus
                .peripherals
                .serial
                .output()
                .starts_with(&[0x81, 8, 0, 0x77]));
            if delay == 200_000 {
                assert!(m
                    .bus
                    .peripherals
                    .serial
                    .output()
                    .ends_with(&[0x81, 9, 0, 0x76]));
            }
        }
    }

    #[test]
    #[ignore = "requires local MSI firmware and the recorded native serial trace; simulated controller completion only"]
    fn local_native_wake_completion_restores_gpio_after_serial_ack() {
        // Replay only Tech2's recorded serial writes to the real CANdi CPU.
        // The test's explicit controller completion is not a live adapter ACK,
        // and no vehicle response or guest LCD value is supplied.
        let trace =
            std::fs::read_to_string("runs/candi-native-link/electrical-mode-check/trace.jsonl")
                .unwrap();
        let mut requests = Vec::new();
        for line in trace.lines() {
            let Some((_, bytes)) = line.split_once("Tech2->CANdi bytes=[") else {
                continue;
            };
            let (bytes, rest) = bytes.split_once(']').unwrap();
            let insns: u64 = rest
                .split_once("native_insns=")
                .unwrap()
                .1
                .split_whitespace()
                .next()
                .unwrap()
                .parse()
                .unwrap();
            let bytes = bytes
                .split(", ")
                .map(|b| u8::from_str_radix(b, 16).unwrap())
                .collect::<Vec<_>>();
            assert_eq!(bytes.iter().fold(0u8, |sum, b| sum.wrapping_add(*b)), 0);
            let wake = bytes == [0x88, 0x0e, 0x6a];
            requests.push((insns, bytes));
            if wake {
                break;
            }
        }
        assert_eq!(requests.last().unwrap().1, [0x88, 0x0e, 0x6a]);
        let mut m = Machine::new(Firmware::load(Path::new("dumps/candi/candi.bin")).unwrap());
        m.install_application_inventory().unwrap();
        m.enable_can_register_probe();
        m.enable_can_transport(2).unwrap();
        for (at, bytes) in requests {
            while m.attempted() < at {
                assert!(m.step(u64::MAX), "{:?}", m.snapshot().reason);
                if m.attempted() % 128 == 0 {
                    m.take_can_events();
                    m.take_serial_events();
                }
            }
            m.receive_serial_frame(&bytes).unwrap();
        }
        m.take_serial_events();
        let mut frame = None;
        let deadline = m.attempted() + 100_000;
        while m.attempted() < deadline {
            assert!(m.step(u64::MAX), "{:?}", m.snapshot().reason);
            m.take_can_events();
            let mut txs = m.take_can_transmissions();
            if !txs.is_empty() {
                assert_eq!(txs.len(), 1);
                frame = txs.pop();
                break;
            }
        }
        let (controller, tx) = frame.expect("native wake-up request");
        assert_eq!(controller, 2);
        assert_eq!(tx.frame.id, 0x100);
        assert_eq!(tx.frame.dlc, 0);
        assert!(tx.frame.data.is_empty());
        assert!(tx.electrical.wake_control_selected());
        eprintln!(
            "OFFLINE ONLY native wake intent at {} electrical={}; external_tx=false",
            m.attempted(),
            tx.electrical
        );
        let mut output = Vec::new();
        let mut restored = false;
        // Observe what the native serial protocol does while the controller
        // still has a pending TX. Its acknowledgement may mean queue acceptance.
        for _ in 0..2_000 {
            assert!(m.step(u64::MAX), "{:?}", m.snapshot().reason);
            for event in m.take_can_events() {
                eprintln!("OFFLINE BEFORE COMPLETION {event}");
                restored |= event.contains("after=[single-wire-gpio latch=0x27");
            }
            output.extend(m.take_serial_events().into_iter().flatten());
        }
        eprintln!(
            "OFFLINE BEFORE COMPLETION native serial={output:02x?} SR={:#04x}",
            m.bus.read_byte(0x202002)
        );
        assert_eq!(
            m.bus.read_byte(0x202002) & 0x08,
            0,
            "controller reported TX success without completion"
        );
        assert_eq!(
            output,
            [0x89, 0x0e, 0, 0x69],
            "native serial acceptance missing"
        );
        assert!(
            !restored,
            "transceiver restored before controller completion"
        );
        m.complete_can_transmission(controller, tx.ticket).unwrap();
        let deadline = m.attempted() + 2_000_000;
        while m.attempted() < deadline {
            assert!(m.step(u64::MAX), "{:?}", m.snapshot().reason);
            for event in m.take_can_events() {
                eprintln!("OFFLINE ONLY {event}");
                restored |= event.contains("after=[single-wire-gpio latch=0x27");
            }
            for byte in m.take_serial_events() {
                if let Some(byte) = byte {
                    output.push(byte);
                } else {
                    eprintln!("OFFLINE ONLY native serial reply={output:02x?}; external_tx=false");
                }
            }
            assert!(
                m.take_can_transmissions().is_empty(),
                "unexpected additional TX"
            );
            if restored && output == [0x89, 0x0e, 0, 0x69] {
                break;
            }
        }
        assert!(restored, "native GPIO restore missing");
        assert_eq!(output, [0x89, 0x0e, 0, 0x69]);
    }

    #[test]
    fn disabled_serial_transmit_stops() {
        let mut m = machine(&[0x13, 0xfc, 0, 0x55, 0, 0xff, 0xfc, 0x0f]);
        assert!(!m.step(10));
        assert_eq!(m.snapshot().completed, 0);
        assert!(matches!(
            m.snapshot().reason,
            Some(StopReason::UnsupportedAccess(Access {
                address: 0xfffc0f,
                write: true,
                ..
            }))
        ));
    }

    #[test]
    fn executes_registers_ram_and_cpu32_long_branch() {
        // MOVEA.L #RAM_END,A7; MOVEQ #42,D0; MOVE.L D0,-(A7); BRA.L back to MOVEQ.
        let mut m = machine(&[
            0x2e, 0x7c, 0, 0x14, 0, 0, 0x70, 42, 0x2f, 0, 0x60, 0xff, 0xff, 0xff, 0xff, 0xfa,
        ]);
        for _ in 0..4 {
            assert!(m.step(10));
        }
        assert_eq!(m.cpu.pc, APPLICATION_ENTRY + 6);
        assert_eq!(m.cpu.dar[0], 42);
        assert_eq!(m.bus.read_long(0x13fffc), 42);
    }

    #[test]
    fn unsupported_read_stops_without_counting_fake_reply() {
        let mut m = machine(&[0x10, 0x39, 0, 0xff, 0xfc, 0x1f]);
        assert!(!m.step(10));
        assert_eq!(m.snapshot().completed, 0);
        assert_eq!(
            m.snapshot().reason,
            Some(StopReason::UnsupportedAccess(Access {
                address: 0xfffc1f,
                width: 1,
                write: false,
                value: 0,
            }))
        );
        assert!(!m.step(10));
        assert_eq!(m.snapshot().attempted, 1);
    }

    #[test]
    fn budget_and_cancel_stop_infinite_loop() {
        let mut m = machine(&[0x60, 0xfe]);
        while m.step(100) {}
        assert_eq!(m.snapshot().completed, 100);
        assert_eq!(m.snapshot().reason, Some(StopReason::Budget));
        let mut m = machine(&[0x60, 0xfe]);
        m.cancel();
        assert!(!m.step(100));
        assert_eq!(m.snapshot().reason, Some(StopReason::Cancelled));
    }

    #[test]
    fn rejects_truncated_and_unknown_images() {
        assert!(Firmware::parse(&[]).is_err());
        assert!(Firmware::parse(&vec![0; ROM_SIZE + 8]).is_err());
        assert!(Firmware::parse(&vec![0; ROM_SIZE + 9]).is_err());
        assert!(Firmware::load(&std::env::temp_dir()).is_err());
    }

    #[test]
    fn rom_and_cross_boundary_writes_are_rejected() {
        let mut m = machine(&[0x4e, 0x71]);
        let original = m.bus.read_word(APPLICATION_ENTRY);
        m.bus.write_word(APPLICATION_ENTRY, 0);
        assert!(m.bus.fault.is_some());
        assert_eq!(m.bus.read_word(APPLICATION_ENTRY), original);
        assert!(Bus::range(u32::MAX, 4, RAM_BASE, RAM_SIZE).is_none());
        assert!(Bus::range(RAM_BASE + RAM_SIZE as u32 - 1, 2, RAM_BASE, RAM_SIZE).is_none());
    }
}
