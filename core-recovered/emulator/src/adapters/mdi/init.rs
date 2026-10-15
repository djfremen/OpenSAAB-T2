// SPDX-License-Identifier: MPL-2.0
//! One fresh host VIN read, with cleanup after every acquired-resource failure.
use super::profile;
use crate::adapter_translation::{host, profile as contract, Bus, Observation, Origin, Packet};
use std::time::Instant;

#[repr(C)]
#[derive(Clone, Debug)]
pub struct Message {
    pub protocol: u32,
    pub rx_status: u32,
    pub tx_flags: u32,
    pub timestamp: u32,
    pub data_size: u32,
    pub extra_data_index: u32,
    pub data: [u8; 4128],
}
impl Default for Message {
    fn default() -> Self {
        Self {
            protocol: 0,
            rx_status: 0,
            tx_flags: 0,
            timestamp: 0,
            data_size: 0,
            extra_data_index: 0,
            data: [0; 4128],
        }
    }
}
impl Message {
    pub fn new(id: u32, payload: &[u8]) -> Self {
        assert!(payload.len() <= 4124);
        let mut result = Self {
            protocol: profile::PROTOCOL,
            data_size: (4 + payload.len()) as u32,
            ..Self::default()
        };
        result.data[..4].copy_from_slice(&id.to_be_bytes());
        result.data[4..4 + payload.len()].copy_from_slice(payload);
        result
    }
    fn observation(&self, at_us: u64) -> Result<Option<Observation>, String> {
        let size = self.data_size as usize;
        if size > self.data.len()
            || (self.extra_data_index != 0 && self.extra_data_index != self.data_size)
        {
            return Err("Invalid MDI receive bounds/extra-data index".into());
        }
        if self.protocol != profile::PROTOCOL || self.rx_status != 0 {
            return Ok(None);
        }
        if size < 4 {
            return Err("Truncated MDI CAN identifier".into());
        }
        let id = u32::from_be_bytes(self.data[..4].try_into().unwrap());
        if id > 0x7ff {
            return Err("Invalid standard CAN identifier".into());
        }
        Ok(Some(Observation {
            session_id: 1,
            at_us,
            device_timestamp: Some(self.timestamp as u64),
            origin: Origin::Physical,
            packet: Packet::IsoTpPayload {
                bus: Bus::HsCan,
                id,
                extended: false,
                data: self.data[4..size].to_vec(),
            },
        }))
    }
}

pub trait Driver {
    fn open(&mut self) -> Result<u32, String>;
    fn version(&mut self, device: u32) -> Result<[String; 3], String>;
    fn voltage(&mut self, device: u32) -> Result<u32, String>;
    fn connect(&mut self, device: u32) -> Result<u32, String>;
    fn filter(&mut self, channel: u32) -> Result<u32, String>;
    fn clear_rx(&mut self, channel: u32) -> Result<(), String>;
    fn write(&mut self, channel: u32, message: &mut Message) -> Result<u32, String>;
    fn read(&mut self, channel: u32) -> Result<Option<Message>, String>;
    fn stop_filter(&mut self, channel: u32, filter: u32) -> Result<(), String>;
    fn disconnect(&mut self, channel: u32) -> Result<(), String>;
    fn close(&mut self, device: u32) -> Result<(), String>;
}

#[derive(Default, Debug)]
pub struct Outcome {
    pub vin: Option<String>,
    pub version: Option<[String; 3]>,
    pub voltage_mv: Option<u32>,
    pub vehicle_requests: u32,
    pub rx_records: u32,
    pub cleanup_ok: bool,
    pub error: Option<String>,
}
impl Outcome {
    pub fn success(&self) -> bool {
        self.vin.is_some()
            && self.version.is_some()
            && self.voltage_mv.is_some()
            && self.vehicle_requests == 1
            && self.cleanup_ok
            && self.error.is_none()
    }
    pub fn json(&self) -> serde_json::Value {
        serde_json::json!({"schema":1,"adapter":"bosch-mdi-classic","status":if self.success(){"vin_received"}else{"failed"},
            "request_origin":"host-startup-discovery","fresh_vin":self.vin,
            "version":self.version,"voltage_mv":self.voltage_mv,"vehicle_requests":self.vehicle_requests,
            "rx_records":self.rx_records,"cleanup_ok":self.cleanup_ok,"error":self.error,
            "process_bits":32,"driver_sha256":profile::DRIVER_SHA,"runtime_sha256":profile::RUNTIME_SHA,
            "pdu_sha256":profile::PDU_SHA,"startup":"bosch-windows-driver","cold_start_qualified":false})
    }
}

fn transaction() -> Result<host::Session, String> {
    let capabilities = contract::Capabilities {
        family: contract::CommandFamily::J2534,
        buses: vec![Bus::HsCan],
        raw_receive: false,
        raw_transmit: false,
        assembled_receive: true,
        adapter_flow_control: true,
        simultaneous_buses: false,
    };
    host::Session::new(
        host::Config {
            session_id: 1,
            profile: contract::SessionProfile {
                bus: Bus::HsCan,
                mode: contract::Mode::HostIsoTp,
                representation: contract::Representation::IsoTpPayload,
                flow_control: contract::FlowControlOwner::Adapter,
            },
            request_id: profile::REQUEST_ID,
            reply_id: profile::REPLY_ID,
            extended: false,
            protocol: host::Protocol::IsoTp {
                service: 0x1a,
                positive_prefix: vec![0x5a, 0x90],
            },
            limits: host::Limits {
                payload_bytes: 19,
                idle_us: 5_000_000,
                total_us: 5_000_000,
                ..Default::default()
            },
            host_flow_control: host::FlowControl {
                block_size: 0,
                st_min: 0,
            },
        },
        &capabilities,
        0,
    )
}

pub fn read_vin(driver: &mut impl Driver) -> Outcome {
    let mut result = Outcome {
        cleanup_ok: true,
        ..Default::default()
    };
    let (mut device, mut channel, mut filter) = (None, None, None);
    let operation = (|| -> Result<(), String> {
        let d = driver.open()?;
        device = Some(d);
        let version = driver.version(d)?;
        if version[1] != "2.5.33.154" || version[2] != "04.04" {
            return Err("MDI driver/API identity differs from selected profile".into());
        }
        result.version = Some(version);
        result.voltage_mv = Some(driver.voltage(d)?);
        let c = driver.connect(d)?;
        channel = Some(c);
        filter = Some(driver.filter(c)?);
        driver.clear_rx(c)?;
        let mut reply = transaction()?;
        let started = Instant::now();
        let mut request = Message::new(profile::REQUEST_ID, &[0x1a, 0x90]);
        request.tx_flags = 0x40;
        result.vehicle_requests = driver.write(c, &mut request)?;
        if result.vehicle_requests != 1 {
            return Err("MDI did not accept exactly one VIN request; no retry".into());
        }
        loop {
            reply.tick(started.elapsed().as_micros() as u64)?;
            if let Some(message) = driver.read(c)? {
                result.rx_records += 1;
                if result.rx_records > 1024 {
                    return Err("MDI receive record limit".into());
                }
                if let Some(observation) =
                    message.observation(started.elapsed().as_micros() as u64)?
                {
                    match reply.feed(&observation)? {
                        host::Action::Complete(host::ResultData::Payload(payload)) => {
                            result.vin = Some(crate::vin_probe::assembled_saab_vin(&payload)?);
                            break;
                        }
                        host::Action::Ignored | host::Action::Pending | host::Action::Progress => {}
                        _ => return Err("Unexpected MDI ISO-TP result/flow-control owner".into()),
                    }
                }
            }
        }
        Ok(())
    })();
    if let Err(error) = operation {
        result.error = Some(error);
    }
    // Always attempt each remaining release, even if an earlier release fails.
    let mut cleanup_errors = Vec::new();
    if let (Some(c), Some(f)) = (channel, filter) {
        if let Err(e) = driver.stop_filter(c, f) {
            cleanup_errors.push(e);
        }
    }
    if let Some(c) = channel {
        if let Err(e) = driver.disconnect(c) {
            cleanup_errors.push(e);
        }
    }
    if let Some(d) = device {
        if let Err(e) = driver.close(d) {
            cleanup_errors.push(e);
        }
    }
    if !cleanup_errors.is_empty() {
        result.cleanup_ok = false;
        result.error = Some(format!(
            "{}; cleanup: {}",
            result.error.unwrap_or_default(),
            cleanup_errors.join("; ")
        ));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fake {
        calls: Vec<&'static str>,
        fail: Option<&'static str>,
        release_fail: bool,
        queue: Vec<Message>,
    }
    impl Fake {
        fn step(&mut self, name: &'static str) -> Result<(), String> {
            self.calls.push(name);
            if self.fail == Some(name) || (self.release_fail && name == "stop") {
                Err(name.into())
            } else {
                Ok(())
            }
        }
        fn new() -> Self {
            let mut indication = Message::new(0x7e8, b"\x5a\x90YS3FD49YX41000001");
            indication.rx_status = 1;
            Self {
                calls: vec![],
                fail: None,
                release_fail: false,
                queue: vec![
                    Message::new(0x7e8, b"\x5a\x90YS3FD49YX41000001"),
                    indication,
                ],
            }
        }
    }
    impl Driver for Fake {
        fn open(&mut self) -> Result<u32, String> {
            self.step("open")?;
            Ok(0)
        }
        fn version(&mut self, _: u32) -> Result<[String; 3], String> {
            self.step("version")?;
            Ok(["8.5.33".into(), "2.5.33.154".into(), "04.04".into()])
        }
        fn voltage(&mut self, _: u32) -> Result<u32, String> {
            self.step("voltage")?;
            Ok(14000)
        }
        fn connect(&mut self, _: u32) -> Result<u32, String> {
            self.step("connect")?;
            Ok(0)
        }
        fn filter(&mut self, _: u32) -> Result<u32, String> {
            self.step("filter")?;
            Ok(0)
        }
        fn clear_rx(&mut self, _: u32) -> Result<(), String> {
            self.step("clear")
        }
        fn write(&mut self, _: u32, m: &mut Message) -> Result<u32, String> {
            self.step("write")?;
            assert_eq!(&m.data[..6], &[0, 0, 7, 0xe0, 0x1a, 0x90]);
            assert_eq!(m.tx_flags, 0x40);
            Ok(1)
        }
        fn read(&mut self, _: u32) -> Result<Option<Message>, String> {
            self.step("read")?;
            Ok(self.queue.pop())
        }
        fn stop_filter(&mut self, _: u32, _: u32) -> Result<(), String> {
            self.step("stop")
        }
        fn disconnect(&mut self, _: u32) -> Result<(), String> {
            self.step("disconnect")
        }
        fn close(&mut self, _: u32) -> Result<(), String> {
            self.step("close")
        }
    }
    #[test]
    fn fresh_reply_after_clear_single_write_and_cleanup() {
        assert_eq!(std::mem::size_of::<Message>(), 4152);
        assert_eq!(std::mem::offset_of!(Message, data), 24);
        let mut f = Fake::new();
        let o = read_vin(&mut f);
        assert!(o.success(), "{o:?}");
        assert_eq!(o.rx_records, 2);
        assert_eq!(
            f.calls,
            [
                "open",
                "version",
                "voltage",
                "connect",
                "filter",
                "clear",
                "write",
                "read",
                "read",
                "stop",
                "disconnect",
                "close"
            ]
        );
    }
    #[test]
    fn failure_at_each_acquisition_still_releases_every_acquired_handle() {
        for point in [
            "open", "version", "voltage", "connect", "filter", "clear", "write", "read",
        ] {
            let mut f = Fake::new();
            f.fail = Some(point);
            let o = read_vin(&mut f);
            assert!(!o.success());
            if point != "open" {
                assert_eq!(f.calls.last(), Some(&"close"));
            }
            if matches!(point, "filter" | "clear" | "write" | "read") {
                assert!(f.calls.contains(&"disconnect"));
            }
            if matches!(point, "clear" | "write" | "read") {
                assert!(f.calls.contains(&"stop"));
            }
            assert_eq!(
                f.calls.iter().filter(|&&c| c == "write").count(),
                usize::from(matches!(point, "write" | "read"))
            );
        }
    }
    #[test]
    fn failed_filter_release_does_not_skip_disconnect_close_or_publish_success() {
        let mut f = Fake::new();
        f.release_fail = true;
        let o = read_vin(&mut f);
        assert!(!o.success());
        assert!(!o.cleanup_ok);
        assert!(f.calls.ends_with(&["stop", "disconnect", "close"]));
    }
    #[test]
    fn malformed_bounds_and_ecu_rejection_fail_and_close() {
        for message in [
            Message::new(0x7e8, &[0x7f, 0x1a, 0x11]),
            Message {
                data_size: 4129,
                ..Default::default()
            },
            Message::new(0x7e8, b"\x5a\x90YS3FD49YX4100000I"),
        ] {
            let mut f = Fake::new();
            f.queue = vec![message];
            assert!(!read_vin(&mut f).success());
            assert!(f.calls.ends_with(&["stop", "disconnect", "close"]));
        }
    }
    #[test]
    fn pending_is_not_completion_and_does_not_create_another_request() {
        let mut f = Fake::new();
        f.queue.push(Message::new(0x7e8, &[0x7f, 0x1a, 0x78]));
        let o = read_vin(&mut f);
        assert!(o.success());
        assert_eq!(f.calls.iter().filter(|&&c| c == "write").count(), 1);
        let mut session = transaction().unwrap();
        assert!(session.tick(5_000_000).is_err());
    }
}
