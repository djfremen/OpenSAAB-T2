// SPDX-License-Identifier: MPL-2.0
//! Bounded MC68331 registers, virtual SCI, PIT and GPT counter/overflow. No CAN controller.
//!
//! Register map and GPIO behavior: MC68331UM sections 4.9, 7.6, appendix D.
//! External GPIO inputs are explicitly held low in this disconnected research
//! board. This is not a measurement of physical CANdi board straps.

#[path = "serial.rs"]
mod serial;

#[derive(Clone)]
pub(super) struct Peripherals {
    registers: [u8; 0x700],
    pub serial: serial::Serial,
    now: u64,
    pit_deadline: Option<u64>,
    pit_pending: bool,
    gpt_counter: u16,
    gpt_prescaler: u16,
    gpt_cpr_written: bool,
    gpt_overflow_epoch: u64,
    gpt_overflow_read: Option<u64>,
}

impl Default for Peripherals {
    fn default() -> Self {
        let mut registers = [0; 0x700];
        registers[0x305] = 0x0f; // QIVR reset; bit 0 always reads one.
        registers[0x309] = 4; // SCCR0 baud divisor reset.
        Self {
            registers,
            serial: serial::Serial::default(),
            now: 0,
            pit_deadline: None,
            pit_pending: false,
            gpt_counter: 0,
            gpt_prescaler: 0,
            gpt_cpr_written: false,
            gpt_overflow_epoch: 0,
            gpt_overflow_read: None,
        }
    }
}

impl Peripherals {
    /// PDRQS/PQSPAR/DDRQS. No read side effects and no inferred physical voltage.
    pub fn single_wire_gpio(&self) -> super::sja1000::ElectricalState {
        super::sja1000::ElectricalState::SingleWireGpio {
            latch: self.registers[0x315],
            assignment: self.registers[0x316],
            direction: self.registers[0x317],
        }
    }
    pub fn external_irq_enabled(&self, level: u8) -> bool {
        (2..=4).contains(&level) && self.registers[0x11f] & (1 << level) != 0
    }

    fn word(&self, offset: usize) -> u16 {
        u16::from_be_bytes([self.registers[offset], self.registers[offset + 1]])
    }
    fn character_cycles(&self) -> u64 {
        // SCI: system clock / (32 * SCBR), ten bits per 8N1 byte.
        // Core cycles are only an approximation of system clocks here.
        320 * u64::from((self.word(0x308) & 0x1fff).max(1))
    }
    pub fn advance(&mut self, now: u64) {
        // MC68331UM 7.7/7.8.1: free-running TCNT, prescaler taps /4..256.
        // PCLK has no external edges in this virtual board. Use the same
        // approximate system-clock domain as SCI/PIT, never a constant timer.
        let elapsed = now.saturating_sub(self.now);
        if self.word(0) & 0x9000 == 0 {
            let cpr = self.registers[0x21] & 7;
            if cpr < 7 {
                let divisor = 4u64 << cpr;
                let ticks = (u64::from(self.gpt_prescaler) % divisor + elapsed) / divisor;
                let total = u64::from(self.gpt_counter) + ticks;
                if total >> 16 != 0 {
                    self.registers[0x23] |= 0x80;
                    self.gpt_overflow_epoch = self.gpt_overflow_epoch.wrapping_add(total >> 16);
                }
                self.gpt_counter = total as u16;
            }
            self.gpt_prescaler = ((u64::from(self.gpt_prescaler) + elapsed) & 511) as u16;
        }
        self.now = now;
        if self.word(0x300) & 0x8000 == 0 && self.word(0x308) & 0x1fff != 0 {
            self.serial
                .advance(now, self.word(0x30a), self.character_cycles());
        }
        // MC68331UM 4.4.2: EXTAL / 4 / PITM, optional /512.
        // Explicit research clock ratio: 16.777216 MHz core : 32.768 kHz EXTAL.
        // Core instruction cycles are approximate; this is not hardware latency.
        let pitr = self.word(0x124);
        if pitr & 255 == 0 {
            self.pit_deadline = None;
            self.pit_pending = false;
        } else {
            let period = u64::from(pitr & 255) * 2048 * if pitr & 0x100 != 0 { 512 } else { 1 };
            match self.pit_deadline {
                None => self.pit_deadline = Some(now.saturating_add(period)),
                Some(deadline) if now >= deadline => {
                    self.pit_pending = true;
                    self.pit_deadline = Some(now.saturating_add(period));
                }
                _ => {}
            }
        }
    }
    pub fn interrupt(&self) -> Option<(u8, u32)> {
        let level = self.registers[0x304] & 7;
        let sci = (level != 0
            && self.word(0x300) & 0x8000 == 0
            && self.serial.interrupt(self.word(0x30a)))
        .then_some((level, u32::from(self.registers[0x305] & 0xfe)));
        let picr = self.word(0x122);
        let pit_level = ((picr >> 8) & 7) as u8;
        let pit =
            (self.pit_pending && pit_level != 0).then_some((pit_level, u32::from(picr & 255)));
        let icr = self.word(4);
        let gpt_level = ((icr >> 8) & 7) as u8;
        let gpt = (self.registers[0x23] & self.registers[0x21] & 0x80 != 0
            && self.word(0) & 0x8000 == 0
            && self.word(0) & 15 != 0
            && gpt_level != 0)
            .then_some((
                gpt_level,
                u32::from((icr & 0xf0) | if icr >> 12 == 9 { 0 } else { 9 }),
            ));
        sci.into_iter()
            .chain(pit)
            .chain(gpt)
            .max_by_key(|(level, _)| *level)
    }
    pub fn acknowledge(&mut self, level: u8) -> u32 {
        if let Some((priority, vector)) = self.interrupt() {
            if priority == level {
                if self.pit_pending
                    && self.word(0x122) & 0x7ff == ((u16::from(level) << 8) | vector as u16)
                {
                    self.pit_pending = false;
                }
                return vector;
            }
        }
        0xffff_ffff
    }
    pub fn transmit(&mut self, value: u8) -> bool {
        self.word(0x30a) & 8 != 0
            && self.word(0x308) & 0x1fff != 0
            && self
                .serial
                .write_data(value, self.now, self.character_cycles())
    }
    /// Startup configures CSBOOT as a 256 KiB memory aperture at zero, with
    /// internally generated bus acknowledgement (MC68331UM 4.8 / appendix D).
    pub fn boot_rom_selected(&self, address: u32, width: u8) -> bool {
        self.registers[0x148..0x14c] == [0, 5, 0x78, 0x70]
            && address
                .checked_add(u32::from(width))
                .is_some_and(|end| end <= 0x40000)
    }

    fn configuration(address: u32) -> bool {
        matches!(address,
            0xfff900..=0xfff901 | 0xfff904..=0xfff906 |
            0xfff91e..=0xfff921 | 0xfffa00..=0xfffa01 |
            0xfffa14..=0xfffa17 | 0xfffa1c..=0xfffa1f |
            0xfffa22..=0xfffa25 | 0xfffa44..=0xfffa73 |
            0xfffc00..=0xfffc01 | 0xfffc04..=0xfffc05 |
            0xfffc08..=0xfffc0b | 0xfffc16..=0xfffc17)
    }

    pub fn read8(&mut self, address: u32) -> Option<u8> {
        if Self::configuration(address) {
            return Some(self.registers[(address - 0xfff900) as usize]);
        }
        match address {
            0xfff90a => Some((self.gpt_counter >> 8) as u8),
            0xfff90b => Some(self.gpt_counter as u8),
            0xfff92c => Some((self.gpt_prescaler >> 8) as u8),
            0xfff92d => Some(self.gpt_prescaler as u8),
            0xfffc0c => Some((self.serial.read_status() >> 8) as u8),
            0xfffc0d => Some(self.serial.read_status() as u8),
            // RDR reset contents are undefined. Choose zero for the disconnected
            // board; no new-data flag or interrupt accompanies this empty read.
            0xfffc0e => Some(0),
            0xfffc0f => Some(self.serial.read_data()),
            0xfffc15 if self.registers[0x316] == 0 => {
                Some(self.registers[0x315] & self.registers[0x317])
            }
            // Read pins, not the output latch. Timer output modes are not yet
            // modeled; refuse that read instead of claiming a GPIO level.
            0xfff907 if self.registers[0x1e] == 0 => Some(self.registers[7] & self.registers[6]),
            // Port F aliases, with peripheral-assigned pins unsupported.
            0xfffa19 | 0xfffa1b if self.registers[0x11f] == 0 => {
                Some(self.registers[0x119] & self.registers[0x11d])
            }
            // Input-capture latches on the disconnected board: deterministic
            // zero initial contents; no input edge or capture interrupt implied.
            0xfff90e..=0xfff913 => Some(0),
            0xfff922 => Some(self.registers[0x22]),
            0xfff923 => {
                self.gpt_overflow_read =
                    (self.registers[0x23] & 0x80 != 0).then_some(self.gpt_overflow_epoch);
                Some(self.registers[0x23])
            }
            _ => None,
        }
    }

    pub fn write8(&mut self, address: u32, value: u8) {
        if address == 0xfffc0b && value & 1 != 0 && self.registers[0x30b] & 1 == 0 {
            self.serial.send_break();
        }
        if address == 0xfffc05 {
            self.registers[0x305] = value | 1;
        } else if address == 0xfff921 {
            // CPR can be written only once after reset in normal operation.
            let cpr = if self.gpt_cpr_written {
                self.registers[0x21] & 7
            } else {
                value & 7
            };
            self.gpt_cpr_written = true;
            self.registers[0x21] = (value & 0xf8) | cpr;
        } else if Self::configuration(address) || matches!(address, 0xfff907 | 0xfffc15) {
            self.registers[(address - 0xfff900) as usize] = value;
        } else if matches!(address, 0xfffa19 | 0xfffa1b) {
            self.registers[0x119] = value;
        } else if address == 0xfff923 {
            // Read-before-write-zero clear; a later overflow must survive.
            if value & 0x80 == 0 && self.gpt_overflow_read == Some(self.gpt_overflow_epoch) {
                self.registers[0x23] &= !0x80;
                self.gpt_overflow_read = None;
            }
            self.registers[0x23] &= value | 0x80;
        } else if address == 0xfff922 {
            self.registers[0x22] &= value;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn counter(p: &mut Peripherals) -> u16 {
        u16::from_be_bytes([p.read8(0xfff90a).unwrap(), p.read8(0xfff90b).unwrap()])
    }
    #[test]
    fn gpt_counter_prescaler_stop_and_read_only_semantics() {
        for cpr in 0..7 {
            let mut p = Peripherals::default();
            p.write8(0xfff921, cpr);
            let divisor = 4u64 << cpr;
            p.advance(divisor - 1);
            assert_eq!(counter(&mut p), 0);
            p.advance(divisor);
            assert_eq!(counter(&mut p), 1);
            p.write8(0xfff90b, 99);
            assert_eq!(counter(&mut p), 1);
            p.write8(0xfff900, 0x80);
            p.advance(divisor * 10);
            assert_eq!(counter(&mut p), 1);
            p.write8(0xfff900, 0);
            p.advance(divisor * 11);
            assert_eq!(counter(&mut p), 2);
            p.write8(0xfff921, cpr ^ 7);
            assert_eq!(p.read8(0xfff921).unwrap() & 7, cpr);
        }
        let mut p = Peripherals::default();
        p.write8(0xfff921, 7);
        p.advance(1_000_000);
        assert_eq!(counter(&mut p), 0);
    }
    #[test]
    fn gpt_overflow_irq_and_read_clear_race() {
        let mut p = Peripherals::default();
        p.write8(0xfff901, 0x0d);
        p.write8(0xfff904, 0x84);
        p.write8(0xfff905, 0x50);
        p.write8(0xfff921, 0x86);
        let period = 256 * 65536;
        p.advance(period - 256);
        assert_eq!(counter(&mut p), 65535);
        assert_eq!(p.interrupt(), None);
        p.advance(period);
        assert_eq!(counter(&mut p), 0);
        assert_eq!(p.interrupt(), Some((4, 0x59)));
        assert_eq!(p.acknowledge(4), 0x59);
        assert_eq!(p.interrupt(), Some((4, 0x59)));
        p.write8(0xfff923, 0);
        assert_eq!(p.interrupt(), Some((4, 0x59))); // No prior read.
        assert_eq!(p.read8(0xfff923).unwrap() & 0x80, 0x80);
        p.advance(period * 2);
        p.write8(0xfff923, 0);
        assert_eq!(p.interrupt(), Some((4, 0x59)));
        p.read8(0xfff923);
        p.write8(0xfff923, 0);
        assert_eq!(p.interrupt(), None);
        p.advance(period * 3);
        p.write8(0xfff904, 0x94);
        assert_eq!(p.interrupt(), Some((4, 0x50)));
        p.write8(0xfff921, 6);
        assert_eq!(p.interrupt(), None);
    }

    #[test]
    fn pit_is_periodic_acknowledged_and_disabled_by_zero_modulus() {
        let mut p = Peripherals::default();
        p.write8(0xfffa22, 1);
        p.write8(0xfffa23, 0x52);
        p.write8(0xfffa25, 8);
        p.advance(100);
        p.advance(100 + 16383);
        assert_eq!(p.interrupt(), None);
        p.advance(100 + 16384);
        assert_eq!(p.interrupt(), Some((1, 0x52)));
        assert_eq!(p.acknowledge(1), 0x52);
        assert_eq!(p.interrupt(), None);
        p.advance(100 + 32768);
        assert_eq!(p.interrupt(), Some((1, 0x52)));
        p.write8(0xfffa25, 0);
        p.advance(100 + 32769);
        assert_eq!(p.interrupt(), None);
    }

    #[test]
    fn gpio_latch_only_drives_output_pins() {
        let mut p = Peripherals::default();
        p.write8(0xfff907, 0x81);
        assert_eq!(p.read8(0xfff907), Some(0));
        p.write8(0xfff906, 0x80);
        assert_eq!(p.read8(0xfff907), Some(0x80));
        p.write8(0xfff91e, 1);
        assert_eq!(p.read8(0xfff907), None);
    }

    #[test]
    fn no_serial_or_can_reply_from_written_registers() {
        let mut p = Peripherals::default();
        assert_eq!(p.read8(0xfffc0d).unwrap() & 0x40, 0);
        assert_eq!(p.read8(0xfffc0f), Some(0));
        assert_eq!(p.read8(0x200000), None);
        p.write8(0xfff922, 0xff);
        assert_eq!(p.read8(0xfff922), Some(0));
    }
}
