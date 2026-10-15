// SPDX-License-Identifier: MPL-2.0
//! Research client for classic-MDI HS ISO15765 VIN and bounded ECM current data.
//! An OS route or explicit userspace carrier supplies the TCP endpoint.
//! No OEM DLL, saved session, discovery replay or raw-CAN reconstruction.
use crate::{
    trace,
    wire::{self, Record, Tags},
};
use serde_json::{json, Value};
use std::{
    io::{Read, Write},
    net::{Shutdown, SocketAddr, TcpStream},
    time::{Duration, Instant},
};

const IO_LIMIT: Duration = Duration::from_secs(2);
const ATTEMPT_LIMIT: Duration = Duration::from_secs(15);

pub struct Attempt {
    pub passed: bool,
    pub report: Value,
}

struct Connection {
    socket: TcpStream,
    buffer: Vec<u8>,
}
impl Connection {
    fn open(peer: SocketAddr) -> Result<Self, String> {
        let socket =
            TcpStream::connect_timeout(&peer, IO_LIMIT).map_err(|e| format!("TCP connect: {e}"))?;
        socket
            .set_nodelay(true)
            .map_err(|e| format!("TCP no delay: {e}"))?;
        Ok(Self {
            socket,
            buffer: Vec::new(),
        })
    }
    fn send(&mut self, record: &Record) -> Result<(), String> {
        self.socket
            .set_write_timeout(Some(IO_LIMIT))
            .map_err(|e| e.to_string())?;
        self.socket
            .write_all(&record.encoded()?)
            .map_err(|e| format!("TCP write: {e}"))
    }
    // Keep partial records across read timeouts. A timeout never discards bytes.
    fn receive(&mut self, deadline: Instant) -> Result<Option<Record>, String> {
        loop {
            if self.buffer.len() >= 8 {
                let n = u32::from_le_bytes(self.buffer[..4].try_into().unwrap()) as usize;
                if !(8..=wire::MAX_RECORD).contains(&n) {
                    return Err("TCP record bounds".into());
                }
                if self.buffer.len() >= n {
                    let record = Record::new(
                        u32::from_le_bytes(self.buffer[4..8].try_into().unwrap()),
                        self.buffer[8..n].to_vec(),
                    );
                    self.buffer.drain(..n);
                    return Ok(Some(record));
                }
            }
            let Some(remaining) = deadline
                .checked_duration_since(Instant::now())
                .filter(|remaining| !remaining.is_zero())
            else {
                return Ok(None);
            };
            self.socket
                .set_read_timeout(Some(remaining.min(Duration::from_millis(30))))
                .map_err(|e| e.to_string())?;
            let mut b = [0; 4096];
            match self.socket.read(&mut b) {
                Ok(0) => return Err("TCP peer closed".into()),
                Ok(n) => {
                    if self.buffer.len() + n > wire::MAX_RECORD + b.len() {
                        return Err("TCP buffer bounds".into());
                    }
                    self.buffer.extend(&b[..n]);
                }
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock
                            | std::io::ErrorKind::TimedOut
                            | std::io::ErrorKind::Interrupted
                    ) => {}
                Err(e) => return Err(format!("TCP receive: {e}")),
            }
        }
    }
    fn shutdown(&self) -> bool {
        match self.socket.shutdown(Shutdown::Both) {
            Ok(()) => true,
            Err(e) => e.kind() == std::io::ErrorKind::NotConnected,
        }
    }
}

struct Session {
    command: Option<Connection>,
    notification: Option<Connection>,
    start: Instant,
    deadline: Instant,
    session: u32,
    link: u32,
    receiver: u32,
    created: bool,
    connected: bool,
    receiving: bool,
    requests: u32,
    request_id: u32,
    raw_spa: bool,
    next_request_token: u32,
    last_heartbeat: Instant,
    operations: Vec<Value>,
    events: Vec<Value>,
    cleanup: Value,
}
impl Session {
    fn rpc(&mut self, request: Record, expected: u32) -> Result<Record, String> {
        let deadline = (Instant::now() + IO_LIMIT).min(self.deadline);
        if Instant::now() >= deadline {
            return Err("Attempt deadline".into());
        }
        let c = self.command.as_mut().ok_or("Command connection absent")?;
        c.send(&request)?;
        let reply = c.receive(deadline)?.ok_or("RPC deadline")?;
        self.operations.push(json!({"elapsed_ms": self.start.elapsed().as_millis(), "request": request.opcode,
            "reply": reply.opcode, "request_body_hex": trace::hex(&request.body), "reply_body_hex": trace::hex(&reply.body)}));
        reply.reply(expected)?;
        Ok(reply)
    }
    fn heartbeat(&mut self) -> Result<(), String> {
        if self.last_heartbeat.elapsed() >= Duration::from_secs(1) {
            let reply = self.rpc(wire::heartbeat(), 0x889)?;
            wire::heartbeat_reply(&reply)?;
            self.last_heartbeat = Instant::now();
        }
        Ok(())
    }
    fn notification(&mut self, deadline: Instant, phase: &str) -> Result<Vec<Value>, String> {
        let record = self
            .notification
            .as_mut()
            .ok_or("Notification connection absent")?
            .receive(deadline)?;
        let Some(record) = record else {
            return Ok(Vec::new());
        };
        if !matches!(record.opcode, 0x863 | 0x5e1 | 0x5df | 0x5e3) {
            return Err("Unexpected notification opcode".into());
        }
        let decoded = trace::decode_stream(&record.encoded()?)
            .map_err(|e| format!("Callback byte {}: {}", e.offset, e.reason))?;
        let value = &decoded[0]["decoded"];
        let items = if value["kind"] == "event_batch" {
            value["items"].as_array().unwrap().clone()
        } else {
            vec![value.clone()]
        };
        for item in &items {
            self.events.push(json!({"phase": phase, "elapsed_ms": self.start.elapsed().as_millis(), "opcode": record.opcode, "event": item}));
        }
        if self.events.len() > 4096 {
            return Err("Callback count limit".into());
        }
        Ok(items)
    }
    fn initialize(&mut self, peer: SocketAddr) -> Result<(), String> {
        self.initialize_profile(peer, false, false)
    }
    fn register_connections(&mut self, peer: SocketAddr) -> Result<(), String> {
        self.command = Some(Connection::open(peer)?);
        let reply = self.rpc(wire::register(0), 0x885)?;
        self.session = wire::registration_reply(&reply, None)?;
        self.notification = Some(Connection::open(peer)?);
        let c = self.notification.as_mut().unwrap();
        c.send(&wire::register(self.session))?;
        let reply = c
            .receive(Instant::now() + IO_LIMIT)?
            .ok_or("Callback registration deadline")?;
        wire::registration_reply(&reply, Some(self.session))?;
        self.operations.push(json!({"request":0x884,"reply":reply.opcode,"callback_registration":true,"reply_body_hex":trace::hex(&reply.body)}));
        Ok(())
    }
    fn initialize_profile(&mut self, peer: SocketAddr, spa: bool, raw: bool) -> Result<(), String> {
        self.register_connections(peer)?;
        let mut tags = Tags::for_session(self.session);
        self.configure_link(spa, raw, &mut tags)
    }
    fn configure_link(&mut self, spa: bool, raw: bool, tags: &mut Tags) -> Result<(), String> {
        self.created = false;
        self.connected = false;
        self.receiving = false;
        self.link = tags.link();
        let tokens: Vec<u32> = (0..6).map(|_| tags.primitive()).collect();
        self.receiver = tokens[1];
        self.next_request_token = tokens[5];
        self.created = true; // May be acquired even if the reply is lost.
        let parameters: &[(u32, u32, u32)] = if spa {
            self.request_id = 0x25b;
            self.raw_spa = raw;
            self.rpc(
                if raw {
                    wire::create_sw_raw(self.link)
                } else {
                    wire::create_sw_iso15765(self.link)
                },
                0x450,
            )?;
            &[
                (22, 3, if raw { 0 } else { 1 }),
                (177, 5, 0),
                (9, 5, 33333),
                (12, 5, 80),
                (153, 5, 0),
                (167, 5, 15),
                (133, 1, 10000),
                (152, 3, 2),
            ]
        } else if raw {
            self.rpc(wire::create_hs_raw(self.link), 0x450)?;
            &[
                (9, 5, 500000),
                (177, 5, 2),
                (12, 5, 80),
                (153, 5, 0),
                (167, 5, 15),
                (22, 3, 0),
                (21, 3, 0),
                (129, 1, 0),
                (133, 1, 0),
            ]
        } else {
            self.rpc(wire::create_hs_iso15765(self.link), 0x450)?;
            &[
                (9, 5, 500000),
                (190, 3, 1),
                (22, 3, 0),
                (21, 3, 0),
                (25, 3, 0),
                (164, 3, 1),
                (129, 1, 0),
                (133, 1, 0),
                (27, 3, 0),
            ]
        };
        for &(id, class, value) in parameters {
            if raw && spa && id == 133 {
                continue;
            }
            self.rpc(wire::set_unum32(self.link, id, class, value), 0x45a)?;
        }
        if raw && spa {
            self.rpc(wire::set_unum32(self.link, 166, 3, 0), 0x45a)?;
        }
        self.connected = true;
        self.rpc(wire::link_operation(0x45f, self.link)?, 0x460)?;
        self.rpc(wire::response_table(self.link, &[])?, 0x45e)?;
        self.rpc(
            wire::primitive(self.link, tokens[0], 0x8003, &[], &[], 0, 0, None)?,
            0x464,
        )?;
        self.receiving = true;
        self.rpc(
            if raw {
                wire::primitive(
                    self.link,
                    self.receiver,
                    0x8004,
                    &[],
                    &[0xa0, 1, 0, 0],
                    0,
                    -1,
                    Some(0x2534),
                )?
            } else if spa {
                wire::receive_on_id(self.link, self.receiver, 0x65b)?
            } else {
                wire::primitive(
                    self.link,
                    self.receiver,
                    0x8004,
                    &[],
                    &[0xa0, 1, 0, 0],
                    0,
                    -1,
                    Some(0x2534),
                )?
            },
            0x464,
        )?;
        self.rpc(
            wire::primitive(self.link, tokens[2], 0x8001, &[], &[0; 4], 0, 0, None)?,
            0x464,
        )?;
        self.rpc(
            wire::primitive(self.link, tokens[3], 0x8003, &[], &[], 0, 0, None)?,
            0x464,
        )?;
        if raw {
            self.rpc(
                wire::response_table(self.link, &[(0xfffffffe, vec![])])?,
                0x45e,
            )?;
            self.rpc(wire::set_unum32(self.link, 112, 3, 1), 0x45a)?;
        }
        if !raw {
            self.rpc(
                wire::response_table(
                    self.link,
                    &[(
                        0,
                        vec![
                            (34, 6, if spa { 0x65b } else { 0x7e8 }),
                            (33, 6, 5),
                            (32, 6, 0),
                            (31, 6, self.request_id),
                            (30, 6, if spa { 37 } else { 5 }),
                            (29, 6, 0),
                            // Native SW ISO table explicitly clears these extra IDs.
                            // HS uses its historical six-parameter table unchanged.
                        ],
                    )],
                )?,
                0x45e,
            )?;
        }
        if spa && !raw {
            self.rpc(
                wire::response_table(
                    self.link,
                    &[(
                        0,
                        vec![
                            (31, 6, 0x25b),
                            (30, 6, 37),
                            (29, 6, 0),
                            (34, 6, 0x65b),
                            (33, 6, 5),
                            (32, 6, 0),
                            (37, 6, 0),
                            (36, 6, 0),
                            (35, 6, 0),
                        ],
                    )],
                )?,
                0x45e,
            )?;
        }
        self.rpc(
            wire::primitive(self.link, tokens[4], 0x8003, &[], &[], 0, 0, None)?,
            0x464,
        )?;
        // Drain only pre-request callbacks, retaining them with an explicit phase.
        let drain_deadline = (Instant::now() + Duration::from_millis(100)).min(self.deadline);
        loop {
            let items = self.notification(
                drain_deadline.min(Instant::now() + Duration::from_millis(10)),
                "before_request",
            )?;
            if items.is_empty() {
                break;
            }
            reject_async_error(&items)?;
            if Instant::now() >= drain_deadline {
                if raw {
                    break;
                } // Raw periodic bus traffic need not become quiet.
                return Err("Pre-request notifications did not settle".into());
            }
        }
        Ok(())
    }
    fn send_vehicle(&mut self, payload: &[u8]) -> Result<(), String> {
        let token = self.next_request_token;
        self.next_request_token = token.checked_add(1).ok_or("Request tag exhausted")?;
        let mut request = self.request_id.to_be_bytes().to_vec();
        request.extend(payload);
        self.requests += 1;
        if self.request_id == 0x25b {
            if payload != [0x1a, 0x9a] {
                return Err("Unselected SPA operation".into());
            }
            self.rpc(
                if self.raw_spa {
                    wire::spa_raw_preconditions(self.link, token)
                } else {
                    wire::spa_preconditions(self.link, token)
                },
                0x464,
            )?;
            return Ok(());
        }
        self.rpc(
            wire::primitive(
                self.link,
                token,
                0x8004,
                &request,
                &[0x22, 0, 0, 0x40],
                1,
                0,
                None,
            )?,
            0x464,
        )?;
        Ok(())
    }
    fn run(&mut self, peer: SocketAddr) -> Result<String, String> {
        self.initialize(peer)?;
        self.send_vehicle(&[0x1a, 0x90])?;
        let deadline = (Instant::now() + Duration::from_secs(5)).min(self.deadline);
        while Instant::now() < deadline {
            let items = self.notification(deadline, "after_request")?;
            reject_async_error(&items)?;
            for item in items {
                if let Some(vin) = match_vin(&item, self.link, self.receiver)? {
                    return Ok(vin);
                }
            }
        }
        Err("No matching complete fresh VIN before deadline".into())
    }
    fn current_data(&mut self, pid: u8) -> Result<Vec<u8>, String> {
        if pid != 0 && !crate::ecm::PIDS.contains(&pid) {
            return Err("Unselected ECM current-data PID".into());
        }
        // Drain earlier callbacks before a new explicit request; retain every observation.
        let quiet = (Instant::now() + Duration::from_millis(100)).min(self.deadline);
        loop {
            let items = self.notification(
                quiet.min(Instant::now() + Duration::from_millis(10)),
                "before_ECM_request",
            )?;
            reject_async_error(&items)?;
            if items.is_empty() {
                break;
            }
            if Instant::now() >= quiet {
                return Err("ECM pre-request callbacks did not settle".into());
            }
        }
        self.send_vehicle(&[1, pid])?;
        let deadline = (Instant::now() + IO_LIMIT).min(self.deadline);
        while Instant::now() < deadline {
            let items = self.notification(deadline, "after_ECM_request")?;
            reject_async_error(&items)?;
            for item in items {
                if let Some(data) = match_current_data(&item, self.link, self.receiver, pid)? {
                    return Ok(data);
                }
            }
        }
        Err(format!(
            "No matching fresh Mode01 PID{pid:02x} response before deadline"
        ))
    }
    fn close_link(&mut self) {
        // Give cleanup its own finite deadline after any initialization/VIN failure.
        self.deadline = Instant::now() + Duration::from_secs(10);
        if self.receiving {
            let result = self.rpc(wire::cancel(self.link, self.receiver), 0x466);
            self.cleanup["receiver_cancel_rpc"] = json!(result.is_ok());
            if let Err(e) = result {
                self.cleanup["receiver_cancel_error"] = json!(e);
            }
            let deadline = Instant::now() + IO_LIMIT;
            let mut terminal = false;
            while Instant::now() < deadline && !terminal {
                match self.notification(deadline, "cleanup") {
                    Ok(items) => {
                        terminal = items.iter().any(|e| {
                            e["link"] == self.link
                                && e["token"] == self.receiver
                                && e["data"]["kind"] == "status"
                                && e["data"]["status"] == 0x8013
                        })
                    }
                    Err(e) => {
                        self.cleanup["receiver_terminal_error"] = json!(e);
                        break;
                    }
                }
            }
            self.cleanup["receiver_cancel_terminal"] = json!(terminal);
        }
        for (acquired, op, reply, name) in [
            (self.connected, 0x461, 0x462, "link_disconnect"),
            (self.created, 0x451, 0x452, "link_destroy"),
        ] {
            if acquired {
                let result = self.rpc(wire::link_operation(op, self.link).unwrap(), reply);
                self.cleanup[name] = json!(result.is_ok());
                if let Err(e) = result {
                    self.cleanup[format!("{name}_error")] = json!(e);
                }
            }
        }
        self.receiving = false;
        self.connected = false;
        self.created = false;
    }
    fn close(&mut self) {
        self.close_link();
        if self.session != 0 {
            let result = self
                .rpc(wire::release_session(self.session), 0x887)
                .and_then(|r| wire::session_released(&r));
            self.cleanup["session_release"] = json!(result.is_ok());
            if let Err(e) = result {
                self.cleanup["session_release_error"] = json!(e);
            }
        }
        let mut closed = true;
        if let Some(c) = self.notification.take() {
            closed &= c.shutdown();
        }
        if let Some(c) = self.command.take() {
            closed &= c.shutdown();
        }
        self.cleanup["sockets_shutdown"] = json!(closed);
    }
}

fn reject_async_error(items: &[Value]) -> Result<(), String> {
    if items.iter().any(|e| e["data"]["kind"] == "error") {
        return Err("Asynchronous MDI error".into());
    }
    Ok(())
}

/// Select one complete assembled Saab reply, preserving native CAN ownership.
fn match_vin(event: &Value, link: u32, receiver: u32) -> Result<Option<String>, String> {
    if event["link"] != link
        || event["token"] != receiver
        || event["data"]["kind"] != "result"
        || event["data"]["rx_flags_hex"] != "00000040"
    {
        return Ok(None);
    }
    let data = event["data"]["data_hex"]
        .as_str()
        .ok_or("Result payload absent")?;
    if !data.starts_with("000007e8") {
        return Ok(None);
    }
    let payload = &data[8..];
    if payload.starts_with("7f1a") {
        if payload == "7f1a78" {
            return Ok(None);
        }
        return Err("ECU negative VIN response".into());
    }
    if data.len() != 46 || !payload.starts_with("5a90") {
        return Ok(None);
    }
    let vin: Vec<u8> = (12..46)
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&data[i..i + 2], 16).map_err(|_| "VIN hex encoding".to_string())
        })
        .collect::<Result<_, _>>()?;
    if !vin.starts_with(b"YS3")
        || !vin
            .iter()
            .all(|c| c.is_ascii_digit() || (c.is_ascii_uppercase() && !b"IOQ".contains(c)))
    {
        return Err("VIN alphabet or Saab prefix".into());
    }
    Ok(Some(String::from_utf8(vin).map_err(|_| "VIN ASCII")?))
}

fn match_current_data(
    event: &Value,
    link: u32,
    receiver: u32,
    pid: u8,
) -> Result<Option<Vec<u8>>, String> {
    if event["link"] != link
        || event["token"] != receiver
        || event["data"]["kind"] != "result"
        || event["data"]["rx_flags_hex"] != "00000040"
    {
        return Ok(None);
    }
    let text = event["data"]["data_hex"]
        .as_str()
        .ok_or("Current-data payload absent")?;
    if text.len() % 2 != 0 {
        return Err("Current-data hex length".into());
    }
    let data = (0..text.len())
        .step_by(2)
        .map(|n| {
            u8::from_str_radix(&text[n..n + 2], 16)
                .map_err(|_| "Current-data hex encoding".to_owned())
        })
        .collect::<Result<Vec<_>, _>>()?;
    if data.get(..4) != Some(&[0, 0, 7, 0xe8]) {
        return Ok(None);
    }
    let payload = &data[4..];
    if payload.starts_with(&[0x7f, 1]) {
        if payload == [0x7f, 1, 0x78] {
            return Ok(None);
        }
        return Err(format!(
            "ECM negative current-data response: {}",
            trace::hex(payload)
        ));
    }
    if !payload.starts_with(&[0x41, pid]) {
        return Ok(None);
    }
    let expected = if pid == 0 {
        4
    } else if matches!(pid, 0x0c | 0x10) {
        2
    } else {
        1
    };
    if payload.len() != expected + 2 {
        return Err("Current-data reply exact width".into());
    }
    Ok(Some(payload[2..].to_vec()))
}

fn new_session(start: Instant) -> Session {
    Session {
        command: None,
        notification: None,
        start,
        deadline: start + ATTEMPT_LIMIT,
        session: 0,
        link: 0,
        receiver: 0,
        created: false,
        connected: false,
        receiving: false,
        requests: 0,
        request_id: 0x7e0,
        raw_spa: false,
        next_request_token: 0,
        last_heartbeat: start,
        operations: Vec::new(),
        events: Vec::new(),
        cleanup: json!({}),
    }
}
fn cleanup_ok(s: &Session) -> bool {
    [
        "receiver_cancel_rpc",
        "receiver_cancel_terminal",
        "link_disconnect",
        "link_destroy",
        "session_release",
        "sockets_shutdown",
    ]
    .iter()
    .all(|k| s.cleanup[*k] == true)
}

/// Original firmware transport. This API does not construct diagnostic requests,
/// assemble ISO-TP or grant security access. The caller owns command permissions.
/// All socket I/O belongs on a worker thread, never the emulation/UI thread.
/// Two physical raw CAN links owned by one fresh MDI registration/socket pair.
/// The device rejects a concurrent second owner registration with 0x10000008.
/// Original diagnostic frames and flow control remain owned by the caller.
pub struct RawPair {
    inner: Session,
    channels: Vec<LinkState>,
    next_token: u32,
    pending: std::collections::BTreeMap<u32, (usize, u32, Vec<u8>, Instant)>,
    count: u64,
    closed: bool,
    cleanup: Value,
}
#[derive(Clone)]
struct LinkState {
    link: u32,
    receiver: u32,
    created: bool,
    connected: bool,
    receiving: bool,
    sw: bool,
}
impl LinkState {
    fn capture(s: &Session, sw: bool) -> Self {
        Self {
            link: s.link,
            receiver: s.receiver,
            created: s.created,
            connected: s.connected,
            receiving: s.receiving,
            sw,
        }
    }
    fn select(&self, s: &mut Session) {
        s.link = self.link;
        s.receiver = self.receiver;
        s.created = self.created;
        s.connected = self.connected;
        s.receiving = self.receiving;
        s.raw_spa = self.sw;
    }
}
#[derive(Debug)]
pub struct RawOpenFailure {
    pub error: String,
    pub report: Value,
}
impl RawPair {
    pub fn open(peer: SocketAddr) -> Result<Self, RawOpenFailure> {
        let mut pair = Self {
            inner: new_session(Instant::now()),
            channels: Vec::new(),
            next_token: 0,
            pending: Default::default(),
            count: 0,
            closed: false,
            cleanup: Value::Null,
        };
        let result = (|| -> Result<(), String> {
            pair.inner.register_connections(peer)?;
            let mut tags = Tags::for_session(pair.inner.session);
            for sw in [false, true] {
                let result = pair.inner.configure_link(sw, true, &mut tags);
                pair.channels.push(LinkState::capture(&pair.inner, sw));
                result?;
            }
            pair.next_token = tags.primitive();
            pair.inner.deadline = Instant::now() + Duration::from_secs(900);
            Ok(())
        })();
        if let Err(error) = result {
            let cleanup = pair.close();
            let report = json!({"error":error,"cleanup":cleanup,"activity":pair.take_activity()});
            return Err(RawOpenFailure { error, report });
        }
        Ok(pair)
    }
    pub fn transmit(
        &mut self,
        channel: usize,
        id: u32,
        data: &[u8],
        wake: bool,
    ) -> Result<u32, String> {
        if self.closed
            || self.pending.len() >= 16
            || channel >= self.channels.len()
            || (wake && channel != 1)
        {
            return Err("Raw pair ownership/channel/TX boundary".into());
        }
        self.channels[channel].select(&mut self.inner);
        let token = self.next_token;
        let request = wire::raw_frame(self.inner.link, token, id, data, wake)?;
        self.next_token = token.checked_add(1).ok_or("Raw pair token exhausted")?;
        if wake {
            self.inner
                .rpc(wire::set_unum32(self.inner.link, 166, 3, 1), 0x45a)?;
            self.inner
                .rpc(wire::set_unum32(self.inner.link, 150, 4, 60), 0x45a)?;
        }
        if true && !wake {
            self.inner
                .rpc(wire::set_unum32(self.inner.link, 166, 3, 0), 0x45a)?;
        }
        self.pending.insert(
            token,
            (channel, id, data.to_vec(), Instant::now() + IO_LIMIT),
        );
        self.inner.requests += 1;
        self.inner.rpc(request, 0x464)?;
        Ok(token)
    }
    pub fn poll(&mut self) -> Result<Vec<(usize, RawEvent)>, String> {
        if self.closed || Instant::now() >= self.inner.deadline {
            return Err("Raw pair closed/deadline".into());
        }
        self.inner.heartbeat()?;
        let mut result = Vec::new();
        let deadline = Instant::now() + Duration::from_millis(2);
        for _ in 0..128 {
            let items = self.inner.notification(deadline, "original_firmware")?;
            reject_async_error(&items)?;
            if items.is_empty() {
                break;
            }
            for item in items {
                self.count += 1;
                if self.count > 1000000 {
                    return Err("Raw pair event boundary".into());
                }
                let Some(channel) = self.channels.iter().position(|c| item["link"] == c.link)
                else {
                    continue;
                };
                let c = &self.channels[channel];
                if let Some(event) = raw_event_observed_mdi(&item, c.link, c.receiver, true)? {
                    if let RawEvent::Transmitted {
                        token,
                        id,
                        ref data,
                    } = event
                    {
                        let Some((wanted_channel, wanted_id, wanted_data, _)) =
                            self.pending.get(&token)
                        else {
                            return Err("Unmatched raw pair TX indication".into());
                        };
                        if channel != *wanted_channel || id != *wanted_id || data != wanted_data {
                            return Err("Mismatched raw pair TX indication".into());
                        }
                        self.pending.remove(&token);
                    }
                    result.push((channel, event));
                }
            }
            if Instant::now() >= deadline {
                break;
            }
        }
        if self
            .pending
            .values()
            .any(|(_, _, _, end)| Instant::now() >= *end)
        {
            return Err("Raw pair TX indication deadline; outcome uncertain, no replay".into());
        }
        Ok(result)
    }
    pub fn take_activity(&mut self) -> Value {
        json!({"operations":std::mem::take(&mut self.inner.operations),"events":std::mem::take(&mut self.inner.events)})
    }
    pub fn close(&mut self) -> Value {
        if self.closed {
            return self.cleanup.clone();
        }
        self.closed = true;
        let mut links = Vec::new();
        for channel in self.channels.iter().rev() {
            channel.select(&mut self.inner);
            self.inner.cleanup = json!({});
            self.inner.close_link();
            let clean = [
                (channel.receiving, "receiver_cancel_rpc"),
                (channel.receiving, "receiver_cancel_terminal"),
                (channel.connected, "link_disconnect"),
                (channel.created, "link_destroy"),
            ]
            .iter()
            .all(|(acquired, key)| !acquired || self.inner.cleanup[*key] == true);
            links.push(json!({"bus":if channel.sw{"SW"}else{"HS"},"cleanup":self.inner.cleanup,"cleanup_ok":clean}));
        }
        self.inner.cleanup = json!({});
        self.inner.close();
        let owner_ok = (self.inner.session == 0 || self.inner.cleanup["session_release"] == true)
            && self.inner.cleanup["sockets_shutdown"] == true;
        self.cleanup = json!({"cleanup_ok":owner_ok&&links.iter().all(|c|c["cleanup_ok"]==true),"links":links,"owner":self.inner.cleanup,"pending_transmissions":self.pending.len(),"vehicle_transmissions":self.inner.requests});
        self.cleanup.clone()
    }
}
impl Drop for RawPair {
    fn drop(&mut self) {
        self.close();
    }
}

pub struct RawSession {
    inner: Session,
    sw: bool,
    closed: bool,
    pending: std::collections::BTreeMap<u32, (u32, Vec<u8>, Instant)>,
    count: u64,
}
#[derive(Debug, PartialEq, Eq)]
pub enum RawEvent {
    Received {
        id: u32,
        data: Vec<u8>,
        device_timestamp: u32,
    },
    /// Private classic-MDI raw compatibility observation. Original flags retained.
    ObservedMdiData {
        id: u32,
        data: Vec<u8>,
        device_timestamp: u32,
        original_rx_flags: String,
    },
    /// Driver loopback/indication; never independent electrical ACK evidence.
    Transmitted {
        token: u32,
        id: u32,
        data: Vec<u8>,
    },
    Metadata(Value),
}
impl RawSession {
    pub fn open(peer: SocketAddr, sw: bool) -> Result<Self, String> {
        let mut inner = new_session(Instant::now());
        if let Err(e) = inner.initialize_profile(peer, sw, true) {
            inner.close();
            return Err(format!(
                "Raw initialization: {e}; cleanup={}",
                cleanup_ok(&inner)
            ));
        }
        // Long interactive procedures remain finite; caller cancellation closes
        // the receiver, link and fresh session. This is not automatic reconnect.
        inner.deadline = Instant::now() + Duration::from_secs(900);
        Ok(Self {
            inner,
            sw,
            closed: false,
            pending: Default::default(),
            count: 0,
        })
    }
    pub fn transmit(&mut self, id: u32, data: &[u8], wake: bool) -> Result<u32, String> {
        if self.closed || self.pending.len() >= 16 || (wake && !self.sw) {
            return Err("Raw transport ownership/TX/electrical boundary".into());
        }
        let token = self.inner.next_request_token;
        let request = wire::raw_frame(self.inner.link, token, id, data, wake)?;
        self.inner.next_request_token = token.checked_add(1).ok_or("Raw token exhausted")?;
        if wake {
            // Native J2534 recording stages HV parameters for one primitive.
            // TemporaryParams restores the previous normal data configuration.
            self.inner
                .rpc(wire::set_unum32(self.inner.link, 166, 3, 1), 0x45a)?;
            self.inner
                .rpc(wire::set_unum32(self.inner.link, 150, 4, 60), 0x45a)?;
        }
        if self.sw && !wake {
            self.inner
                .rpc(wire::set_unum32(self.inner.link, 166, 3, 0), 0x45a)?;
        }
        self.pending
            .insert(token, (id, data.to_vec(), Instant::now() + IO_LIMIT));
        self.inner.requests += 1;
        self.inner.rpc(request, 0x464)?;
        Ok(token)
    }
    pub fn poll(&mut self) -> Result<Vec<RawEvent>, String> {
        if self.closed || Instant::now() >= self.inner.deadline {
            return Err("Raw session closed/deadline".into());
        }
        self.inner.heartbeat()?;
        let mut result = Vec::new();
        let deadline = Instant::now() + Duration::from_millis(2);
        for _ in 0..128 {
            let items = self.inner.notification(deadline, "original_firmware")?;
            reject_async_error(&items)?;
            if items.is_empty() {
                break;
            }
            for item in items {
                self.count += 1;
                if self.count > 1000000 {
                    return Err("Raw event count boundary".into());
                }
                if let Some(event) = raw_event(&item, self.inner.link, self.inner.receiver)? {
                    if let RawEvent::Transmitted {
                        token,
                        id,
                        ref data,
                    } = event
                    {
                        let Some((wanted_id, wanted_data, _)) = self.pending.get(&token) else {
                            return Err("Unmatched raw TX indication".into());
                        };
                        if id != *wanted_id || *data != *wanted_data {
                            return Err("Mismatched raw TX indication".into());
                        }
                        self.pending.remove(&token);
                    }
                    result.push(event);
                }
            }
            if Instant::now() >= deadline {
                break;
            }
        }
        if self
            .pending
            .values()
            .any(|(_, _, end)| Instant::now() >= *end)
        {
            return Err("Raw TX indication deadline; outcome uncertain, no replay".into());
        }
        Ok(result)
    }
    /// Take bounded structured observations; raw byte journals belong to carrier.
    pub fn take_activity(&mut self) -> Value {
        json!({"operations":std::mem::take(&mut self.inner.operations),
            "events":std::mem::take(&mut self.inner.events)})
    }
    pub fn close(&mut self) -> Value {
        if !self.closed {
            self.closed = true;
            self.inner.close();
        }
        json!({"cleanup":self.inner.cleanup,"cleanup_ok":cleanup_ok(&self.inner),
            "pending_transmissions":self.pending.len(),"vehicle_transmissions":self.inner.requests})
    }
}
impl Drop for RawSession {
    fn drop(&mut self) {
        self.close();
    }
}
fn raw_event(item: &Value, link: u32, receiver: u32) -> Result<Option<RawEvent>, String> {
    raw_event_observed_mdi(item, link, receiver, false)
}

// Private research only: native Windows raw HS and SW captures flag complete data
// (including required CIM VIN consecutive frames) with 80000000. This is
// explicitly scoped to the selected raw receiver, not a generic RTR conversion.
fn raw_event_observed_mdi(
    item: &Value,
    link: u32,
    receiver: u32,
    observed_mdi: bool,
) -> Result<Option<RawEvent>, String> {
    if item["link"] != link || item["data"]["kind"] != "result" {
        return Ok(None);
    }
    let flags = item["data"]["rx_flags_hex"]
        .as_str()
        .ok_or("Raw RX flags absent")?;
    let observed_data = observed_mdi
        && flags == "80000000"
        && item["data"]["data_hex"]
            .as_str()
            .is_some_and(|v| v.len() == 24);
    if !observed_data && !matches!(flags, "00000000" | "00000001" | "00010001") {
        // Remote-frame indications carrying data remain ambiguous. Retain their
        // original form but never turn them into guest CAN data or completions.
        return Ok(Some(RawEvent::Metadata(item.clone())));
    }
    let text = item["data"]["data_hex"].as_str().ok_or("Raw data absent")?;
    if !(8..=24).contains(&text.len()) || text.len() % 2 != 0 {
        return Err("Raw result frame length boundary".into());
    }
    let bytes = (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).map_err(|_| "Raw result hex".into()))
        .collect::<Result<Vec<_>, String>>()?;
    let id = u32::from_be_bytes(bytes[..4].try_into().unwrap());
    if id > 0x7ff {
        return Err("Unselected extended/invalid raw CAN ID".into());
    }
    let data = bytes[4..].to_vec();
    if flags == "00000000" || observed_data {
        if item["token"] != receiver {
            return Err("Raw result receiver mismatch".into());
        }
        let device_timestamp = item["device_timestamp"]
            .as_u64()
            .ok_or("Raw timestamp absent")?
            .try_into()
            .map_err(|_| "Raw timestamp boundary")?;
        Ok(Some(if observed_data {
            RawEvent::ObservedMdiData {
                id,
                data,
                device_timestamp,
                original_rx_flags: flags.to_string(),
            }
        } else {
            RawEvent::Received {
                id,
                data,
                device_timestamp,
            }
        }))
    } else {
        if flags == "00010001" && (id != 0x100 || !data.is_empty()) {
            return Err("Unselected HV raw indication".into());
        }
        Ok(Some(RawEvent::Transmitted {
            token: item["token"]
                .as_u64()
                .ok_or("Raw TX token absent")?
                .try_into()
                .map_err(|_| "Raw TX token boundary")?,
            id,
            data,
        }))
    }
}

#[test]
fn raw_events_preserve_actual_frames_and_completion_provenance() {
    let mut item = json!({"link":3,"token":5,"device_timestamp":123,
        "data":{"kind":"result","rx_flags_hex":"00000000","data_hex":"0000065b045a9a0303000000"}});
    assert_eq!(
        raw_event(&item, 3, 5).unwrap(),
        Some(RawEvent::Received {
            id: 0x65b,
            data: vec![4, 0x5a, 0x9a, 3, 3, 0, 0, 0],
            device_timestamp: 123
        })
    );
    assert!(raw_event(&item, 3, 6).is_err());
    item["data"]["rx_flags_hex"] = json!("00000040");
    assert!(matches!(
        raw_event(&item, 3, 5).unwrap(),
        Some(RawEvent::Metadata(_))
    ));
    item["data"]["rx_flags_hex"] = json!("80000000");
    assert!(matches!(
        raw_event(&item, 3, 5).unwrap(),
        Some(RawEvent::Metadata(_))
    ));
    item["token"] = json!(6);
    item["data"]["rx_flags_hex"] = json!("00000001");
    assert_eq!(
        raw_event(&item, 3, 5).unwrap(),
        Some(RawEvent::Transmitted {
            token: 6,
            id: 0x65b,
            data: vec![4, 0x5a, 0x9a, 3, 3, 0, 0, 0]
        })
    );
    item["data"]["rx_flags_hex"] = json!("00010001");
    assert!(raw_event(&item, 3, 5).is_err());
    item["data"]["data_hex"] = json!("00000100");
    assert_eq!(
        raw_event(&item, 3, 5).unwrap(),
        Some(RawEvent::Transmitted {
            token: 6,
            id: 0x100,
            data: vec![]
        })
    );
}

/// Three explicit read-only sample rounds. Advertised support is required per PID.
/// No automatic retry, fallback service, actuation, clearing or cached result reuse.
pub fn read_ecm_live(peer: SocketAddr) -> Attempt {
    let start = Instant::now();
    let mut s = new_session(start);
    let mut samples = Vec::<Value>::new();
    let mut mask = None;
    let mut selected = Vec::new();
    let result = (|| -> Result<(), String> {
        s.initialize(peer)?;
        let bytes = s.current_data(0)?;
        let supported = u32::from_be_bytes(bytes.try_into().map_err(|_| "Support bitmap width")?);
        mask = Some(supported);
        selected = crate::ecm::PIDS
            .into_iter()
            .filter(|pid| crate::ecm::supported(supported, *pid))
            .collect();
        if selected.is_empty() {
            return Err("ECM advertises none of the selected current-data PIDs".into());
        }
        for round in 0..3 {
            let cycle = Instant::now();
            for pid in &selected {
                let requested_ms = start.elapsed().as_millis();
                let raw = s.current_data(*pid)?;
                let mut value = crate::ecm::decode(*pid, &raw)?;
                value["round"] = json!(round);
                value["requested_ms"] = json!(requested_ms);
                value["received_ms"] = json!(start.elapsed().as_millis());
                samples.push(value);
            }
            if round < 2 {
                if let Some(wait) = Duration::from_millis(500).checked_sub(cycle.elapsed()) {
                    std::thread::sleep(wait)
                }
            }
        }
        Ok(())
    })();
    s.close();
    let clean = cleanup_ok(&s);
    Attempt {
        passed: result.is_ok() && clean,
        report: json!({"schema":1,"profile":"standard_Mode01_ECM_current_data","peer":peer.to_string(),"status":if result.is_ok(){"fresh_ECM_samples_received"}else if samples.is_empty(){"failed"}else{"partial_ECM_samples"},"error":result.as_ref().err(),"elapsed_ms":start.elapsed().as_millis(),"requested_rounds":3,"complete":result.is_ok(),"supported_01_20_bitmap":mask,"selected_PIDs":selected,"samples":samples,"vehicle_requests":s.requests,"session_token":s.session,"link_tag":s.link,"receiver_tag":s.receiver,"vendor_dll_loaded":false,"operations":s.operations,"events":s.events,"cleanup":s.cleanup,"cleanup_ok":clean,"automatic_retry":false,"original_Tech2_Read_Values_qualified":false}),
    }
}

pub fn read_vin(peer: SocketAddr) -> Attempt {
    let start = Instant::now();
    let mut s = new_session(start);
    let result = s.run(peer);
    s.close();
    let clean = cleanup_ok(&s);
    Attempt {
        passed: result.is_ok() && clean,
        report: json!({"schema":1,"peer":peer.to_string(),"status":if result.is_ok(){"fresh_vin_received"}else{"failed"},
        "fresh_vin":result.as_ref().ok(),"error":result.as_ref().err(),"elapsed_ms":start.elapsed().as_millis(),"vehicle_requests":s.requests,
        "session_token":s.session,"link_tag":s.link,"receiver_tag":s.receiver,"vendor_dll_loaded":false,"physical_cold_qualified":false,
        "operations":s.operations,"events":s.events,"cleanup":s.cleanup,"cleanup_ok":clean}),
    }
}

/// One captured SPA precondition query. No seed/key submission or ECU writes.
pub fn read_spa_preconditions(peer: SocketAddr) -> Attempt {
    let start = Instant::now();
    let mut s = new_session(start);
    let result = (|| -> Result<Vec<u8>, String> {
        s.initialize_profile(peer, true, false)?;
        s.send_vehicle(&[0x1a, 0x9a])?;
        let deadline = (Instant::now() + IO_LIMIT).min(s.deadline);
        while Instant::now() < deadline {
            let items = s.notification(deadline, "after_SPA_preconditions_request")?;
            reject_async_error(&items)?;
            for item in items {
                if let Some(data) = match_spa_preconditions(&item, s.link, s.receiver)? {
                    return Ok(data);
                }
            }
        }
        Err("No matching SPA precondition reply before deadline".into())
    })();
    s.close();
    let clean = cleanup_ok(&s);
    Attempt {
        passed: result.is_ok() && clean,
        report: json!({"schema":1,"profile":"SPA_SW_CAN_preconditions_read","status":if result.is_ok(){"fresh_SPA_preconditions_received"}else{"failed"},"error":result.as_ref().err(),"precondition_bytes_hex":result.as_ref().ok().map(|b|trace::hex(b)),"elapsed_ms":start.elapsed().as_millis(),"vehicle_requests":s.requests,"operations":s.operations,"events":s.events,"cleanup":s.cleanup,"cleanup_ok":clean,"request_id":"25B","reply_id":"65B","baud":33333,"security_collected":false,"security_key_submitted":false,"ECU_Add_executed":false,"automatic_retry":false}),
    }
}
/// One complete raw CAN response to the same captured read. No guest reconstruction.
pub fn read_spa_raw(peer: SocketAddr) -> Attempt {
    let start = Instant::now();
    let mut s = new_session(start);
    let result = (|| -> Result<Vec<u8>, String> {
        s.initialize_profile(peer, true, true)?;
        s.send_vehicle(&[0x1a, 0x9a])?;
        let deadline = (Instant::now() + IO_LIMIT).min(s.deadline);
        while Instant::now() < deadline {
            let items = s.notification(deadline, "after_raw_SPA_request")?;
            reject_async_error(&items)?;
            for item in items {
                if let Some(frame) = match_spa_raw(&item, s.link, s.receiver)? {
                    return Ok(frame);
                }
            }
        }
        Err("No matching complete raw SPA frame before deadline".into())
    })();
    s.close();
    let clean = cleanup_ok(&s);
    Attempt {
        passed: result.is_ok() && clean,
        report: json!({"schema":1,"profile":"SPA_SW_CAN_raw_preconditions_read","status":if result.is_ok(){"fresh_raw_SPA_frame_received"}else{"failed"},"error":result.as_ref().err(),"CAN_payload_hex":result.as_ref().ok().map(|b|trace::hex(b)),"elapsed_ms":start.elapsed().as_millis(),"vehicle_requests":s.requests,"operations":s.operations,"events":s.events,"cleanup":s.cleanup,"cleanup_ok":clean,"request_id":"25B","reply_id":"65B","baud":33333,"security_collected":false,"security_key_submitted":false,"ECU_Add_executed":false,"automatic_retry":false}),
    }
}
fn match_spa_raw(item: &Value, link: u32, receiver: u32) -> Result<Option<Vec<u8>>, String> {
    if item["link"] != link
        || item["token"] != receiver
        || item["data"]["kind"] != "result"
        || item["data"]["rx_flags_hex"] != "00000000"
    {
        return Ok(None);
    }
    let text = item["data"]["data_hex"]
        .as_str()
        .ok_or("Raw SPA payload absent")?;
    if text.len() != 24 {
        return Ok(None);
    }
    let bytes = (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).map_err(|_| "Raw SPA hex".into()))
        .collect::<Result<Vec<_>, String>>()?;
    if bytes[..4] != [0, 0, 6, 0x5b] {
        return Ok(None);
    }
    let frame = &bytes[4..];
    if frame[..3] == [3, 0x7f, 0x1a] {
        if frame[3] == 0x78 {
            return Ok(None);
        }
        return Err(format!("Raw SPA negative response NRC{:02x}", frame[3]));
    }
    if frame[..3] == [4, 0x5a, 0x9a] {
        Ok(Some(frame.to_vec()))
    } else {
        Ok(None)
    }
}
fn match_spa_preconditions(
    item: &Value,
    link: u32,
    receiver: u32,
) -> Result<Option<Vec<u8>>, String> {
    if item["link"] != link
        || item["token"] != receiver
        || item["data"]["kind"] != "result"
        || item["data"]["rx_flags_hex"] != "00000040"
    {
        return Ok(None);
    }
    let text = item["data"]["data_hex"]
        .as_str()
        .ok_or("SPA payload absent")?;
    if text.len() % 2 != 0 {
        return Err("SPA payload hex width".into());
    }
    let bytes = (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).map_err(|_| "SPA payload hex".into()))
        .collect::<Result<Vec<_>, String>>()?;
    if bytes.get(..4) != Some(&[0, 0, 6, 0x5b]) {
        return Ok(None);
    }
    let data = &bytes[4..];
    if data.starts_with(&[0x7f, 0x1a]) {
        if data == [0x7f, 0x1a, 0x78] {
            return Ok(None);
        }
        return Err(format!(
            "SPA negative precondition response: {}",
            trace::hex(data)
        ));
    }
    if data.starts_with(&[0x5a, 0x9a]) && data.len() > 2 && data.len() <= 128 {
        Ok(Some(data[2..].to_vec()))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn event(payload: &[u8]) -> Value {
        json!({"link":3,"token":5,"data":{"kind":"result","rx_flags_hex":"00000040","data_hex":trace::hex(payload)}})
    }
    #[test]
    fn spa_precondition_reply_requires_its_source_receiver_and_assembled_flags() {
        let bytes = [0, 0, 6, 0x5b, 0x5a, 0x9a, 1];
        let e = event(&bytes);
        assert_eq!(match_spa_preconditions(&e, 3, 5).unwrap(), Some(vec![1]));
        assert!(match_spa_preconditions(&e, 3, 6).unwrap().is_none());
        let mut wrong = bytes;
        wrong[3] = 0x5a;
        assert!(match_spa_preconditions(&event(&wrong), 3, 5)
            .unwrap()
            .is_none());
        let mut flags = e;
        flags["data"]["rx_flags_hex"] = json!("80000000");
        assert!(match_spa_preconditions(&flags, 3, 5).unwrap().is_none());
        assert!(match_spa_preconditions(&event(&bytes[..6]), 3, 5)
            .unwrap()
            .is_none());
        assert!(match_spa_preconditions(&event(&[0, 0, 6, 0x5b, 0x7f, 0x1a, 0x33]), 3, 5).is_err());
    }
    #[test]
    fn only_selected_complete_positive_vin_is_accepted() {
        let mut p = vec![0, 0, 7, 0xe8, 0x5a, 0x90];
        p.extend(b"YS3FD49Y041000000");
        let e = event(&p);
        assert_eq!(match_vin(&e, 3, 5).unwrap().unwrap(), "YS3FD49Y041000000");
        assert!(match_vin(&e, 3, 6).unwrap().is_none());
        let mut tx = e.clone();
        tx["data"]["rx_flags_hex"] = json!("00000008");
        assert!(match_vin(&tx, 3, 5).unwrap().is_none());
        p.pop();
        assert!(match_vin(&event(&p), 3, 5).unwrap().is_none());
        p.push(b'I');
        assert!(match_vin(&event(&p), 3, 5).is_err());
    }
    #[test]
    fn pending_negative_and_async_error_are_distinct() {
        assert!(match_vin(&event(&[0, 0, 7, 0xe8, 0x7f, 0x1a, 0x78]), 3, 5)
            .unwrap()
            .is_none());
        assert!(match_vin(&event(&[0, 0, 7, 0xe8, 0x7f, 0x1a, 0x31]), 3, 5).is_err());
        assert!(reject_async_error(&[json!({"data":{"kind":"error","code":0x101}})]).is_err());
    }
    #[test]
    fn current_data_requires_selected_source_pid_and_exact_length() {
        let data = [0, 0, 7, 0xe8, 0x41, 0x0c, 0x1f, 0x40];
        let e = event(&data);
        assert_eq!(
            match_current_data(&e, 3, 5, 12).unwrap(),
            Some(vec![0x1f, 0x40])
        );
        assert!(match_current_data(&e, 3, 5, 5).unwrap().is_none());
        assert!(match_current_data(&e, 3, 6, 12).unwrap().is_none());
        let mut bad = e.clone();
        bad["data"]["rx_flags_hex"] = json!("00000008");
        assert!(match_current_data(&bad, 3, 5, 12).unwrap().is_none());
        assert!(match_current_data(&event(&data[..7]), 3, 5, 12).is_err());
        let mut wrong = data;
        wrong[3] = 0xe9;
        assert!(match_current_data(&event(&wrong), 3, 5, 12)
            .unwrap()
            .is_none());
        assert!(
            match_current_data(&event(&[0, 0, 7, 0xe8, 0x7f, 1, 0x78]), 3, 5, 12)
                .unwrap()
                .is_none()
        );
        assert!(match_current_data(&event(&[0, 0, 7, 0xe8, 0x7f, 1, 0x12]), 3, 5, 12).is_err());
    }
    #[test]
    fn full_session_uses_fresh_registration_and_once_only_cleanup() {
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let peer = listener.local_addr().unwrap();
        let worker = std::thread::spawn(move || {
            fn words(v: &[u32]) -> Vec<u8> {
                v.iter().flat_map(|v| v.to_le_bytes()).collect()
            }
            let (mut command, _) = listener.accept().unwrap();
            command
                .set_read_timeout(Some(Duration::from_secs(4)))
                .unwrap();
            assert_eq!(Record::read(&mut command).unwrap(), wire::register(0));
            Record::new(0x885, words(&[0, 1, 0, 73]))
                .write(&mut command)
                .unwrap();
            let (mut notification, _) = listener.accept().unwrap();
            assert_eq!(Record::read(&mut notification).unwrap(), wire::register(73));
            Record::new(0x885, words(&[0, 1, 0, 73]))
                .write(&mut notification)
                .unwrap();
            let mut receiver = 0;
            let mut link = 0;
            let mut requests = 0;
            let mut ops = Vec::new();
            loop {
                let request = Record::read(&mut command).unwrap();
                ops.push(request.opcode);
                let reply = match request.opcode {
                    0x44f => {
                        link = u32::from_le_bytes(request.body[..4].try_into().unwrap());
                        0x450
                    }
                    0x459 => 0x45a,
                    0x45f => 0x460,
                    0x45d => 0x45e,
                    0x463 => {
                        let decoded = trace::decode_stream(&request.encoded().unwrap()).unwrap();
                        let d = &decoded[0]["decoded"];
                        if d["control"]["receive_cycles"] == -1 {
                            receiver = d["token"].as_u64().unwrap() as u32;
                        }
                        if d["data_hex"] == "000007e01a90" {
                            requests += 1;
                            let mut body = words(&[link, 1, 0x1300, 0x1300, 0x1300, receiver, 100]);
                            body.push(1);
                            body.extend(words(&[4]));
                            body.extend([0, 0, 0, 0x40]);
                            body.extend(words(&[0, 0x2534, 0, 0, 0]));
                            body.push(0);
                            body.extend(words(&[23]));
                            body.extend([0, 0, 7, 0xe8, 0x5a, 0x90]);
                            body.extend(b"YS3FD49Y041000000");
                            Record::new(0x863, body).write(&mut notification).unwrap();
                        }
                        0x464
                    }
                    0x465 => {
                        let mut body = words(&[link, 1, 0x1301, 0x1301, 0x1301, receiver, 100]);
                        body.push(1);
                        body.extend(words(&[0x8013, 0]));
                        Record::new(0x863, body).write(&mut notification).unwrap();
                        0x466
                    }
                    0x461 => 0x462,
                    0x451 => 0x452,
                    0x886 => 0x887,
                    _ => panic!("Unexpected opcode"),
                };
                Record::new(
                    reply,
                    if reply == 0x887 {
                        words(&[0, 1])
                    } else {
                        words(&[0])
                    },
                )
                .write(&mut command)
                .unwrap();
                if reply == 0x887 {
                    break;
                }
            }
            assert_eq!(requests, 1);
            assert_eq!(&ops[ops.len() - 4..], &[0x465, 0x461, 0x451, 0x886]);
        });
        let result = read_vin(peer);
        worker.join().unwrap();
        assert!(result.passed, "{}", result.report);
        assert_eq!(result.report["fresh_vin"], "YS3FD49Y041000000");
    }
    #[test]
    fn partial_tcp_header_survives_a_timeout() {
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let peer = listener.local_addr().unwrap();
        let writer = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let bytes = wire::register(73).encoded().unwrap();
            socket.write_all(&bytes[..3]).unwrap();
            std::thread::sleep(Duration::from_millis(80));
            socket.write_all(&bytes[3..]).unwrap();
        });
        let mut c = Connection::open(peer).unwrap();
        assert!(c
            .receive(Instant::now() + Duration::from_millis(40))
            .unwrap()
            .is_none());
        assert_eq!(
            c.receive(Instant::now() + IO_LIMIT).unwrap().unwrap(),
            wire::register(73)
        );
        writer.join().unwrap();
    }
}
#[test]
fn spa_raw_requires_actual_complete_raw_frame() {
    let mut e = json!({"link":3,"token":5,"data":{"kind":"result","rx_flags_hex":"00000040","data_hex":"0000065b045a9a0303000000"}});
    assert!(match_spa_raw(&e, 3, 5).unwrap().is_none());
    e["data"]["rx_flags_hex"] = json!("00000000");
    assert_eq!(
        match_spa_raw(&e, 3, 5).unwrap(),
        Some(vec![4, 0x5a, 0x9a, 3, 3, 0, 0, 0])
    );
    assert!(match_spa_raw(&e, 3, 6).unwrap().is_none());
    e["data"]["data_hex"] = json!("0000065b5a9a0303");
    assert!(match_spa_raw(&e, 3, 5).unwrap().is_none());
}

#[test]
fn raw_pair_owns_two_links_one_registration_and_matches_original_frames() {
    use std::net::TcpListener;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let peer = listener.local_addr().unwrap();
    let worker = std::thread::spawn(move || {
        let words = |v: &[u32]| -> Vec<u8> { v.iter().flat_map(|n| n.to_le_bytes()).collect() };
        let (mut command, _) = listener.accept().unwrap();
        command
            .set_read_timeout(Some(Duration::from_secs(4)))
            .unwrap();
        assert_eq!(Record::read(&mut command).unwrap(), wire::register(0));
        Record::new(0x885, words(&[0, 1, 0, 73]))
            .write(&mut command)
            .unwrap();
        let (mut notification, _) = listener.accept().unwrap();
        assert_eq!(Record::read(&mut notification).unwrap(), wire::register(73));
        Record::new(0x885, words(&[0, 1, 0, 73]))
            .write(&mut notification)
            .unwrap();
        let mut links = Vec::new();
        let mut receivers = std::collections::BTreeMap::new();
        let mut sends = Vec::new();
        let mut cancels = Vec::new();
        let mut destroyed = Vec::new();
        let mut heartbeats = 0;
        loop {
            let request = Record::read(&mut command).unwrap();
            let decoded = trace::decode_stream(&request.encoded().unwrap()).unwrap();
            let d = &decoded[0]["decoded"];
            let reply = match request.opcode {
                0x44f => {
                    links.push(d["link"].as_u64().unwrap() as u32);
                    assert_eq!(d["resource"]["protocol"], 6);
                    0x450
                }
                0x888 => {
                    assert!(request.body.is_empty());
                    heartbeats += 1;
                    0x889
                }
                0x459 => 0x45a,
                0x45f => 0x460,
                0x45d => 0x45e,
                0x463 => {
                    let link = d["link"].as_u64().unwrap() as u32;
                    let token = d["token"].as_u64().unwrap() as u32;
                    if d["control"]["receive_cycles"] == -1 {
                        receivers.insert(link, token);
                    }
                    let text = d["data_hex"].as_str().unwrap_or("");
                    if !text.is_empty() {
                        sends.push(text.to_string());
                        let data = (0..text.len())
                            .step_by(2)
                            .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
                            .collect::<Vec<_>>();
                        let mut body = words(&[link, 1, 0x1300, 0x1300, 0x1300, token, 100]);
                        body.push(1);
                        body.extend(words(&[4]));
                        body.extend([0, 0, 0, 1]);
                        body.extend(words(&[0, 0x2534, 0, 0, 0]));
                        body.push(0);
                        body.extend(words(&[data.len() as u32]));
                        body.extend(data);
                        Record::new(0x863, body).write(&mut notification).unwrap();
                    }
                    0x464
                }
                0x465 => {
                    let link = u32::from_le_bytes(request.body[..4].try_into().unwrap());
                    let receiver = u32::from_le_bytes(request.body[4..8].try_into().unwrap());
                    assert_eq!(receivers[&link], receiver);
                    cancels.push(link);
                    let mut body = words(&[link, 1, 0x1301, 0x1301, 0x1301, receiver, 100]);
                    body.push(1);
                    body.extend(words(&[0x8013, 0]));
                    Record::new(0x863, body).write(&mut notification).unwrap();
                    0x466
                }
                0x461 => 0x462,
                0x451 => {
                    destroyed.push(d["link"].as_u64().unwrap() as u32);
                    0x452
                }
                0x886 => 0x887,
                _ => panic!("Unexpected opcode {}", request.opcode),
            };
            Record::new(
                reply,
                if reply == 0x887 {
                    words(&[0, 1])
                } else {
                    words(&[0])
                },
            )
            .write(&mut command)
            .unwrap();
            if reply == 0x887 {
                break;
            }
        }
        assert_eq!(heartbeats, 1);
        assert_eq!(links.len(), 2);
        assert_ne!(links[0], links[1]);
        assert_eq!(cancels.len(), 2);
        assert_eq!(destroyed.len(), 2);
        assert_eq!(
            sends,
            vec!["000007e0021a900000000000", "0000025b021a9a0000000000"]
        );
    });
    let mut pair = RawPair::open(peer).unwrap();
    pair.inner.last_heartbeat = Instant::now() - Duration::from_secs(2);
    let a = pair
        .transmit(0, 0x7e0, &[2, 0x1a, 0x90, 0, 0, 0, 0, 0], false)
        .unwrap();
    let b = pair
        .transmit(1, 0x25b, &[2, 0x1a, 0x9a, 0, 0, 0, 0, 0], false)
        .unwrap();
    assert_ne!(a, b);
    let end = Instant::now() + IO_LIMIT;
    let mut completed = Vec::new();
    while completed.len() < 2 && Instant::now() < end {
        for (channel, event) in pair.poll().unwrap() {
            if let RawEvent::Transmitted { token, .. } = event {
                completed.push((channel, token));
            }
        }
    }
    assert_eq!(completed, vec![(0, a), (1, b)]);
    let report = pair.close();
    assert_eq!(report["cleanup_ok"], true);
    assert_eq!(report["pending_transmissions"], 0);
    assert_eq!(pair.close(), report);
    worker.join().unwrap();
}

#[test]
fn private_mdi_data_flag_qualification_remains_scoped() {
    let mut item = json!({"link":3,"token":5,"device_timestamp":123,
        "data":{"kind":"result","rx_flags_hex":"80000000","data_hex":"000006412130313233343536"}});
    assert!(matches!(raw_event_observed_mdi(&item,3,5,true).unwrap(),
        Some(RawEvent::ObservedMdiData {id:0x641,data,device_timestamp:123,original_rx_flags})
        if data == [0x21,0x30,0x31,0x32,0x33,0x34,0x35,0x36] && original_rx_flags == "80000000"));
    assert!(matches!(
        raw_event_observed_mdi(&item, 3, 5, false).unwrap(),
        Some(RawEvent::Metadata(_))
    ));
    assert!(raw_event_observed_mdi(&item, 3, 6, true).is_err());
    item["data"]["data_hex"] = json!("0000064108");
    assert!(matches!(
        raw_event_observed_mdi(&item, 3, 5, true).unwrap(),
        Some(RawEvent::Metadata(_))
    ));
    item["data"]["data_hex"] = json!("000008002130313233343536");
    assert!(raw_event_observed_mdi(&item, 3, 5, true).is_err());
}

#[cfg(test)]
mod expired_poll_tests {
    use super::*;
    #[test]
    fn expired_poll_preserves_partial_record_and_never_sets_zero_timeout() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let mut client = Connection::open(listener.local_addr().unwrap()).unwrap();
        let (mut server, _) = listener.accept().unwrap();
        let packet = Record::new(0x889, vec![0, 0, 0, 0]).encoded().unwrap();
        client.buffer.extend_from_slice(&packet[..6]);
        assert!(client.receive(Instant::now()).unwrap().is_none());
        assert_eq!(client.buffer, &packet[..6]);
        server.write_all(&packet[6..]).unwrap();
        assert_eq!(
            client
                .receive(Instant::now() + Duration::from_secs(1))
                .unwrap()
                .unwrap()
                .opcode,
            0x889
        );
        assert!(client.buffer.is_empty());
    }
}
