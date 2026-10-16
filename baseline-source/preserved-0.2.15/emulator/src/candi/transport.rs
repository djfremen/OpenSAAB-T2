// SPDX-License-Identifier: MPL-2.0
//! Raw-CAN boundary for the verified Nano / Trionic8 bench configuration.
//! This module does not open hardware or synthesize ISO-TP/ECU responses.
//! A caller must close/reopen the driver channel after an error, reset or abort;
//! J2534 echoes contain no ticket and cannot disambiguate an old identical TX.

use crate::candi_cpu::{CanElectricalState, CanFrame, CanTransmission};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhysicalBus {
    HighSpeed,
    Middle,
    SingleWire,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Route {
    pub controller: usize,
    pub native_channel: u8,
    pub address: u32,
    pub interrupt_level: u8,
    pub vector: u32,
    pub bus: PhysicalBus,
}

/// Native helpers 0x143aa and 0x1366c. Bus labels follow observed timing plus
/// the channel-zero wake-up helper, not the order of J2534 channel handles.
pub const ROUTES: [Route; 3] = [
    Route {
        controller: 0,
        native_channel: 4,
        address: 0x200000,
        interrupt_level: 4,
        vector: 28,
        bus: PhysicalBus::HighSpeed,
    },
    Route {
        controller: 1,
        native_channel: 2,
        address: 0x201000,
        interrupt_level: 3,
        vector: 27,
        bus: PhysicalBus::Middle,
    },
    Route {
        controller: 2,
        native_channel: 0,
        address: 0x202000,
        interrupt_level: 2,
        vector: 26,
        bus: PhysicalBus::SingleWire,
    },
];

#[derive(Debug, PartialEq, Eq)]
pub enum Event {
    Transmitted { controller: usize, ticket: u64 },
    Received { controller: usize, frame: CanFrame },
}

struct Pending {
    tx: CanTransmission,
    accepted: bool,
    deadline: Instant,
}

/// One physical channel, one outstanding frame, bounded confirmation deadline.
/// Use a fresh instance only with a freshly opened/drained driver channel.
pub struct NanoHsSession {
    pending: Option<Pending>,
    last_ticket: u64,
    closed: bool,
}

impl Default for NanoHsSession {
    fn default() -> Self {
        Self::new()
    }
}

impl NanoHsSession {
    pub fn new() -> Self {
        Self {
            pending: None,
            last_ticket: 0,
            closed: false,
        }
    }

    fn fail<T>(&mut self, reason: &str) -> Result<T, String> {
        self.close();
        Err(reason.into())
    }

    pub fn close(&mut self) {
        self.pending = None;
        self.closed = true;
    }

    pub fn check_deadline(&mut self, now: Instant) -> Result<(), String> {
        if self.closed {
            return Err("Raw CAN session closed; reopen adapter channel".into());
        }
        if self.pending.as_ref().is_some_and(|p| now >= p.deadline) {
            return self.fail(
                "CAN transmit confirmation timeout; outcome unknown; reopen adapter channel",
            );
        }
        Ok(())
    }

    /// Returns J2534 protocol-5 Data bytes (big-endian ID then unmodified CAN data).
    /// The driver must use LOOPBACK=1, standard CAN, 500000 baud, pins 6/14.
    pub fn prepare(
        &mut self,
        controller: usize,
        tx: CanTransmission,
        now: Instant,
    ) -> Result<Vec<u8>, String> {
        self.check_deadline(now)?;
        let route = ROUTES.get(controller).ok_or("Unknown CANdi controller")?;
        if route.bus != PhysicalBus::HighSpeed {
            return Err(format!("Bus unavailable: {:?}, native channel {}, window {:#x}; no remapping to high-speed CAN", route.bus, route.native_channel, route.address));
        }
        if tx.electrical != CanElectricalState::StandardCan {
            return Err("Unsupported electrical state for Nano high-speed route; no wake-up mode conversion".into());
        }
        // Known firmware 500k timing, with inferred 24MHz controller clock.
        if (tx.btr0, tx.btr1) != (0xc1, 0x36) {
            return Err("Unverified CAN bit timing for Nano 500000-baud route".into());
        }
        tx.frame.validate()?;
        if tx.frame.extended || tx.frame.rtr {
            return Err("Nano bench route supports standard data frames only".into());
        }
        if self.pending.is_some() {
            return Err("CAN transmit already pending".into());
        }
        if tx.ticket <= self.last_ticket {
            return self.fail("Stale or reused CAN transmission ticket");
        }
        let mut data = tx.frame.id.to_be_bytes().to_vec();
        data.extend_from_slice(&tx.frame.data);
        self.last_ticket = tx.ticket;
        self.pending = Some(Pending {
            tx,
            accepted: false,
            deadline: now + Duration::from_secs(1),
        });
        Ok(data)
    }

    /// Call only after PassThruWriteMsgs returned success. Errors must close().
    /// This does not complete the emulated transmission or raise its interrupt.
    pub fn driver_accepted(&mut self, count: u32, now: Instant) -> Result<(), String> {
        self.check_deadline(now)?;
        if count != 1 || self.pending.as_ref().is_none_or(|p| p.accepted) {
            return self.fail("Unexpected raw CAN driver acceptance");
        }
        self.pending.as_mut().unwrap().accepted = true;
        Ok(())
    }

    /// Decode a driver message. TX_MSG_TYPE (RxStatus=1) is confirmation;
    /// RxStatus=0 is a received frame. Neither is an ISO-TP assembled payload.
    pub fn receive(
        &mut self,
        protocol: u32,
        status: u32,
        data: &[u8],
        now: Instant,
    ) -> Result<Event, String> {
        self.check_deadline(now)?;
        if protocol != 5 || !matches!(status, 0 | 1) || !(4..=12).contains(&data.len()) {
            return self.fail("Unsupported or malformed raw CAN driver message");
        }
        let frame = CanFrame {
            id: u32::from_be_bytes(data[..4].try_into().unwrap()),
            extended: false,
            rtr: false,
            dlc: (data.len() - 4) as u8,
            data: data[4..].to_vec(),
        };
        if frame.validate().is_err() {
            return self.fail("Invalid raw CAN driver identifier");
        }
        if status == 0 {
            // Matches the bench PASS_FILTER. Expand only with an explicit route profile.
            if frame.id != 0x7e8 {
                return self.fail("Unexpected CAN ID outside Nano bench receive filter");
            }
            return Ok(Event::Received {
                controller: 0,
                frame,
            });
        }
        if !self
            .pending
            .as_ref()
            .is_some_and(|p| p.accepted && p.tx.frame == frame)
        {
            return self.fail("Unmatched CAN transmit confirmation; reopen adapter channel");
        }
        let pending = self.pending.take().unwrap();
        Ok(Event::Transmitted {
            controller: 0,
            ticket: pending.tx.ticket,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires the untracked live Nano raw identity capture"]
    fn local_raw_identity_capture_validates_driver_event_order() {
        let log =
            std::fs::read_to_string("runs/nano-raw-can-identity-20260907/stdout.log").unwrap();
        let origin = Instant::now();
        let mut s = NanoHsSession::new();
        let (mut ticket, mut echoed, mut received, mut identity) = (0, 0, 0, 0);
        fn hex(s: &str) -> Vec<u8> {
            s.split('-')
                .map(|b| u8::from_str_radix(b, 16).unwrap())
                .collect()
        }
        for line in log.lines() {
            let Some(elapsed) = line.split_once(" +").and_then(|(_, s)| s.split_once("ms ")) else {
                continue;
            };
            let now = origin + Duration::from_millis(elapsed.0.parse().unwrap());
            let body = elapsed.1;
            if let Some(txdata) = body.strip_prefix("RAW_TX id=7E0 data=") {
                let data = hex(txdata.split_whitespace().next().unwrap());
                ticket += 1;
                s.prepare(
                    0,
                    CanTransmission {
                        ticket,
                        frame: CanFrame {
                            dlc: data.len() as u8,
                            data,
                            ..tx().frame
                        },
                        ..tx()
                    },
                    now,
                )
                .unwrap();
            } else if body == "RAW PassThruWriteMsgs rc=0x00000000"
                || body == "RAW flow control rc=0x00000000"
            {
                s.driver_accepted(1, now).unwrap();
            } else if body.starts_with("RAW_RX ") {
                let fields: std::collections::HashMap<_, _> = body
                    .split_whitespace()
                    .filter_map(|s| s.split_once('='))
                    .collect();
                match s
                    .receive(
                        fields["protocol"].parse().unwrap(),
                        u32::from_str_radix(fields["status"], 16).unwrap(),
                        &hex(fields["bytes"]),
                        now,
                    )
                    .unwrap()
                {
                    Event::Transmitted { .. } => echoed += 1,
                    Event::Received { .. } => received += 1,
                }
            } else if body.starts_with("RAW_IDENTITY ") {
                identity += 1;
            }
        }
        assert_eq!(identity, 7);
        assert_eq!(ticket, 13);
        assert_eq!(echoed, ticket);
        assert_eq!(received, 18);
        assert!(s.pending.is_none());
        assert!(log.contains("PROBE_RESULT=0"));
        assert!(log.contains("HELPER_EXIT=0"));
    }
    fn tx() -> CanTransmission {
        CanTransmission {
            electrical: CanElectricalState::StandardCan,
            ticket: 1,
            btr0: 0xc1,
            btr1: 0x36,
            frame: CanFrame {
                id: 0x7e0,
                extended: false,
                rtr: false,
                dlc: 8,
                data: vec![2, 0x1a, 0x90, 0, 0, 0, 0, 0],
            },
        }
    }
    #[test]
    fn captured_nano_loopback_and_ecu_response_have_different_meanings() {
        let now = Instant::now();
        let mut s = NanoHsSession::new();
        let packet = s.prepare(0, tx(), now).unwrap();
        s.driver_accepted(1, now).unwrap();
        // Raw frames from the live 2026-09-07 bench capture; offline fixture.
        assert_eq!(
            s.receive(5, 1, &packet, now).unwrap(),
            Event::Transmitted {
                controller: 0,
                ticket: 1
            }
        );
        let rx = [
            0, 0, 7, 0xe8, 0x10, 0x13, 0x5a, 0x90, 0x59, 0x53, 0x33, 0x46,
        ];
        match s.receive(5, 0, &rx, now).unwrap() {
            Event::Received { controller, frame } => {
                assert_eq!(controller, 0);
                assert_eq!(frame.id, 0x7e8);
                assert_eq!(frame.data, &rx[4..]);
            }
            _ => panic!("ECU response confused with driver confirmation"),
        }
        assert!(s.receive(5, 1, &packet, now).is_err()); // Duplicate completion poisons session.
        assert!(s.check_deadline(now).is_err());
    }
    #[test]
    fn missing_buses_and_wrong_timing_never_reach_adapter() {
        let now = Instant::now();
        let mut s = NanoHsSession::new();
        assert!(s.prepare(2, tx(), now).unwrap_err().contains("SingleWire"));
        assert!(s.prepare(1, tx(), now).unwrap_err().contains("Middle"));
        assert!(s.prepare(3, tx(), now).is_err());
        assert!(s
            .prepare(0, CanTransmission { btr0: 0xdd, ..tx() }, now)
            .is_err());
        assert!(s.prepare(0, tx(), now).is_ok());
    }
    #[test]
    fn acceptance_without_echo_times_out_and_cannot_be_reused() {
        let now = Instant::now();
        let mut s = NanoHsSession::new();
        let data = s.prepare(0, tx(), now).unwrap();
        s.driver_accepted(1, now).unwrap();
        assert!(s.check_deadline(now + Duration::from_millis(999)).is_ok());
        assert!(s
            .check_deadline(now + Duration::from_secs(1))
            .unwrap_err()
            .contains("timeout"));
        assert!(s
            .receive(5, 1, &data, now + Duration::from_secs(1))
            .is_err());
        assert!(s
            .prepare(0, CanTransmission { ticket: 2, ..tx() }, now)
            .is_err());
    }
    #[test]
    fn hs_route_rejects_unspecified_or_single_wire_electrical_state() {
        let mut s = NanoHsSession::new();
        let now = Instant::now();
        for electrical in [
            CanElectricalState::Unspecified,
            CanElectricalState::SingleWireGpio {
                latch: 2,
                assignment: 0,
                direction: 3,
            },
        ] {
            assert!(s
                .prepare(0, CanTransmission { electrical, ..tx() }, now)
                .unwrap_err()
                .contains("electrical state"));
        }
        // Rejected requests did not reserve or submit a packet.
        assert!(s.prepare(0, tx(), now).is_ok());
    }
    #[test]
    fn malformed_and_unmatched_messages_close_session() {
        let now = Instant::now();
        for (protocol, status, data) in [
            (6, 0, vec![0, 0, 7, 0xe8]),
            (5, 2, vec![0, 0, 7, 0xe8]),
            (5, 0, vec![0, 0, 8, 0]),
            (5, 0, vec![0; 13]),
            (5, 0, vec![0; 3]),
            (5, 1, vec![0, 0, 7, 0xe0]),
        ] {
            let mut s = NanoHsSession::new();
            s.prepare(0, tx(), now).unwrap();
            s.driver_accepted(1, now).unwrap();
            assert!(s.receive(protocol, status, &data, now).is_err());
            assert!(s.check_deadline(now).is_err());
        }
        let mut s = NanoHsSession::new();
        s.prepare(0, tx(), now).unwrap();
        assert!(s.driver_accepted(0, now).is_err());
    }
}
