// SPDX-License-Identifier: MPL-2.0
//! Chipsoft Pro USB CDC framing, verified against the user's USBPcap capture.
//!
//! All four header fields are u16 LE: opcode, payload length, device status,
//! payload checksum. In particular, bytes 2..6 are NOT one u32 length.
//! USB/serial read boundaries need not coincide with frame boundaries.
//! See audit/2026-09-04/CHIPSOFT_MACOS.md for evidence and remaining limits.

use std::fmt;

pub const HEADER_SIZE: usize = 8;
pub const MAX_PAYLOAD: usize = u16::MAX as usize;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub opcode: u16,
    /// Nonzero means a device-reported result, not a framing/checksum error.
    pub status: u16,
    pub payload: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecodeError {
    PayloadTooLarge(usize),
    Checksum { expected: u16, actual: u16 },
    Truncated { buffered: usize, needed: usize },
    Desynchronized,
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PayloadTooLarge(n) => write!(f, "payload {n} exceeds {MAX_PAYLOAD} bytes"),
            Self::Checksum { expected, actual } => write!(
                f,
                "checksum mismatch: header={expected:04X}, calculated={actual:04X}"
            ),
            Self::Truncated { buffered, needed } => write!(
                f,
                "incomplete frame: {buffered} bytes buffered, {needed} required"
            ),
            Self::Desynchronized => {
                write!(f, "decoder stopped after corrupt framing; reset session")
            }
        }
    }
}

impl std::error::Error for DecodeError {}

pub fn checksum(payload: &[u8]) -> u16 {
    payload
        .iter()
        .fold(0u16, |sum, &byte| sum.wrapping_add(u16::from(byte)))
}

impl Frame {
    /// Only creates wire bytes; does not open or transmit to an adapter.
    pub fn encode(&self) -> Result<Vec<u8>, DecodeError> {
        let length = u16::try_from(self.payload.len())
            .map_err(|_| DecodeError::PayloadTooLarge(self.payload.len()))?;
        let mut bytes = Vec::with_capacity(HEADER_SIZE + self.payload.len());
        for value in [self.opcode, length, self.status, checksum(&self.payload)] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes.extend_from_slice(&self.payload);
        Ok(bytes)
    }
}

/// One decoder per serial stream/direction. Buffers at most one 65,543-byte
/// frame, even if a read contains many frames. A checksum failure is terminal:
/// there is no verified sync marker with which to safely guess a new boundary.
#[derive(Default)]
pub struct Decoder {
    pending: Vec<u8>,
    failed: bool,
}

impl Decoder {
    fn needed(&self) -> usize {
        if self.pending.len() < HEADER_SIZE {
            HEADER_SIZE
        } else {
            HEADER_SIZE + usize::from(u16::from_le_bytes([self.pending[2], self.pending[3]]))
        }
    }

    pub fn feed(
        &mut self,
        mut bytes: &[u8],
        mut emit: impl FnMut(Frame),
    ) -> Result<(), DecodeError> {
        if self.failed {
            return Err(DecodeError::Desynchronized);
        }
        while !bytes.is_empty() {
            let n = (self.needed() - self.pending.len()).min(bytes.len());
            self.pending.extend_from_slice(&bytes[..n]);
            bytes = &bytes[n..];
            if self.pending.len() < HEADER_SIZE || self.pending.len() < self.needed() {
                continue;
            }
            let expected = u16::from_le_bytes([self.pending[6], self.pending[7]]);
            let actual = checksum(&self.pending[HEADER_SIZE..]);
            if expected != actual {
                self.failed = true;
                return Err(DecodeError::Checksum { expected, actual });
            }
            let frame = Frame {
                opcode: u16::from_le_bytes([self.pending[0], self.pending[1]]),
                status: u16::from_le_bytes([self.pending[4], self.pending[5]]),
                payload: self.pending[HEADER_SIZE..].to_vec(),
            };
            self.pending.clear();
            emit(frame);
        }
        Ok(())
    }

    /// Call at EOF or when a transport deadline expires. Never turn a partial
    /// reply into success. Live transports must supply their own wall-clock deadline.
    pub fn finish(&self) -> Result<(), DecodeError> {
        if self.failed {
            Err(DecodeError::Desynchronized)
        } else if self.pending.is_empty() {
            Ok(())
        } else {
            Err(DecodeError::Truncated {
                buffered: self.pending.len(),
                needed: self.needed(),
            })
        }
    }
}

pub fn opcode_name(opcode: u16) -> &'static str {
    match opcode {
        0x01 => "GET_INFO",
        0x04 => "CONNECT",
        0x05 => "DISCONNECT",
        0x08 => "OPEN_DEVICE",
        0x0b => "SET_PARAM",
        0x0c => "GET_PARAM",
        0x0f => "WRITE_COMMIT",
        0x10 => "READ_MSGS",
        0x12 => "ARM_CHANNEL",
        0x17 => "SET_FILTER",
        0x20 => "CLOSE_DEVICE",
        0x22 => "WRITE_QUEUE",
        _ => "UNKNOWN",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Exact frames reassembled from records 151+153, 157+159, 205+207 of
    // the 2026-05-08 J2534 USB capture. The initial opcode byte arrived alone.
    const INFO: &[u8] = b"\x01\x00\x1b\x00\x00\x00\xc1\x06CHIPSOFT J2534 Pro v. 1.5.2";
    const CONNECT_ACK: &[u8] = &[4, 0, 0, 0, 0, 0, 0, 0];
    const READ_ERROR: &[u8] = &[0x10, 0, 0, 0, 0x85, 0, 0, 0];

    #[test]
    fn capture_split_reply_at_every_boundary() {
        for split in 0..=INFO.len() {
            let mut decoder = Decoder::default();
            let mut frames = Vec::new();
            decoder.feed(&INFO[..split], |f| frames.push(f)).unwrap();
            decoder.feed(&INFO[split..], |f| frames.push(f)).unwrap();
            decoder.finish().unwrap();
            assert_eq!(frames.len(), 1);
            assert_eq!(frames[0].payload, b"CHIPSOFT J2534 Pro v. 1.5.2");
            assert_eq!(frames[0].encode().unwrap(), INFO);
        }
    }

    #[test]
    fn coalesced_frames_preserve_device_error_and_empty_ack() {
        let mut decoder = Decoder::default();
        let mut frames = Vec::new();
        decoder
            .feed(&[INFO, CONNECT_ACK, READ_ERROR].concat(), |f| {
                frames.push(f)
            })
            .unwrap();
        decoder.finish().unwrap();
        assert_eq!(frames.len(), 3);
        assert_eq!(frames[1].status, 0);
        assert_eq!(frames[2].status, 0x85);
        assert!(frames[2].payload.is_empty());
    }

    #[test]
    fn corrupt_checksum_stops_instead_of_guessing_next_boundary() {
        let mut wire = INFO.to_vec();
        wire[8] ^= 1;
        let mut decoder = Decoder::default();
        assert!(matches!(
            decoder.feed(&wire, |_| panic!("corrupt frame emitted")),
            Err(DecodeError::Checksum { .. })
        ));
        assert_eq!(
            decoder.feed(CONNECT_ACK, |_| {}),
            Err(DecodeError::Desynchronized)
        );
        assert_eq!(decoder.finish(), Err(DecodeError::Desynchronized));
    }

    #[test]
    fn eof_or_deadline_detects_each_truncated_prefix() {
        for end in 1..INFO.len() {
            let mut decoder = Decoder::default();
            decoder
                .feed(&INFO[..end], |_| panic!("partial frame emitted"))
                .unwrap();
            assert!(matches!(
                decoder.finish(),
                Err(DecodeError::Truncated { .. })
            ));
        }
    }

    #[test]
    fn size_limit_and_checksum_wrap() {
        let frame = Frame {
            opcode: 0xabcd,
            status: 0,
            payload: vec![255; MAX_PAYLOAD],
        };
        assert_eq!(checksum(&frame.payload), 0xff01);
        let wire = frame.encode().unwrap();
        let mut decoder = Decoder::default();
        let mut count = 0;
        for chunk in wire.chunks(1) {
            decoder
                .feed(chunk, |f| {
                    assert_eq!(f, frame);
                    count += 1;
                })
                .unwrap();
        }
        assert_eq!(count, 1);
        decoder.finish().unwrap();
        let oversized = Frame {
            payload: vec![0; MAX_PAYLOAD + 1],
            ..frame
        };
        assert!(matches!(
            oversized.encode(),
            Err(DecodeError::PayloadTooLarge(_))
        ));
    }
}
