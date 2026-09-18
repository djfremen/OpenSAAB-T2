// SPDX-License-Identifier: MPL-2.0
//! UART register behavior is available before the secondary CANdi CPU starts.
use std::collections::VecDeque;
#[derive(Clone, Default)]
pub struct Uart {
    pub(super) registers: [u8; 8],
    pub(super) divisor: [u8; 2],
    pub(super) tx: Vec<u8>,
    pub(super) rx: VecDeque<Option<u8>>,
    pub(super) tx_irq: bool,
    pub(super) line_ack: bool,
    pub(super) stopped: bool,
}
impl Uart {
    pub fn receive_idle(&self) -> bool {
        self.rx.is_empty()
    }
    pub fn needs_processor(&self, address: u32, value: u8) -> bool {
        let index = ((address >> 1) & 7) as usize;
        if self.registers[3] & 0x80 != 0 && index < 2 {
            return false;
        }
        (index == 0
            && (self.registers[4] & 0x10 == 0 || (self.registers[3] & 0x3f == 0x3f && value == 0)))
            || (index == 3 && value & 0x40 != 0 && self.registers[3] & 0x40 == 0)
    }
    pub fn interrupt_id(&self) -> u8 {
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
    pub fn read(&mut self, address: u32, events: &mut Vec<String>) -> u8 {
        let index = ((address >> 1) & 7) as usize;
        if self.registers[3] & 0x80 != 0 && index < 2 {
            return self.divisor[index];
        }
        match index {
            0 => {
                self.line_ack = false;
                let byte = self.rx.pop_front();
                events.push(format!(
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
    pub fn write(
        &mut self,
        address: u32,
        value: u8,
        events: &mut Vec<String>,
    ) -> Option<(Vec<u8>, &'static str)> {
        let mut frame = None;
        events.push(format!(
            "UART write address={address:#08x} value={value:#04x} origin=tech2-firmware"
        ));
        let index = ((address >> 1) & 7) as usize;
        if self.registers[3] & 0x80 != 0 && index < 2 {
            self.divisor[index] = value;
            return None;
        }
        match index {
            0 => {
                if self.registers[3] & 0x3f == 0x3f && value == 0 {
                    // Guest sends zero with space parity (LCR 0x3f): the low
                    // parity bit occupies SCI's stop bit, terminating the frame.
                    frame = Some((std::mem::take(&mut self.tx), "space-parity-zero"));
                } else if self.registers[4] & 0x10 != 0 {
                    if self.rx.len() < 4096 {
                        self.rx.push_back(Some(value));
                    }
                } else if self.tx.len() < 4096 {
                    self.tx.push(value);
                } else {
                    self.stopped = true;
                    events.push("UART transmit overflow; link stopped".into());
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
                    frame = Some((std::mem::take(&mut self.tx), "break"));
                }
                self.registers[3] = value;
            }
            _ => self.registers[index] = value,
        }
        frame
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn setup_and_loopback_stay_passive_and_transmit_triggers_processor() {
        let mut u = Uart::default();
        let mut events = Vec::new();
        for (a, v) in [(0x400806, 0x80), (0x400800, 0x34), (0x400802, 0x12)] {
            assert!(!u.needs_processor(a, v));
            assert!(u.write(a, v, &mut events).is_none());
        }
        assert_eq!(u.read(0x400800, &mut events), 0x34);
        assert_eq!(u.read(0x400802, &mut events), 0x12);
        u.write(0x400806, 3, &mut events);
        u.write(0x400808, 0x10, &mut events); // local UART loopback
        assert!(!u.needs_processor(0x400800, 0x5a));
        u.write(0x400800, 0x5a, &mut events);
        assert_eq!(u.read(0x40080a, &mut events), 0x61);
        assert_eq!(u.read(0x400800, &mut events), 0x5a);
        assert_eq!(u.read(0x40080a, &mut events), 0x60);
        u.write(0x400808, 0, &mut events);
        assert!(u.needs_processor(0x400800, 0x90));
        assert!(u.write(0x400800, 0x90, &mut events).is_none());
        u.write(0x400800, 0x70, &mut events);
        assert!(u.needs_processor(0x400806, 0x43));
        let (bytes, why) = u.write(0x400806, 0x43, &mut events).unwrap();
        assert_eq!(bytes, [0x90, 0x70]);
        assert_eq!(why, "break");
        assert!(u.tx.is_empty());
    }
    #[test]
    fn irq_ack_and_divisor_do_not_require_running_processor() {
        let mut u = Uart::default();
        let mut events = Vec::new();
        u.write(0x400802, 2, &mut events);
        assert!(u.irq());
        assert_eq!(u.read(0x400804, &mut events), 2);
        assert!(!u.irq());
        assert_eq!(u.read(0x400804, &mut events), 1);
        u.write(0x400806, 0x80, &mut events);
        u.write(0x400802, 0xff, &mut events);
        assert!(!u.irq());
    }
}
