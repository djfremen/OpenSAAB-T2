// SPDX-License-Identifier: MPL-2.0
//! VCX serial framing, independently reconstructed from the paired USB capture.
//! This is not a Nano authorization or CAN-channel implementation.
use std::time::{Duration, Instant};

pub const MAX_BODY: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub header: [u8; 4],
    pub payload: Vec<u8>,
}

impl Frame {
    pub fn command(opcode: u8, payload: &[u8]) -> Self {
        Self {
            header: [0x80, 0, opcode, 0],
            payload: payload.to_vec(),
        }
    }

    pub fn encode(&self) -> Result<Vec<u8>, String> {
        if self.payload.len() + 5 > MAX_BODY {
            return Err("Nano frame exceeds bounded body size".into());
        }
        let mut body = self.header.to_vec();
        body.extend_from_slice(&self.payload);
        body.push(body.iter().fold(0u8, |a, b| a.wrapping_add(*b)));
        let mut result = vec![0xbb];
        for byte in body {
            match byte {
                0xbb => result.extend_from_slice(&[0xdd, 0x44]),
                0xdd => result.extend_from_slice(&[0xdd, 0x22]),
                0xee => result.extend_from_slice(&[0xdd, 0x11]),
                _ => result.push(byte),
            }
        }
        result.push(0xbb);
        Ok(result)
    }
}

#[derive(Default)]
pub struct Decoder {
    started: bool,
    escaped: bool,
    body: Vec<u8>,
}

impl Decoder {
    pub fn feed(&mut self, bytes: &[u8]) -> Result<Vec<Frame>, String> {
        let mut result = Vec::new();
        for &byte in bytes {
            if byte == 0xbb {
                if self.escaped {
                    return Err("Nano delimiter inside escape".into());
                }
                self.started = true;
                if self.body.is_empty() {
                    continue;
                }
                let body = std::mem::take(&mut self.body);
                if body.len() < 5 {
                    return Err("Nano frame shorter than header/checksum".into());
                }
                let sum = body[..body.len() - 1]
                    .iter()
                    .fold(0u8, |a, b| a.wrapping_add(*b));
                if sum != body[body.len() - 1] {
                    return Err("Nano checksum mismatch".into());
                }
                result.push(Frame {
                    header: body[..4].try_into().unwrap(),
                    payload: body[4..body.len() - 1].to_vec(),
                });
                continue;
            }
            if !self.started {
                return Err("Nano bytes before start delimiter".into());
            }
            let decoded = if self.escaped {
                self.escaped = false;
                match byte {
                    0x44 => 0xbb,
                    0x22 => 0xdd,
                    0x11 => 0xee,
                    _ => return Err("Unknown Nano escape".into()),
                }
            } else if byte == 0xdd {
                self.escaped = true;
                continue;
            } else {
                byte
            };
            if self.body.len() >= MAX_BODY {
                return Err("Nano frame exceeds bounded body size".into());
            }
            self.body.push(decoded);
        }
        Ok(result)
    }

    pub fn finish(&self) -> Result<(), String> {
        if self.escaped || !self.body.is_empty() {
            Err("Truncated Nano frame".into())
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Identity {
    pub hardware: String,
    pub firmware: String,
}

pub fn identity(frame: &Frame) -> Result<Identity, String> {
    if frame.header != [0x80, 0, 0x8c, 0] || frame.payload.len() != 65 || frame.payload[0] != 0 {
        return Err("Unexpected Nano information reply header, size or status".into());
    }
    // Offsets cross-checked with the OEM VCX_DEV_GetInfo log for this firmware.
    if &frame.payload[29..39] != b"VCX-NANO\0\0" {
        return Err("USB candidate did not identify as VCX-NANO".into());
    }
    Ok(Identity {
        hardware: "VCX-NANO".into(),
        firmware: frame.payload[53..57]
            .iter()
            .rev()
            .map(u8::to_string)
            .collect::<Vec<_>>()
            .join("."),
    })
}

/// One outstanding request; a deadline expires even with continual unrelated RX.
pub struct ReplyDeadline {
    started: Instant,
    timeout: Duration,
}
impl ReplyDeadline {
    pub fn new(timeout: Duration) -> Self {
        Self {
            started: Instant::now(),
            timeout,
        }
    }
    pub fn expired(&self) -> bool {
        self.started.elapsed() >= self.timeout
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "Requires private local USB captures; never commit those payloads"]
    fn actual_usb_captures_validate_all_frames_without_transfer_boundary_assumptions() {
        for path in [
            "runs/vcx-usb-identity/vcx-identity.pcap",
            "runs/vcx-usb-dtc/vcx-dtc.pcap",
            "runs/ibus-1367-20260907-211424/windows-capture/vcx-ibus.pcap",
        ] {
            let bytes = std::fs::read(path).unwrap();
            assert_eq!(&bytes[..4], b"\xd4\xc3\xb2\xa1");
            assert_eq!(u32::from_le_bytes(bytes[20..24].try_into().unwrap()), 249);
            let mut pos = 24;
            let mut decoders = [Decoder::default(), Decoder::default()];
            let mut counts = [0, 0];
            while pos < bytes.len() {
                let size =
                    u32::from_le_bytes(bytes[pos + 8..pos + 12].try_into().unwrap()) as usize;
                pos += 16;
                let record = &bytes[pos..pos + size];
                pos += size;
                assert!(record.len() >= 27);
                if record[22] != 3 {
                    continue;
                }
                let index = match record[21] {
                    0x02 => 0,
                    0x82 => 1,
                    _ => panic!("Unexpected capture endpoint"),
                };
                let start = u16::from_le_bytes(record[..2].try_into().unwrap()) as usize;
                let len = u32::from_le_bytes(record[23..27].try_into().unwrap()) as usize;
                if len == 0 {
                    continue;
                }
                assert_eq!(u32::from_le_bytes(record[10..14].try_into().unwrap()), 0);
                // Deliberately split every actual USB transfer down to single bytes.
                for byte in &record[start..start + len] {
                    for frame in decoders[index]
                        .feed(&[*byte])
                        .unwrap_or_else(|e| panic!("{path} offset {pos}: {e}"))
                    {
                        counts[index] += 1;
                        if frame.header == [0x80, 0, 0x8c, 0] && index == 1 {
                            identity(&frame).unwrap();
                        }
                    }
                }
            }
            for decoder in decoders {
                decoder.finish().unwrap();
            }
            assert!(counts.iter().all(|n| *n > 0));
            println!("{path}: valid TX={} RX={} frames", counts[0], counts[1]);
        }
    }
    #[test]
    fn captured_echo_and_information_request() {
        assert_eq!(
            Frame::command(0x80, b"\0TEST").encode().unwrap(),
            b"\xbb\x80\0\x80\0\0TEST\x40\xbb"
        );
        assert_eq!(
            Frame::command(0x8c, &[]).encode().unwrap(),
            b"\xbb\x80\0\x8c\0\x0c\xbb"
        );
    }
    #[test]
    fn every_split_and_coalesced_frames_preserve_escaped_data() {
        let frame = Frame::command(0x84, &[0xbb, 0xdd, 0xee, 0xff, 0x44, 0, 0xdd]);
        let wire = frame.encode().unwrap();
        for split in 0..=wire.len() {
            let mut d = Decoder::default();
            let mut frames = d.feed(&wire[..split]).unwrap();
            frames.extend(d.feed(&wire[split..]).unwrap());
            assert_eq!(frames, vec![frame.clone()]);
            d.finish().unwrap();
        }
        let mut d = Decoder::default();
        assert_eq!(
            d.feed(&[wire.clone(), wire].concat()).unwrap(),
            vec![frame.clone(), frame]
        );
    }
    #[test]
    fn reject_corruption_truncation_and_unbounded_input() {
        for wire in [
            &b"\xbb\x80\0\x8c\0\x0d\xbb"[..],
            &b"\xbb\xdd\x55"[..],
            &b"\xbb\xdd\xbb"[..],
            &b"noise"[..],
        ] {
            assert!(Decoder::default().feed(wire).is_err());
        }
        let mut d = Decoder::default();
        d.feed(b"\xbb\x80\xdd").unwrap();
        assert!(d.finish().is_err());
        let mut d = Decoder::default();
        d.feed(&[0xbb]).unwrap();
        assert!(d.feed(&vec![1; MAX_BODY + 1]).is_err());
    }
    #[test]
    fn identity_requires_correct_status_size_and_device_name() {
        let mut f = Frame::command(0x8c, &[0; 65]);
        f.payload[29..39].copy_from_slice(b"VCX-NANO\0\0");
        f.payload[53..57].copy_from_slice(&[2, 4, 9, 1]);
        assert_eq!(identity(&f).unwrap().firmware, "1.9.4.2");
        f.payload[0] = 1;
        assert!(identity(&f).is_err());
        f.payload[0] = 0;
        f.payload[29] = b'X';
        assert!(identity(&f).is_err());
        f.payload.pop();
        assert!(identity(&f).is_err());
    }
}
