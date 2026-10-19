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
    CanError,
    Stopped,
    MissingPrompt,
    TransmitOnly,
    VinBurst,
    NoBatchCapability,
    EarlyPrompt,
    DuplicateBoundary,
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
                        ordinary = true;
                        ordinary_reads = 0;
                        requests += 1;
                    } else {
                        let value = match c {
                            "ATZ" => "ELM327 v1.4b",
                            "STI" => "STN2255 v5.12.4",
                            "STDI" => "OBDLink MX+ r3.1.3",
                            "STSN" => "225530563988",
                            "ATRV" => "13.5V",
                            "STPR" => {
                                if protocol == 31 {
                                    "31"
                                } else {
                                    "61"
                                }
                            }
                            "STPBRR" => {
                                if protocol == 31 {
                                    "500000"
                                } else {
                                    "33333"
                                }
                            }
                            "ATCS" => "T:00 R:00",
                            "STBC1" if matches!(mode, Mode::NoBatchCapability) => "?",
                            _ => "OK",
                        };
                        if c == "STP31" {
                            protocol = 31;
                        } else if c == "STP61" {
                            protocol = 61;
                        }
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
