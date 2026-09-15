// SPDX-License-Identifier: MPL-2.0
//! Opt-in research cooperative Tech2 UART ↔ native CANdi SCI experiment.
//! Optional raw Nano backend. Unknown CANdi devices stop its CPU and are reported.
use std::collections::VecDeque;
use std::path::Path;
use tech2_emu::candi_cpu::{Firmware, Machine};

pub struct NativeLink {
    machine: Machine,
    bridge: Option<Box<dyn tech2_emu::can_adapter::Backend>>,
    live_clock: Option<(std::time::Instant, u64)>,
    clock_sample_at: std::time::Duration,
    pending_rx: tech2_emu::can_adapter::ReceiveStaging,
    rx_deferred_polls: u64,
    rx_overruns: [u64; 3],
    ignition: Option<tech2_emu::ignition::Monitor>,
    adc_channel: u8,
    rx_frame: Vec<u8>,
    registers: [u8; 8],
    divisor: [u8; 2],
    tx: Vec<u8>,
    rx: VecDeque<Option<u8>>,
    tx_irq: bool,
    line_ack: bool,
    stopped: bool,
    pub presence_announced: bool,
    events: Vec<String>,
}

impl NativeLink {
    pub fn checkpoint(&self) -> Result<Self, serde_json::Error> {
        if self.bridge.is_some() {
            return Err(serde_json::Error::io(std::io::Error::other(
                "Cannot rewind live Nano adapter state",
            )));
        }
        Ok(Self {
            bridge: None,
            live_clock: None,
            clock_sample_at: std::time::Duration::ZERO,
            pending_rx: self.pending_rx.clone(),
            rx_deferred_polls: self.rx_deferred_polls,
            rx_overruns: self.rx_overruns,
            ignition: None,
            machine: self.machine.checkpoint()?,
            adc_channel: self.adc_channel,
            rx_frame: self.rx_frame.clone(),
            registers: self.registers,
            divisor: self.divisor,
            tx: self.tx.clone(),
            rx: self.rx.clone(),
            tx_irq: self.tx_irq,
            line_ack: self.line_ack,
            stopped: self.stopped,
            presence_announced: self.presence_announced,
            events: self.events.clone(),
        })
    }
    pub fn new(path: &Path) -> Result<Self, String> {
        let mut machine = Machine::new(Firmware::load(path)?);
        machine.install_application_inventory()?;
        machine.enable_can_register_probe();
        Ok(Self {
            bridge: None,
            live_clock: None,
            clock_sample_at: std::time::Duration::ZERO,
            pending_rx: Default::default(),
            rx_deferred_polls: 0,
            rx_overruns: [0; 3],
            ignition: None,
            machine,
            adc_channel: 0,
            rx_frame: Vec::new(),
            registers: [0; 8],
            divisor: [0; 2],
            tx: Vec::new(),
            rx: VecDeque::new(),
            tx_irq: false,
            line_ack: false,
            stopped: false,
            presence_announced: false,
            events: vec!["virtual application inventory at 0x4000; source=loaded-header+OEM-record-layout; bootloader absent".into()],
        })
    }
    pub fn attach_nano(
        &mut self,
        connection: tech2_emu::vcx::Connection,
        directory: &Path,
        seed_reads: bool,
    ) -> Result<(), String> {
        self.attach_j2534(
            connection,
            directory,
            seed_reads,
            tech2_emu::vcx::J2534Adapter::Nano,
            false,
        )
    }
    pub fn attach_j2534(
        &mut self,
        connection: tech2_emu::vcx::Connection,
        directory: &Path,
        seed_reads: bool,
        adapter: tech2_emu::vcx::J2534Adapter,
        seatbelt_audible: bool,
    ) -> Result<(), String> {
        let bridge = tech2_emu::candi_nano::Bridge::start_with_adapter(
            connection,
            directory,
            seed_reads,
            adapter,
            seatbelt_audible,
        )?;
        self.machine.enable_can_transport(0)?;
        // Native channel 2 is a separate, unconnected 95-kbit bus. Its TX
        // remains pending without an ACK; never remap it to HS/SW or complete it.
        self.machine.enable_can_transport(1)?;
        self.machine.enable_can_transport(2)?;
        self.bridge = Some(Box::new(bridge));
        self.live_clock = Some((std::time::Instant::now(), self.machine.elapsed_cycles()));
        self.events.push("Live CANdi clock paced to existing 16777216 Hz research model; native timer values unchanged".into());
        self.events.push(format!("Native J2534 raw bridge connected; adapter={}; controller0=HS500k controller2=SW33k; diagnostic requests originate in firmware", adapter.name()));
        Ok(())
    }
    pub fn attach_nano_usb(
        &mut self,
        token: &str,
        directory: &Path,
        profile: tech2_emu::nano_native::Profile,
    ) -> Result<(), String> {
        let bridge = tech2_emu::nano_backend::Bridge::start(token, directory, profile)?;
        self.machine.enable_can_transport(0)?;
        self.machine.enable_can_transport(1)?;
        self.machine.enable_can_transport(2)?;
        self.bridge = Some(Box::new(bridge));
        self.live_clock = Some((std::time::Instant::now(), self.machine.elapsed_cycles()));
        self.events.push("Direct Android USB native bridge; requests=original-firmware completion=vcx-usb-write-compatibility electrical_ack=unverified".into());
        Ok(())
    }
    pub fn attach_chipsoft_usb(
        &mut self,
        token: &str,
        directory: &Path,
        symbol_only: bool,
        seeds: bool,
        audible: bool,
    ) -> Result<(), String> {
        let bridge = tech2_emu::chipsoft_backend::Bridge::start(
            token,
            directory,
            symbol_only,
            seeds,
            audible,
        )?;
        for controller in 0..3 {
            self.machine.enable_can_transport(controller)?;
        }
        self.bridge = Some(Box::new(bridge));
        self.live_clock = Some((std::time::Instant::now(), self.machine.elapsed_cycles()));
        self.events.push("Direct Android Chipsoft USB; original firmware requests; completion=device-reply-compatibility electrical_ack=unverified".into());
        Ok(())
    }
    pub fn enable_ignition_monitor(&mut self, directory: &Path) {
        if self.bridge.is_some() {
            self.ignition = Some(tech2_emu::ignition::Monitor::new(directory));
        }
    }
    pub fn live_confirmations(&self) -> u64 {
        self.bridge.as_ref().map_or(0, |b| b.confirmations())
    }
    pub fn has_live_clock(&self) -> bool {
        self.live_clock.is_some()
    }
    pub fn live_elapsed(&self) -> Option<std::time::Duration> {
        self.live_clock.map(|(start, _)| start.elapsed())
    }
    pub fn is_stopped(&self) -> bool {
        self.stopped
    }
    fn poll_bridge(&mut self) -> Result<(), String> {
        let Some(bridge) = &mut self.bridge else {
            return Ok(());
        };
        for event in bridge.poll()? {
            match event {
                tech2_emu::can_adapter::Event::Completed {
                    controller: c,
                    ticket,
                    source,
                } => {
                    let applied = self.machine.complete_can_transmission(c, ticket)?;
                    self.events.push(format!(
                        "NANO driver TX indication origin={} electrical_ack=unverified controller={c} ticket={ticket} external_tx=true emulated_completion_applied={applied}", source.label()
                    ));
                }
                tech2_emu::can_adapter::Event::Received(c, frame) => {
                    if let Some(m) = &mut self.ignition {
                        m.received(c, &frame);
                    }
                    self.pending_rx.push(c, frame)?;
                }
            }
        }
        let deferred = self.pending_rx.drain(|c, frame| {
            let btr0 = if c == 0 { 0xc1 } else { 0xdd };
            if self.machine.should_defer_can_receive(c, frame, btr0, 0x36)? {
                return Ok(false);
            }
            let overrun = self.machine.can_receive_would_overrun(c, frame, btr0, 0x36)?;
            let accepted = self.machine.receive_can_frame_at_timing(c, frame, btr0, 0x36)
                .map_err(|e| format!("controller={c} id={:03X}: {e}", frame.id))?;
            if accepted {
                self.events.push(format!("NANO RX controller={c} id={:03X} data={:02X?} accepted_by_native_filter=true origin=adapter",frame.id,frame.data));
            } else if overrun {
                self.rx_overruns[c] = self.rx_overruns[c].saturating_add(1);
                // Count every lost hardware arrival, without flooding logs
                // with unrelated bus chatter. Raw adapter captures retain it.
                if self.rx_overruns[c] == 1 || (0x640..=0x65f).contains(&frame.id) || frame.id >= 0x7e8 {
                    self.events.push(format!("CAN RX not delivered controller={c} id={:03X} reason=hardware-fifo-overrun rx_irq_routed={} state={:?} retained_raw_in_adapter_log=true",frame.id,self.machine.can_interrupt_routed(c)?,self.machine.can_receive_state(c)?));
                }
            } else if (0x640..=0x65f).contains(&frame.id) || frame.id >= 0x7e8 {
                self.events.push(format!("NANO RX not delivered controller={c} id={:03X} reason=controller-reset-filter-or-bit-timing; retained_raw_in_adapter_log=true",frame.id));
            }
            Ok(true)
        })?;
        self.rx_deferred_polls = self.rx_deferred_polls.saturating_add(deferred);
        for (c, tx) in self.machine.take_can_transmissions() {
            if c == 1 {
                self.events.push(format!("Disconnected CAN bus native_channel=2 controller=1 ticket={} id={:03X} data={:02X?}; TX pending without ACK; external_tx=false; error-counter dynamics unimplemented",tx.ticket,tx.frame.id,tx.frame.data));
                continue;
            }
            self.events.push(format!("NANO submit native TX controller={c} ticket={} id={:03X} data={:02X?}; outcome pending",tx.ticket,tx.frame.id,tx.frame.data));
            bridge.transmit(c, tx.clone())?;
            if let Some(m) = &mut self.ignition {
                m.state.transmitted(c, &tx.frame);
            }
        }
        if let Some(m) = &mut self.ignition {
            m.tick();
        }
        Ok(())
    }
    /// Original emulator.exe 0x4483a0: mux latch at 0x600000, ADC PCS=3.
    /// These are virtual cable/reference voltages, never ECU measurements.
    pub fn adc_transfer(&mut self, mux: u8, command: u16) -> u16 {
        match command {
            0x0180 => self.adc_channel = mux & 15,
            0x01c0 => self.adc_channel = mux >> 4,
            _ => {}
        }
        match self.adc_channel {
            15 => 0x0225, // Original cable table index 1 (default at 0x509d58).
            13 => 0x07fe, // Original reference word at 0x509d54.
            _ => 0xffff,  // Original default word at 0x509d50.
        }
    }
    pub fn advance(&mut self) {
        if self.stopped {
            return;
        }
        if let Some((start, cycles)) = self.live_clock {
            let elapsed = start.elapsed();
            let elapsed_cycles = self.machine.elapsed_cycles().saturating_sub(cycles);
            if elapsed.saturating_sub(self.clock_sample_at) >= std::time::Duration::from_secs(1) {
                self.clock_sample_at = elapsed;
                // Observe the existing research clock; do not advance guest
                // timers, inject interrupts, or generate diagnostic requests.
                let virtual_us = u128::from(elapsed_cycles) * 1_000_000 / 16_777_216;
                let lag_us = elapsed.as_micros() as i128 - virtual_us as i128;
                self.events.push(format!("CANDI_CLOCK wall_us={} virtual_us={virtual_us} lag_us={lag_us} cycles={elapsed_cycles} model_hz=16777216 timing_changed=false rx_staged={} rx_deferred_polls={}", elapsed.as_micros(), self.pending_rx.len(), self.rx_deferred_polls));
                self.events.push(format!("CAN_RX_STATE staged_by_controller={:?} hardware_overruns={:?} controllers={:?}", self.pending_rx.depths(), self.rx_overruns, std::array::from_fn::<_, 3, _>(|c| (self.machine.can_interrupt_routed(c), self.machine.can_receive_state(c)))));
            }
            let delay = tech2_emu::can_adapter::realtime_delay(elapsed_cycles, elapsed);
            if !delay.is_zero() {
                std::thread::sleep(delay);
            }
        }
        if let Err(error) = self.poll_bridge() {
            self.events.push(format!("Native adapter stopped: {error}"));
            self.stopped = true;
            if let Some(m) = &mut self.ignition {
                m.close();
            }
            if let Some(bridge) = &mut self.bridge {
                bridge.close();
            }
            return;
        }
        for _ in 0..128 {
            if !self.machine.step(u64::MAX) {
                self.events
                    .push(format!("CPU stopped: {:?}", self.machine.snapshot().reason));
                self.stopped = true;
                break;
            }
        }
        if self.stopped {
            if let Some(m) = &mut self.ignition {
                m.close();
            }
            if let Some(bridge) = &mut self.bridge {
                bridge.close();
            }
        }
        let output = self.machine.take_serial_events();
        self.events.extend(self.machine.take_can_events());
        if !output.is_empty() {
            for event in &output {
                if let Some(byte) = event {
                    if self.rx_frame.len() < 4096 {
                        self.rx_frame.push(*byte);
                    }
                } else {
                    self.events.push(frame_log(false, &self.rx_frame));
                    self.rx_frame.clear();
                }
            }
            self.events.push(format!(
                "CANdi->Tech2 events={output:02x?} origin=candi-firmware"
            ));
            self.receive_events(output);
        }
    }
    fn receive_events(&mut self, output: Vec<Option<u8>>) {
        if output.is_empty() {
            return;
        }
        if self.rx.len() + output.len() > 4096 {
            self.events
                .push("UART receive overflow; link stopped".into());
            self.stopped = true;
        } else {
            // A status read before a character arrives cannot acknowledge its
            // error. The final delimiter can follow the checksum in a later slice.
            if self.rx.is_empty() {
                self.line_ack = false;
            }
            self.rx.extend(output);
        }
    }
    pub fn snapshot(&self) -> tech2_emu::candi_cpu::Snapshot {
        self.machine.snapshot()
    }
    pub fn events(&mut self) -> Vec<String> {
        std::mem::take(&mut self.events)
    }
    pub fn connected(&self) -> bool {
        !self.stopped
    }
    pub fn uart_receive_idle(&self) -> bool {
        self.rx.is_empty()
    }
    fn interrupt_id(&self) -> u8 {
        let ier = self.registers[1];
        if ier & 4 != 0 && self.rx.front() == Some(&None) && !self.line_ack {
            6
        } else if ier & 1 != 0 && !self.rx.is_empty() {
            4
        } else if ier & 2 != 0 && self.tx_irq {
            2
        } else {
            1
        }
    }
    pub fn irq(&self) -> bool {
        self.interrupt_id() & 1 == 0
    }
    pub fn read(&mut self, address: u32) -> u8 {
        let index = ((address >> 1) & 7) as usize;
        if self.registers[3] & 0x80 != 0 && index < 2 {
            return self.divisor[index];
        }
        match index {
            0 => {
                self.line_ack = false;
                let byte = self.rx.pop_front();
                self.events.push(format!(
                    "UART receive read event={byte:02x?} origin=tech2-firmware"
                ));
                byte.flatten().unwrap_or(0)
            }
            2 => {
                let id = self.interrupt_id();
                if id == 2 {
                    self.tx_irq = false;
                }
                id
            }
            5 => {
                let line = if self.rx.front() == Some(&None) && !self.line_ack {
                    0x18
                } else {
                    0
                };
                self.line_ack = true;
                0x60 | u8::from(!self.rx.is_empty()) | line
            }
            _ => self.registers[index],
        }
    }
    fn finish_frame(&mut self, delimiter: &str) {
        let frame = std::mem::take(&mut self.tx);
        self.events.push(frame_log(true, &frame));
        self.events.push(format!(
            "Tech2->CANdi bytes={frame:02x?} end={delimiter} native_pc={:#010x} native_insns={} origin=tech2-firmware",
            self.machine.snapshot().pc, self.machine.attempted()
        ));
        if let Err(e) = self.machine.receive_serial_frame(&frame) {
            self.stopped = true;
            self.events.push(e);
        }
    }
    pub fn write(&mut self, address: u32, value: u8) {
        self.events.push(format!(
            "UART write address={address:#08x} value={value:#04x} origin=tech2-firmware"
        ));
        let index = ((address >> 1) & 7) as usize;
        if self.registers[3] & 0x80 != 0 && index < 2 {
            self.divisor[index] = value;
            return;
        }
        match index {
            0 => {
                if self.registers[3] & 0x3f == 0x3f && value == 0 {
                    // Guest sends zero with space parity (LCR 0x3f): the low
                    // parity bit occupies SCI's stop bit, terminating the frame.
                    self.finish_frame("space-parity-zero");
                } else if self.registers[4] & 0x10 != 0 {
                    if self.rx.len() < 4096 {
                        self.rx.push_back(Some(value));
                    }
                } else if self.tx.len() < 4096 {
                    self.tx.push(value);
                } else {
                    self.stopped = true;
                    self.events
                        .push("UART transmit overflow; link stopped".into());
                }
                self.tx_irq = true;
            }
            1 => {
                self.tx_irq |= value & 2 != 0 && self.registers[1] & 2 == 0;
                self.registers[1] = value;
            }
            2 => {
                if value & 2 != 0 {
                    self.rx.clear();
                    self.line_ack = false;
                }
            }
            3 => {
                if value & 0x40 != 0 && self.registers[3] & 0x40 == 0 {
                    self.finish_frame("break");
                }
                self.registers[3] = value;
            }
            _ => self.registers[index] = value,
        }
    }
}

/// Observation only: preserve every byte; labels never change transport behavior.
fn frame_log(tx: bool, bytes: &[u8]) -> String {
    let valid = bytes.len() >= 2 && bytes.iter().fold(0u8, |s, b| s.wrapping_add(*b)) == 0;
    let label = if !valid {
        "unverified frame"
    } else {
        match bytes {
            // MSI dispatch table 0x1a5fe: command 0x28, channel bits 0x06,
            // handler 0x11ab8 -> 0x11892 reads the native receive queue.
            [0x2e, 0xd2] => "CANdi receive-queue poll (all channels)",
            [0x2f, 0xd2, 0xff] => "CANdi receive queue empty",
            [0x90, 0x03, ..] => "Device information request",
            [0x91, 0x03, ..] => "Device information reply",
            [0x80, 0x09, ..] => "Serial baud-rate request",
            [0x81, 0x09, ..] => "Serial baud-rate reply",
            [0x80, 0x0c, ..] => "Firmware inventory request",
            [0x81, 0x0c, ..] => "Firmware inventory reply",
            [0x80, 0x01, ..] => "CANdi memory read request",
            [0x81, 0x01, ..] => "CANdi memory read reply",
            [0x80, 0x08, ..] => "CANdi application start request",
            [0x81, 0x08, ..] => "CANdi application start reply",
            // Native dispatch 0x120c2 -> 0x13f70, captured CAN ID 0x100.
            // GMW3110 (2010) 5.2.4 identifies the SWCAN wake-up frame.
            [0x88, 0x0e, 0x6a] => "CANdi channel 0 wake-up request",
            // Native firmware emits this before SJA1000 TCS. It confirms the
            // serial command, not physical CAN transmission or a vehicle reply.
            [0x89, 0x0e, 0x00, 0x69] => "CANdi wake-up command accepted; CAN TX unconfirmed",
            _ => "Unclassified CANdi command",
        }
    };
    let hex = bytes
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        "FRAME {} | {label} | {} bytes | checksum={} | {hex} | virtual serial; not ECU CAN",
        if tx {
            "Tech2 -> CANdi"
        } else {
            "CANdi -> Tech2"
        },
        bytes.len(),
        if valid { "OK" } else { "BAD/SHORT" }
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn frame_logging_preserves_bytes_and_does_not_label_bad_checksums() {
        let info = frame_log(true, &[0x90, 3, 0x6d]);
        assert!(info.contains("Tech2 -> CANdi"));
        assert!(info.contains("90 03 6D"));
        assert!(info.contains("checksum=OK"));
        let wake_ack = frame_log(false, &[0x89, 0x0e, 0, 0x69]);
        assert!(wake_ack.contains("CAN TX unconfirmed"));
        assert!(wake_ack.contains("89 0E 00 69"));
        assert!(!frame_log(false, &[0x89, 0x0e, 0, 0x68]).contains("command accepted"));
        let bad = frame_log(true, &[0x90, 3, 0]);
        assert!(bad.contains("unverified frame"));
        assert!(!bad.contains("information request"));
        assert!(bad.contains("not ECU CAN"));
        assert!(frame_log(true, &[0x2e, 0xd2]).contains("checksum=OK"));
        assert!(frame_log(false, &[0x2f, 0xd2, 0xff]).contains("queue empty"));
        assert!(frame_log(true, &[0]).contains("BAD/SHORT"));
    }

    #[cfg(feature = "gui")]
    #[test]
    #[ignore = "requires the untracked local MSI CANdi firmware"]
    fn checkpoint_restores_both_cpu_and_pending_serial_state() {
        let mut link = NativeLink::new(Path::new("dumps/candi/candi.bin")).unwrap();
        for _ in 0..800 {
            link.advance();
        }
        link.write(0x400800, 0x90);
        let mut saved = link.checkpoint().unwrap();
        assert_eq!(saved.tx, [0x90]);
        for candidate in [&mut link, &mut saved] {
            candidate.write(0x400800, 3);
            candidate.write(0x400800, 0x6d);
            candidate.write(0x400806, 0x3f);
            candidate.write(0x400800, 0);
            for _ in 0..5000 {
                candidate.advance();
            }
        }
        assert_eq!(link.snapshot().pc, saved.snapshot().pc);
        assert_eq!(link.snapshot().serial_tx, saved.snapshot().serial_tx);
        assert_eq!(link.rx, saved.rx);
        assert_eq!(link.snapshot().serial_tx.len(), 24);
    }

    #[test]
    #[ignore = "requires the untracked local MSI CANdi firmware"]
    fn local_uart_registers_and_cable_adc() {
        let mut link = NativeLink::new(Path::new("dumps/candi/candi.bin")).unwrap();
        assert_eq!(link.adc_transfer(0xfd, 0x180), 0x7fe);
        assert_eq!(link.adc_transfer(0xfd, 0x1c0), 0x225);
        assert_eq!(link.adc_transfer(0, 0), 0x225); // Sample command retains mux.
        link.write(0x400806, 0x80);
        link.write(0x400800, 26);
        assert_eq!(link.read(0x400800), 26);
        assert!(link.tx.is_empty());
        link.write(0x400806, 7);
        link.write(0x400802, 2);
        assert!(link.irq());
        assert_eq!(link.read(0x400804), 2);
        assert!(!link.irq());
        link.write(0x400808, 0x10);
        link.write(0x400800, 0x42);
        assert_eq!(link.read(0x400800), 0x42);
        assert!(link.tx.is_empty());
        link.rx.push_back(None);
        link.write(0x400802, 5);
        assert_eq!(link.read(0x400804), 6);
        assert_ne!(link.read(0x40080a) & 0x10, 0);
        assert_eq!(link.read(0x400804), 4);
        assert_eq!(link.read(0x400800), 0);
        assert!(!link.irq());
        // Regression: checksum consumed, LSR polled empty, break arrives later.
        assert_eq!(link.read(0x40080a), 0x60);
        link.receive_events(vec![None]);
        assert_eq!(link.read(0x400804), 6);
        assert_eq!(link.read(0x40080a) & 0x18, 0x18);
        link.read(0x400800);
        assert!(!link.irq());
    }
}
