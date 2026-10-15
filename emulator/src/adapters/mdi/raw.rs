// SPDX-License-Identifier: MPL-2.0
//! Classic MDI raw CAN lifecycle. Original firmware owns segmentation and FC.
use super::init::{Driver, Message};
use super::polling::{self, Action};
use crate::{
    adapters::common::policy::{CommandGate, Profile},
    candi_cpu::{CanElectricalState, CanFrame, CanTransmission},
};
use std::time::Instant;

pub const SW_PROTOCOL: u32 = 0x8008;
pub const SW_BAUD: u32 = 33333;
pub const PINS: u32 = 0x8001;
pub const PIN1: u32 = 0x0100;
pub const LOOPBACK: u32 = 3;
pub const HV_TX: u32 = 0x400;
pub const HV_RX: u32 = 0x10000;

pub trait RawDriver: Driver {
    fn raw_connect(&mut self, device: u32, protocol: u32, baud: u32) -> Result<u32, String>;
    fn config(&mut self, channel: u32, parameter: u32, value: u32) -> Result<(), String>;
    fn raw_filter(&mut self, channel: u32, protocol: u32) -> Result<u32, String>;
    fn raw_read(&mut self, channel: u32) -> Result<Option<Message>, String>;
    fn raw_write(&mut self, channel: u32, message: &mut Message) -> Result<u32, String>;
}

pub struct Session<D: RawDriver> {
    pub driver: D,
    device: Option<u32>,
    channels: [Option<u32>; 3],
    filters: [Option<u32>; 3],
    gate: CommandGate,
    tickets: [u64; 3],
    polling_active: [bool; 4],
    pub cleanup_ok: bool,
    pub wake_notices: u64,
}
impl<D: RawDriver> Session<D> {
    pub fn new(driver: D) -> Self {
        Self {
            driver,
            device: None,
            channels: [None; 3],
            filters: [None; 3],
            gate: Default::default(),
            tickets: [0; 3],
            polling_active: [false; 4],
            cleanup_ok: true,
            wake_notices: 0,
        }
    }
    pub fn start(&mut self, high_speed: bool) -> Result<(), String> {
        if self.device.is_some() {
            return Err("MDI raw session already open".into());
        }
        let d = self.driver.open()?;
        self.device = Some(d);
        let version = self.driver.version(d)?;
        if version[1] != "2.5.33.154" || version[2] != "04.04" {
            return Err("Unexpected classic MDI DLL/API version".into());
        }
        println!(
            "BRIDGE IDENTITY firmware={:?} dll={} api={} voltage_mv={}",
            version[0],
            version[1],
            version[2],
            self.driver.voltage(d)?
        );
        for c in [0, 2] {
            if c == 0 && !high_speed {
                continue;
            }
            let protocol = protocol(c)?;
            let ch = self
                .driver
                .raw_connect(d, protocol, if c == 0 { 500000 } else { SW_BAUD })?;
            self.channels[c] = Some(ch);
            if c == 2 {
                self.driver.config(ch, PINS, PIN1)?;
            }
            self.driver.config(ch, LOOPBACK, 1)?;
            self.filters[c] = Some(self.driver.raw_filter(ch, protocol)?);
            self.driver.clear_rx(ch)?;
        }
        Ok(())
    }
    pub fn transmit(&mut self, line: &str) -> Result<(usize, u64), String> {
        self.transmit_origin(line, "native-candi")
    }
    fn transmit_origin(&mut self, line: &str, origin: &str) -> Result<(usize, u64), String> {
        let (c, ticket, mut message, tx) = parse_tx(line)?;
        let ch = self.channels[c].ok_or("MDI raw bus is not open")?;
        if ticket == 0 || ticket <= self.tickets[c] {
            return Err("MDI stale/zero transmission ticket".into());
        }
        let polling_action = if polling::ENABLED { polling::classify(c, &tx) } else { None };
        if polling_action.is_none() && !self.gate.allowed(c, &tx, Profile::Read, Instant::now()) {
            let offset = usize::from(tx.frame.data.first() == Some(&0xfe));
            let service = tx.frame.data.get(offset + 1).copied().unwrap_or(0);
            return Err(format!("MDI command guard blocked before USB: controller={c} CAN_ID={:03X} service={service:02X} profile=Read; frame={}",tx.frame.id,hex(&tx.frame.data)));
        }
        // Never retry an uncertain vendor write.
        self.tickets[c] = ticket;
        // A start with uncertain completion still needs one bounded stop.
        // A failed stop is never retried automatically, including from Drop.
        match polling_action {
            Some(Action::Start(i)) => self.polling_active[i] = true,
            Some(Action::Stop(i)) => self.polling_active[i] = false,
            None => {}
        }
        println!(
            "BRIDGE TX {c} {ticket} {:04X} {:03X} {} origin={origin}",
            message.tx_flags,
            tx.frame.id,
            hex(&tx.frame.data)
        );
        let write = self.driver.raw_write(ch, &mut message);
        if matches!(polling_action, Some(Action::Stop(_))) && !matches!(&write, Ok(1)) {
            self.cleanup_ok = false;
        }
        if write? != 1 {
            return Err("MDI short raw write; outcome unknown".into());
        }
        Ok((c, ticket))
    }
    /// Separate authored host experiment; never called by the firmware server.
    /// Shared ISO-TP assembler owns FC only in this mode. One request, no retry.
    pub fn probe_identity(&mut self) -> Result<Vec<u8>, String> {
        use crate::adapter_translation::{
            host, profile as p, Bus, Observation, Origin, Packet, RawFrame,
        };
        let caps = p::Capabilities {
            family: p::CommandFamily::J2534,
            buses: vec![Bus::SwCan],
            raw_receive: true,
            raw_transmit: true,
            assembled_receive: false,
            adapter_flow_control: false,
            simultaneous_buses: false,
        };
        let mut reply = host::Session::new(
            host::Config {
                session_id: 1,
                profile: p::SessionProfile {
                    bus: Bus::SwCan,
                    mode: p::Mode::HostIsoTp,
                    representation: p::Representation::RawCan,
                    flow_control: p::FlowControlOwner::Host,
                },
                request_id: 0x242,
                reply_id: 0x642,
                extended: false,
                protocol: host::Protocol::IsoTp {
                    service: 0x1a,
                    positive_prefix: vec![0x5a, 0x97],
                },
                limits: host::Limits {
                    payload_bytes: 128,
                    idle_us: 2_000_000,
                    total_us: 5_000_000,
                    ..Default::default()
                },
                host_flow_control: host::FlowControl {
                    block_size: 0,
                    st_min: 0,
                },
            },
            &caps,
            0,
        )?;
        self.transmit_origin("TX 2 1 0000 242 021A970000000000", "host-ibus-experiment")?;
        let at = Instant::now();
        let mut ticket = 1;
        let mut count = 0;
        loop {
            reply.tick(at.elapsed().as_micros() as u64)?;
            if let Some(m) = self.receive(2)? {
                count += 1;
                if count > 10000 {
                    return Err("MDI host receive limit".into());
                }
                println!(
                    "BRIDGE RX 2 {} {:08X} {} {}",
                    m.protocol,
                    m.rx_status,
                    m.timestamp,
                    hex(&m.data[..m.data_size as usize])
                );
                if m.rx_status != 0 {
                    continue;
                }
                let data = &m.data[..m.data_size as usize];
                let o = Observation {
                    session_id: 1,
                    at_us: at.elapsed().as_micros() as u64,
                    device_timestamp: Some(m.timestamp as u64),
                    origin: Origin::Physical,
                    packet: Packet::Raw(RawFrame {
                        bus: Bus::SwCan,
                        id: u32::from_be_bytes(data[..4].try_into().unwrap()),
                        extended: false,
                        rtr: false,
                        dlc: (data.len() - 4) as u8,
                        data: data[4..].to_vec(),
                    }),
                };
                match reply.feed(&o)? {
                    host::Action::Complete(host::ResultData::Payload(payload)) => {
                        return Ok(payload)
                    }
                    host::Action::SendFlowControl(frame) => {
                        ticket += 1;
                        self.transmit_origin(
                            &format!("TX 2 {ticket} 0000 {:03X} {}", frame.id, hex(&frame.data)),
                            "host-ibus-flow-control",
                        )?;
                    }
                    host::Action::Ignored | host::Action::Pending | host::Action::Progress => {}
                    _ => return Err("Unexpected MDI host identity result".into()),
                }
            } else {
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        }
    }
    pub fn receive(&mut self, c: usize) -> Result<Option<Message>, String> {
        let Some(ch) = self.channels.get(c).copied().flatten() else {
            return Ok(None);
        };
        let Some(message) = self.driver.raw_read(ch)? else {
            return Ok(None);
        };
        // Bosch 2.5.33.154 reports an empty bit16 record after HV transmission.
        // It carries no identifier/payload: retain the observation, never invent
        // a CAN frame or count it as TX completion / electrical CAN ACK.
        if is_wake_notice(c, &message) {
            self.wake_notices += 1;
            println!("BRIDGE CALL MDI_WAKE_NOTICE controller={c} protocol={} status={:08X} timestamp={} size=0 extra=0 frame=false can_ack=false",message.protocol,message.rx_status,message.timestamp);
            return Ok(None);
        }
        if let Err(error) = validate_rx(c, &message) {
            return Err(format!("{error}: controller={c} protocol={} status={:08X} size={} extra={} timestamp={} data={}",message.protocol,message.rx_status,message.data_size,message.extra_data_index,message.timestamp,hex(&message.data[..(message.data_size as usize).min(4128)])));
        }
        Ok(Some(message))
    }
    pub fn close(&mut self) -> bool {
        if let Some(ch) = self.channels[2] {
            for (index, id) in polling::IDS.iter().enumerate() {
                if std::mem::take(&mut self.polling_active[index]) {
                    let mut stop = Message::new(*id, &polling::STOP);
                    stop.protocol = SW_PROTOCOL;
                    println!("BRIDGE CLEANUP_STOP 2 {id:03X} {} origin=bounded-mdi-polling-cleanup", hex(&polling::STOP));
                    self.cleanup_ok &= matches!(self.driver.raw_write(ch, &mut stop), Ok(1));
                }
            }
        }
        for c in [2, 0] {
            if let Some(ch) = self.channels[c] {
                if let Some(f) = self.filters[c].take() {
                    self.cleanup_ok &= self.driver.stop_filter(ch, f).is_ok();
                }
                self.channels[c] = None;
                self.cleanup_ok &= self.driver.disconnect(ch).is_ok();
            }
        }
        if let Some(d) = self.device.take() {
            self.cleanup_ok &= self.driver.close(d).is_ok();
        }
        self.cleanup_ok
    }
}
impl<D: RawDriver> Drop for Session<D> {
    fn drop(&mut self) {
        self.close();
    }
}
pub fn protocol(c: usize) -> Result<u32, String> {
    match c {
        0 => Ok(5),
        2 => Ok(SW_PROTOCOL),
        _ => Err("MDI unmapped CAN controller".into()),
    }
}
pub fn hex(data: &[u8]) -> String {
    if data.is_empty() {
        "-".into()
    } else {
        data.iter().map(|v| format!("{v:02X}")).collect()
    }
}
pub fn validate_rx(c: usize, m: &Message) -> Result<(), String> {
    if m.protocol != protocol(c)?
        || !(4..=12).contains(&m.data_size)
        || (m.extra_data_index != 0 && m.extra_data_index != m.data_size)
        || !(matches!(m.rx_status, 0 | 1) || (c == 2 && matches!(m.rx_status, HV_RX | 0x10001)))
    {
        return Err("MDI raw receive protocol/status/bounds rejected".into());
    }
    if u32::from_be_bytes(m.data[..4].try_into().unwrap()) > 0x7ff {
        return Err("MDI raw extended/invalid identifier rejected".into());
    }
    Ok(())
}
pub fn parse_tx(line: &str) -> Result<(usize, u64, Message, CanTransmission), String> {
    let p: Vec<_> = line.split_whitespace().collect();
    if line.len() > 256 || p.len() != 6 || p[0] != "TX" {
        return Err("Invalid MDI raw command".into());
    }
    let c: usize = p[1].parse().map_err(|_| "Invalid controller")?;
    let protocol = protocol(c)?;
    let ticket = p[2].parse().map_err(|_| "Invalid ticket")?;
    let flags = u32::from_str_radix(p[3], 16).map_err(|_| "Invalid flags")?;
    let id = u32::from_str_radix(p[4], 16).map_err(|_| "Invalid CAN ID")?;
    if flags != 0 && !(c == 2 && flags == HV_TX && id == 0x100 && p[5] == "-") {
        return Err("Unqualified MDI electrical flags".into());
    }
    let data = if p[5] == "-" {
        vec![]
    } else {
        if p[5].len() > 16 || p[5].len() % 2 != 0 || !p[5].is_ascii() {
            return Err("Invalid CAN data".into());
        }
        (0..p[5].len())
            .step_by(2)
            .map(|i| {
                u8::from_str_radix(&p[5][i..i + 2], 16).map_err(|_| "Invalid CAN hex".to_string())
            })
            .collect::<Result<Vec<_>, _>>()?
    };
    let frame = CanFrame {
        id,
        extended: false,
        rtr: false,
        dlc: data.len() as u8,
        data,
    };
    frame.validate()?;
    let tx = CanTransmission {
        ticket,
        frame,
        btr0: if c == 0 { 0xc1 } else { 0xdd },
        btr1: 0x36,
        electrical: if c == 0 {
            CanElectricalState::StandardCan
        } else {
            CanElectricalState::SingleWireGpio {
                latch: if flags == HV_TX { 2 } else { 3 },
                assignment: 0,
                direction: 3,
            }
        },
    };
    let mut m = Message::new(id, &tx.frame.data);
    m.protocol = protocol;
    m.tx_flags = flags;
    Ok((c, ticket, m, tx))
}
pub fn is_wake_notice(c: usize, m: &Message) -> bool {
    c == 2
        && m.protocol == SW_PROTOCOL
        && m.rx_status == HV_RX
        && m.data_size == 0
        && m.extra_data_index == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn frames_and_wake_preserve_the_guest_bytes_and_flags() {
        for line in [
            "TX 2 1 0400 100 -",
            "TX 2 2 0000 242 021A970000000000",
            "TX 0 1 0000 7E0 013E000000000000",
        ] {
            let (c, _, m, tx) = parse_tx(line).unwrap();
            assert_eq!(&m.data[4..m.data_size as usize], tx.frame.data);
            assert!(crate::adapters::common::policy::diagnostic_allowed(c, &tx));
        }
        for line in [
            "TX 0 1 0400 100 -",
            "TX 2 1 0400 242 -",
            "TX 1 1 0000 242 021A97",
            "TX 2 1 0000 800 021A97",
            "TX 2 1 0000 242 000000000000000000",
        ] {
            assert!(parse_tx(line).is_err());
        }
        for d in ["022701", "0104", "033B0199", "023401"] {
            let (c, _, _, tx) = parse_tx(&format!("TX 2 1 0000 242 {d}")).unwrap();
            assert!(!crate::adapters::common::policy::diagnostic_allowed(c, &tx));
        }
    }
    #[test]
    fn raw_receive_never_accepts_assembled_or_truncated_data() {
        let mut m = Message::new(0x642, &[4, 0x5a, 0x97, b'B', b'C', b'M']);
        m.protocol = SW_PROTOCOL;
        assert!(validate_rx(2, &m).is_ok());
        m.rx_status = HV_RX;
        assert!(validate_rx(2, &m).is_ok());
        assert!(validate_rx(0, &m).is_err());
        m.rx_status = 0;
        for (protocol, status, size, extra) in [
            (6, 0, 10, 0),
            (SW_PROTOCOL, 2, 10, 0),
            (SW_PROTOCOL, 0, 13, 0),
            (SW_PROTOCOL, 0, 3, 0),
            (SW_PROTOCOL, 0, 10, 9),
        ] {
            m.protocol = protocol;
            m.rx_status = status;
            m.data_size = size;
            m.extra_data_index = extra;
            assert!(validate_rx(2, &m).is_err());
        }
    }
    #[derive(Default)]
    struct Fake {
        calls: Vec<String>,
        fail: Option<String>,
        rx: std::collections::VecDeque<Message>,
    }
    impl Fake {
        fn call(&mut self, s: String) -> Result<(), String> {
            self.calls.push(s.clone());
            if self.fail.as_ref() == Some(&s) {
                Err(s)
            } else {
                Ok(())
            }
        }
    }
    impl Driver for Fake {
        fn open(&mut self) -> Result<u32, String> {
            self.call("open".into())?;
            Ok(8)
        }
        fn version(&mut self, _: u32) -> Result<[String; 3], String> {
            self.call("version".into())?;
            Ok(["8.5.33".into(), "2.5.33.154".into(), "04.04".into()])
        }
        fn voltage(&mut self, _: u32) -> Result<u32, String> {
            self.call("voltage".into())?;
            Ok(14118)
        }
        fn connect(&mut self, _: u32) -> Result<u32, String> {
            unreachable!()
        }
        fn filter(&mut self, _: u32) -> Result<u32, String> {
            unreachable!()
        }
        fn clear_rx(&mut self, c: u32) -> Result<(), String> {
            self.call(format!("clear {c}"))
        }
        fn write(&mut self, _: u32, _: &mut Message) -> Result<u32, String> {
            unreachable!()
        }
        fn read(&mut self, _: u32) -> Result<Option<Message>, String> {
            unreachable!()
        }
        fn stop_filter(&mut self, c: u32, _: u32) -> Result<(), String> {
            self.call(format!("stop {c}"))
        }
        fn disconnect(&mut self, c: u32) -> Result<(), String> {
            self.call(format!("disconnect {c}"))
        }
        fn close(&mut self, _: u32) -> Result<(), String> {
            self.call("close".into())
        }
    }
    impl RawDriver for Fake {
        fn raw_connect(&mut self, _: u32, p: u32, _: u32) -> Result<u32, String> {
            self.call(format!("connect {p}"))?;
            Ok(p)
        }
        fn config(&mut self, c: u32, p: u32, v: u32) -> Result<(), String> {
            self.call(format!("config {c} {p} {v}"))
        }
        fn raw_filter(&mut self, c: u32, _: u32) -> Result<u32, String> {
            self.call(format!("filter {c}"))?;
            Ok(c + 100)
        }
        fn raw_read(&mut self, _: u32) -> Result<Option<Message>, String> {
            Ok(self.rx.pop_front())
        }
        fn raw_write(&mut self, c: u32, m: &mut Message) -> Result<u32, String> {
            self.call(format!(
                "write {c} {}",
                hex(&m.data[..m.data_size as usize])
            ))?;
            Ok(1)
        }
    }
    #[test]
    fn every_raw_setup_failure_releases_all_acquired_resources() {
        let mut complete = Session::new(Fake::default());
        complete.start(true).unwrap();
        let setup = complete.driver.calls.clone();
        assert!(complete.close());
        for failure in setup {
            let mut s = Session::new(Fake {
                fail: Some(failure.clone()),
                ..Default::default()
            });
            assert!(s.start(true).is_err());
            assert!(s.close());
            let calls = &s.driver.calls;
            if failure != "open" {
                assert_eq!(calls.last().unwrap(), "close");
            }
            for c in [5, SW_PROTOCOL] {
                let connected = calls.iter().any(|x| x == &format!("connect {c}"))
                    && failure != format!("connect {c}");
                assert_eq!(
                    calls.iter().any(|x| x == &format!("disconnect {c}")),
                    connected
                );
            }
        }
    }
    #[test]
    fn failed_release_continues_and_cannot_be_reported_clean() {
        let mut s = Session::new(Fake {
            fail: Some(format!("stop {SW_PROTOCOL}")),
            ..Default::default()
        });
        s.start(true).unwrap();
        assert!(!s.close());
        assert!(s.driver.calls.ends_with(&[
            format!("stop {SW_PROTOCOL}"),
            format!("disconnect {SW_PROTOCOL}"),
            "stop 5".into(),
            "disconnect 5".into(),
            "close".into()
        ]));
        let n = s.driver.calls.len();
        assert!(!s.close());
        assert_eq!(s.driver.calls.len(), n);
    }
    #[test]
    fn fresh_probe_uses_raw_response_and_one_explicit_host_request() {
        let mut m = Message::new(0x642, &[5, 0x5a, 0x97, b'B', b'C', b'M', 0, 0]);
        m.protocol = SW_PROTOCOL;
        let mut s = Session::new(Fake {
            rx: [m].into(),
            ..Default::default()
        });
        s.start(false).unwrap();
        assert_eq!(s.probe_identity().unwrap(), b"\x5a\x97BCM");
        assert_eq!(
            s.driver
                .calls
                .iter()
                .filter(|x| x.starts_with("write "))
                .count(),
            1
        );
        assert!(s.close());
    }
    #[test]
    fn bosch_empty_wake_notice_never_becomes_a_frame_or_completion() {
        let notice = Message {
            protocol: SW_PROTOCOL,
            rx_status: HV_RX,
            timestamp: 42,
            ..Default::default()
        };
        assert!(is_wake_notice(2, &notice));
        assert!(!is_wake_notice(0, &notice));
        assert!(validate_rx(2, &notice).is_err());
        for (status, size, extra) in [
            (0, 0, 0),
            (1, 0, 0),
            (HV_RX, 0, 1),
            (HV_RX, 4, 0),
            (0x20000, 0, 0),
        ] {
            let mut bad = notice.clone();
            bad.rx_status = status;
            bad.data_size = size;
            bad.extra_data_index = extra;
            assert!(!is_wake_notice(2, &bad));
        }
        let mut frame = Message::new(0x135, &[0]);
        frame.protocol = SW_PROTOCOL;
        let mut s = Session::new(Fake {
            rx: [notice, frame].into(),
            ..Default::default()
        });
        s.start(false).unwrap();
        assert!(s.receive(2).unwrap().is_none());
        assert!(s.receive(2).unwrap().is_some());
        assert_eq!(s.wake_notices, 1);
        assert!(s.close());
    }
    #[test]
    #[cfg(not(feature = "mdi-body-read-experiment"))]
    fn policy_block_is_reported_as_local_and_never_reaches_the_driver() {
        let mut s = Session::new(Fake::default());
        s.start(false).unwrap();
        let error = s.transmit("TX 2 1 0000 242 03AA030300000000").unwrap_err();
        assert_eq!(
            crate::adapter_failure::classify(&error, None)["kind"],
            "command_blocked"
        );
        assert!(!s.driver.calls.iter().any(|x| x.starts_with("write ")));
        assert!(s.close());
    }
    #[test]
    #[cfg(feature = "mdi-body-read-experiment")]
    fn uncertain_start_gets_one_stop_before_release_and_no_drop_retry() {
        let mut s = Session::new(Fake { fail: Some(format!("write {SW_PROTOCOL} 0000024203AA030300000000")), ..Default::default() });
        s.start(false).unwrap();
        assert!(s.transmit("TX 2 1 0000 242 03AA030300000000").is_err());
        assert!(s.close());
        let writes: Vec<_> = s.driver.calls.iter().filter(|c| c.starts_with("write ")).collect();
        assert_eq!(writes.len(), 2);
        assert!(writes[1].ends_with("0000024202AA000000000000"));
        let n = s.driver.calls.len(); s.close(); assert_eq!(s.driver.calls.len(), n);
    }
    #[test]
    #[cfg(feature = "mdi-body-read-experiment")]
    fn failed_guest_stop_is_not_retried_and_cleanup_cannot_pass() {
        let mut s = Session::new(Fake::default()); s.start(false).unwrap();
        s.transmit("TX 2 1 0000 242 03AA030300000000").unwrap();
        s.driver.fail = Some(format!("write {SW_PROTOCOL} 0000024202AA000000000000"));
        assert!(s.transmit("TX 2 2 0000 242 02AA000000000000").is_err());
        assert!(!s.close());
        assert_eq!(s.driver.calls.iter().filter(|c| c.starts_with("write ")).count(), 2);
        assert_eq!(s.driver.calls.last().unwrap(), "close");
    }
}
