// SPDX-License-Identifier: MPL-2.0
//! Narrow raw wake/DPID01 and assembled CIM exchange from the October14 captures.
use super::protocol::Record;
use serde_json::{json, Value};

pub const ISO_ROUTE: u16 = 0x1801;
pub const RAW_ROUTE: u16 = 0x1701;
pub const FLOW_FILTER: [u8; 19] = [
    2, 0, 0, 0, 0x40, 0, 0, 0, 0xff, 0, 0, 6, 0x41, 0, 0, 0, 2, 0x41, 0,
];
pub const RAW_FILTER: [u8; 16] = [
    0, 0, 0, 0, 0, 0, 1, 4, 0xff, 0xff, 0xff, 0xff, 0, 0, 5, 0x41,
];
pub fn message(flags: u32, address: u32, payload: &[u8]) -> Vec<u8> {
    [
        flags.to_le_bytes().to_vec(),
        vec![0; 4],
        ((payload.len() + 4) as u16).to_le_bytes().to_vec(),
        vec![0; 2],
        address.to_be_bytes().to_vec(),
        payload.to_vec(),
    ]
    .concat()
}
pub fn wake() -> Vec<u8> {
    message(0x400, 0x100, &[])
}
pub fn tester_present() -> Vec<u8> {
    message(0, 0x101, &[0xfe, 1, 0x3e])
}
pub fn status_request() -> Vec<u8> {
    message(0x40, 0x241, &[0xaa, 1, 1])
}
pub fn vin_request() -> Vec<u8> {
    message(0x40, 0x241, &[0x1a, 0x90])
}
pub fn stop_request() -> Vec<u8> {
    message(0x40, 0x241, &[0x20])
}

#[derive(Default)]
pub struct Exchange {
    pub listening: bool,
    pub status_attempted: bool,
    pub tuple: Option<[u8; 3]>,
    pub ignored: usize,
    pub pending_responses: usize,
    tickets: Vec<Ticket>,
}
struct Ticket {
    route: u16,
    sequence: u16,
    stage: String,
    accepted: bool,
    completed: bool,
}
impl Exchange {
    pub fn issued(&mut self, r: &Record, stage: &str) -> Result<(), String> {
        let allowed = match stage {
            "sw_wake" => r.destination == RAW_ROUTE && r.body == wake(),
            "sw_tester_present" => r.destination == RAW_ROUTE && r.body == tester_present(),
            "sw_cim_status" => r.destination == ISO_ROUTE && r.body == status_request(),
            "sw_cim_vin" => r.destination == ISO_ROUTE && r.body == vin_request(),
            "sw_cim_stop" => r.destination == ISO_ROUTE && r.body == stop_request(),
            _ => false,
        };
        if !allowed || self.tickets.len() >= 5 || self.tickets.iter().any(|t| t.stage == stage) {
            return Err("SW-CAN test request outside captured bounded recipe".into());
        }
        if stage == "sw_cim_status" {
            self.status_attempted = true;
            self.listening = true;
        }
        self.tickets.push(Ticket {
            route: r.destination,
            sequence: r.sequence,
            stage: stage.into(),
            accepted: false,
            completed: false,
        });
        Ok(())
    }
    pub fn accept_rpc(&mut self, r: &Record) -> Result<(), String> {
        let ticket = self
            .tickets
            .iter_mut()
            .find(|t| t.route == r.source && t.sequence == r.sequence)
            .ok_or("SW-CAN transmit reply has no issued request")?;
        if r.destination != 0
            || r.opcode != 0x8008
            || r.body.len() != 12
            || r.body[..4] != 0x100u32.to_le_bytes()
            || r.body[8..] != [0; 4]
        {
            return Err(
                "SW-CAN transmit acceptance differs from successful captured layout".into(),
            );
        }
        ticket.accepted = true;
        Ok(())
    }
    pub fn feed(&mut self, r: &Record) -> Result<(), String> {
        if r.destination != 0 {
            self.ignored += 1;
            return Ok(());
        }
        if r.opcode == 10 && r.sequence != 0 {
            if let Some(t) = self.tickets.iter_mut().find(|t| t.sequence == r.sequence) {
                if r.source != t.route || r.body.len() != 8 || r.body[..4] != 0x106u32.to_le_bytes()
                {
                    return Err("SW-CAN completion route/code differs from captured request".into());
                }
                if t.completed {
                    return Err("Duplicate SW-CAN transmit completion".into());
                }
                t.completed = true;
            } else {
                self.ignored += 1;
            }
        } else if r.opcode == 9 && self.listening {
            let m = r.received_message()?.ok_or("Missing SW-CAN receive body")?;
            if r.source == ISO_ROUTE
                && m.data.len() == 12
                && m.data[..4] == 0x641u32.to_be_bytes()
                && m.data[4..8] == [3, 0x7f, 0xaa, 0x78]
            {
                if r.sequence != 0 || m.status != 0 || m.extra_index != 12 {
                    return Err("Invalid CIM pending-response metadata".into());
                }
                self.pending_responses += 1;
                return Ok(());
            }
            if r.source != RAW_ROUTE || m.data.len() < 4 || m.data[..4] != 0x541u32.to_be_bytes() {
                self.ignored += 1;
                return Ok(());
            }
            if r.sequence != 0 || m.status != 0 || m.data.len() != 12 || m.extra_index != 12 {
                return Err("CIM raw status format differs from captured profile".into());
            }
            if m.data[4] != 1 {
                self.ignored += 1;
                return Ok(());
            }
            // Exact existing profile tuples only; zero/intermediate remain unknown.
            if self.tuple.is_none() {
                self.tuple = Some(m.data[6..9].try_into().unwrap());
            }
        } else {
            self.ignored += 1;
        }
        Ok(())
    }
    pub fn completed(&self, stage: &str) -> bool {
        self.tickets
            .iter()
            .any(|t| t.stage == stage && t.accepted && t.completed)
    }
    pub fn status_complete(&self) -> bool {
        self.completed("sw_cim_status") && self.tuple.is_some()
    }
    pub fn report(&self) -> Value {
        let state = match self.tuple {
            Some([6, 2, 2]) => "on",
            Some([1, 1, 0]) => "lock",
            _ => "unknown",
        };
        json!({"wake_requested":self.tickets.iter().any(|t|t.stage=="sw_wake"),
            "wake_accepted_and_completed":self.completed("sw_wake"),
            "cim_reply_verified":self.status_complete(),"cim_ignition_state":state,
            "cim_state_tuple":self.tuple,"initial_off_reference_tuple":self.tuple==Some([0,0,0]),
            "ignition_mapping":"Only captured ON=[6,2,2] and LOCK=[1,1,0] are named. Zero/intermediate stay unknown; no engine or sleep inference.",
            "pending_CIM_responses":self.pending_responses,"ignored_records":self.ignored,
            "transmits":self.tickets.iter().map(|t|json!({"stage":t.stage,"route":format!("0x{:04x}",t.route),"sequence":t.sequence,"accepted":t.accepted,"completed":t.completed})).collect::<Vec<_>>(),
            "electrical_wake_verified":false,"prior_bus_sleep_verified":false,"engine_running":Value::Null})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn r(src: u16, op: u16, seq: u16, body: Vec<u8>) -> Record {
        Record {
            destination: 0,
            source: src,
            opcode: op,
            sequence: seq,
            auxiliary: [0; 4],
            body,
        }
    }
    #[test]
    fn only_fresh_raw_cim_and_correlated_ticket_complete_status() {
        let mut x = Exchange::default();
        let mut req = r(0, 8, 23, status_request());
        req.destination = ISO_ROUTE;
        let raw = r(
            RAW_ROUTE,
            9,
            0,
            [
                0u32.to_le_bytes().to_vec(),
                vec![0; 4],
                12u16.to_le_bytes().to_vec(),
                12u16.to_le_bytes().to_vec(),
                0x541u32.to_be_bytes().to_vec(),
                vec![1, 0, 0, 0, 0, 0, 0, 0],
            ]
            .concat(),
        );
        x.feed(&raw).unwrap();
        assert!(x.tuple.is_none());
        x.issued(&req, "sw_cim_status").unwrap();
        x.feed(&raw).unwrap();
        assert!(!x.status_complete());
        x.accept_rpc(&r(
            ISO_ROUTE,
            0x8008,
            23,
            [0x100u32.to_le_bytes(), [0; 4], [0; 4]].concat(),
        ))
        .unwrap();
        x.feed(&r(
            ISO_ROUTE,
            10,
            23,
            [0x106u32.to_le_bytes(), [0; 4]].concat(),
        ))
        .unwrap();
        assert!(x.status_complete());
        assert_eq!(x.report()["cim_ignition_state"], "unknown");
        x.listening = false;
        let mut late = raw.clone();
        late.body[18..21].copy_from_slice(&[6, 2, 2]);
        x.feed(&late).unwrap();
        assert_eq!(x.tuple, Some([0, 0, 0]));
    }
    #[test]
    fn wake_is_empty_data_high_voltage_flag_and_cannot_repeat() {
        assert_eq!(
            wake(),
            [
                0x400u32.to_le_bytes().to_vec(),
                vec![0; 4],
                4u16.to_le_bytes().to_vec(),
                vec![0; 2],
                0x100u32.to_be_bytes().to_vec()
            ]
            .concat()
        );
        let mut x = Exchange::default();
        let mut q = r(0, 8, 17, wake());
        q.destination = RAW_ROUTE;
        x.issued(&q, "sw_wake").unwrap();
        assert!(x.issued(&q, "sw_wake").is_err());
        assert!(x
            .feed(&r(
                ISO_ROUTE,
                10,
                17,
                [0x106u32.to_le_bytes(), [0; 4]].concat()
            ))
            .is_err());
    }
}
