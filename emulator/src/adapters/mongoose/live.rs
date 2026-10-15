// SPDX-License-Identifier: MPL-2.0
//! Capture-derived, bounded adapter-only beta test. No vehicle transmit opcode.
use super::{
    init,
    protocol::{Decoder, Record},
};
use crate::adapters::common::usb::{SocketUsb, UsbTransport};
use serde_json::{json, Value};
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

pub trait DeviceIo: UsbTransport {
    fn control(&mut self, value: u16) -> Result<(), String>;
    fn close_usb(&mut self) -> Result<(), String>;
}
impl DeviceIo for SocketUsb {
    fn control(&mut self, value: u16) -> Result<(), String> {
        if self.command(&format!("CTRL {value}"))? == "CTRLOK" {
            Ok(())
        } else {
            Err("Mongoose USB control was not accepted".into())
        }
    }
    fn close_usb(&mut self) -> Result<(), String> {
        if self.command("QUIT")? == "USB_CLOSED" {
            Ok(())
        } else {
            Err("Android USB release was not confirmed".into())
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Startup,
    HighSpeedChannel,
}

struct Session<'a, T> {
    io: &'a mut T,
    decoder: Decoder,
    queued: VecDeque<Record>,
    pending: Vec<Record>,
    started: Instant,
    deadline: Duration,
    sequence: u16,
    events: Vec<Value>,
    received: usize,
    asynchronous: usize,
    late: usize,
    stage: String,
}
impl<T: DeviceIo> Session<'_, T> {
    fn request(&mut self, mut r: Record, stage: &str) -> Result<Record, String> {
        self.stage = stage.into();
        r.sequence = self.sequence;
        self.sequence = if self.sequence == 255 {
            1
        } else {
            self.sequence + 1
        };
        let since = Instant::now();
        let mut event = json!({"stage":stage,"opcode":format!("0x{:04x}",r.opcode),
            "destination":format!("0x{:04x}",r.destination),"sequence":r.sequence,
            "request_body_bytes":r.body.len(),"at_ms":self.started.elapsed().as_millis()});
        self.pending.push(r.clone());
        let result = (|| {
            self.io.write(&r.encode()?)?;
            while since.elapsed() < self.deadline {
                if self.queued.is_empty() {
                    let b = self.io.read()?;
                    self.queued.extend(self.decoder.push(&b)?);
                }
                while let Some(reply) = self.queued.pop_front() {
                    self.received += 1;
                    if self.received > 512 {
                        return Err("Mongoose beta receive limit".into());
                    }
                    if reply.opcode == 9 || reply.opcode == 10 {
                        if reply.opcode == 9 {
                            reply.received_message()?;
                        }
                        self.asynchronous += 1;
                        continue;
                    }
                    let index = self
                        .pending
                        .iter()
                        .position(|p| p.matches_reply(&reply))
                        .ok_or("Unexpected Mongoose reply route/opcode/sequence")?;
                    let issued = self.pending.remove(index);
                    if issued != r {
                        self.late += 1;
                        continue;
                    }
                    event["reply_body_bytes"] = json!(reply.body.len());
                    if r.opcode == 0x100 {
                        if !reply.body.is_empty() {
                            return Err("Echo differs from captured profile".into());
                        }
                    } else {
                        let status = reply
                            .reply_first_word()
                            .ok_or("Missing Mongoose result word")?;
                        event["result_word"] = json!(format!("0x{status:08x}"));
                        if status != 0 && !(r.opcode == 0x103 && status == 7) {
                            return Err(format!(
                                "Mongoose opcode 0x{:04x} result 0x{status:08x}",
                                r.opcode
                            ));
                        }
                    }
                    return Ok(reply);
                }
            }
            Err("Mongoose reply deadline expired".into())
        })();
        event["elapsed_ms"] = json!(since.elapsed().as_millis());
        event["accepted"] = json!(result.is_ok());
        if let Err(error) = &result {
            event["error"] = json!(error);
        }
        self.events.push(event);
        result
    }
    fn command(
        &mut self,
        destination: u16,
        opcode: u16,
        body: Vec<u8>,
        stage: &str,
    ) -> Result<Record, String> {
        self.request(
            Record {
                destination,
                source: 0,
                opcode,
                sequence: 0,
                auxiliary: [0; 4],
                body,
            },
            stage,
        )
    }
}

pub fn run<T: DeviceIo>(io: &mut T, mode: Mode) -> Value {
    run_with_deadline(io, mode, Duration::from_secs(2))
}
fn run_with_deadline<T: DeviceIo>(io: &mut T, mode: Mode, deadline: Duration) -> Value {
    let mut s = Session {
        io,
        decoder: Decoder::default(),
        queued: VecDeque::new(),
        pending: vec![],
        started: Instant::now(),
        deadline,
        sequence: 1,
        events: vec![],
        received: 0,
        asynchronous: 0,
        late: 0,
        stage: "usb_control_enable".into(),
    };
    let mut open_attempted = false;
    let mut channel_attempted = false;
    let mut firmware = None;
    let mut bootloader = None;
    let operation = (|| -> Result<(), String> {
        s.io.control(1)?;
        for (index, r) in init::startup_reference().into_iter().enumerate() {
            if r.opcode == 3 {
                open_attempted = true;
            }
            let reply = s.request(r, &format!("startup_{}", index + 1))?;
            if index == 7 || index == 8 {
                if reply.body.len() != 16 {
                    return Err("Version query differs from captured layout".into());
                }
                let packed = u32::from_le_bytes(reply.body[12..16].try_into().unwrap());
                if index == 7 {
                    firmware = Some(packed);
                } else {
                    bootloader = Some(packed);
                }
            }
        }
        s.stage = "firmware_profile".into();
        if firmware != Some(0x01020800) || bootloader != Some(0x01010800) {
            return Err(
                "Firmware/bootloader differ from captured GM II profile; channel test withheld"
                    .into(),
            );
        }
        if mode == Mode::HighSpeedChannel {
            channel_attempted = true;
            s.request(init::observed_channel_open(0x601, 0)?, "hs_channel_open")?;
            s.command(
                0x601,
                0x12,
                [1u32.to_le_bytes(), 6u32.to_le_bytes(), 14u32.to_le_bytes()].concat(),
                "hs_pin_setup",
            )?;
        }
        Ok(())
    })();
    let failed_stage = if operation.is_err() {
        Some(s.stage.clone())
    } else {
        None
    };
    let mut cleanup_errors = Vec::new();
    let mut channel_close = !channel_attempted;
    let mut device_close = !open_attempted;
    if channel_attempted {
        match s.command(0x601, 7, vec![], "hs_channel_close") {
            Ok(_) => channel_close = true,
            Err(e) => cleanup_errors.push(format!("channel: {e}")),
        }
    }
    if open_attempted {
        match s.command(1, 5, vec![], "device_close") {
            Ok(_) => device_close = true,
            Err(e) => cleanup_errors.push(format!("device: {e}")),
        }
    }
    let control_disabled = match s.io.control(0) {
        Ok(()) => true,
        Err(e) => {
            cleanup_errors.push(e);
            false
        }
    };
    let usb_closed = match s.io.close_usb() {
        Ok(()) => true,
        Err(e) => {
            cleanup_errors.push(e);
            false
        }
    };
    if s.decoder.finish().is_err() {
        cleanup_errors.push("Incomplete/invalid receive stream at close".into());
    }
    let cleanup_ok = cleanup_errors.is_empty();
    json!({"schema":1,"adapter":"mongoose-pro-gm-ii","test":if mode==Mode::Startup{"startup"}else{"hs_channel"},
        "capture_sha256":"65f291c3c27c713ea30c374cba6e1dcacddf838fae08c9607d4dc8e1a7f8f30d",
        "status":if operation.is_ok()&&cleanup_ok{"passed"}else{"failed"},"operation_passed":operation.is_ok(),
        "error":operation.err(),"failure_stage":failed_stage,"elapsed_ms":s.started.elapsed().as_millis(),
        "firmware_packed":firmware.map(|n|format!("0x{n:08x}")),"bootloader_packed":bootloader.map(|n|format!("0x{n:08x}")),
        "events":s.events,"received_records":s.received,"asynchronous_records":s.asynchronous,"late_replies":s.late,
        "channel_close_reply_ok":channel_close,"device_close_reply_ok":device_close,"control_disabled":control_disabled,
        "usb_closed":usb_closed,"cleanup_ok":cleanup_ok,"cleanup_errors":cleanup_errors,
        "application_vehicle_transmits":0,"vehicle_link_verified":false,"vin_read_implemented":false,
        "raw_payloads_included":false,"request_origin":"capture-derived-adapter-test",
        "limits":"Experimental startup/channel test; does not qualify vehicle traffic, cold startup, VIN/DTC or original firmware integration."})
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fake {
        requests: Vec<Record>,
        rx: VecDeque<Vec<u8>>,
        controls: Vec<u16>,
        bad_open: bool,
        bad_cleanup: bool,
        bad_version: bool,
    }
    impl Fake {
        fn new() -> Self {
            Self {
                requests: vec![],
                rx: VecDeque::new(),
                controls: vec![],
                bad_open: false,
                bad_cleanup: false,
                bad_version: false,
            }
        }
    }
    impl UsbTransport for Fake {
        fn write(&mut self, bytes: &[u8]) -> Result<(), String> {
            let r = Record::decode(bytes)?;
            assert_ne!(r.opcode, 8); // Adapter-only beta cannot transmit a vehicle request.
            let mut reply = r.clone();
            reply.destination = r.source;
            reply.source = r.destination;
            reply.opcode |= 0x8000;
            let status: u32 = if self.bad_open && r.opcode == 6 {
                0xfe
            } else if self.bad_cleanup && r.opcode == 5 {
                1
            } else if r.opcode == 0x103 {
                7
            } else {
                0
            };
            reply.body = if r.opcode == 0x100 {
                vec![]
            } else {
                status.to_le_bytes().to_vec()
            };
            if r.opcode == 0xc {
                reply.body.resize(16, 0);
                let selector = u32::from_le_bytes(r.body[..4].try_into().unwrap());
                let value: u32 = match selector {
                    0x2b => {
                        if self.bad_version {
                            0x01020900
                        } else {
                            0x01020800
                        }
                    }
                    0x2a => 0x01010800,
                    _ => 0,
                };
                reply.body[12..].copy_from_slice(&value.to_le_bytes());
            }
            let encoded = reply.encode()?;
            self.rx.push_back(encoded[..3].to_vec());
            self.rx.push_back(encoded[3..].to_vec());
            self.requests.push(r);
            Ok(())
        }
        fn read(&mut self) -> Result<Vec<u8>, String> {
            Ok(self.rx.pop_front().unwrap_or_default())
        }
    }
    impl DeviceIo for Fake {
        fn control(&mut self, v: u16) -> Result<(), String> {
            self.controls.push(v);
            Ok(())
        }
        fn close_usb(&mut self) -> Result<(), String> {
            Ok(())
        }
    }
    #[test]
    fn captures_start_firmware_seven_and_closes_in_reverse_order() {
        let mut io = Fake::new();
        let report = run(&mut io, Mode::HighSpeedChannel);
        assert_eq!(report["status"], "passed");
        assert_eq!(io.controls, vec![1, 0]);
        assert_eq!(&io.requests[..11], init::startup_reference());
        assert_eq!(io.requests[11].opcode, 6);
        assert_eq!(io.requests[12].opcode, 0x12);
        assert_eq!(io.requests[13].opcode, 7);
        assert_eq!(io.requests[14].opcode, 5);
        assert_eq!(report["application_vehicle_transmits"], 0);
        assert_eq!(report["vehicle_link_verified"], false);
    }
    #[test]
    fn rejected_channel_open_still_attempts_channel_and_device_cleanup() {
        let mut io = Fake::new();
        io.bad_open = true;
        let report = run(&mut io, Mode::HighSpeedChannel);
        assert_eq!(report["status"], "failed");
        assert_eq!(report["failure_stage"], "hs_channel_open");
        assert_eq!(report["cleanup_ok"], true);
        assert_eq!(io.requests.last().unwrap().opcode, 5);
        assert_eq!(io.requests[12].opcode, 7);
    }
    #[test]
    fn cleanup_failure_does_not_turn_into_a_pass() {
        let mut io = Fake::new();
        io.bad_cleanup = true;
        let report = run(&mut io, Mode::Startup);
        assert_eq!(report["operation_passed"], true);
        assert_eq!(report["status"], "failed");
        assert_eq!(report["usb_closed"], true);
    }
    #[test]
    fn different_firmware_withholds_channel_but_releases_device() {
        let mut io = Fake::new();
        io.bad_version = true;
        let report = run(&mut io, Mode::HighSpeedChannel);
        assert_eq!(report["failure_stage"], "firmware_profile");
        assert!(!io.requests.iter().any(|r| r.opcode == 6));
        assert_eq!(io.requests.last().unwrap().opcode, 5);
    }
}
