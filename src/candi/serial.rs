// SPDX-License-Identifier: MPL-2.0
//! Bounded virtual SCI wire for local CANdi experiments. Not CAN/ECU RX.
use std::collections::VecDeque;

#[derive(Clone, Default)]
pub(crate) struct Serial {
    input: VecDeque<Option<u8>>, // None is a received line break.
    rx: u8,
    flags: u16,
    read_flags: u16,
    next_rx: u64,
    tx: Option<(u8, u64)>,
    output: Vec<u8>,
    events: VecDeque<Option<u8>>,
    pending_break: bool,
    pub received: u64,
    pub breaks: u64,
    pub transmitted: u64,
}

impl Serial {
    pub fn queue(&mut self, bytes: &[u8]) -> Result<(), String> {
        if self.input.len() + bytes.len() + 1 > 4096 {
            return Err("CANdi virtual serial input queue is full".into());
        }
        self.input.extend(bytes.iter().copied().map(Some));
        self.input.push_back(None);
        Ok(())
    }
    pub fn status(&self) -> u16 {
        self.flags | if self.tx.is_none() { 0x180 } else { 0 }
    }
    pub fn read_status(&mut self) -> u16 {
        self.read_flags = self.flags;
        self.status()
    }
    pub fn read_data(&mut self) -> u8 {
        self.flags &= !self.read_flags;
        self.read_flags = 0;
        self.rx
    }
    pub fn write_data(&mut self, value: u8, now: u64, character_cycles: u64) -> bool {
        if self.tx.is_some() || self.events.len() >= 4095 {
            return false;
        }
        self.tx = Some((value, now.saturating_add(character_cycles)));
        true
    }
    pub fn advance(&mut self, now: u64, control: u16, character_cycles: u64) {
        if let Some((byte, deadline)) = self.tx {
            if now >= deadline {
                self.tx = None;
                // Preserve the bounded startup-history prefix without turning
                // diagnostic recording capacity into a hardware TX failure.
                // The drained event stream retains every subsequent byte.
                if self.output.len() < 4096 {
                    self.output.push(byte);
                }
                self.events.push_back(Some(byte));
                self.transmitted += 1;
            }
        }
        if self.pending_break && self.tx.is_none() && self.events.len() < 4096 {
            self.events.push_back(None);
            self.pending_break = false;
        }
        if control & 4 == 0 || now < self.next_rx || self.flags & 0x40 != 0 {
            return;
        }
        if let Some(event) = self.input.pop_front() {
            self.rx = event.unwrap_or(0);
            self.flags |= 0x40 | if event.is_none() { 2 } else { 0 };
            self.received += u64::from(event.is_some());
            self.breaks += u64::from(event.is_none());
            self.next_rx = now.saturating_add(character_cycles);
        }
    }
    pub fn interrupt(&self, control: u16) -> bool {
        let status = self.status();
        (control & 0x20 != 0 && status & 0x48 != 0)
            || (control & 0x80 != 0 && status & 0x100 != 0)
            || (control & 0x40 != 0 && status & 0x80 != 0)
    }
    pub fn send_break(&mut self) {
        self.pending_break = true;
    }
    pub fn take_events(&mut self) -> Vec<Option<u8>> {
        self.events.drain(..).collect()
    }
    pub fn output(&self) -> &[u8] {
        &self.output
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recording_limit_does_not_stop_a_drained_serial_wire() {
        let mut s = Serial::default();
        for n in 0..5000u64 {
            assert!(s.write_data(n as u8, n * 2, 1));
            s.advance(n * 2 + 1, 0, 1);
            assert_eq!(s.take_events(), vec![Some(n as u8)]);
        }
        assert_eq!(s.transmitted, 5000);
        assert_eq!(s.output().len(), 4096);
        assert_eq!(s.received, 0);
        for n in 0..4095u64 {
            assert!(s.write_data(1, 10000 + n * 2, 1));
            s.advance(10001 + n * 2, 0, 1);
        }
        assert!(!s.write_data(1, 20000, 1)); // Undrained wire still backpressures.
    }
    #[test]
    fn receive_requires_enable_and_status_then_data_acknowledgement() {
        let mut s = Serial::default();
        s.queue(&[0x90]).unwrap();
        s.advance(1, 0, 10);
        assert_eq!(s.status(), 0x180);
        s.advance(1, 4, 10);
        assert_eq!(s.read_data(), 0x90);
        assert!(s.interrupt(0x24));
        assert_eq!(s.read_status(), 0x1c0);
        assert_eq!(s.read_data(), 0x90);
        assert!(!s.interrupt(0x24));
        s.advance(11, 4, 10);
        assert_eq!(s.read_status(), 0x1c2);
    }
    #[test]
    fn transmit_is_timed_bounded_and_never_echoed_into_receive() {
        let mut s = Serial::default();
        assert!(s.write_data(0x91, 10, 20));
        assert!(!s.write_data(0, 10, 20));
        s.advance(29, 4, 20);
        assert!(s.output().is_empty());
        s.advance(30, 4, 20);
        assert_eq!(s.output(), &[0x91]);
        assert_eq!(s.status(), 0x180);
        assert_eq!(s.received, 0);
        assert!(s.queue(&[0; 4096]).is_err());
    }
}
