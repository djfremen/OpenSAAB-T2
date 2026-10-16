// SPDX-License-Identifier: MPL-2.0
//! Hardware-independent native CANdi -> Nano driver protocol core.
//! No adapter is opened here. The caller owns USB and must report its exact
//! write outcome. USB acceptance models OEM software-loopback semantics only;
//! diagnostic success still requires a fresh ECU response through guest firmware.
use crate::{
    can_adapter::{electrical_route, CompletionSource, Event},
    candi_cpu::{CanFrame, CanTransmission},
    nano_channel::RawCan,
    nano_usb::Frame,
};
use std::time::{Duration, Instant};

pub use crate::adapters::common::policy::{
    allowed_for_profile, allowed_for_session, diagnostic_allowed, CommandGate, Profile,
};

/// Pure encoding. It preserves every original data/padding byte, including
/// guest ISO-TP headers; no host segmentation, query generation or ECU rewriting.
pub fn encode(controller: usize, tx: &CanTransmission, sequence: u8) -> Result<Frame, String> {
    encode_for_session(controller, tx, sequence, false)
}
pub fn encode_for_session(
    controller: usize,
    tx: &CanTransmission,
    sequence: u8,
    seeds: bool,
) -> Result<Frame, String> {
    encode_for_profile(controller, tx, sequence, Profile::collection(seeds))
}
pub fn encode_for_profile(
    controller: usize,
    tx: &CanTransmission,
    sequence: u8,
    profile: Profile,
) -> Result<Frame, String> {
    if !allowed_for_profile(controller, tx, profile) {
        return Err("Native Android session gate rejected request".into());
    }
    encode_authorized(controller, tx, sequence)
}
fn encode_authorized(
    controller: usize,
    tx: &CanTransmission,
    sequence: u8,
) -> Result<Frame, String> {
    let route = electrical_route(controller, tx)?;
    let mut payload = route.wire_flags.to_be_bytes().to_vec();
    payload.extend_from_slice(&((4 + tx.frame.data.len()) as u16).to_be_bytes());
    payload.extend_from_slice(&tx.frame.id.to_be_bytes());
    payload.extend_from_slice(&tx.frame.data);
    Ok(Frame {
        header: [0x80, sequence, 0, route.channel],
        payload,
    })
}

pub fn receive(frame: &Frame) -> Result<Event, String> {
    // The failing Android DTC capture returned 80 61 00 01 | 9B.
    // Preserve adapter status separately from CAN data. Its precise meaning is
    // not established, and it must never become an ECU reply or TX success.
    if frame.header[0] == 0x80
        && frame.header[2] == 0
        && frame.header[3] <= 1
        && frame.payload.len() == 1
    {
        return Err(format!(
            "Nano adapter status {:02X} on channel {} sequence {:02X}; no CAN frame returned",
            frame.payload[0], frame.header[3], frame.header[1]
        ));
    }
    let raw = RawCan::decode(frame)?;
    // Only flag-zero, standard data RX is proven by this capture-backed profile.
    // Unknown flags are surfaced, not silently stripped or treated as loopback.
    if raw.flags != 0 || raw.id > 0x7ff {
        return Err("Unverified Nano RX flags or extended identifier".into());
    }
    let frame = CanFrame {
        id: raw.id,
        extended: false,
        rtr: false,
        dlc: raw.data.len() as u8,
        data: raw.data,
    };
    frame.validate()?;
    Ok(Event::Received(if raw.channel == 0 { 0 } else { 2 }, frame))
}

#[derive(Clone, Debug)]
pub struct Prepared {
    pub controller: usize,
    pub ticket: u64,
    pub wire: Vec<u8>,
    deadline: Instant,
}

/// One in-flight write per native controller. A short/error/late/duplicate
/// completion poisons this session so a replacement USB connection starts fresh.
/// Wire sequence bytes wrap; the native ticket remains the completion identity.
pub struct TxLedger {
    pending: [Option<Prepared>; 3],
    last_ticket: [u64; 3],
    sequence: [u8; 3],
    closed: bool,
    gate: CommandGate,
}
impl Default for TxLedger {
    fn default() -> Self {
        Self {
            pending: std::array::from_fn(|_| None),
            last_ticket: [0; 3],
            sequence: [0; 3],
            closed: false,
            gate: CommandGate::default(),
        }
    }
}
impl TxLedger {
    pub fn close(&mut self) {
        self.closed = true;
        self.gate = CommandGate::default();
        self.pending = std::array::from_fn(|_| None);
    }
    pub fn is_closed(&self) -> bool {
        self.closed
    }
    fn fail<T>(&mut self, message: &str) -> Result<T, String> {
        self.close();
        Err(message.into())
    }
    pub fn check_deadline(&mut self, now: Instant) -> Result<(), String> {
        if self.closed {
            return Err("Native Nano session closed; create a fresh connection".into());
        }
        if self.pending.iter().flatten().any(|p| now >= p.deadline) {
            return self.fail("Native Nano USB write deadline expired; outcome unknown");
        }
        Ok(())
    }
    pub fn prepare(
        &mut self,
        controller: usize,
        tx: &CanTransmission,
        now: Instant,
    ) -> Result<Prepared, String> {
        self.prepare_for_session(controller, tx, now, false)
    }
    pub fn prepare_for_session(
        &mut self,
        controller: usize,
        tx: &CanTransmission,
        now: Instant,
        seeds: bool,
    ) -> Result<Prepared, String> {
        self.prepare_for_profile(controller, tx, now, Profile::collection(seeds))
    }
    pub fn prepare_for_profile(
        &mut self,
        controller: usize,
        tx: &CanTransmission,
        now: Instant,
        profile: Profile,
    ) -> Result<Prepared, String> {
        self.check_deadline(now)?;
        // Validate before indexing arrays or mutating any ledger state.
        let sequence = self
            .sequence
            .get(controller)
            .copied()
            .unwrap_or(0)
            .wrapping_add(1);
        if self.pending.get(controller).is_some_and(Option::is_some) {
            return Err("Native Nano write already pending on this controller".into());
        }
        if self
            .last_ticket
            .get(controller)
            .is_some_and(|last| tx.ticket <= *last)
        {
            return self.fail("Stale/reused native Nano transmission ticket");
        }
        if !self.gate.allowed(controller, tx, profile, now) {
            return Err("Native Android command/continuation gate rejected request".into());
        }
        let frame = encode_authorized(controller, tx, sequence)?;
        let prepared = Prepared {
            controller,
            ticket: tx.ticket,
            wire: frame.encode()?,
            deadline: now + Duration::from_secs(2),
        };
        self.last_ticket[controller] = tx.ticket;
        self.sequence[controller] = sequence;
        self.pending[controller] = Some(prepared.clone());
        Ok(prepared)
    }
    /// Call only after the transport's full-write result, never on queued work.
    /// The returned source names the compatibility behavior; it is NOT a CAN ACK.
    pub fn usb_write_finished(
        &mut self,
        controller: usize,
        ticket: u64,
        outcome: Result<usize, String>,
        now: Instant,
    ) -> Result<Event, String> {
        self.check_deadline(now)?;
        let Some(pending) = self.pending.get(controller).and_then(Option::as_ref) else {
            return self.fail("USB completion has no pending native ticket");
        };
        if pending.ticket != ticket {
            return self.fail("USB completion does not match the pending native ticket");
        }
        match outcome {
            Ok(n) if n == pending.wire.len() => {}
            _ => return self.fail("Native Nano USB write failed or was short; outcome unknown"),
        }
        self.pending[controller] = None;
        Ok(Event::Completed {
            controller,
            ticket,
            source: CompletionSource::VcxUsbWriteCompatibility,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::candi_cpu::CanElectricalState;
    fn tx(controller: usize, ticket: u64, id: u32, data: &[u8]) -> CanTransmission {
        CanTransmission {
            ticket,
            frame: CanFrame {
                id,
                extended: false,
                rtr: false,
                dlc: data.len() as u8,
                data: data.to_vec(),
            },
            btr0: if controller == 0 { 0xc1 } else { 0xdd },
            btr1: 0x36,
            electrical: if controller == 0 {
                CanElectricalState::StandardCan
            } else {
                CanElectricalState::SingleWireGpio {
                    latch: 3,
                    assignment: 0,
                    direction: 3,
                }
            },
        }
    }
    #[test]
    fn engine_multiframe_preserves_payload_and_enforces_transaction_boundaries() {
        let now = Instant::now();
        let first = tx(0, 1, 0x7e0, &[0x10, 0x10, 0x2c, 0xf3, 0, 0x0f, 0x11, 0]);
        let one = tx(0, 2, 0x7e0, &[0x21, 1, 2, 3, 4, 5, 6, 7]);
        let two = tx(0, 3, 0x7e0, &[0x22, 8, 9, 10, 0, 0, 0, 0]);
        let mut ledger = TxLedger::default();
        let p = ledger.prepare(0, &first, now).unwrap();
        assert!(ledger.prepare(0, &one, now).is_err()); // Must not advance stream while FF write pending.
        ledger
            .usb_write_finished(0, 1, Ok(p.wire.len()), now)
            .unwrap();
        let p = ledger.prepare(0, &one, now).unwrap();
        ledger
            .usb_write_finished(0, 2, Ok(p.wire.len()), now)
            .unwrap();
        let p = ledger.prepare(0, &two, now).unwrap();
        ledger
            .usb_write_finished(0, 3, Ok(p.wire.len()), now)
            .unwrap();
        assert!(ledger
            .prepare(0, &tx(0, 4, 0x7e0, &[0x23, 0, 0, 0, 0, 0, 0, 0]), now)
            .is_err());
        for bad in [
            tx(0, 2, 0x7e0, &[0x22, 0, 0, 0, 0, 0, 0, 0]),
            tx(2, 2, 0x241, &one.frame.data),
            tx(0, 2, 0x7e0, &[0x21, 0]),
        ] {
            let mut gate = CommandGate::default();
            assert!(gate.allowed(0, &first, Profile::Read, now));
            assert!(!gate.allowed(
                if bad.frame.id == 0x241 { 2 } else { 0 },
                &bad,
                Profile::Read,
                now
            ));
        }
        let mut gate = CommandGate::default();
        assert!(!gate.allowed(0, &one, Profile::Read, now));
        assert!(gate.allowed(0, &first, Profile::Read, now));
        assert!(!gate.allowed(0, &first, Profile::Read, now));
        assert!(!gate.allowed(0, &one, Profile::Seeds, now));
        assert!(!gate.allowed(0, &one, Profile::Read, now + Duration::from_secs(3)));
        for data in [
            [0x10, 0x10, 0x34, 0xf3, 0, 0, 0, 0],
            [0x10, 0x10, 0x27, 0xf3, 0, 0, 0, 0],
            [0x10, 0x10, 0x2c, 0xff, 0, 0, 0, 0],
            [0x10, 65, 0x2c, 0xf3, 0, 0, 0, 0],
        ] {
            assert!(!CommandGate::default().allowed(
                0,
                &tx(0, 1, 0x7e0, &data),
                Profile::Read,
                now
            ));
        }
    }

    #[test]
    fn engine_segmented_periodic_reads_validate_all_packet_ids() {
        let now = Instant::now();
        let first = tx(0, 1, 0x7e0, &[0x10, 0x0d, 0xaa, 4, 0x12, 0x13, 0x15, 0x16]);
        let mut gate = CommandGate::default();
        assert!(gate.allowed(0, &first, Profile::Read, now));
        assert!(!gate.allowed(
            0,
            &tx(0, 2, 0x7e0, &[0x21, 0xff, 1, 2, 3, 4, 5, 6]),
            Profile::Read,
            now
        ));
        assert!(gate.allowed(
            0,
            &tx(
                0,
                2,
                0x7e0,
                &[0x21, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d]
            ),
            Profile::Read,
            now
        ));
        assert!(!gate.allowed(
            0,
            &tx(0, 3, 0x7e0, &[0x22, 1, 2, 3, 4, 5, 6, 7]),
            Profile::Read,
            now
        ));
        for data in [
            [0x10, 13, 0xaa, 5, 1, 2, 3, 4],
            [0x10, 13, 0xaa, 4, 0xff, 2, 3, 4],
            [0x10, 33, 0xaa, 4, 1, 2, 3, 4],
        ] {
            assert!(!CommandGate::default().allowed(
                0,
                &tx(0, 1, 0x7e0, &data),
                Profile::Read,
                now
            ));
        }
    }

    #[test]
    fn engine_packet_definitions_are_read_configuration_only() {
        // Original 2004 T8 engine-entry request captured through Nano J2534.
        let t8_packet = [4, 0x2c, 0xfe, 0x03, 0x8e, 0, 0, 0];
        assert!(diagnostic_allowed(0, &tx(0, 1, 0x7e0, &t8_packet)));
        assert!(!diagnostic_allowed(2, &tx(2, 1, 0x241, &t8_packet)));
        for dpid in 0xf3..=0xfe {
            let data = [0x10, 0x0c, 0x2c, dpid, 0x14, 0x70, 0x15, 0x34];
            assert!(CommandGate::default().allowed(
                0,
                &tx(0, 1, 0x7e0, &data),
                Profile::Read,
                Instant::now()
            ));
            assert!(diagnostic_allowed(
                0,
                &tx(0, 1, 0x7e0, &[6, 0x2c, dpid, 0, 1, 0, 2])
            ));
        }
        for rate in 0..=4 {
            assert!(diagnostic_allowed(
                0,
                &tx(0, 1, 0x7e0, &[4, 0xaa, rate, 0xf3, 0xf4])
            ));
            assert!(!diagnostic_allowed(
                2,
                &tx(2, 1, 0x241, &[4, 0xaa, rate, 0xf3, 0xf4])
            ));
        }
        assert!(diagnostic_allowed(0, &tx(0, 1, 0x7e0, &[2, 0xaa, 0])));
        assert!(!diagnostic_allowed(
            0,
            &tx(0, 1, 0x7e0, &[3, 0xaa, 5, 0xf3])
        ));
        for pid in [0x0001u16, 0x001c, 0x1234] {
            let d = [4, 0x2c, 0xf3, (pid >> 8) as u8, pid as u8, 0, 0, 0];
            assert!(diagnostic_allowed(0, &tx(0, 1, 0x7e0, &d)));
            assert!(!diagnostic_allowed(2, &tx(2, 1, 0x241, &d)));
        }
        for d in [
            vec![4, 0x2c, 0xff, 0, 1],
            vec![3, 0x2c, 0xf3, 1],
            vec![5, 0x2c, 0xf3, 0, 1, 2],
            vec![1, 4],
            vec![1, 0x34],
            vec![2, 0x27, 2],
        ] {
            assert!(!diagnostic_allowed(0, &tx(0, 1, 0x7e0, &d)));
        }
    }

    #[test]
    fn sps_state_query_does_not_admit_programming_or_other_routes() {
        let data = [0xfe, 1, 0xa2, 0, 0, 0, 0, 0];
        assert!(diagnostic_allowed(0, &tx(0, 1, 0x101, &data)));
        assert!(!diagnostic_allowed(2, &tx(2, 1, 0x101, &data)));
        assert!(!diagnostic_allowed(0, &tx(0, 1, 0x242, &data)));
        for service in [0xa5, 0x28, 0x34, 0x36, 0x3b, 0x27, 0x04] {
            let mut denied = data;
            denied[2] = service;
            assert!(!diagnostic_allowed(0, &tx(0, 1, 0x101, &denied)));
        }
        for length in 0..8 {
            assert!(!diagnostic_allowed(0, &tx(0, 1, 0x101, &data[..length])));
        }
        let mut denied = data;
        denied[7] = 1;
        assert!(!diagnostic_allowed(0, &tx(0, 1, 0x101, &denied)));
    }

    #[test]
    fn seed_collection_covers_physical_modules_on_both_buses_without_keys() {
        for controller in [0, 2] {
            for id in 0x240..=0x25f {
                let seed = tx(controller, 1, id, &[2, 0x27, 1, 0, 0, 0, 0, 0]);
                assert!(allowed_for_profile(controller, &seed, Profile::Seeds));
                assert!(!diagnostic_allowed(controller, &seed));
                for data in [vec![4, 0x27, 2, 1, 2], vec![4, 0x3b, 1, 0x99, 0xc0]] {
                    assert!(!allowed_for_profile(
                        controller,
                        &tx(controller, 1, id, &data),
                        Profile::Seeds
                    ));
                }
            }
        }
    }

    #[test]
    fn tcm_dtc_reads_use_high_speed_7e1_without_write_authority() {
        for data in [
            vec![3, 0xa9, 0x81, 0x10, 0, 0, 0, 0],
            vec![3, 0xa9, 0x81, 0x12],
            vec![3, 0xa9, 0x81, 0x0c],
            vec![0x30, 0, 0],
            vec![2, 0x1a, 0x90],
        ] {
            assert!(diagnostic_allowed(0, &tx(0, 1, 0x7e1, &data)));
            assert!(!diagnostic_allowed(2, &tx(2, 1, 0x7e1, &data)));
            assert!(!diagnostic_allowed(0, &tx(0, 1, 0x7e2, &data)));
        }
        for data in [
            vec![1, 4],
            vec![3, 0xa9, 0x81, 0x0a],
            vec![2, 0x27, 1],
            vec![4, 0x27, 2, 1, 2],
            vec![4, 0x3b, 1, 0x99, 0],
        ] {
            assert!(!diagnostic_allowed(0, &tx(0, 1, 0x7e1, &data)));
        }
    }

    #[test]
    fn clear_is_explicit_and_preserves_original_wire_payload() {
        for (c, id, data) in [
            (0, 0x7e0, vec![1, 4, 0, 0, 0, 0, 0, 0]),
            (2, 0x241, vec![1, 4]),
            (2, 0x101, vec![0xfe, 1, 4, 0, 0, 0, 0, 0]),
        ] {
            let request = tx(c, 1, id, &data);
            for profile in [Profile::Read, Profile::Seeds] {
                assert!(!allowed_for_profile(c, &request, profile));
            }
            let encoded = encode_for_profile(c, &request, 1, Profile::ClearDtc).unwrap();
            assert_eq!(&encoded.payload[10..], data.as_slice());
            assert!(TxLedger::default()
                .prepare_for_profile(c, &request, Instant::now(), Profile::ClearDtc)
                .is_ok());
        }
        assert!(allowed_for_profile(
            2,
            &tx(2, 1, 0x257, &[3, 0xa9, 0x81, 0x0a, 0, 0, 0, 0]),
            Profile::ClearDtc
        ));
        assert!(!diagnostic_allowed(
            2,
            &tx(2, 1, 0x257, &[3, 0xa9, 0x81, 0x0a])
        ));
        for data in [
            vec![2, 4, 0],
            vec![1, 0x14],
            vec![2, 0x27, 1],
            vec![4, 0x27, 2, 1, 2],
            vec![2, 0xae, 0],
            vec![1, 0x34],
            vec![1, 0x11],
        ] {
            assert!(!allowed_for_profile(
                2,
                &tx(2, 1, 0x241, &data),
                Profile::ClearDtc
            ));
        }
        assert!(!allowed_for_profile(
            2,
            &tx(2, 1, 0x777, &[1, 4]),
            Profile::ClearDtc
        ));
        assert!(!allowed_for_profile(
            1,
            &tx(1, 1, 0x241, &[1, 4]),
            Profile::ClearDtc
        ));
    }

    #[test]
    fn seed_collection_requires_explicit_profile_and_never_accepts_keys() {
        for (c, id, data) in [
            (2, 0x241, vec![2, 0xae, 0, 0, 0, 0, 0, 0]),
            (2, 0x241, vec![2, 0x27, 0x0b, 0, 0, 0, 0, 0]),
            (2, 0x244, vec![2, 0x27, 1, 0, 0, 0, 0, 0]),
            (0, 0x7e1, vec![2, 0x27, 1, 0, 0, 0, 0, 0]),
        ] {
            let request = tx(c, 1, id, &data);
            assert!(!diagnostic_allowed(c, &request));
            assert!(allowed_for_session(c, &request, true));
            assert!(encode_for_session(c, &request, 1, true).is_ok());
        }
        for data in [
            &[4, 0x27, 2, 0x12, 0x34, 0, 0, 0][..],
            &[2, 0x27, 0x0c, 0, 0, 0, 0, 0],
            &[2, 0xae, 1, 0, 0, 0, 0, 0],
            &[0xfe, 2, 0x27, 1, 0, 0, 0, 0],
            &[1, 0x14, 0, 0, 0, 0, 0, 0],
        ] {
            assert!(!allowed_for_session(2, &tx(2, 1, 0x241, data), true));
        }
        assert!(!allowed_for_session(
            0,
            &tx(0, 1, 0x241, &[2, 0x27, 0x0b, 0, 0, 0, 0, 0]),
            true
        ));
        assert!(!allowed_for_session(
            2,
            &tx(2, 1, 0x242, &[2, 0xae, 0, 0, 0, 0, 0, 0]),
            true
        ));
    }
    #[test]
    #[ignore = "requires NANO_DTC_USB_LOG private Android capture; no hardware is opened"]
    fn android_dtc_failure_capture_preserves_adapter_status() {
        let text = std::fs::read_to_string(std::env::var("NANO_DTC_USB_LOG").unwrap()).unwrap();
        let mut decoder = crate::nano_usb::Decoder::default();
        let mut can_frames = 0;
        let mut statuses = Vec::new();
        for line in text.lines() {
            let Some((_, wire)) = line.split_once(" RX ") else {
                continue;
            };
            let wire = wire.split_whitespace().next().unwrap();
            let bytes: Vec<_> = (0..wire.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&wire[i..i + 2], 16).unwrap())
                .collect();
            for frame in decoder.feed(&bytes).unwrap() {
                if frame.header[2] != 0 {
                    continue;
                }
                match receive(&frame) {
                    Ok(Event::Received(..)) => can_frames += 1,
                    Ok(_) => panic!("Capture RX cannot become TX completion"),
                    Err(error) => statuses.push(error),
                }
            }
        }
        decoder.finish().unwrap();
        assert!(can_frames > 90_000);
        assert_eq!(
            statuses,
            ["Nano adapter status 9B on channel 1 sequence 61; no CAN frame returned"]
        );
        println!("PRIVATE_ANDROID_DTC_REPLAY can_frames={can_frames} adapter_status=9B checksum_errors=0 hardware_opened=false");
    }
    #[test]
    #[ignore = "requires private native DTC driver capture; no hardware is opened"]
    fn full_native_capture_matches_android_raw_protocol() {
        let text =
            std::fs::read_to_string("runs/native-dtc-20260908-215951Z/windows-vcx-driver.log")
                .unwrap();
        fn unhex(s: &str) -> Vec<u8> {
            s.split_whitespace()
                .map(|b| u8::from_str_radix(b, 16).unwrap())
                .collect()
        }
        let mut tx_count = 0;
        let mut rx_count = 0;
        let mut cim_status = false;
        for line in text.lines() {
            let Some((_, record)) = line.split_once("] ") else {
                continue;
            };
            let Some((direction, record)) = record.split_once(' ') else {
                continue;
            };
            if direction != ">" && direction != "<" {
                continue;
            }
            let parts: Vec<_> = record.split('|').collect();
            if parts.len() != 3 {
                continue;
            }
            let header = unhex(parts[0]);
            if header.len() != 4 || header[2] != 0 {
                continue;
            }
            let payload = unhex(parts[1]);
            let checksum = unhex(parts[2]);
            assert_eq!(
                checksum,
                [header
                    .iter()
                    .chain(payload.iter())
                    .fold(0u8, |a, b| a.wrapping_add(*b))]
            );
            let frame = Frame {
                header: header.try_into().unwrap(),
                payload,
            };
            if direction == ">" {
                let raw = RawCan::decode(&frame).unwrap();
                let c = if raw.channel == 0 { 0 } else { 2 };
                let mut request = tx(c, tx_count + 1, raw.id, &raw.data);
                if raw.flags == 0x1000 {
                    request.electrical = CanElectricalState::SingleWireGpio {
                        latch: 2,
                        assignment: 0,
                        direction: 3,
                    };
                } else {
                    assert_eq!(raw.flags, 0);
                }
                assert_eq!(
                    encode(c, &request, raw.sequence).unwrap(),
                    frame,
                    "captured TX #{tx_count}"
                );
                let decoded = crate::nano_usb::Decoder::default()
                    .feed(&frame.encode().unwrap())
                    .unwrap();
                assert_eq!(decoded, [frame]);
                tx_count += 1;
            } else {
                let Event::Received(c, can) = receive(&frame).unwrap() else {
                    panic!("RX became completion")
                };
                if c == 2 && can.id == 0x541 && can.data == [0x81, 0x45, 0x47, 4, 0x11, 0, 0, 0] {
                    cim_status = true;
                }
                rx_count += 1;
            }
        }
        assert_eq!((tx_count, rx_count), (223, 110480));
        assert!(cim_status);
        println!("PRIVATE_CAPTURE_CHECK tx={tx_count} rx={rx_count} original_cim_status_preserved=true hardware_opened=false");
    }
    #[test]
    fn exact_captured_cim_request_and_wake_routing() {
        let tx = tx(2, 1, 0x241, &[3, 0xa9, 0x81, 0x10, 0, 0, 0, 0]);
        let frame = encode(2, &tx, 0x96).unwrap();
        assert_eq!(frame.header, [0x80, 0x96, 0, 1]);
        assert_eq!(
            frame.payload,
            [0, 0, 0, 0, 0, 12, 0, 0, 2, 0x41, 3, 0xa9, 0x81, 0x10, 0, 0, 0, 0]
        );
        let mut wake = tx.clone();
        wake.frame.id = 0x100;
        wake.frame.dlc = 0;
        wake.frame.data.clear();
        wake.electrical = CanElectricalState::SingleWireGpio {
            latch: 2,
            assignment: 0,
            direction: 3,
        };
        assert_eq!(
            encode(2, &wake, 1).unwrap().payload,
            [0, 0, 0x10, 0, 0, 4, 0, 0, 1, 0]
        );
        assert!(encode(0, &wake, 1).is_err());
        assert!(encode(1, &tx, 1).is_err());
    }
    #[test]
    fn gate_rejects_writes_wrong_timing_and_unknown_electrical_modes() {
        for data in [
            [2, 0x27, 1],
            [2, 0x27, 2],
            [1, 4, 0],
            [2, 0x34, 0],
            [2, 0xae, 0],
        ] {
            assert!(!diagnostic_allowed(2, &tx(2, 1, 0x241, &data)));
        }
        let mut request = tx(0, 1, 0x7e0, &[2, 0x1a, 0x90]);
        assert!(diagnostic_allowed(0, &request));
        request.btr0 = 0;
        assert!(encode(0, &request, 1).is_err());
        request = tx(2, 1, 0x241, &[3, 0xaa, 1, 1]);
        request.electrical = CanElectricalState::Unspecified;
        assert!(encode(2, &request, 1).is_err());
        assert!(!diagnostic_allowed(0, &tx(0, 1, 0x7e0, &[7, 0x1a, 0x90])));
        assert!(!diagnostic_allowed(
            0,
            &tx(0, 1, 0x7e0, &[0x10, 19, 0x1a, 0x90])
        ));
    }
    #[test]
    fn pending_per_controller_and_usb_completion_provenance() {
        let now = Instant::now();
        let mut ledger = TxLedger::default();
        let a = tx(0, 1, 0x7e0, &[2, 0x1a, 0x90]);
        let b = tx(2, 9, 0x241, &[3, 0xa9, 0x81, 0x10]);
        let pa = ledger.prepare(0, &a, now).unwrap();
        let pb = ledger.prepare(2, &b, now).unwrap();
        assert!(ledger.prepare(0, &a, now).is_err());
        assert_eq!(
            ledger
                .usb_write_finished(2, 9, Ok(pb.wire.len()), now)
                .unwrap(),
            Event::Completed {
                controller: 2,
                ticket: 9,
                source: CompletionSource::VcxUsbWriteCompatibility
            }
        );
        assert_eq!(
            ledger
                .usb_write_finished(0, 1, Ok(pa.wire.len()), now)
                .unwrap(),
            Event::Completed {
                controller: 0,
                ticket: 1,
                source: CompletionSource::VcxUsbWriteCompatibility
            }
        );
        assert!(ledger
            .usb_write_finished(0, 1, Ok(pa.wire.len()), now)
            .is_err());
        assert!(ledger.is_closed());
    }
    #[test]
    fn short_failed_late_or_wrong_ticket_never_completes() {
        for kind in 0..4 {
            let now = Instant::now();
            let mut ledger = TxLedger::default();
            let t = tx(0, 1, 0x7e0, &[2, 0x1a, 0x90]);
            let p = ledger.prepare(0, &t, now).unwrap();
            let outcome = match kind {
                0 => Ok(p.wire.len() - 1),
                1 => Err("detached".into()),
                _ => Ok(p.wire.len()),
            };
            let at = if kind == 2 {
                now + Duration::from_secs(2)
            } else {
                now
            };
            let ticket = if kind == 3 { 2 } else { 1 };
            assert!(ledger.usb_write_finished(0, ticket, outcome, at).is_err());
            assert!(ledger.is_closed());
            assert!(ledger.prepare(0, &t, now).is_err());
        }
    }
    #[test]
    fn received_data_is_not_completion_and_unknown_flags_fail() {
        let mut f = Frame {
            header: [0x80, 0x96, 0, 1],
            payload: vec![
                0, 0, 0, 0, 0, 12, 0, 0, 5, 0x41, 0x81, 0x45, 0x47, 4, 0x11, 0, 0, 0,
            ],
        };
        let Event::Received(c, rx) = receive(&f).unwrap() else {
            panic!("RX cannot complete TX")
        };
        assert_eq!(c, 2);
        assert_eq!(rx.data, [0x81, 0x45, 0x47, 4, 0x11, 0, 0, 0]);
        f.payload[3] = 1;
        assert!(receive(&f).is_err());
    }
    #[test]
    fn captured_short_adapter_status_is_not_a_can_frame_or_success() {
        let frames = crate::nano_usb::Decoder::default()
            .feed(&[0xbb, 0x80, 0x61, 0, 1, 0x9b, 0x7d, 0xbb])
            .unwrap();
        assert_eq!(frames.len(), 1);
        assert_eq!(
            receive(&frames[0]).unwrap_err(),
            "Nano adapter status 9B on channel 1 sequence 61; no CAN frame returned"
        );
        let mut zero = frames[0].clone();
        zero.payload[0] = 0;
        assert!(receive(&zero).is_err());
    }
    #[test]
    fn sequence_wrap_never_reuses_native_ticket() {
        let mut ledger = TxLedger::default();
        let now = Instant::now();
        for ticket in 1..=257 {
            let prepared = ledger
                .prepare(0, &tx(0, ticket, 0x7e0, &[2, 0x1a, 0x90]), now)
                .unwrap();
            let frames = crate::nano_usb::Decoder::default()
                .feed(&prepared.wire)
                .unwrap();
            assert_eq!(frames[0].header[1], ticket as u8);
            ledger
                .usb_write_finished(0, ticket, Ok(prepared.wire.len()), now)
                .unwrap();
        }
        assert!(ledger
            .prepare(0, &tx(0, 1, 0x7e0, &[2, 0x1a, 0x90]), now)
            .is_err());
    }
}
