// SPDX-License-Identifier: MPL-2.0
//! Chipsoft raw channel wire contract, derived from OEM DLL and USBPcap.
pub use super::init::{disconnect, setup, terminate, PROTOCOLS};
pub use super::protocol::{command, words};
use crate::{
    adapters::chipsoft::protocol::{Decoder, Frame},
    adapters::common::usb::UsbTransport,
};
use std::time::{Duration, Instant};
/// Packed OEM message: timeout, protocol, token, u16 size, u32 flags,
/// u16 status, big-endian CAN id, untouched original data (including PCI).
pub fn transmit(
    controller: usize,
    tx: &crate::candi_cpu::CanTransmission,
) -> Result<Frame, String> {
    let route = crate::can_adapter::electrical_route(controller, tx)?;
    let protocol = PROTOCOLS[route.channel as usize];
    let mut p = words(&[50, protocol, 0]);
    p.extend(((4 + tx.frame.data.len()) as u16).to_le_bytes());
    let flags: u32 = match route.mode {
        crate::can_adapter::ElectricalMode::Data => 0,
        crate::can_adapter::ElectricalMode::SingleWireWake => 0x400,
    };
    p.extend(flags.to_le_bytes());
    p.extend([0; 2]);
    p.extend(tx.frame.id.to_be_bytes());
    p.extend(&tx.frame.data);
    Ok(command(0xf, p))
}
#[derive(Debug, PartialEq, Eq)]
pub struct RawCan {
    pub protocol: u32,
    pub timestamp: u32,
    pub flags: u32,
    pub id: u32,
    pub data: Vec<u8>,
}
pub fn decode_read(frame: &Frame) -> Result<Vec<RawCan>, String> {
    if frame.opcode != 0x10 || frame.status != 0 {
        return Err("Not a successful READ_MSGS reply".into());
    }
    let b = &frame.payload;
    if b.len() < 4 {
        return Err("Truncated read header".into());
    }
    let count = u16::from_le_bytes(b[0..2].try_into().unwrap()) as usize;
    let size = u16::from_le_bytes(b[2..4].try_into().unwrap()) as usize;
    if size != b.len() - 4 || count > size / 20 {
        return Err("Read count/length mismatch".into());
    }
    let mut at = 4;
    let mut out = Vec::new();
    for _ in 0..count {
        if at + 16 > b.len() {
            return Err("Truncated CAN record".into());
        }
        let len = u16::from_le_bytes(b[at + 8..at + 10].try_into().unwrap()) as usize;
        if !(4..=12).contains(&len) || at + 16 + len > b.len() {
            return Err("Invalid raw CAN record size".into());
        }
        let protocol = u32::from_le_bytes(b[at..at + 4].try_into().unwrap());
        let timestamp = u32::from_le_bytes(b[at + 4..at + 8].try_into().unwrap());
        let flags = u32::from_le_bytes(b[at + 10..at + 14].try_into().unwrap());
        let status = u16::from_le_bytes(b[at + 14..at + 16].try_into().unwrap());
        let id = u32::from_be_bytes(b[at + 16..at + 20].try_into().unwrap());
        if !PROTOCOLS.contains(&protocol) || status != 0 || id > 0x1fff_ffff {
            return Err("Unsupported CAN record protocol/status/id".into());
        }
        out.push(RawCan {
            protocol,
            timestamp,
            flags,
            id,
            data: b[at + 20..at + 16 + len].to_vec(),
        });
        at += 16 + len;
    }
    if at != b.len() {
        return Err("Trailing bytes after CAN records".into());
    }
    Ok(out)
}
pub struct Client<T> {
    pub transport: T,
    decoder: Decoder,
    failed: bool,
}
impl<T: UsbTransport> Client<T> {
    pub fn new(transport: T) -> Self {
        Self {
            transport,
            decoder: Decoder::default(),
            failed: false,
        }
    }
    pub fn exchange(&mut self, frame: &Frame) -> Result<Frame, String> {
        if self.failed {
            return Err("Chipsoft stream requires reopening".into());
        }
        let result = (|| {
            self.transport
                .write(&frame.encode().map_err(|e| e.to_string())?)?;
            let until = Instant::now() + Duration::from_secs(3);
            while Instant::now() < until {
                let bytes = self.transport.read()?;
                if bytes.is_empty() {
                    continue;
                }
                let mut replies = Vec::new();
                self.decoder
                    .feed(&bytes, |f| replies.push(f))
                    .map_err(|e| e.to_string())?;
                if replies.is_empty() {
                    continue;
                }
                self.decoder.finish().map_err(|e| e.to_string())?;
                if replies.len() != 1 || replies[0].opcode != frame.opcode {
                    return Err("Unexpected Chipsoft reply sequence".into());
                }
                return Ok(replies.remove(0));
            }
            Err(format!("Chipsoft deadline; {:?}", self.decoder.finish()))
        })();
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    pub fn checked(&mut self, frame: &Frame) -> Result<Frame, String> {
        let r = self.exchange(frame)?;
        if r.status != 0 {
            return Err(format!(
                "Chipsoft opcode={:04X} status={:04X}",
                r.opcode, r.status
            ));
        }
        Ok(r)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn unhex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }
    #[test]
    fn actual_capture_three_raw_records() {
        let bytes=unhex("10005800000027040300540005000000040000000c000000000000000000030100004b000000ca0005000000040000000c0000000000000000000410000000000000000005000000040000000c00000000000000000003001bc000800000fe08");
        let mut f = Vec::new();
        Decoder::default().feed(&bytes, |r| f.push(r)).unwrap();
        let r = decode_read(&f[0]).unwrap();
        assert_eq!(r.len(), 3);
        assert_eq!(r[0].id, 0x301);
        assert_eq!(r[2].data, unhex("1bc000800000fe08"));
        for n in 0..f[0].payload.len() {
            let mut truncated = f[0].clone();
            truncated.payload.truncate(n);
            assert!(decode_read(&truncated).is_err());
        }
        let mut bad = f[0].clone();
        bad.payload[2] += 1;
        assert!(decode_read(&bad).is_err());
        let mut bad = f[0].clone();
        bad.payload[18] = 1;
        assert!(decode_read(&bad).is_err());
    }
    #[test]
    fn captured_hs_setup() {
        let s = setup(5).unwrap();
        assert_eq!(
            s[0].encode().unwrap(),
            unhex("04000c000000cd00050000000000000020a10700")
        );
        assert_eq!(s[1].encode().unwrap(),unhex("1700470000001d000500000001000000000000000000000000000000040500000000000000000000000000000000000000040500000000000000000000000000000000000000000500000000000000"));
        assert!(setup(6).is_err());
    }
}
