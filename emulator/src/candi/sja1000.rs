// SPDX-License-Identifier: MPL-2.0
//! Bounded SJA1000-compatible register model with opt-in frame transport.
//! Inferred CANdi windows: native helper 0x143aa. Register semantics: NXP
//! SJA1000 datasheet (2000-01-04), sections 6.3-6.5. Default stops on TX.
//! Async completion requires a dispatched ticket confirmed by the backend.

use std::collections::VecDeque;

/// Electrical context captured when firmware requests TX. GPIO values are
/// virtual register state, not measured voltage or a verified board pinout.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ElectricalState {
    #[default]
    Unspecified,
    StandardCan,
    SingleWireGpio {
        latch: u8,
        assignment: u8,
        direction: u8,
    },
}

impl ElectricalState {
    pub fn wake_control_selected(self) -> bool {
        matches!(self, Self::SingleWireGpio { latch, assignment: 0, direction }
            if latch & 3 == 2 && direction & 3 == 3)
    }
}

impl std::fmt::Display for ElectricalState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unspecified => write!(f, "unspecified"),
            Self::StandardCan => write!(f, "standard-can"),
            Self::SingleWireGpio { latch, assignment, direction } => write!(f,
                "single-wire-gpio latch={latch:#04x} assignment={assignment:#04x} direction={direction:#04x} wake_control_selected={} board_mapping=unverified",
                self.wake_control_selected()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub id: u32,
    pub extended: bool,
    pub rtr: bool,
    pub dlc: u8,
    pub data: Vec<u8>,
}
impl Frame {
    pub fn validate(&self) -> Result<(), String> {
        if self.id > if self.extended { 0x1fffffff } else { 0x7ff }
            || self.dlc > 8
            || self.data.len() != if self.rtr { 0 } else { self.dlc as usize }
        {
            return Err("Invalid classical CAN frame".into());
        }
        Ok(())
    }
    fn encode(&self) -> Vec<u8> {
        let mut out =
            vec![self.dlc | if self.extended { 0x80 } else { 0 } | if self.rtr { 0x40 } else { 0 }];
        if self.extended {
            out.extend_from_slice(&[
                (self.id >> 21) as u8,
                (self.id >> 13) as u8,
                (self.id >> 5) as u8,
                (self.id << 3) as u8,
            ]);
        } else {
            out.extend_from_slice(&[(self.id >> 3) as u8, (self.id << 5) as u8]);
        }
        out.extend_from_slice(&self.data);
        out
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Transmission {
    pub ticket: u64,
    pub frame: Frame,
    pub btr0: u8,
    pub btr1: u8,
    pub electrical: ElectricalState,
}

/// Side-effect-free receive state for transport diagnostics. In particular,
/// observing this must not acknowledge the interrupt register.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReceiveState {
    pub mode: u8,
    pub interrupt_enable: u8,
    pub status: u8,
    pub queued_messages: usize,
    pub queued_bytes: usize,
    pub bit_timing: (u8, u8),
}

#[derive(Clone)]
pub(super) struct Controller {
    registers: [u8; 32],
    filter: [u8; 8],
    ram: [u8; 80],
    tx: [u8; 13],
    transport: bool,
    next_ticket: u64,
    pending: Option<Transmission>,
    dispatched: bool,
    cancelled: VecDeque<u64>,
    rx: VecDeque<Vec<u8>>,
    rx_bytes: usize,
    electrical: ElectricalState,
}

impl Default for Controller {
    fn default() -> Self {
        let mut registers = [0; 32];
        registers[0] = 1;
        registers[2] = 0x3c;
        registers[13] = 96;
        registers[31] = 5; // Motorola interface reset clock divider.
        Self {
            registers,
            filter: [0; 8],
            ram: [0; 80],
            tx: [0; 13],
            transport: false,
            next_ticket: 1,
            pending: None,
            dispatched: false,
            cancelled: VecDeque::new(),
            rx: VecDeque::new(),
            rx_bytes: 0,
            electrical: ElectricalState::Unspecified,
        }
    }
}

impl Controller {
    pub fn receive_state(&self) -> ReceiveState {
        ReceiveState {
            mode: self.registers[0],
            interrupt_enable: self.registers[4],
            status: self.registers[2],
            queued_messages: self.rx.len(),
            queued_bytes: self.rx_bytes,
            bit_timing: self.bit_timing(),
        }
    }
    fn pelican(&self) -> bool {
        self.registers[31] & 0x80 != 0
    }
    fn reset(&self) -> bool {
        self.registers[0] & 1 != 0
    }

    pub fn bit_timing(&self) -> (u8, u8) {
        (self.registers[6], self.registers[7])
    }

    pub fn read(&mut self, offset: u8) -> Option<u8> {
        let i = usize::from(offset);
        if offset == 0 {
            return Some(self.registers[0] | if self.pelican() { 0 } else { 0xe0 });
        }
        if !self.pelican() {
            return match offset {
                1 => Some(0xff),
                6..=8 if !self.reset() => Some(0xff),
                2..=8 | 31 => Some(self.registers[i]),
                _ => None,
            };
        }
        match offset {
            // Factory test mode is never enabled. Native 0x13b5e scans every
            // register and discards this value. Zero is an explicit virtual
            // test-register value, not a claimed physical reset measurement.
            9 => Some(0),
            3 => {
                let flags = self.registers[3]
                    | if !self.rx.is_empty() && self.registers[4] & 1 != 0 {
                        1
                    } else {
                        0
                    };
                self.registers[3] = 0;
                Some(flags)
            }
            29 => Some(self.rx.len() as u8),
            1 | 5 | 10 | 11 | 12 | 112..=127 => Some(0),
            2 | 4 | 6..=8 | 13..=15 | 30 | 31 => Some(self.registers[i]),
            16..=23 if self.reset() => Some(self.filter[i - 16]),
            24..=28 if self.reset() => Some(0),
            // Empty receive FIFO: contents are explicitly uninitialized virtual
            // RAM, with RBS/RMC zero; never expose the TX buffer as received data.
            16..=28 => Some(
                self.rx
                    .front()
                    .and_then(|f| f.get(i - 16))
                    .copied()
                    .unwrap_or(0),
            ),
            32..=111 => Some(self.ram[i - 32]),
            _ => None,
        }
    }

    /// Error stops native execution. A TX request includes its composed frame.
    pub fn write(&mut self, offset: u8, value: u8) -> Result<(), String> {
        let i = usize::from(offset);
        match offset {
            0 => {
                let was_reset = self.reset();
                self.registers[0] = value & 0x1f;
                if self.reset() {
                    self.cancel_pending();
                    self.rx.clear();
                    self.rx_bytes = 0;
                    self.registers[3] = 0;
                }
                // Virtual bus is recessive with no external traffic. No TX has
                // occurred; reset TCS is retained until the first TX request.
                if self.reset() {
                    self.registers[2] = 0x3c;
                } else if was_reset {
                    self.registers[2] = 0x0c;
                }
                Ok(())
            }
            31 if self.reset() => {
                self.registers[31] = value & !0x10;
                Ok(())
            }
            1 => self.command(value),
            4 if self.pelican() => {
                self.registers[i] = value;
                Ok(())
            }
            4..=8 if !self.pelican() && !self.reset() => Ok(()), // BasicCAN configuration is reset-only.
            4..=8 if self.reset() => {
                self.registers[i] = value;
                Ok(())
            }
            13..=15 if self.pelican() && self.reset() => {
                self.registers[i] = value;
                Ok(())
            }
            16..=23 if self.pelican() && self.reset() => {
                self.filter[i - 16] = value;
                Ok(())
            }
            30 if self.pelican() && self.reset() => {
                self.registers[30] = value & 63;
                Ok(())
            }
            16..=28 if self.pelican() && !self.reset() => {
                if self.pending.is_some() {
                    return Ok(());
                }
                self.tx[i - 16] = value;
                self.ram[64 + i - 16] = value;
                Ok(())
            }
            32..=111 if self.pelican() && self.reset() => {
                self.ram[i - 32] = value;
                Ok(())
            }
            _ => Err(format!(
                "unsupported SJA1000 register write offset={offset:#04x} value={value:#04x}"
            )),
        }
    }

    fn command(&mut self, value: u8) -> Result<(), String> {
        if value & 0xe0 != 0 {
            return Err("Unsupported CAN command bits".into());
        }
        if value & 0x11 == 0x10 {
            return Err("SJA1000 self reception is not implemented".into());
        }
        if value & 3 == 3 {
            return Err("SJA1000 single-shot transmission is not implemented".into());
        }
        if value & 1 != 0 {
            if !self.transport {
                return Err(format!(
                    "CAN TX requested; backend absent; {}",
                    self.tx_description()
                ));
            }
            if !self.pelican()
                || self.reset()
                || self.registers[0] & 0x16 != 0
                || self.pending.is_some()
            {
                return Err("CAN TX unavailable: controller mode or pending transmission".into());
            }
            let frame = self.tx_frame();
            frame.validate()?;
            let ticket = self.next_ticket;
            self.next_ticket = self
                .next_ticket
                .checked_add(1)
                .ok_or("CAN transmission ticket exhausted")?;
            self.pending = Some(Transmission {
                ticket,
                frame,
                btr0: self.registers[6],
                btr1: self.registers[7],
                electrical: self.electrical,
            });
            self.dispatched = false;
            self.registers[2] = (self.registers[2] & !0x0c) | 0x20;
        }
        if value & 2 != 0 && self.cancel_pending() {
            self.registers[2] = (self.registers[2] & !0x28) | 4;
            self.registers[3] |= self.registers[4] & 2; // Buffer released, but TCS remains clear.
        }
        if value & 4 != 0 {
            if let Some(frame) = self.rx.pop_front() {
                self.rx_bytes -= frame.len();
                self.registers[30] = (self.registers[30] + frame.len() as u8) & 63;
            }
            if self.rx.is_empty() {
                self.registers[2] &= !1;
            }
        }
        if value & 8 != 0 {
            self.registers[2] &= !2;
        }
        Ok(())
    }

    /// Retain only dispatched cancellations: a real driver indication may
    /// arrive after the guest aborts/resets. It must not complete a newer TX.
    fn cancel_pending(&mut self) -> bool {
        let pending = self.pending.take();
        if let Some(tx) = &pending {
            if self.dispatched {
                if self.cancelled.len() == 64 {
                    self.cancelled.pop_front();
                }
                self.cancelled.push_back(tx.ticket);
            }
        }
        self.dispatched = false;
        pending.is_some()
    }

    pub fn enable_transport(&mut self) {
        self.transport = true;
    }
    /// Updating pins after a command must not change that pending command's snapshot.
    pub fn set_electrical(&mut self, state: ElectricalState) {
        self.electrical = state;
    }
    pub fn transport_enabled(&self) -> bool {
        self.transport
    }
    pub fn take_transmission(&mut self) -> Option<Transmission> {
        if self.dispatched {
            return None;
        }
        self.dispatched = true;
        self.pending.clone()
    }
    /// Only a backend-confirmed completion may set TCS/TI. Driver acceptance
    /// alone must not call this. Reset/abort invalidates an outstanding ticket.
    /// Returns false for one known cancelled ticket, without changing status.
    /// Unknown, duplicate and undispatched completions remain errors.
    pub fn complete(&mut self, ticket: u64) -> Result<bool, String> {
        if let Some(index) = self.cancelled.iter().position(|t| *t == ticket) {
            self.cancelled.remove(index);
            return Ok(false);
        }
        if !self.dispatched || !self.pending.as_ref().is_some_and(|p| p.ticket == ticket) {
            return Err("Stale CAN transmission completion".into());
        }
        self.pending = None;
        self.registers[2] = (self.registers[2] & !0x20) | 0x0c;
        self.registers[3] |= self.registers[4] & 2;
        Ok(true)
    }
    pub fn interrupt(&self) -> bool {
        self.registers[3] != 0 || (!self.rx.is_empty() && self.registers[4] & 1 != 0)
    }
    fn accepts(&self, frame: &Frame) -> Result<bool, String> {
        frame.validate()?;
        if !self.transport {
            return Err("CAN RX transport is disabled".into());
        }
        if self.reset() {
            return Ok(false);
        }
        if self.registers[0] & 0x14 != 0 {
            return Err("CAN RX in sleep/self-test mode is not implemented".into());
        }
        if !self.pelican() || self.registers[0] & 8 == 0 {
            return Err(format!(
                "CAN RX requires PeliCAN single acceptance filter; mode={:#04x} clock={:#04x}",
                self.registers[0], self.registers[31]
            ));
        }
        let (key, mut valid) = if frame.extended {
            (
                [
                    (frame.id >> 21) as u8,
                    (frame.id >> 13) as u8,
                    (frame.id >> 5) as u8,
                    ((frame.id << 3) as u8) | if frame.rtr { 4 } else { 0 },
                ],
                [255, 255, 255, 252],
            )
        } else {
            (
                [
                    (frame.id >> 3) as u8,
                    ((frame.id << 5) as u8) | if frame.rtr { 16 } else { 0 },
                    frame.data.first().copied().unwrap_or(0),
                    frame.data.get(1).copied().unwrap_or(0),
                ],
                [255, 240, 255, 255],
            )
        };
        if !frame.extended {
            if frame.data.is_empty() {
                valid[2] = 0;
            }
            if frame.data.len() < 2 {
                valid[3] = 0;
            }
        }
        if (0..4).any(|i| (key[i] ^ self.filter[i]) & !self.filter[i + 4] & valid[i] != 0) {
            return Ok(false);
        }
        Ok(true)
    }
    /// Driver batching is not a physical arrival at this controller. The live
    /// bridge can retain an accepted frame outside the hardware FIFO until the
    /// guest has room; direct receive still models a real hardware overrun.
    pub fn receive_would_overrun(&self, frame: &Frame) -> Result<bool, String> {
        Ok(self.accepts(frame)? && self.rx_bytes + frame.encode().len() > 64)
    }
    pub fn receive(&mut self, frame: &Frame) -> Result<bool, String> {
        if !self.accepts(frame)? {
            return Ok(false);
        }
        let bytes = frame.encode();
        if self.rx_bytes + bytes.len() > 64 {
            if self.registers[2] & 2 == 0 {
                self.registers[3] |= self.registers[4] & 8;
            }
            self.registers[2] |= 2;
            // NXP 6.4.14: hardware drops this message and signals DOS/DOI;
            // it does not halt the attached CPU or discard the older FIFO.
            return Ok(false);
        }
        for (i, byte) in bytes.iter().enumerate() {
            self.ram[(usize::from(self.registers[30]) + self.rx_bytes + i) & 63] = *byte;
        }
        self.rx_bytes += bytes.len();
        self.rx.push_back(bytes);
        self.registers[2] |= 1;
        Ok(true)
    }
    fn tx_frame(&self) -> Frame {
        let extended = self.tx[0] & 0x80 != 0;
        let rtr = self.tx[0] & 0x40 != 0;
        let dlc = self.tx[0] & 15;
        let (id, start) = if extended {
            (
                (u32::from(self.tx[1]) << 21)
                    | (u32::from(self.tx[2]) << 13)
                    | (u32::from(self.tx[3]) << 5)
                    | (u32::from(self.tx[4]) >> 3),
                5,
            )
        } else {
            (
                (u32::from(self.tx[1]) << 3) | (u32::from(self.tx[2]) >> 5),
                3,
            )
        };
        let length = if rtr { 0 } else { dlc.min(8) as usize };
        Frame {
            id,
            extended,
            rtr,
            dlc,
            data: self.tx[start..start + length].to_vec(),
        }
    }
    pub fn tx_description(&self) -> String {
        if !self.pelican() {
            return "BasicCAN TX decoding not implemented".into();
        }
        let f = self.tx_frame();
        format!("id={:#010x} extended={} rtr={} dlc={} data={:02x?} btr0={:#04x} btr1={:#04x} external_tx=false",f.id,f.extended,f.rtr,f.dlc,f.data,self.registers[6],self.registers[7])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn connected() -> Controller {
        let mut c = Controller::default();
        c.enable_transport();
        c.write(31, 0xc7).unwrap();
        c.write(6, 0xc1).unwrap();
        c.write(7, 0x36).unwrap();
        for offset in 20..24 {
            c.write(offset, 0xff).unwrap();
        }
        c.write(4, 0x0b).unwrap(); // Receive, transmit, overrun.
        c.write(0, 8).unwrap();
        c
    }
    fn response() -> Frame {
        // Captured live Nano response, 2026-09-07; fixture only, never a live reply.
        Frame {
            id: 0x7e8,
            extended: false,
            rtr: false,
            dlc: 8,
            data: vec![0x10, 0x13, 0x5a, 0x90, 0x59, 0x53, 0x33, 0x46],
        }
    }
    fn request(c: &mut Controller) {
        let f = Frame {
            id: 0x7e0,
            data: vec![2, 0x1a, 0x90, 0, 0, 0, 0, 0],
            ..response()
        };
        for (i, b) in f.encode().into_iter().enumerate() {
            c.write(16 + i as u8, b).unwrap();
        }
    }
    #[test]
    fn only_dispatched_matching_confirmation_completes_tx() {
        let mut c = connected();
        request(&mut c);
        c.write(1, 1).unwrap();
        assert_eq!(c.read(2).unwrap() & 0x2c, 0x20);
        assert!(!c.interrupt());
        assert!(c.complete(1).is_err()); // Not handed to backend yet.
        let tx = c.take_transmission().unwrap();
        assert_eq!((tx.frame.id, tx.btr0, tx.btr1), (0x7e0, 0xc1, 0x36));
        assert!(c.take_transmission().is_none());
        c.write(0, 8).unwrap(); // No reset transition: cannot clear busy status.
        assert_eq!(c.read(2).unwrap() & 0x2c, 0x20);
        assert!(c.complete(tx.ticket + 1).is_err());
        c.complete(tx.ticket).unwrap();
        assert_eq!(c.read(2).unwrap() & 0x2c, 0x0c);
        assert!(c.interrupt());
        assert_eq!(c.read(3), Some(2));
        assert!(!c.interrupt());
        assert_eq!(c.read(3), Some(0));
        assert!(c.complete(tx.ticket).is_err());
    }
    #[test]
    fn abort_and_reset_invalidate_outstanding_tickets() {
        let mut c = connected();
        request(&mut c);
        c.write(1, 1).unwrap();
        let old = c.take_transmission().unwrap();
        c.write(1, 2).unwrap();
        assert_eq!(c.read(2).unwrap() & 0x2c, 4); // Released != successfully sent.
        assert_eq!(c.read(3), Some(2));
        assert!(!c.complete(old.ticket).unwrap());
        assert!(c.complete(old.ticket).is_err());
        request(&mut c);
        c.write(1, 1).unwrap();
        let next = c.take_transmission().unwrap();
        assert!(next.ticket > old.ticket);
        c.write(0, 9).unwrap();
        assert!(!c.complete(next.ticket).unwrap());
        assert!(c.complete(next.ticket).is_err());
        assert!(!c.interrupt());
        c.write(0, 8).unwrap();
        assert!(c.write(1, 3).unwrap_err().contains("single-shot"));
        assert!(c.take_transmission().is_none());
    }
    #[test]
    fn late_cancelled_completion_cannot_complete_a_new_transmission() {
        let mut c = connected();
        request(&mut c);
        c.write(1, 1).unwrap();
        let old = c.take_transmission().unwrap();
        c.write(1, 2).unwrap();
        c.read(3); // consume abort's buffer-release interrupt
        request(&mut c);
        c.write(1, 1).unwrap();
        let new = c.take_transmission().unwrap();
        let before = c.registers;
        assert!(!c.complete(old.ticket).unwrap());
        assert_eq!(c.registers, before);
        assert_eq!(c.pending.as_ref().unwrap().ticket, new.ticket);
        assert!(!c.interrupt());
        assert!(c.complete(old.ticket).is_err());
        assert!(c.complete(new.ticket).unwrap());
        assert_eq!(c.read(2).unwrap() & 0x2c, 0x0c);
    }
    #[test]
    fn cancelling_an_undispatched_request_does_not_authorize_a_completion() {
        let mut c = connected();
        request(&mut c);
        c.write(1, 1).unwrap();
        c.write(1, 2).unwrap();
        assert!(c.complete(1).is_err());
    }
    #[test]
    fn receive_fifo_interrupt_and_direct_ram_follow_release() {
        let mut c = connected();
        let f = response();
        assert!(c.receive(&f).unwrap());
        assert!(c.receive(&f).unwrap());
        assert_eq!(c.read(29), Some(2));
        assert_eq!(c.read(3), Some(1));
        assert_eq!(c.read(3), Some(1));
        for (i, b) in f.encode().iter().enumerate() {
            assert_eq!(c.read(16 + i as u8), Some(*b));
            assert_eq!(c.read(32 + i as u8), Some(*b));
        }
        c.write(1, 4).unwrap();
        assert_eq!(c.read(30), Some(11));
        assert_eq!(c.read(29), Some(1));
        assert!(c.interrupt());
        c.write(4, 0).unwrap();
        assert!(!c.interrupt());
        c.write(4, 1).unwrap();
        assert!(c.interrupt());
        c.write(1, 4).unwrap();
        assert!(!c.interrupt());
        assert_eq!(c.read(29), Some(0));
        c.write(0, 9).unwrap();
        c.write(30, 60).unwrap();
        c.write(0, 8).unwrap();
        c.receive(&f).unwrap();
        assert_eq!(c.read(92), Some(8));
        assert_eq!(c.read(32), Some(0x13)); // Frame wraps at RAM byte 63.
        c.write(0, 9).unwrap();
        assert_eq!(c.read(29), Some(0));
        assert_eq!(c.read(30), Some(60));
    }
    #[test]
    fn acceptance_filter_rejects_other_ids_and_fifo_overrun_is_bounded() {
        let mut c = connected();
        c.write(0, 9).unwrap();
        c.write(16, 0xfd).unwrap();
        c.write(17, 0).unwrap(); // 7E8, data frame.
        c.write(20, 0).unwrap();
        c.write(21, 0x0f).unwrap();
        c.write(0, 8).unwrap();
        let mut f = response();
        f.id = 0x7e9;
        assert!(!c.receive(&f).unwrap());
        f.id = 0x7e8;
        f.rtr = true;
        f.data.clear();
        assert!(!c.receive(&f).unwrap());
        let f = response();
        for _ in 0..5 {
            assert!(c.receive(&f).unwrap());
        }
        assert!(!c.receive(&f).unwrap());
        assert_eq!(c.read(29), Some(5));
        assert_eq!(c.read(3), Some(9));
        assert!(!c.receive(&f).unwrap());
        assert_eq!(c.read(3), Some(1)); // No repeated DOI until cleared.
        c.write(1, 12).unwrap();
        assert_eq!(c.read(2).unwrap() & 2, 0);
        assert!(c.receive(&f).unwrap());
        assert_eq!(c.read(29), Some(5));
    }
    #[test]
    fn driver_batch_can_wait_for_fifo_space_without_losing_diagnostic_reply() {
        let mut c = connected();
        let mut input: VecDeque<_> = (0..32)
            .map(|i| {
                let mut f = response();
                f.id = if i == 17 { 0x647 } else { 0x110 };
                f.data = vec![i; 8];
                f
            })
            .collect();
        let expected: Vec<_> = input.iter().map(|f| (f.id, f.data[0])).collect();
        let mut received = Vec::new();
        let mut deferred = 0;
        while !input.is_empty() || c.read(29).unwrap() != 0 {
            while let Some(f) = input.front() {
                if c.receive_would_overrun(f).unwrap() {
                    deferred += 1;
                    break;
                }
                assert!(c.receive(&input.pop_front().unwrap()).unwrap());
            }
            assert!(c.read(29).unwrap() <= 5);
            assert_eq!(c.read(2).unwrap() & 2, 0); // Capacity query is side-effect free.
            let id = (u32::from(c.read(17).unwrap()) << 3) | (u32::from(c.read(18).unwrap()) >> 5);
            received.push((id, c.read(19).unwrap()));
            c.write(1, 4).unwrap(); // Original guest releases the oldest message.
        }
        assert!(deferred > 0);
        assert_eq!(received, expected);
    }
    #[test]
    fn extended_filter_and_malformed_frames_are_checked() {
        let mut c = connected();
        c.write(0, 9).unwrap();
        let f = Frame {
            id: 0x18daf110,
            extended: true,
            rtr: false,
            dlc: 2,
            data: vec![0x5a, 0x90],
        };
        for (i, b) in f.encode()[1..5].iter().enumerate() {
            c.write(16 + i as u8, *b).unwrap();
            c.write(20 + i as u8, 0).unwrap();
        }
        c.write(0, 8).unwrap();
        assert!(c.receive(&f).unwrap());
        assert!(!c
            .receive(&Frame {
                id: f.id + 1,
                ..f.clone()
            })
            .unwrap());
        for bad in [
            Frame {
                id: 0x20000000,
                ..f.clone()
            },
            Frame {
                dlc: 9,
                ..f.clone()
            },
            Frame {
                data: vec![],
                ..f.clone()
            },
            Frame {
                rtr: true,
                ..f.clone()
            },
        ] {
            assert!(c.receive(&bad).is_err());
        }
        assert_eq!(c.read(29), Some(1));
    }

    #[test]
    fn configuration_is_separate_from_tx_and_empty_rx() {
        let mut c = Controller::default();
        assert_eq!(c.read(0), Some(0xe1));
        c.write(31, 0xc7).unwrap();
        c.write(16, 0xaa).unwrap();
        c.write(6, 1).unwrap();
        c.write(0, 0).unwrap();
        c.write(16, 3).unwrap();
        c.write(17, 0x4a).unwrap();
        c.write(18, 0x80).unwrap(); // CAN ID 0x254.
        for (i, byte) in [0x01, 0x02, 0x03].into_iter().enumerate() {
            c.write(19 + i as u8, byte).unwrap();
        }
        assert_eq!(c.read(16), Some(0));
        assert_eq!(c.read(29), Some(0));
        let error = c.write(1, 1).unwrap_err();
        assert!(error.contains("id=0x00000254"));
        assert!(error.contains("data=[01, 02, 03]"));
        assert!(error.contains("backend absent"));
        c.write(0, 1).unwrap();
        assert_eq!(c.read(16), Some(0xaa));
        assert!(c.write(9, 1).is_err());
        assert_eq!(c.read(128), None);
        c.write(0, 0).unwrap();
        c.write(16, 0xc8).unwrap(); // Extended remote frame, DLC eight, no data.
        for (i, byte) in [0xff, 0xff, 0xff, 0xf8].into_iter().enumerate() {
            c.write(17 + i as u8, byte).unwrap();
        }
        let error = c.write(1, 1).unwrap_err();
        assert!(error.contains("id=0x1fffffff"));
        assert!(error.contains("rtr=true dlc=8 data=[]"));
    }
}
