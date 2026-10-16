// SPDX-License-Identifier: MPL-2.0
//! Raw Nano channel control shared by Android probes and the future live backend.
//! USB writes are transport operations, never CAN transmit confirmations.
use crate::nano_usb::{Decoder, Frame, ReplyDeadline};
use std::time::Duration;

pub use crate::adapters::common::usb::UsbTransport;

pub fn control(channel: u8, opcode: u8, payload: &[u8]) -> Frame {
    Frame {
        header: [0x80, 0, opcode, channel],
        payload: payload.to_vec(),
    }
}

/// Captured OEM raw-CAN startup. These are normal channels, not silent listen-only.
pub fn setup(channel: u8) -> Result<Vec<Frame>, String> {
    let hs = [0, 3, 0, 7, 0xa1, 0x20];
    let sw = [0, 3, 0, 0, 0x82, 0x35];
    let params: &[u8] = match channel {
        0 => &[0, 3, 0, 7, 0xa1, 0x20, 0, 2, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0],
        1 => &[0, 3, 0, 0, 0x82, 0x35, 0, 2, 1, 0, 0, 0, 1, 0, 0, 0, 0, 2],
        _ => return Err("Unsupported Nano raw channel".into()),
    };
    let mut frames = vec![
        control(channel, 0x40, &[0, 0, 0x81, 1]),
        control(channel, 0x45, if channel == 0 { &hs } else { &sw }),
        control(channel, 0x47, &[1]),
    ];
    if channel == 1 {
        frames.push(control(channel, 0x43, &[]));
        frames.push(control(channel, 0x45, params));
    }
    frames.push(control(channel, 0x42, &[0]));
    frames.push(control(channel, 0x45, params));
    frames.push(control(
        channel,
        0x48,
        &[1, 0, 1, 1, 4, 0, 0, 0, 0, 0, 0, 0, 0],
    ));
    Ok(frames)
}
pub fn shutdown(channel: u8) -> [Frame; 3] {
    [
        control(channel, 0x48, &[1, 0, 1, 1, 0]),
        control(channel, 0x43, &[]),
        control(channel, 0x41, &[]),
    ]
}

#[derive(Debug, PartialEq, Eq)]
pub struct RawCan {
    pub channel: u8,
    pub sequence: u8,
    pub flags: u32,
    pub id: u32,
    pub data: Vec<u8>,
}
impl RawCan {
    pub fn decode(frame: &Frame) -> Result<Self, String> {
        if frame.header[0] != 0x80
            || frame.header[2] != 0
            || frame.header[3] > 1
            || frame.payload.len() < 10
        {
            return Err("Unexpected Nano CAN header/size".into());
        }
        let p = &frame.payload;
        let size = u16::from_be_bytes([p[4], p[5]]) as usize;
        if !(4..=12).contains(&size) || p.len() != size + 6 {
            return Err("Unexpected Nano CAN declared length".into());
        }
        let id = u32::from_be_bytes(p[6..10].try_into().unwrap());
        if id > 0x1fff_ffff {
            return Err("Nano CAN identifier exceeds 29 bits".into());
        }
        Ok(Self {
            channel: frame.header[3],
            sequence: frame.header[1],
            flags: u32::from_be_bytes(p[..4].try_into().unwrap()),
            id,
            data: p[10..].to_vec(),
        })
    }
}

pub struct Client<T> {
    transport: T,
    decoder: Decoder,
}
impl<T: UsbTransport> Client<T> {
    pub fn new(transport: T) -> Self {
        Self {
            transport,
            decoder: Decoder::default(),
        }
    }
    pub fn transport_mut(&mut self) -> &mut T {
        &mut self.transport
    }
    /// The persistent decoder spans reads AND requests. CAN traffic may precede,
    /// follow or share a USB transfer with the outstanding control reply.
    pub fn exchange(
        &mut self,
        request: &Frame,
        observed: &mut impl FnMut(&Frame) -> Result<(), String>,
    ) -> Result<Frame, String> {
        self.transport.write(&request.encode()?)?;
        let deadline = ReplyDeadline::new(Duration::from_secs(3));
        while !deadline.expired() {
            let frames = self.read_frames()?;
            let mut reply = None;
            for frame in frames {
                observed(&frame)?;
                if frame.header == request.header {
                    if reply.replace(frame).is_some() {
                        return Err("Duplicate Nano control reply".into());
                    }
                } else if frame.header[0] != 0x80 || frame.header[2] != 0 {
                    return Err("Unmatched Nano control reply".into());
                }
            }
            if let Some(reply) = reply {
                return Ok(reply);
            }
        }
        Err("Nano control reply deadline expired".into())
    }
    fn read_frames(&mut self) -> Result<Vec<Frame>, String> {
        self.decoder.feed(&self.transport.read()?)
    }
    pub fn receive(
        &mut self,
        observed: &mut impl FnMut(&Frame) -> Result<(), String>,
    ) -> Result<(), String> {
        for frame in self.read_frames()? {
            observed(&frame)?;
            if frame.header[0] != 0x80 || frame.header[2] != 0 {
                return Err("Unexpected control reply during receive".into());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    struct Mock {
        reads: VecDeque<Vec<u8>>,
    }
    impl UsbTransport for Mock {
        fn write(&mut self, _: &[u8]) -> Result<(), String> {
            Ok(())
        }
        fn read(&mut self) -> Result<Vec<u8>, String> {
            self.reads.pop_front().ok_or("Mock exhausted".into())
        }
    }
    fn raw() -> Frame {
        Frame {
            header: [0x80, 0x96, 0, 1],
            payload: vec![
                0, 0, 0, 0, 0, 12, 0, 0, 5, 0x41, 0x81, 0x45, 0x47, 4, 0x11, 0, 0, 0,
            ],
        }
    }
    #[test]
    fn raw_length_and_status_preserved() {
        let frame = raw();
        let parsed = RawCan::decode(&frame).unwrap();
        assert_eq!(
            (parsed.channel, parsed.sequence, parsed.id),
            (1, 0x96, 0x541)
        );
        assert_eq!(parsed.data, [0x81, 0x45, 0x47, 4, 0x11, 0, 0, 0]);
        let mut wrong = frame.clone();
        wrong.payload[5] = 11;
        assert!(RawCan::decode(&wrong).is_err());
        wrong = frame;
        wrong.payload.push(0);
        assert!(RawCan::decode(&wrong).is_err());
    }
    #[test]
    fn interleaved_partial_data_survives_control_boundaries() {
        let req = control(0, 0x42, &[0]);
        let response = control(0, 0x42, &[0]).encode().unwrap();
        let data = raw().encode().unwrap();
        let split = 7;
        let mut first = data.clone();
        first.extend(response);
        first.extend(&data[..split]);
        let mut client = Client::new(Mock {
            reads: VecDeque::from([first, data[split..].to_vec()]),
        });
        let mut seen = vec![];
        let mut observe = |f: &Frame| {
            seen.push(f.clone());
            Ok(())
        };
        assert_eq!(client.exchange(&req, &mut observe).unwrap().payload, [0]);
        client.receive(&mut observe).unwrap();
        assert_eq!(seen.iter().filter(|f| f.header[2] == 0).count(), 2);
    }
    #[test]
    fn unrelated_or_duplicate_controls_fail() {
        for bytes in [
            control(1, 0x42, &[0]).encode().unwrap(),
            [
                control(0, 0x42, &[0]).encode().unwrap(),
                control(0, 0x42, &[0]).encode().unwrap(),
            ]
            .concat(),
        ] {
            let mut client = Client::new(Mock {
                reads: VecDeque::from([bytes]),
            });
            assert!(client
                .exchange(&control(0, 0x42, &[0]), &mut |_| Ok(()))
                .is_err());
        }
    }
}
