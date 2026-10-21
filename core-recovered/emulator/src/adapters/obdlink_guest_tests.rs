// SPDX-License-Identifier: MPL-2.0
//! Owned loopback byte peer. No Bluetooth, vehicle, or ECU firmware is opened.
use super::*;
use std::io::BufRead;
use std::net::TcpListener;
use std::sync::Mutex;
use std::sync::atomic::AtomicU64;
static PEER_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy)]
enum Mode {
    Success,
    DiscoveryLateReply,
    CanError,
    Stopped,
    MissingPrompt,
    TransmitOnly,
    VinBurst,
    NoBatchCapability,
    EarlyPrompt,
    DuplicateBoundary,
    PeriodicHsStream,
    ConfigWrongRate,
    ConfigEarlyPrompt,
    ConfigDuplicateBoundary,
    ConfigMissingPrompt,
    ParkConfigError,
}

fn setup_value(command: &str, protocol: &mut u32) -> &'static str {
    match command {
        "ATZ" => "ELM327 v1.4b",
        "STI" => "STN2255 v5.12.4",
        "STDI" => "OBDLink MX+ r3.1.3",
        "STSN" => "225530563988",
        "ATRV" => "13.5V",
        "STPR" => if *protocol == 31 { "31" } else { "61" },
        "STPBRR" => if *protocol == 31 { "500000" } else { "33333" },
        "STP31" => { *protocol = 31; "OK" },
        "STP61" => { *protocol = 61; "OK" },
        "ATCS" => "T:00 R:00",
        _ => "OK",
    }
}

struct Peer {
    directory: std::path::PathBuf,
    commands: Arc<Mutex<Vec<String>>>,
    thread: Option<JoinHandle<()>>,
}
impl Peer {
    fn open(mode: Mode) -> (Self, Link) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let directory = std::env::temp_dir().join(format!(
            "opensaab-stn-peer-{}-{}-{}",
            std::process::id(),
            PEER_SEQUENCE.fetch_add(1, Ordering::Relaxed),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&directory).unwrap();
        let commands = Arc::new(Mutex::new(Vec::<String>::new()));
        let log = commands.clone();
        let worker = thread::spawn(move || {
            let (socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            let mut reader = BufReader::new(socket);
            let mut replies = std::collections::VecDeque::<Vec<u8>>::new();
            let mut protocol = 31;
            let mut ordinary = false;
            let mut monitor = false;
            let mut ordinary_reads = 0;
            let mut requests = 0;
            let mut batched = false;
            let mut saw_sw_tester = false;
            let mut saw_sw_addressed = false;
            let mut sw_reply_sent = false;
            let mut next_periodic = Instant::now();
            let mut periodic_count = 0u8;
            let mut discovery_started = None;
            let mut discovery_lost = false;
            let mut discovery_replied = false;
            loop {
                let mut line = String::new();
                let Ok(n) = reader.read_line(&mut line) else {
                    break;
                };
                if n == 0 {
                    break;
                }
                let command = line.trim_end().to_string();
                log.lock().unwrap().push(command.clone());
                let response = if command.starts_with("HELLO ") {
                    "READY obdlink-native-read".to_string()
                } else if let Some(wire) = command.strip_prefix("WRITE ") {
                    if wire.contains('|') && !wire.ends_with("|STM") {
                        assert!(!ordinary && !monitor, "Setup batch raced live I/O");
                        let mut groups = Vec::new();
                        for c in wire.split('|') {
                            if c == "STP61" && discovery_started.is_some() && !discovery_replied {
                                discovery_lost = true;
                            }
                            let value = if matches!(mode, Mode::ParkConfigError) && saw_sw_tester && c == "STP31" {
                                "?"
                            } else if matches!(mode, Mode::ConfigWrongRate) && c == "STPBRR" {
                                "500000"
                            } else { setup_value(c, &mut protocol) };
                            groups.push(value);
                            if value == "?" { break; }
                        }
                        let mut bytes = format!("{}\r>", groups.join("\r|"));
                        if matches!(mode, Mode::ConfigEarlyPrompt) { bytes = "OK\r>".into(); }
                        if matches!(mode, Mode::ConfigDuplicateBoundary) { bytes = bytes.replacen('|', "||", 1); }
                        if !matches!(mode, Mode::ConfigMissingPrompt) {
                            // Exercise boundaries and final prompt fragmented across reads.
                            for chunk in bytes.as_bytes().chunks(7) { replies.push_back(chunk.to_vec()); }
                        }
                        writeln!(reader.get_mut(), "WRITTEN").unwrap();
                        continue;
                    }
                    batched=wire.ends_with("|STM");
                    let c=wire.strip_suffix("|STM").unwrap_or(wire);
                    assert!(!ordinary, "A new write raced an ordinary request");
                    if c == "STM" {
                        monitor = true;
                        if matches!(mode, Mode::TransmitOnly) && requests == 1 {
                            replies.push_back(b"541 1 ".to_vec());
                            replies.push_back(b"01\r".to_vec());
                        }
                    } else if c == "FE013E0000000000" || (batched && c.starts_with("STPXH:")) {
                        if protocol == 31 && c == "STPXH:101,D:FE021A9A00000000,R:0" {
                            discovery_started = Some(Instant::now());
                        }
                        if protocol == 61 && c == "STPXH:101,D:FE013E0000000000,R:0" { saw_sw_tester = true; }
                        if protocol == 61 && c.starts_with("STPXH:241,") { saw_sw_addressed = true; }
                        ordinary = true;
                        ordinary_reads = 0;
                        requests += 1;
                    } else {
                        let value = match c {
                            "STBC1" if matches!(mode, Mode::NoBatchCapability) => "?",
                            _ => setup_value(c, &mut protocol),
                        };
                        replies.push_back(format!("{value}\r\r>").into_bytes());
                    }
                    "WRITTEN".to_string()
                } else if command == "READ" {
                    thread::sleep(Duration::from_millis(2));
                    if let Some(bytes) = replies.pop_front() {
                        serde_json::json!({"type":"rx","bytes":bytes}).to_string()
                    } else if ordinary {
                        ordinary_reads += 1;
                        let bytes = if requests == 1 {
                            match mode {
                                Mode::MissingPrompt => Vec::new(),
                                Mode::CanError => b"CAN ERROR\r\r>".to_vec(),
                                Mode::Stopped => b"STOPPED\r\r>".to_vec(),
                                Mode::NoBatchCapability => unreachable!(),
                                Mode::EarlyPrompt => b">".to_vec(),
                                Mode::DuplicateBoundary => b"||".to_vec(),
                                Mode::VinBurst => b"|641 8 21 41 42 43 44 45 46 47\r641 8 22 48 49 4A 4B 4C 4D 4E\r".to_vec(),
                                Mode::TransmitOnly => b"|".to_vec(),
                                Mode::Success => match ordinary_reads {
                                    1 => b"541 1 ".to_vec(),
                                    2 => b"01\r".to_vec(),
                                    _ => if batched { b"|".to_vec() } else { b"NO DATA\r\r>".to_vec() },
                                },
                                _ => b"|".to_vec(),
                            }
                        } else {
                            if batched { b"|".to_vec() } else { b"NO DATA\r\r>".to_vec() }
                        };
                        if bytes.contains(&b'|') {
                            ordinary = false;
                            monitor = true;
                            if matches!(mode, Mode::TransmitOnly) && requests==1 {
                                replies.push_back(b"541 1 ".to_vec());
                                replies.push_back(b"01\r".to_vec());
                            }
                        }
                        if bytes.contains(&b'>') {
                            ordinary = false;
                        }
                        if bytes.is_empty() {
                            "EMPTY".to_string()
                        } else {
                            serde_json::json!({"type":"rx","bytes":bytes}).to_string()
                        }
                    } else if monitor && matches!(mode, Mode::DiscoveryLateReply) && protocol == 31
                        && !discovery_lost && !discovery_replied
                        && discovery_started.is_some_and(|start| start.elapsed() >= Duration::from_millis(60)) {
                        discovery_replied = true;
                        serde_json::json!({"type":"rx","bytes":b"651 5 04 5A 9A 00 01\r".to_vec()}).to_string()
                    } else if monitor && matches!(mode, Mode::PeriodicHsStream | Mode::ParkConfigError)
                        && protocol == 31 && Instant::now() >= next_periodic {
                        next_periodic = Instant::now() + Duration::from_millis(300);
                        periodic_count = periodic_count.wrapping_add(1);
                        let bytes = format!("5E8 2 {:02X} {periodic_count:02X}\r", if saw_sw_tester { 0xbb } else { 0xaa }).into_bytes();
                        serde_json::json!({"type":"rx","bytes":bytes}).to_string()
                    } else if monitor && matches!(mode, Mode::PeriodicHsStream) && protocol == 61
                        && saw_sw_addressed && saw_sw_tester && !sw_reply_sent {
                        sw_reply_sent = true;
                        serde_json::json!({"type":"rx","bytes":b"641 4 03 EE 03 02\r".to_vec()}).to_string()
                    } else {
                        "EMPTY".to_string()
                    }
                } else if command == "INTERRUPT" {
                    // The adversarial peer models the captured failure on premature abort.
                    if ordinary {
                        ordinary = false;
                        replies.push_back(b"CAN ERROR\r\r>".to_vec());
                    } else {
                        assert!(monitor, "Unexpected interrupt outside monitoring");
                        monitor = false;
                        replies.push_back(b"STOPPED\r\r>".to_vec());
                    }
                    "WRITTEN".to_string()
                } else if command == "CLOSE" {
                    writeln!(reader.get_mut(), "CLOSED").unwrap();
                    break;
                } else {
                    panic!("Unknown byte operation {command}")
                };
                writeln!(reader.get_mut(), "{response}").unwrap();
            }
        });
        let socket = TcpStream::connect(address).unwrap();
        let link =
            Link::from_socket("00000000000000000000000000000000", &directory, socket).unwrap();
        (
            Self {
                directory,
                commands,
                thread: Some(worker),
            },
            link,
        )
    }
    fn close(mut self) -> Vec<String> {
        self.thread.take().unwrap().join().unwrap();
        let commands = self.commands.lock().unwrap().clone();
        std::fs::remove_dir_all(&self.directory).unwrap();
        commands
    }
}
fn tx(controller: usize, ticket: u64) -> CanTransmission {
    use crate::candi_cpu::{CanElectricalState, CanFrame};
    CanTransmission {
        ticket,
        btr0: if controller == 2 { 0xdd } else { 0xc1 },
        btr1: 0x36,
        electrical: if controller == 2 {
            CanElectricalState::SingleWireGpio {
                latch: 0x27,
                assignment: 0,
                direction: 0x7f,
            }
        } else {
            CanElectricalState::StandardCan
        },
        frame: CanFrame {
            id: 0x101,
            extended: false,
            rtr: false,
            dlc: 8,
            data: vec![0xfe, 1, 0x3e, 0, 0, 0, 0, 0],
        },
    }
}

#[test]
fn two_queued_buses_finish_at_batch_boundary_and_preserve_fragmented_receive() {
    let (peer, link) = Peer::open(Mode::Success);
    let mut backend =
        crate::can_adapter::TranslationBackend::new(Box::new(Bridge::from_link(link)));
    backend.transmit(2, tx(2, 41)).unwrap();
    backend.transmit(0, tx(0, 42)).unwrap();
    let end = Instant::now() + Duration::from_secs(5);
    let mut events = vec![];
    while Instant::now() < end {
        events.extend(backend.poll().unwrap());
        if events
            .iter()
            .filter(|e| matches!(e, Event::Completed { .. }))
            .count()
            == 2
        {
            break;
        }
        thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, Event::Completed { .. }))
            .count(),
        2
    );
    assert!(matches!(&events[0],Event::Received(2,f)if f.id==0x541&&f.data==[1]));
    assert!(matches!(
        events[1],
        Event::Completed {
            controller: 2,
            ticket: 41,
            ..
        }
    ));
    assert!(matches!(
        events[2],
        Event::Completed {
            controller: 0,
            ticket: 42,
            ..
        }
    ));
    backend.close();
    let commands = peer.close();
    let first = commands
        .iter()
        .position(|c| c == "WRITE STPXH:101,D:FE013E0000000000,R:0|STM")
        .unwrap();
    assert_eq!(&commands[first + 1..first + 4], ["READ", "READ", "READ"]);
    assert_eq!(
        commands
            .iter()
            .filter(|c| *c == "WRITE STPXH:101,D:FE013E0000000000,R:0|STM")
            .count(),
        2
    );
}

fn failure_case(mode: Mode, expected: &str) {
    let (peer, link) = Peer::open(mode);
    let mut backend =
        crate::can_adapter::TranslationBackend::new(Box::new(Bridge::from_link(link)));
    backend.transmit(2, tx(2, 41)).unwrap();
    backend.transmit(0, tx(0, 42)).unwrap();
    let end = Instant::now() + Duration::from_secs(5);
    let mut events = vec![];
    let mut error = None;
    while Instant::now() < end {
        match backend.poll() {
            Ok(v) => events.extend(v),
            Err(e) => {
                error = Some(e);
                break;
            }
        }
        thread::sleep(Duration::from_millis(2));
    }
    assert!(error.unwrap().contains(expected));
    assert!(!events.iter().any(|e| matches!(e, Event::Completed { .. })));
    let actual = backend.failure_context().unwrap();
    assert_eq!(
        (actual.tx.controller, actual.ticket, actual.phase),
        (2, 41, "can-request")
    );
    backend.close();
    let commands = peer.close();
    assert_eq!(
        commands
            .iter()
            .filter(|c| *c == "WRITE STPXH:101,D:FE013E0000000000,R:0|STM")
            .count(),
        1
    );
    let first = commands
        .iter()
        .position(|c| c == "WRITE STPXH:101,D:FE013E0000000000,R:0|STM")
        .unwrap();
    assert!(!commands[first + 1..].iter().any(|c| c == "INTERRUPT"));
    assert_eq!(commands.iter().filter(|c| *c == "CLOSE").count(), 1);
}
#[test]
fn can_error_prevents_completion_and_the_queued_next_write() {
    failure_case(Mode::CanError, "CAN ERROR");
}
#[test]
fn unexpected_stopped_is_not_a_successful_completion() {
    failure_case(Mode::Stopped, "unexpectedly interrupted");
}
#[test]
fn missing_prompt_closes_native_without_interrupt_or_replay() {
    failure_case(Mode::MissingPrompt, "prompt timeout");
}

#[test]
fn captured_abort_control_reproduces_peer_error() {
    let (peer, mut link) = Peer::open(Mode::Success);
    let (events, _) = mpsc::sync_channel(10);
    link.send("FE013E0000000000").unwrap();
    assert!(link
        .interrupt_monitor(&events, "next-guest-request")
        .unwrap_err()
        .contains("Refused"));
    // Explicit negative control reproduces the old worker's abort on an owned peer.
    assert_eq!(link.socket("INTERRUPT").unwrap(), "WRITTEN");
    assert!(link.receive(&events).unwrap_err().contains("CAN ERROR"));
    link.cleanup(&events);
    let commands = peer.close();
    assert_eq!(
        &commands[1..4],
        ["WRITE FE013E0000000000", "INTERRUPT", "READ"]
    );
}

#[test]
fn transmit_only_completion_validates_boundary_with_immediate_receive() {
    let (peer, link) = Peer::open(Mode::TransmitOnly);
    let mut backend = Bridge::from_link(link);
    backend.transmit(2, tx(2, 51)).unwrap();
    let end = Instant::now() + Duration::from_secs(5);
    let mut events = vec![];
    while Instant::now() < end {
        events.extend(backend.poll().unwrap());
        if events.iter().any(|e| matches!(e, Event::Received(_, _))) {
            break;
        }
        thread::sleep(Duration::from_millis(2));
    }
    assert!(matches!(
        events[0],
        Event::Completed {
            controller: 2,
            ticket: 51,
            ..
        }
    ));
    assert!(matches!(&events[1],Event::Received(2,f)if f.id==0x541&&f.data==[1]));
    backend.close();
    let commands = peer.close();
    let sent = commands
        .iter()
        .position(|s| s == "WRITE STPXH:101,D:FE013E0000000000,R:0|STM")
        .unwrap();
    assert_eq!(commands[sent+1],"READ");
    assert!(!commands[sent+1..].iter().any(|s|s=="WRITE STM"));
}

#[test]
fn exact_native_key_status_query_is_bounded_and_other_controls_stay_rejected() {
    let (peer, link) = Peer::open(Mode::Success);
    let mut backend = Bridge::from_link(link);
    let mut key = tx(2, 61);
    key.frame.id = 0x241;
    key.frame.data = vec![3, 0xae, 3, 2, 0, 0, 0, 0];
    assert!(!crate::adapters::common::policy::allowed_for_profile(
        2,
        &key,
        Profile::Read
    ));
    for data in [
        vec![3, 0xae, 3, 3, 0, 0, 0, 0],
        vec![3, 0xae, 3, 2, 1, 0, 0, 0],
        vec![2, 0x27, 1, 0, 0, 0, 0, 0],
        vec![2, 0xae, 0, 0, 0, 0, 0, 0],
    ] {
        let mut other = key.clone();
        other.frame.data = data;
        assert!(backend
            .transmit(2, other)
            .unwrap_err()
            .contains("policy rejected"));
    }
    let mut wrong_id = key.clone();
    wrong_id.frame.id = 0x242;
    assert!(backend.transmit(2, wrong_id).is_err());
    for ticket in 61..64 {
        let mut next = key.clone();
        next.ticket = ticket;
        backend.transmit(2, next).unwrap();
    }
    assert!(backend
        .transmit(2, key)
        .unwrap_err()
        .contains("policy rejected"));
    let end = Instant::now() + Duration::from_secs(5);
    while Instant::now() < end && backend.confirmations() < 3 {
        backend.poll().unwrap();
        thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(backend.confirmations(), 3);
    backend.close();
    let commands = peer.close();
    assert_eq!(
        commands
            .iter()
            .filter(|s| *s == "WRITE STPXH:241,D:03AE030200000000,R:0|STM")
            .count(),
        3
    );
    assert!(!commands.iter().any(|s| s.contains("WRITE STPXH:242")
        || s.contains("D:03AE0303")
        || s.contains("D:022701")));
}

#[test]
fn fast_vin_continuations_survive_the_transmit_boundary() {
    let (peer, link)=Peer::open(Mode::VinBurst);
    let mut backend=Bridge::from_link(link);
    backend.transmit(2,tx(2,71)).unwrap();
    let mut events=vec![];let deadline=Instant::now()+Duration::from_secs(5);
    while Instant::now()<deadline {
        events.extend(backend.poll().unwrap());
        if events.iter().filter(|e|matches!(e,Event::Received(_, _))).count()==2 {break;}
        thread::sleep(Duration::from_millis(2));
    }
    let frames:Vec<_>=events.iter().filter_map(|e|if let Event::Received(c,f)=e {Some((*c,f.clone()))}else{None}).collect();
    assert_eq!(frames.len(),2);assert_eq!(frames[0].0,2);assert_eq!(frames[0].1.id,0x641);
    assert_eq!(frames[0].1.data,[0x21,0x41,0x42,0x43,0x44,0x45,0x46,0x47]);
    assert_eq!(frames[1].1.data,[0x22,0x48,0x49,0x4a,0x4b,0x4c,0x4d,0x4e]);
    backend.close();let commands=peer.close();
    assert_eq!(commands.iter().filter(|s|s.starts_with("WRITE STPXH:")).count(),1);
    assert!(!commands.iter().any(|s|s=="WRITE STM"));
}
#[test]
fn unsupported_batching_stops_before_the_queued_vehicle_request() {
    let (peer,link)=Peer::open(Mode::NoBatchCapability);
    let mut backend=Bridge::from_link(link);backend.transmit(2,tx(2,81)).unwrap();
    let deadline=Instant::now()+Duration::from_secs(5);let mut stopped=false;
    while Instant::now()<deadline {match backend.poll(){Ok(events)=>assert!(!events.iter().any(|e|matches!(e,Event::Completed{..}))),Err(e)=>{assert!(e.contains("STBC1"));stopped=true;break;}}thread::sleep(Duration::from_millis(2));}
    assert!(stopped);backend.close();let commands=peer.close();
    assert!(!commands.iter().any(|s|s.starts_with("WRITE STPXH:")));
}

#[test]
fn early_prompt_is_not_a_transmit_completion() { failure_case(Mode::EarlyPrompt,"batch ended"); }
#[test]
fn duplicate_batch_boundary_is_not_a_transmit_completion() { failure_case(Mode::DuplicateBoundary,"Unexpected STN batch boundary"); }

fn hs_read(ticket: u64) -> CanTransmission {
    let mut request = tx(0, ticket);
    request.frame.id = 0x7e0;
    request.frame.data = vec![2, 0x1a, 0x90, 0, 0, 0, 0, 0];
    request
}

fn observe_until(backend: &mut Bridge, events: &mut Vec<Event>, condition: impl Fn(&[Event]) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(4);
    while Instant::now() < deadline {
        events.extend(backend.poll().unwrap());
        if condition(events) { return; }
        thread::sleep(Duration::from_millis(2));
    }
    panic!("Expected owned-peer event absent");
}

#[test]
fn periodic_hs_stream_resumes_after_sw_tester_without_another_hs_transmission() {
    let (peer, link) = Peer::open(Mode::PeriodicHsStream);
    let mut backend = Bridge::from_link(link);
    backend.transmit(0, hs_read(101)).unwrap();
    let mut events = vec![];
    observe_until(&mut backend, &mut events, |v| v.iter().any(|e|
        matches!(e, Event::Received(0, f) if f.id == 0x5e8 && f.data[0] == 0xaa)));
    backend.transmit(2, tx(2, 102)).unwrap();
    observe_until(&mut backend, &mut events, |v| v.iter().filter(|e|
        matches!(e, Event::Received(0, f) if f.id == 0x5e8 && f.data[0] == 0xbb)).count() >= 3);
    assert_eq!(events.iter().filter(|e| matches!(e, Event::Completed { .. })).count(), 2);
    backend.close();
    let commands = peer.close();
    assert_eq!(commands.iter().filter(|s| s.starts_with("WRITE STPXH:")).count(), 2);
    let tester = commands.iter().position(|s| s == "WRITE STPXH:101,D:FE013E0000000000,R:0|STM").unwrap();
    assert!(commands[tester + 1..].iter().any(|s| s.starts_with("WRITE STPC|STP31|STPR|STPBRR|")));
    assert!(commands[tester + 1..].iter().any(|s| s == "WRITE STM"));
}

#[test]
fn addressed_sw_exchange_keeps_sw_receive_after_functional_tester() {
    let (peer, link) = Peer::open(Mode::PeriodicHsStream);
    let mut backend = Bridge::from_link(link);
    backend.transmit(0, hs_read(111)).unwrap();
    let mut events = vec![];
    observe_until(&mut backend, &mut events, |v| v.iter().any(|e|
        matches!(e, Event::Completed { ticket: 111, .. })));
    let mut key = tx(2, 112);
    key.frame.id = 0x241;
    key.frame.data = vec![3, 0xae, 3, 2, 0, 0, 0, 0];
    backend.transmit(2, key).unwrap();
    backend.transmit(2, tx(2, 113)).unwrap();
    observe_until(&mut backend, &mut events, |v| v.iter().any(|e|
        matches!(e, Event::Received(2, f) if f.id == 0x641 && f.data == [3, 0xee, 3, 2])));
    backend.close();
    let commands = peer.close();
    let tester = commands.iter().position(|s| s == "WRITE STPXH:101,D:FE013E0000000000,R:0|STM").unwrap();
    assert!(!commands[tester + 1..].iter().any(|s| s.starts_with("WRITE STPC|STP31|")));
    assert_eq!(commands.iter().filter(|s| s.starts_with("WRITE STPXH:")).count(), 3);
}

#[test]
fn receive_preference_requires_the_exact_functional_frame_and_prior_hs_work() {
    let mut preference = ReceivePreference::default();
    preference.transmitted(2, &tx(2, 1));
    assert!(!preference.return_to_hs);
    preference.transmitted(0, &hs_read(2));
    for mut other in [tx(2, 3), tx(2, 4), tx(2, 5), tx(2, 6)] {
        // Each altered frame selects SW instead of granting the parking exception.
        match other.ticket { 3 => other.frame.id += 1, 4 => other.frame.data[3] = 1,
            5 => other.frame.dlc = 7, _ => other.frame.extended = true }
        preference.transmitted(2, &other);
        assert!(!preference.return_to_hs);
        assert_eq!(preference.last_non_tester_controller, Some(2));
        preference.transmitted(0, &hs_read(7));
    }
    preference.transmitted(2, &tx(2, 8));
    assert!(preference.return_to_hs);
    preference.transmitted(0, &hs_read(9));
    assert!(!preference.return_to_hs); // A queued request supersedes idle parking.
}

fn bad_setup_batch(mode: Mode, expected: &str) {
    let (peer, mut link) = Peer::open(mode);
    link.setup().unwrap();
    assert!(link.select_bus(2).unwrap_err().contains(expected));
    assert_eq!(link.controller, 0);
    assert!(!link.synchronized);
    let (events, _) = mpsc::sync_channel(8);
    link.cleanup(&events);
    let commands = peer.close();
    assert!(!commands.iter().any(|s| s.starts_with("WRITE STPXH:")));
    let batch = commands.iter().position(|s| s.starts_with("WRITE STPC|STP61|")).unwrap();
    assert!(!commands[batch + 1..].iter().any(|s| s.starts_with("WRITE ") || s == "INTERRUPT"));
    assert_eq!(commands.iter().filter(|s| *s == "CLOSE").count(), 1);
}

#[test]
fn wrong_setup_batch_bitrate_stops_before_any_vehicle_request() { bad_setup_batch(Mode::ConfigWrongRate, "STPBRR"); }
#[test]
fn early_setup_batch_prompt_is_rejected() { bad_setup_batch(Mode::ConfigEarlyPrompt, "count mismatch"); }
#[test]
fn duplicate_setup_batch_boundary_is_rejected() { bad_setup_batch(Mode::ConfigDuplicateBoundary, "readback rejected"); }
#[test]
fn missing_setup_batch_prompt_closes_without_in_band_writes() { bad_setup_batch(Mode::ConfigMissingPrompt, "prompt timeout"); }

#[test]
fn empty_or_delimited_setup_commands_never_write_a_replay_or_nested_batch() {
    let (peer, mut link) = Peer::open(Mode::Success);
    for commands in [vec![], vec![("", "OK")], vec![("STPC|STPO", "OK")], vec![("STPC\r", "OK")]] {
        assert!(link.exact_setup_batch(&commands).is_err());
    }
    assert_eq!(link.socket("CLOSE").unwrap(), "CLOSED");
    assert!(!peer.close().iter().any(|s| s.starts_with("WRITE ")));
}

#[test]
fn failed_idle_return_stops_without_replaying_the_completed_tester() {
    let (peer, link) = Peer::open(Mode::ParkConfigError);
    let mut backend = Bridge::from_link(link);
    backend.transmit(0, hs_read(121)).unwrap();
    let mut events = vec![];
    observe_until(&mut backend, &mut events, |v| v.iter().any(|e|
        matches!(e, Event::Completed { ticket: 121, .. })));
    backend.transmit(2, tx(2, 122)).unwrap();
    let deadline = Instant::now() + Duration::from_secs(4);
    let mut failed = false;
    while Instant::now() < deadline {
        match backend.poll() {
            Ok(next) => events.extend(next),
            Err(error) => { assert!(error.contains("STP31")); failed = true; break; }
        }
        thread::sleep(Duration::from_millis(2));
    }
    assert!(failed);
    assert_eq!(events.iter().filter(|e| matches!(e, Event::Completed { ticket: 122, .. })).count(), 1);
    backend.close();
    let commands = peer.close();
    assert_eq!(commands.iter().filter(|s| s.starts_with("WRITE STPXH:")).count(), 2);
    assert_eq!(commands.iter().filter(|s| *s == "CLOSE").count(), 1);
}


#[test]
fn discovery_catches_ehu_at_60ms_before_queued_sw_without_changing_requests() {
    let (peer, link) = Peer::open(Mode::DiscoveryLateReply);
    let mut backend = Bridge::from_link(link);
    let mut discovery = tx(0, 131);
    discovery.frame.data = vec![0xfe, 2, 0x1a, 0x9a, 0, 0, 0, 0];
    backend.transmit(0, discovery).unwrap();
    let mut events = vec![];
    observe_until(&mut backend, &mut events, |v| v.iter().any(|e|
        matches!(e, Event::Completed { ticket: 131, .. })));
    // Same-bus tester work must not erase the outstanding collection window.
    backend.transmit(0, tx(0, 133)).unwrap();
    let mut sw = tx(2, 132);
    sw.frame.data = vec![0xfe, 2, 0x1a, 0x9a, 0, 0, 0, 0];
    backend.transmit(2, sw).unwrap();
    observe_until(&mut backend, &mut events, |v| v.iter().any(|e|
        matches!(e, Event::Completed { ticket: 132, .. })));
    let ehu = events.iter().position(|e|
        matches!(e, Event::Received(0, f) if f.id == 0x651 && f.data == [4, 0x5a, 0x9a, 0, 1])).expect("60ms EHU reply lost before switch");
    let sw = events.iter().position(|e| matches!(e, Event::Completed { ticket: 132, .. })).unwrap();
    assert!(ehu < sw);
    backend.close();
    let commands = peer.close();
    assert_eq!(commands.iter().filter(|s| *s == "WRITE STPXH:101,D:FE021A9A00000000,R:0|STM").count(), 2);
}

#[test]
fn reply_window_excludes_tester_physical_flow_control_and_sw() {
    assert!(!functional_hs_reply_request(0, &tx(0, 1)));
    assert!(!functional_hs_reply_request(0, &hs_read(2)));
    let mut request = tx(0, 3);
    request.frame.data = vec![0xfe, 2, 0x1a, 0x9a, 0, 0, 0, 0];
    assert!(functional_hs_reply_request(0, &request));
    assert!(!functional_hs_reply_request(2, &request));
    request.frame.data[1] = 0x30;
    assert!(!functional_hs_reply_request(0, &request));
}

#[test]
fn cancel_during_discovery_window_does_not_send_queued_cross_bus_request() {
    let (peer, link) = Peer::open(Mode::DiscoveryLateReply);
    let mut backend = Bridge::from_link(link);
    let mut discovery = tx(0, 141);
    discovery.frame.data = vec![0xfe, 2, 0x1a, 0x9a, 0, 0, 0, 0];
    backend.transmit(0, discovery).unwrap();
    let mut events = vec![];
    observe_until(&mut backend, &mut events, |v| v.iter().any(|e|
        matches!(e, Event::Completed { ticket: 141, .. })));
    backend.transmit(2, tx(2, 142)).unwrap();
    backend.close();
    let commands = peer.close();
    assert!(!commands.iter().any(|s| s == "WRITE STPXH:101,D:FE013E0000000000,R:0|STM"));
    assert_eq!(commands.iter().filter(|s| *s == "CLOSE").count(), 1);
}

#[test]
fn ordinary_hs_uses_explicit_stpx_header_without_redundant_setup_roundtrips() {
    let (peer, link) = Peer::open(Mode::Success);
    let mut backend = Bridge::from_link(link);
    backend.transmit(0, hs_read(151)).unwrap();
    let mut events = vec![];
    observe_until(&mut backend, &mut events, |v| v.iter().any(|e|
        matches!(e, Event::Completed { ticket: 151, .. })));
    backend.close();
    let commands = peer.close();
    assert_eq!(commands.iter().filter(|s| *s == "WRITE STPO").count(), 1); // startup only
    assert!(!commands.iter().any(|s| s.starts_with("WRITE ATSH")));
    assert_eq!(commands.iter().filter(|s| *s == "WRITE STPXH:7E0,D:021A900000000000,R:0|STM").count(), 1);
}

#[test]
fn clearing_profile_requires_the_matching_authenticated_bridge_role() {
    for (reply,profile,accepted) in [
        ("READY obdlink-native-read",Profile::ClearDtc,false),
        ("READY obdlink-native-clear-dtc",Profile::Read,false),
        ("READY obdlink-native-reset-counters",Profile::ClearDtc,false),
        ("READY obdlink-native-clear-dtc",Profile::ClearDtc,true),
        ("READY obdlink-native-full-native",Profile::FullNative,true),
        ("READY obdlink-native-seeds",Profile::Seeds,true),
        ("READY obdlink-native-full-native",Profile::Read,false),
        ("READY obdlink-native-read",Profile::FullNative,false),
        ("READY obdlink-native-full-native",Profile::Seeds,false),
        ("READY obdlink-native-seeds",Profile::FullNative,false),
    ] {
        let listener=TcpListener::bind("127.0.0.1:0").unwrap();let address=listener.local_addr().unwrap();
        let worker=thread::spawn(move || {
            let (socket,_)=listener.accept().unwrap();let mut reader=BufReader::new(socket);let mut hello=String::new();reader.read_line(&mut hello).unwrap();
            assert_eq!(hello.trim(),"HELLO 0123456789abcdef0123456789abcdef");
            use std::io::Write;reader.get_mut().write_all(format!("{reply}\n").as_bytes()).unwrap();
        });
        let dir=std::env::temp_dir().join(format!("opensaab-clear-role-{}-{}",std::process::id(),PEER_SEQUENCE.fetch_add(1,Ordering::Relaxed)));std::fs::create_dir(&dir).unwrap();
        let link=Link::from_socket_profile("0123456789abcdef0123456789abcdef",&dir,TcpStream::connect(address).unwrap(),profile);
        assert_eq!(link.is_ok(),accepted);drop(link);worker.join().unwrap();std::fs::remove_dir_all(dir).unwrap();
    }
}
