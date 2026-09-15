// SPDX-License-Identifier: MPL-2.0
//! Bounded in-memory SSA flash view for the observed original card save path.
//! This models only the 714-byte SSA region, not a complete PCMCIA flash device.

pub const OFFSET: usize = 0xfe0000;
pub const SIZE: usize = 714;
const BANK: usize = 0xf00000;

#[derive(Clone, Default)]
pub struct SsaFlash {
    pending: Option<(usize, u8)>,
    program: Option<usize>,
    erase: bool,
    status: Option<u8>,
    pub erases: u64,
    pub programmed_bytes: u64,
}

impl SsaFlash {
    pub fn handles(offset: usize) -> bool {
        (BANK..BANK + 0x100000).contains(&offset)
    }

    pub fn status(&self) -> Option<u8> {
        self.status
    }

    pub fn write_byte(&mut self, offset: usize, value: u8, card: &mut [u8]) {
        if let Some((first, high)) = self.pending.take() {
            if first & 1 == 0 && offset == first + 1 {
                self.write_word(first, u16::from_be_bytes([high, value]), card);
                return;
            }
            // Unpaired lanes are not an observed command. Never invent a word.
            self.program = None;
            self.erase = false;
            self.status = Some(0xb0);
        }
        if offset & 1 == 0 {
            self.pending = Some((offset, value));
        } else {
            self.status = Some(0xb0);
        }
    }

    fn write_word(&mut self, offset: usize, word: u16, card: &mut [u8]) {
        if let Some(destination) = self.program.take() {
            if destination != offset || card.len() < OFFSET + SIZE {
                self.status = Some(0x90);
                return;
            }
            let data = word.to_be_bytes();
            // NOR programming clears bits; a command-shaped payload is data.
            if card[offset..offset + 2]
                .iter()
                .zip(data)
                .any(|(old, new)| old & new != new)
            {
                self.status = Some(0x90);
                return;
            }
            card[offset] &= data[0];
            card[offset + 1] &= data[1];
            self.programmed_bytes += 2;
            self.status = Some(0x80);
            return;
        }
        match word {
            0x2020 if offset == OFFSET => self.erase = true,
            0xd0d0 if self.erase && offset == OFFSET => {
                self.erase = false;
                if let Some(region) = card.get_mut(OFFSET..OFFSET + SIZE) {
                    region.fill(0xff);
                    self.erases += 1;
                    self.status = Some(0x80);
                } else {
                    self.status = Some(0xa0);
                }
            }
            0x4040 | 0x1010 if (OFFSET..OFFSET + SIZE - 1).contains(&offset) => {
                self.program = Some(offset);
            }
            0x5050 => self.status = Some(0x80),
            0x7070 => self.status = Some(self.status.unwrap_or(0x80)),
            0xffff => {
                self.erase = false;
                self.status = None;
            }
            _ => {
                self.erase = false;
                self.status = Some(0xb0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn word(f: &mut SsaFlash, card: &mut [u8], off: usize, data: u16) {
        f.write_byte(off, (data >> 8) as u8, card);
        f.write_byte(off + 1, data as u8, card);
    }
    #[test]
    fn observed_erase_program_status_and_readback_preserve_surroundings() {
        let mut card = vec![0x55; OFFSET + SIZE + 2];
        let mut f = SsaFlash::default();
        word(&mut f, &mut card, BANK, 0x5050);
        word(&mut f, &mut card, OFFSET, 0x2020);
        word(&mut f, &mut card, OFFSET, 0xd0d0);
        word(&mut f, &mut card, BANK, 0x7070);
        assert_eq!(f.status(), Some(0x80));
        assert!(card[OFFSET..OFFSET + SIZE].iter().all(|b| *b == 0xff));
        assert_eq!(card[OFFSET - 1], 0x55);
        assert_eq!(card[OFFSET + SIZE], 0x55);
        for (off, value) in [(OFFSET, 0xb1ff), (OFFSET + 2, 0x4040), (OFFSET + 4, 0x2020)] {
            word(&mut f, &mut card, off, 0x4040);
            word(&mut f, &mut card, off, value);
            word(&mut f, &mut card, BANK, 0x7070);
            assert_eq!(f.status(), Some(0x80));
        }
        word(&mut f, &mut card, BANK, 0xffff);
        assert_eq!(f.status(), None);
        assert_eq!(
            &card[OFFSET..OFFSET + 6],
            &[0xb1, 0xff, 0x40, 0x40, 0x20, 0x20]
        );
        assert_eq!((f.erases, f.programmed_bytes), (1, 6));
    }
    #[test]
    fn rejected_programming_does_not_corrupt_other_addresses_or_set_bits() {
        let mut card = vec![0; OFFSET + SIZE + 2];
        let mut f = SsaFlash::default();
        word(&mut f, &mut card, OFFSET, 0x4040);
        word(&mut f, &mut card, OFFSET, 0xffff);
        assert_eq!(f.status(), Some(0x90));
        word(&mut f, &mut card, OFFSET + SIZE, 0x4040);
        word(&mut f, &mut card, OFFSET + SIZE, 0x1234);
        word(&mut f, &mut card, OFFSET, 0x4040);
        word(&mut f, &mut card, OFFSET + 2, 0x1234);
        assert_eq!(f.programmed_bytes, 0);
        assert!(card.iter().all(|b| *b == 0));
    }
}
