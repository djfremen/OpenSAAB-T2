// SPDX-License-Identifier: MPL-2.0
//! Construction is delayed; the first triggering guest bus access is retained.
use super::native_link::NativeLink;
use crate::options::{HarnessTarget, Options};
use std::path::PathBuf;

#[derive(Clone)]
enum Adapter {
    None,
    Nano(String, tech2_emu::nano_native::Profile),
    Chipsoft(String, bool, bool, bool),
    J2534(
        tech2_emu::vcx::Connection,
        bool,
        tech2_emu::vcx::J2534Adapter,
        bool,
    ),
}
#[derive(Clone)]
pub struct Pending {
    pub uart: super::uart::Uart,
    pub adc_channel: u8,
    pub presence_announced: bool,
    firmware: PathBuf,
    adapter: Adapter,
    pub output: PathBuf,
    pub transport_trace: bool,
    monitor_ignition: bool,
}
impl Pending {
    pub fn from_options(opts: &Options) -> Self {
        let adapter = if let Some(token) = &opts.candi_nano_usb_token {
            Adapter::Nano(
                token.clone(),
                if opts.candi_nano_clear_dtc {
                    tech2_emu::nano_native::Profile::ClearDtc
                } else {
                    tech2_emu::nano_native::Profile::collection(
                        opts.target == HarnessTarget::SecurityLink1367,
                    )
                },
            )
        } else if let Some(token) = &opts.candi_chipsoft_usb_token {
            Adapter::Chipsoft(
                token.clone(),
                opts.candi_chipsoft_symbol_only,
                opts.candi_chipsoft_seeds,
                opts.candi_chipsoft_audible,
            )
        } else if let Some(target) = &opts.candi_nano_ssh {
            Adapter::J2534(
                tech2_emu::vcx::Connection {
                    target: target.clone(),
                    control_path: opts.candi_nano_control.clone(),
                    ecu_profile: Default::default(),
                },
                opts.target == HarnessTarget::SecurityLink1367,
                opts.candi_j2534_adapter,
                opts.candi_seatbelt_audible,
            )
        } else {
            Adapter::None
        };
        Self {
            presence_announced: false,
            adc_channel: 0,
            uart: Default::default(),
            firmware: opts
                .candi_firmware
                .clone()
                .unwrap_or_else(super::worker::default_firmware_path),
            adapter,
            output: opts.output.clone(),
            transport_trace: true,
            monitor_ignition: matches!(
                opts.target,
                HarnessTarget::EcmLink1367
                    | HarnessTarget::SecurityLink1367
                    | HarnessTarget::DtcLink1367
                    | HarnessTarget::EngineData1367
                    | HarnessTarget::NativeManual
            ),
        }
    }
    pub fn has_adapter(&self) -> bool {
        !matches!(self.adapter, Adapter::None)
    }
    pub fn start(&self) -> Result<NativeLink, String> {
        let mut link = NativeLink::new(&self.firmware)?;
        link.prepare_serial()?;
        link.restore_uart(self.uart.clone());
        link.restore_adc(self.adc_channel);
        link.presence_announced = self.presence_announced;
        match &self.adapter {
            Adapter::None => {}
            Adapter::Nano(token, profile) => link.attach_nano_usb(token, &self.output, *profile)?,
            Adapter::Chipsoft(token, symbols, seeds, audible) => {
                link.attach_chipsoft_usb(token, &self.output, *symbols, *seeds, *audible)?
            }
            Adapter::J2534(connection, seeds, adapter, audible) => {
                link.attach_j2534(connection.clone(), &self.output, *seeds, *adapter, *audible)?
            }
        }
        if self.monitor_ignition {
            link.enable_ignition_monitor(&self.output);
        }
        Ok(link)
    }
}

// Existing passive cable/reference voltages: no secondary CPU or vehicle I/O.
pub fn cable_adc(channel: &mut u8, mux: u8, command: u16) -> u16 {
    match command {
        0x0180 => *channel = mux & 15,
        0x01c0 => *channel = mux >> 4,
        _ => {}
    }
    match *channel {
        15 => 0x0225,
        13 => 0x07fe,
        _ => 0xffff,
    }
}

// This experimental profile defers the original guest application loader too.
// Firmware 9.250 normally calls mark-attempt(8), then its inventory/application
// loader from the welcome loop (0x1c1f10). Run those same guest routines once at
// the first communication entry. The original caller and argument frame stay
// intact. pSOS tm_wkafter(1) lets the real presence ISR/driver finish first.
// No successful readiness flag, serial reply, or diagnostic result is fabricated.
#[derive(Clone)]
pub struct GuestInit {
    registers: [u32; 16],
    sr: u16,
    resume: u32,
    started: std::time::Instant,
    inventory: bool,
    mark_attempt: bool,
    marked: bool,
}
impl GuestInit {
    fn call(&mut self, cpu: &mut m68k::CpuCore, bus: &mut crate::bus::Tech2Bus) {
        use m68k::AddressBus;
        let sp = self.registers[15];
        self.inventory = bus.ram_word(0x1fbc10) & 2 != 0;
        self.mark_attempt = self.inventory && !self.marked;
        let call_sp = if self.mark_attempt {
            sp - 6
        } else if self.inventory {
            sp - 4
        } else {
            sp - 8
        };
        if self.mark_attempt {
            bus.write_word(sp - 2, 8);
        }
        if !self.inventory {
            bus.write_long(sp - 4, 1);
        }
        bus.write_long(call_sp, self.resume);
        cpu.set_sp(call_sp);
        cpu.pc = if self.mark_attempt {
            0x1d68c6
        } else if self.inventory {
            0x1d8f46
        } else {
            0x126f6
        };
    }
}
pub fn guest_dependency(cpu: &mut m68k::CpuCore, bus: &mut crate::bus::Tech2Bus) {
    if let Some(mut init) = bus.demand_guest_init.take() {
        if init.started.elapsed() > std::time::Duration::from_secs(60) {
            bus.failure_reason = Some(
                "Deferred CANdi guest initialization timed out; original request not sent".into(),
            );
            return;
        }
        let return_sp = init.registers[15] - if init.inventory { 0 } else { 4 };
        if cpu.pc == init.resume && cpu.sp() == return_sp {
            let result = cpu.dar[0];
            cpu.set_sr_noint_nosp(init.sr);
            cpu.dar = init.registers;
            if init.mark_attempt {
                init.marked = true;
                init.call(cpu, bus);
                bus.demand_guest_init = Some(init);
                return;
            }
            if init.inventory {
                if bus.ram_word(0x1fbc10) & 0xf000 == 0 || !matches!(result as u8, 0 | 2) {
                    bus.failure_reason = Some(format!(
                        "CANdi guest loader failed ({result:#x}); original request not sent"
                    ));
                } else {
                    println!("CANDI_GUEST_INITIALIZED: result={result:#x} flags={:#x} original_pc={:#x} original_sp={:#x} elapsed_ms={}", bus.ram_word(0x1fbc10),init.resume,init.registers[15],init.started.elapsed().as_millis());
                }
                return;
            }
            init.call(cpu, bus);
        }
        bus.demand_guest_init = Some(init);
        return;
    }
    if bus.pending_candi.is_none() || !bus.demand_boot_complete {
        return;
    }
    let init = cpu.pc == 0x1ee5a6
        && bus.peek_bytes(cpu.pc, 12)
            == Some(&[
                0x48, 0xe7, 0x3f, 0x3e, 0x4f, 0xef, 0xff, 0xee, 0x42, 0x43, 0x61, 0x00,
            ]);
    let request = cpu.pc == 0x1dbb96
        && bus.peek_bytes(cpu.pc, 12)
            == Some(&[
                0x48, 0xe7, 0x0f, 0x0c, 0x4f, 0xef, 0xff, 0xe8, 0x2a, 0x6f, 0x00, 0x34,
            ])
        && bus.peek_bytes(cpu.sp() + 8, 4) == Some(&[0, 0, 0, 7]);
    if init || request {
        if bus.peek_bytes(0x1d8f46, 16)
            != Some(&[
                0x48, 0xe7, 0x03, 0x00, 0x4f, 0xef, 0xff, 0xce, 0x61, 0xff, 0xff, 0xff, 0xd9, 0x64,
                0x4a, 0x00,
            ])
        {
            bus.failure_reason = Some("Unsupported deferred CANdi guest loader".into());
            return;
        }
        if bus.peek_bytes(0x126f6, 20)
            != Some(&[
                0x4e, 0x56, 0, 0, 0x2f, 6, 0x2c, 0x2e, 0, 8, 0x70, 0x3c, 0x4e, 0x4b, 0x2c, 0x1f,
                0x4e, 0x5e, 0x4e, 0x75,
            ])
            || bus.peek_bytes(0x1d68c6, 22)
                != Some(&[
                    0x30, 0x39, 0, 0x1f, 0xbc, 0x10, 0x80, 0x6f, 0, 4, 0x33, 0xc0, 0, 0x1f, 0xbc,
                    0x10, 0x4e, 0x74, 0, 2, 0x30, 0x2f,
                ])
            || bus.peek_bytes(cpu.sp().wrapping_sub(8), 8).is_none()
        {
            bus.failure_reason = Some("Unsupported deferred CANdi helper or stack".into());
            return;
        }
        bus.ensure_candi(if init {
            "guest-communication-init"
        } else {
            "guest-adapter-request"
        });
        if bus.failure_reason.is_some() {
            return;
        }
        let mut state = GuestInit {
            registers: cpu.dar,
            sr: cpu.get_sr(),
            resume: cpu.pc,
            started: std::time::Instant::now(),
            inventory: false,
            mark_attempt: false,
            marked: false,
        };
        state.call(cpu, bus);
        bus.demand_guest_init = Some(state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use m68k::AddressBus;
    #[test]
    #[cfg(feature = "load-test")]
    fn missing_firmware_is_deferred_until_request_and_failure_is_terminal() {
        let dir = std::env::temp_dir().join(format!("demand-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let opts = Options::parse(
            [
                "--research-harness",
                "--candi-native-link",
                "--candi-on-demand",
                "--candi-firmware",
                "/no-such-candi-for-test",
            ]
            .into_iter()
            .map(str::to_owned),
            |_| None,
        )
        .unwrap();
        let mut p = Pending::from_options(&opts);
        p.output = dir.clone();
        let mut bus =
            crate::bus::Tech2Bus::new(vec![], vec![], crate::bus::ExecutionMode::ResearchHarness);
        bus.pending_candi = Some(p);
        // Native EXIT must remain a real key even before CANdi construction.
        bus.write_word(0x1fbc72, 0x3456);
        bus.exit_key();
        assert_eq!(bus.ram_word(0x1fbc72), 0x3456);
        bus.write_byte(0x400806, 0x80);
        bus.write_byte(0x400800, 0x33);
        assert_eq!(bus.read_byte(0x400800), 0x33);
        assert!(bus.candi_link.is_none());
        assert!(bus.failure_reason.is_none());
        assert!(bus.pending_candi.is_some());
        bus.write_byte(0x400806, 3);
        bus.write_byte(0x400800, 0x90);
        assert!(bus.pending_candi.is_none());
        assert!(bus.candi_link.is_none());
        assert!(bus
            .failure_reason
            .as_ref()
            .unwrap()
            .contains("CANdi initialization failed"));
        let mut cpu = m68k::CpuCore::new();
        assert!(matches!(
            crate::step_guest(&mut cpu, &mut bus, 0),
            m68k::StepResult::Stopped
        ));
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn guest_loader_restores_caller_and_does_not_repeat_request() {
        let mut bus =
            crate::bus::Tech2Bus::new(vec![], vec![], crate::bus::ExecutionMode::ResearchHarness);
        let mut cpu = m68k::CpuCore::new();
        cpu.pc = 0x1ee5a6;
        cpu.set_sp(0x180000);
        cpu.dar[0] = 0x12345678;
        cpu.dar[8] = 0xabcdef;
        cpu.set_sr(0x2015);
        let original = cpu.dar;
        let sr = cpu.get_sr();
        bus.write_long(cpu.sp(), 0x11223344);
        bus.write_long(cpu.sp() + 4, 0x55667788);
        let mut init = GuestInit {
            registers: original,
            sr,
            resume: cpu.pc,
            started: std::time::Instant::now(),
            inventory: false,
            mark_attempt: false,
            marked: false,
        };
        init.call(&mut cpu, &mut bus);
        assert_eq!(cpu.pc, 0x126f6);
        assert_eq!(bus.ram_long(original[15] - 4), 1);
        // Model a return from the actual pSOS wait after hardware discovery.
        bus.write_word(0x1fbc10, 3);
        cpu.pc = init.resume;
        cpu.set_sp(original[15] - 4);
        bus.demand_guest_init = Some(init);
        guest_dependency(&mut cpu, &mut bus);
        assert_eq!(cpu.pc, 0x1d68c6);
        assert_eq!(bus.ram_word(original[15] - 2), 8);
        // Model mark-attempt RTD #2, then inventory/application loader RTS.
        cpu.pc = 0x1ee5a6;
        cpu.set_sp(original[15]);
        guest_dependency(&mut cpu, &mut bus);
        assert_eq!(cpu.pc, 0x1d8f46);
        bus.write_word(0x1fbc10, 0x100b);
        cpu.pc = 0x1ee5a6;
        cpu.set_sp(original[15]);
        cpu.dar[0] = 2;
        guest_dependency(&mut cpu, &mut bus);
        assert!(bus.demand_guest_init.is_none());
        assert!(bus.failure_reason.is_none());
        assert_eq!(cpu.dar, original);
        assert_eq!(cpu.get_sr(), sr);
        assert_eq!(bus.ram_long(cpu.sp()), 0x11223344);
        assert_eq!(bus.ram_long(cpu.sp() + 4), 0x55667788);
        guest_dependency(&mut cpu, &mut bus);
        assert_eq!(cpu.pc, 0x1ee5a6);
        assert!(bus.demand_guest_init.is_none());
    }
    #[test]
    fn failed_guest_loader_never_releases_original_request() {
        let mut bus =
            crate::bus::Tech2Bus::new(vec![], vec![], crate::bus::ExecutionMode::ResearchHarness);
        let mut cpu = m68k::CpuCore::new();
        cpu.pc = 0x1ee5a6;
        cpu.set_sp(0x180000);
        bus.demand_guest_init = Some(GuestInit {
            registers: cpu.dar,
            sr: cpu.get_sr(),
            resume: cpu.pc,
            started: std::time::Instant::now(),
            inventory: true,
            mark_attempt: false,
            marked: true,
        });
        bus.write_word(0x1fbc10, 0x100b);
        cpu.dar[0] = 4;
        guest_dependency(&mut cpu, &mut bus);
        assert!(bus
            .failure_reason
            .as_ref()
            .unwrap()
            .contains("original request not sent"));
        assert!(matches!(
            crate::step_guest(&mut cpu, &mut bus, 0),
            m68k::StepResult::Stopped
        ));
    }
    #[test]
    fn cable_adc_retains_channel_without_starting_cpu() {
        let mut c = 0;
        assert_eq!(cable_adc(&mut c, 15, 0x180), 0x225);
        assert_eq!(cable_adc(&mut c, 0, 0), 0x225);
        assert_eq!(cable_adc(&mut c, 0xd0, 0x1c0), 0x7fe);
    }
}
