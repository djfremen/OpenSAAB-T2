// SPDX-License-Identifier: MPL-2.0
//! Narrow GM II VIN exchange observed in the contributor's native capture.
//! Adapter owns ISO-TP flow control; USB acceptance alone never passes this test.
use super::protocol::Record;
use crate::adapter_translation::isotp::VinReply;

pub const ROUTE: u16 = 0x0601;
/// Observed flow filter: exact 7E8 response / 7E0 request pair, ISO-TP flags 40.
pub const FLOW_FILTER: [u8; 19] = [
    2, 0, 0, 0, 0x40, 0, 0, 0, 0xff, 0, 0, 7, 0xe8, 0, 0, 0, 7, 0xe0, 0,
];
/// Unsegmented read-only service 1A90. The adapter constructs PCI/flow control.
pub const REQUEST: [u8; 18] = [
    0x40, 0, 0, 0, 0, 0, 0, 0, 6, 0, 0, 0, 0, 0, 7, 0xe0, 0x1a, 0x90,
];

pub struct Exchange {
    pub route: u16,
    request_address: u32,
    response_address: u32,
    pub sequence: u16,
    pub rpc_accepted: bool,
    pub tx_completion: bool,
    pub automatic_flow_control: bool,
    pub frames: usize,
    pub ignored: usize,
    pub active: bool,
    receiver: VinReply,
    value: Option<String>,
}
impl Exchange {
    pub fn new(sequence: u16) -> Self {
        Self::for_route(sequence, ROUTE, 0x7e0, 0x7e8)
    }
    pub fn for_route(
        sequence: u16,
        route: u16,
        request_address: u32,
        response_address: u32,
    ) -> Self {
        Self {
            route,
            request_address,
            response_address,
            sequence,
            rpc_accepted: false,
            tx_completion: false,
            automatic_flow_control: false,
            frames: 0,
            ignored: 0,
            active: true,
            receiver: VinReply::default(),
            value: None,
        }
    }
    pub fn accept_rpc(&mut self, reply: &Record) -> Result<(), String> {
        if reply.source != self.route
            || reply.destination != 0
            || reply.opcode != 0x8008
            || reply.sequence != self.sequence
            || reply.body.len() != 12
            || reply.body[..4] != 0x100u32.to_le_bytes()
            || reply.body[8..] != [0; 4]
        {
            return Err("VIN transmit acceptance differs from captured successful layout".into());
        }
        self.rpc_accepted = true;
        Ok(())
    }
    pub fn feed(&mut self, r: &Record) -> Result<(), String> {
        if !self.active || r.destination != 0 || r.source != self.route {
            self.ignored += 1;
            return Ok(());
        }
        if r.opcode == 10 {
            if r.sequence == self.sequence {
                if r.body.len() != 8 || r.body[..4] != 0x106u32.to_le_bytes() {
                    return Err("VIN transmit completion differs from captured layout".into());
                }
                if self.tx_completion {
                    return Err("Duplicate VIN transmit completion".into());
                }
                self.tx_completion = true;
            } else if r.sequence == 0
                && r.body.len() == 28
                && r.body[..4] == 0x10eu32.to_le_bytes()
                && r.body[8..12] == 0x40u32.to_le_bytes()
                && r.body[12..16] == self.request_address.to_be_bytes()
                && r.body[16..24] == [0x30, 0, 0, 0, 0, 0, 0, 0]
                && r.body[24..] == 12u32.to_le_bytes()
            {
                self.automatic_flow_control = true;
            } else {
                self.ignored += 1;
            }
        } else if r.opcode == 9 {
            let m = r.received_message()?.ok_or("Missing VIN receive message")?;
            if m.data.len() < 4 || m.data[..4] != self.response_address.to_be_bytes() {
                self.ignored += 1;
                return Ok(());
            }
            if r.sequence != 0 || m.status != 0 || m.data.len() != 12 || m.extra_index != 12 {
                return Err(
                    "VIN response status/length differs from captured raw-frame layout".into(),
                );
            }
            // The preceding CIM DPID01 request can leave its captured pending
            // response in the same ISO receive stream. It is not a VIN response.
            if self.route == 0x1801 && m.data[4..8] == [3, 0x7f, 0xaa, 0x78] {
                self.ignored += 1;
                return Ok(());
            }
            if self.value.is_some() {
                return Err("Unexpected extra VIN response frame".into());
            }
            self.receiver.feed(&m.data[4..])?;
            self.frames += 1;
            self.value = self.receiver.vin()?;
        }
        Ok(())
    }
    pub fn complete(&self) -> bool {
        self.rpc_accepted && self.tx_completion && self.value.is_some()
    }
    pub fn value(&self) -> Option<&str> {
        self.value.as_deref()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    pub fn record(opcode: u16, sequence: u16, body: Vec<u8>) -> Record {
        Record {
            destination: 0,
            source: ROUTE,
            opcode,
            sequence,
            auxiliary: [0; 4],
            body,
        }
    }
    pub fn frames() -> Vec<Record> {
        let payload = [b"\x5a\x90".as_slice(), b"YS3FD49Y671000001"].concat();
        let can = [
            [&[0x10, 19][..], &payload[..6]].concat(),
            [&[0x21][..], &payload[6..13]].concat(),
            [&[0x22][..], &payload[13..], &[0][..]].concat(),
        ];
        can.into_iter()
            .map(|data| {
                record(
                    9,
                    0,
                    [
                        0u32.to_le_bytes().as_slice(),
                        123u32.to_le_bytes().as_slice(),
                        &[12, 0, 12, 0],
                        &0x7e8u32.to_be_bytes(),
                        &data,
                    ]
                    .concat(),
                )
            })
            .collect()
    }
    pub fn ack(sequence: u16) -> Record {
        record(
            0x8008,
            sequence,
            [0x100u32.to_le_bytes(), 123u32.to_le_bytes(), [0; 4]].concat(),
        )
    }
    pub fn completion(sequence: u16) -> Record {
        record(
            10,
            sequence,
            [0x106u32.to_le_bytes(), 123u32.to_le_bytes()].concat(),
        )
    }
    #[test]
    fn acceptance_and_three_ordered_frames_need_correlated_completion() {
        let mut e = Exchange::new(15);
        e.accept_rpc(&ack(15)).unwrap();
        for r in frames() {
            e.feed(&r).unwrap();
        }
        assert!(!e.complete());
        e.feed(&completion(16)).unwrap();
        assert!(!e.complete());
        e.feed(&completion(15)).unwrap();
        assert!(e.complete());
        assert_eq!(e.value(), Some("YS3FD49Y671000001"));
    }
    #[test]
    fn rejects_failed_acceptance_bad_frame_sequence_and_error_status() {
        let mut e = Exchange::new(15);
        let mut bad = ack(15);
        bad.body[8..].fill(255);
        assert!(e.accept_rpc(&bad).is_err());
        let mut f = frames();
        f[1].body[16] = 0x22;
        e.feed(&f[0]).unwrap();
        assert!(e.feed(&f[1]).is_err());
        let mut e = Exchange::new(15);
        let mut f = frames().remove(0);
        f.body[0] = 1;
        assert!(e.feed(&f).is_err());
    }
    #[test]
    fn wrong_route_and_id_cannot_contribute_to_a_vin() {
        let mut e = Exchange::new(15);
        for mut r in frames() {
            r.source = 0x1801;
            e.feed(&r).unwrap();
        }
        for mut r in frames() {
            r.body[15] = 0xe9;
            e.feed(&r).unwrap();
        }
        assert_eq!(e.frames, 0);
        assert_eq!(e.ignored, 6);
        assert!(e.value().is_none());
    }
}
