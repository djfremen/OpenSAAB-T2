// SPDX-License-Identifier: MPL-2.0
//! Private owned STN2255 native RFCOMM backend, serialized HS/SW-CAN development preview.
//! Original CANdi owns all requests, padding, segmentation and flow control.
use crate::{
    adapters::{
        common::{
            policy::{CommandGate, KeyStatusGate, Profile},
            usb::SocketUsb,
        },
        stn_parser::{frame, Item, Parser},
    },
    can_adapter::{
        electrical_route, Backend, CompletionSource, ElectricalMode, Event, FailureContext,
    },
    candi_cpu::CanTransmission,
};
use std::{
    fs::{File, OpenOptions},
    io::{BufReader, Write},
    net::TcpStream,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender},
        Arc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

fn encode(controller: usize, tx: &CanTransmission) -> Result<String, String> {
    let route = electrical_route(controller, tx)?;
    if route.mode == ElectricalMode::SingleWireWake {
        // FRPM F explicitly documents STPX with an empty d: parameter.
        return Ok("STPXH:100,D:,R:0".into());
    }
    if tx.frame.dlc == 0 {
        return Err("Unverified zero-DLC frame outside native SW wake".into());
    }
    // FRPM F: raw ISO11898 segmentation sends bytes without inserting PCI.
    // The guarded frame is at most eight bytes, so it remains one raw frame.
    // r:0 overrides response waiting for this transmission only. The prompt
    // still must validate before completion; original CANdi owns subsequent RX/FC.
    let data: String = tx.frame.data.iter().map(|b| format!("{b:02X}")).collect();
    Ok(format!("STPXH:{:03X},D:{data},R:0", tx.frame.id))
}

fn functional_tester_present(tx: &CanTransmission) -> bool {
    !tx.frame.extended && !tx.frame.rtr && tx.frame.id == 0x101
        && tx.frame.dlc == 8 && tx.frame.data == [0xfe, 1, 0x3e, 0, 0, 0, 0, 0]
}

// Only the existing policy-approved functional HS requests reach this point.
// Tester-present has no reply collection window; physical module requests and
// ISO-TP flow control keep their existing latency/ordering.
fn functional_hs_reply_request(controller: usize, tx: &CanTransmission) -> bool {
    controller == 0 && !tx.frame.extended && !tx.frame.rtr && tx.frame.id == 0x101
        && tx.frame.dlc == 8 && tx.frame.data.first() == Some(&0xfe)
        && tx.frame.data.get(1).is_some_and(|n| (1..=7).contains(n))
        && !functional_tester_present(tx)
}

#[derive(Default)]
struct ReceivePreference {
    // A functional tester-present must not replace the receive bus chosen by
    // the last other guest frame. In particular, preserve addressed CIM work.
    last_non_tester_controller: Option<usize>,
    return_to_hs: bool,
}
impl ReceivePreference {
    fn transmitted(&mut self, controller: usize, tx: &CanTransmission) {
        self.return_to_hs = false;
        if functional_tester_present(tx) {
            self.return_to_hs = controller == 2 && self.last_non_tester_controller == Some(0);
        } else {
            self.last_non_tester_controller = Some(controller);
        }
    }
}
struct Link {
    socket: SocketUsb,
    parser: Parser,
    trace: File,
    started: Instant,
    receiving: bool,
    synchronized: bool,
    configured: bool,
    controller: usize,
    protocol_open: bool,
    monitoring: bool,
    active_tx: Option<FailureContext>,
    batch_pending: bool,
    receive_preference: ReceivePreference,
    hs_reply_until: Option<Instant>,
}
impl Link {
    fn open(token: &str, directory: &Path, profile: Profile) -> Result<Self, String> {
        let socket =
            TcpStream::connect_timeout(&"127.0.0.1:35676".parse().unwrap(), Duration::from_secs(2))
                .map_err(|e| e.to_string())?;
        Self::from_socket_profile(token, directory, socket, profile)
    }
    fn from_socket(token: &str, directory: &Path, socket: TcpStream) -> Result<Self, String> {
        Self::from_socket_profile(token, directory, socket, Profile::Read)
    }
    fn from_socket_profile(token: &str, directory: &Path, socket: TcpStream, profile: Profile) -> Result<Self, String> {
        socket.set_nodelay(true).map_err(|e| e.to_string())?;
        socket
            .set_read_timeout(Some(Duration::from_secs(3)))
            .map_err(|e| e.to_string())?;
        socket
            .set_write_timeout(Some(Duration::from_secs(2)))
            .map_err(|e| e.to_string())?;
        let mut socket = SocketUsb(BufReader::new(socket));
        let expected = match profile {Profile::Read=>"READY obdlink-native-read",Profile::ClearDtc=>"READY obdlink-native-clear-dtc",Profile::ResetEngineCounters=>"READY obdlink-native-reset-counters",Profile::Seeds=>return Err("MX+ seed profile unavailable".into())};
        if socket.command(&format!("HELLO {token}"))? != expected {
            return Err("Native MX+ ownership unavailable".into());
        }
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let trace = options
            .open(directory.join("obdlink-native-transcript-private.jsonl"))
            .map_err(|e| e.to_string())?;
        Ok(Self {
            socket,
            parser: Parser::default(),
            trace,
            started: Instant::now(),
            receiving: false,
            synchronized: true,
            configured: false,
            controller: 0,
            protocol_open: false,
            monitoring: false,
            active_tx: None,
            batch_pending: false,
            receive_preference: ReceivePreference::default(),
            hs_reply_until: None,
        })
    }
    fn record(&mut self, event: serde_json::Value) -> Result<(), String> {
        writeln!(
            self.trace,
            "{}",
            serde_json::json!({"elapsed_ms":self.started.elapsed().as_millis(),"event":event})
        )
        .map_err(|e| e.to_string())
    }
    fn socket(&mut self, cmd: &str) -> Result<String, String> {
        if !self.synchronized {
            return Err(
                "STN ordered transport uncertain; no replay or further in-band writes".into(),
            );
        }
        let result = self.socket.command(cmd);
        if result.is_err() {
            self.synchronized = false;
        }
        result
    }
    fn send(&mut self, cmd: &str) -> Result<(), String> {
        if self.receiving {
            return Err("STN command refused while another exchange is active".into());
        }
        self.record(serde_json::json!({"tx_ascii":cmd}))?;
        if self.socket(&format!("WRITE {cmd}"))? != "WRITTEN" {
            return Err("Native RFCOMM write failed".into());
        }
        self.receiving = true;
        self.monitoring = cmd == "STM";
        self.batch_pending = cmd.ends_with("|STM");
        Ok(())
    }
    fn packet(&mut self) -> Result<Vec<Item>, String> {
        let reply = self.socket("READ")?;
        if reply == "EMPTY" {
            return Ok(vec![]);
        }
        let value: serde_json::Value =
            serde_json::from_str(&reply).map_err(|_| "Invalid STN byte packet")?;
        if value["type"] != "rx" {
            return Err("Unexpected STN byte event".into());
        }
        let values = value["bytes"].as_array().ok_or("Missing STN bytes")?;
        if values.len() > 1024 {
            return Err("STN byte fragment too large".into());
        }
        let bytes = values
            .iter()
            .map(|v| {
                v.as_u64()
                    .filter(|v| *v <= 255)
                    .map(|v| v as u8)
                    .ok_or("Invalid STN byte".to_string())
            })
            .collect::<Result<Vec<_>, _>>()?;
        self.record(serde_json::json!({"rx_ascii":String::from_utf8_lossy(&bytes)}))?;
        let items = self.parser.feed(&bytes)?;
        if items.iter().any(|v| matches!(v, Item::Prompt)) {
            self.receiving = false;
            if self.monitoring {
                self.protocol_open = false;
                self.monitoring = false;
                self.record(serde_json::json!({"monitor_exit_protocol_closed":true}))?;
            }
        }
        Ok(items)
    }
    fn reply(&mut self, cmd: &str) -> Result<Vec<String>, String> {
        if self.receiving {
            return Err("STN setup requires an idle prompt".into());
        }
        self.send(cmd)?;
        let end = Instant::now() + Duration::from_secs(3);
        let mut lines = vec![];
        while self.receiving {
            if Instant::now() > end {
                return Err(format!("STN command prompt timeout: {cmd}"));
            }
            for item in self.packet()? {
                if matches!(item, Item::BatchBoundary) { return Err("Unexpected STN batch boundary during setup".into()); }
                if let Item::Line(s) = item {
                    if s.trim() != cmd {
                        lines.push(s);
                    }
                }
            }
        }
        Ok(lines)
    }
    fn exact(&mut self, cmd: &str, expected: &str) -> Result<(), String> {
        let lines = self.reply(cmd)?;
        if lines
            .iter()
            .map(|s| s.split_whitespace().collect::<String>())
            .collect::<Vec<_>>()
            != [expected.split_whitespace().collect::<String>()]
        {
            return Err(format!("STN readback rejected {cmd}: {lines:?}"));
        }
        if cmd == "STPO" {
            self.protocol_open = true;
        }
        if matches!(cmd, "STPC" | "STP31" | "STP61") {
            self.protocol_open = false;
        }
        Ok(())
    }
    fn ensure_protocol_open(&mut self) -> Result<(), String> {
        // UART cancellation of a pending raw request may also close the protocol.
        // Never infer its state from a prior command or monitoring flag.
        self.exact("STPO", "OK")?;
        self.record(serde_json::json!({"explicit_protocol_open_before_mode_or_TX":true,"controller":self.controller}))?;
        Ok(())
    }

    fn exact_setup_batch(&mut self, commands: &[(&str, &str)]) -> Result<(), String> {
        // FRPM F 16.2: verbose output has one response per command, N-1
        // boundaries and one final prompt. No vehicle frame is in this batch.
        if commands.is_empty() || commands.iter().any(|(cmd, _)|
            cmd.is_empty() || cmd.bytes().any(|b| matches!(b, b'|' | b'\r' | b'\n'))) {
            return Err("STN setup batch requires nonempty individual commands; never replay an empty line".into());
        }
        let wire = commands.iter().map(|(cmd, _)| *cmd).collect::<Vec<_>>().join("|");
        let result = (|| {
            self.send(&wire)?;
            let deadline = Instant::now() + Duration::from_secs(3);
            let mut index = 0;
            let mut lines = Vec::new();
            let mut prompted = false;
            while self.receiving {
                if Instant::now() >= deadline { return Err("STN setup batch prompt timeout".into()); }
                for item in self.packet()? {
                    if prompted { return Err("STN setup batch data after final prompt".into()); }
                    match item {
                        Item::Line(line) => lines.push(line),
                        Item::BatchBoundary | Item::Prompt => {
                            let Some((command, expected)) = commands.get(index) else {
                                return Err("STN setup batch has excess responses".into());
                            };
                            if lines.len() != 1 || lines[0].split_whitespace().collect::<String>()
                                != expected.split_whitespace().collect::<String>() {
                                return Err(format!("STN setup batch readback rejected {command}: {lines:?}"));
                            }
                            lines.clear();
                            index += 1;
                            prompted = matches!(item, Item::Prompt);
                            if prompted != (index == commands.len()) {
                                return Err("STN setup batch boundary/prompt count mismatch".into());
                            }
                        }
                    }
                }
            }
            if !prompted || index != commands.len() { return Err("STN setup batch incomplete".into()); }
            Ok(())
        })();
        if result.is_err() {
            self.synchronized = false;
            self.protocol_open = false;
        }
        result
    }
    fn setup(&mut self) -> Result<(), String> {
        // A sleeping adapter may publish a reset banner when RFCOMM opens.
        // Drain only pre-session startup bytes before issuing the first reset.
        let end = Instant::now() + Duration::from_secs(2);
        let mut quiet = Instant::now();
        while quiet.elapsed() < Duration::from_millis(300) {
            if Instant::now() > end {
                return Err("STN startup bytes did not settle".into());
            }
            let items = self.packet()?;
            if !items.is_empty() {
                quiet = Instant::now();
                self.record(serde_json::json!({"startup_banner_drained":true}))?;
            }
        }
        self.parser = Parser::default();
        self.receiving = false;
        let reset = self.reply("ATZ")?;
        if reset.len() != 1 || !reset[0].starts_with("ELM327") {
            return Err(format!("STN reset rejected: {reset:?}"));
        }
        self.exact("ATE0", "OK")?;
        self.exact("STI", "STN2255 v5.12.4")?;
        self.exact("STDI", "OBDLink MX+ r3.1.3")?;
        self.exact("STSN", "225530563988")?;
        let volts = self.reply("ATRV")?;
        let voltage = if volts.len() == 1 {
            volts[0]
                .trim()
                .strip_suffix('V')
                .and_then(|s| s.trim().parse::<f64>().ok())
        } else {
            None
        };
        if !voltage.is_some_and(|v| v.is_finite() && (11.5..=15.5).contains(&v)) {
            return Err(format!(
                "Bench adapter voltage unavailable or outside 11.5–15.5 V: {volts:?}"
            ));
        }
        self.record(serde_json::json!({"adapter_voltage":voltage,"source":"ATRV"}))?;
        for cmd in [
            "ATL0", "ATS1", "ATH1", "ATD1", "ATV1", "ATCAF0", "ATCFC0", "STCMM1", "STCSEGT0",
            "STCSEGR0", "STBC1", "STBCOF0", "ATR1", "ATAT0", "ATST0A", "STP31",
        ] {
            self.exact(cmd, "OK")?;
        }
        self.configured = true;
        self.exact("STPR", "31")?;
        self.exact("STPBRR", "500000")?;
        for cmd in ["STFAC", "STFPA400,400", "STPO"] {
            self.exact(cmd, "OK")?;
        }
        self.record(serde_json::json!({"setup_complete":true,"bus":"HS-CAN; serialized SW-CAN preview","bitrate":500000,"raw_frames":true,"host_flow_control":false,"host_ISOTP_assembly":false}))
    }
    fn select_bus(&mut self, controller: usize) -> Result<(), String> {
        if controller == self.controller {
            return Ok(());
        }
        if ![0, 2].contains(&controller) {
            return Err("MX+ controller unavailable".into());
        }
        let mut commands = vec![
            ("STPC", "OK"), (if controller == 0 { "STP31" } else { "STP61" }, "OK"),
            ("STPR", if controller == 0 { "31" } else { "61" }),
            ("STPBRR", if controller == 0 { "500000" } else { "33333" }),
            ("STCMM1", "OK"), ("STCSEGT0", "OK"), ("STCSEGR0", "OK"),
            ("ATR1", "OK"), ("STFAC", "OK"),
            (if controller == 0 { "STFPA400,400" } else { "STFPA000,000" }, "OK"),
            ("STPO", "OK"),
        ];
        if controller == 2 { commands.push(("STCSWM3", "OK")); }
        self.exact_setup_batch(&commands)?;
        self.protocol_open = true;
        self.controller = controller;
        self.record(serde_json::json!({"selected_controller":controller,"bus":if controller==0 {"HS-CAN"} else {"SW-CAN"},"simultaneous_buses":false}))
    }

    fn restore_idle_hs_receive(&mut self, events: &SyncSender<Event>) -> Result<bool, String> {
        if !self.receive_preference.return_to_hs { return Ok(false); }
        self.interrupt_monitor(events, "restore-HS-after-functional-SW-tester-present")?;
        self.select_bus(0)?;
        self.send("STM")?;
        self.receive_preference.return_to_hs = false;
        self.record(serde_json::json!({"idle_receive_restored_to_controller":0,
            "reason":"last non-tester guest frame was HS; exact functional SW tester-present completed",
            "guest_frame_altered":false,"simultaneous_buses":false}))?;
        Ok(true)
    }
    fn receive(&mut self, events: &SyncSender<Event>) -> Result<(), String> {
        let monitoring = self.monitoring;
        for item in self.packet()? {
            if matches!(item, Item::BatchBoundary) {
                if !self.batch_pending { self.synchronized=false; return Err("Unexpected STN batch boundary".into()); }
                self.batch_pending = false;
                self.monitoring = true;
                self.record(serde_json::json!({"transmit_batch_boundary":true,"continuous_receive_started_in_same_batch":true,"electrical_ACK_verified":false}))?;
                continue;
            }
            if let Item::Line(line) = item {
                if let Some(f) = frame(&line)? {
                    self.record(
                        serde_json::json!({"received_frame":{"id":f.id,"dlc":f.dlc,"data":f.data}}),
                    )?;
                    events
                        .try_send(Event::Received(self.controller, f))
                        .map_err(|_| {
                            "STN receive queue full; session failed rather than discarding"
                                .to_string()
                        })?;
                } else if line.trim() == "STOPPED" && !monitoring {
                    return Err("STN ordinary CAN request was unexpectedly interrupted".into());
                } else if !matches!(line.trim(), "" | "OK" | "STOPPED" | "NO DATA") {
                    return Err(format!("STN raw status: {line}"));
                }
            }
        }
        Ok(())
    }
    fn finish_request(&mut self, events: &SyncSender<Event>) -> Result<(), String> {
        if self.monitoring {
            return Err("Cannot finish indefinite monitoring as an ordinary request".into());
        }
        let end = Instant::now() + Duration::from_secs(3);
        while self.receiving {
            if Instant::now() > end {
                self.synchronized = false;
                return Err("STN ordinary request prompt timeout; no interrupt or replay".into());
            }
            self.receive(events)?;
        }
        Ok(())
    }
    fn finish_transmit_batch(&mut self, events: &SyncSender<Event>) -> Result<(), String> {
        let end = Instant::now() + Duration::from_secs(3);
        while self.batch_pending {
            if Instant::now() > end {
                self.synchronized = false;
                return Err("STN batch boundary prompt timeout; no interrupt or replay".into());
            }
            self.receive(events)?;
            if self.batch_pending && !self.receiving {
                self.synchronized=false;
                return Err("STN batch ended before continuous receive; no completion or replay".into());
            }
        }
        if !self.receiving || !self.monitoring {
            return Err("STN transmit batch did not retain continuous receive".into());
        }
        Ok(())
    }
    fn interrupt_monitor(
        &mut self,
        events: &SyncSender<Event>,
        reason: &str,
    ) -> Result<(), String> {
        if !self.receiving {
            return Ok(());
        }
        if !self.monitoring {
            return Err("Refused implicit interruption of an ordinary CAN request".into());
        }
        self.record(serde_json::json!({"monitor_interrupt":{"reason":reason,"controller":self.controller},"ordinary_CAN_request_interrupted":false}))?;
        if self.socket("INTERRUPT")? != "WRITTEN" {
            return Err("STN interrupt failed".into());
        }
        let end = Instant::now() + Duration::from_secs(2);
        while self.receiving {
            if Instant::now() > end {
                return Err("STN prompt absent after interrupt".into());
            }
            self.receive(events)?;
        }
        Ok(())
    }
    fn cleanup(&mut self, events: &SyncSender<Event>) {
        let protocol = (|| -> Result<(), String> {
            if self.monitoring {
                self.interrupt_monitor(events, "session-close")?;
            } else {
                self.finish_request(events)?;
            }
            let counters = self.reply("ATCS")?;
            self.record(serde_json::json!({"CAN_counters_before_close":counters}))?;
            if self.configured {
                self.exact("STPC", "OK")?;
            }
            Ok(())
        })();
        // CLOSE is a local ownership command, permitted even after uncertain in-band I/O.
        let native = self.socket.command("CLOSE").and_then(|s| {
            if s == "CLOSED" {
                Ok(())
            } else {
                Err("Native Bluetooth close unconfirmed".into())
            }
        });
        let result = protocol.and(native);
        let _=self.record(serde_json::json!({"cleanup":match result {Ok(())=>"confirmed".to_string(),Err(e)=>format!("unconfirmed: {e}")}}));
    }
}
#[derive(Clone, Debug)]
struct WorkerFailure {
    message: String,
    context: Option<FailureContext>,
}
pub struct Bridge {
    input: Option<SyncSender<(usize, CanTransmission)>>,
    output: Receiver<Event>,
    errors: Receiver<WorkerFailure>,
    failure: Option<FailureContext>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    gate: CommandGate,
    profile: Profile,
    key_status: KeyStatusGate,
    confirmations: u64,
}
impl Bridge {
    pub fn start(token: &str, directory: &Path) -> Result<Self, String> {
        Self::start_with_reset(token, directory, false)
    }
    pub fn start_with_reset(token: &str, directory: &Path, reset_counters: bool) -> Result<Self, String> {
        Self::start_with_profile(token, directory, if reset_counters {Profile::ResetEngineCounters} else {Profile::Read})
    }
    pub fn start_with_profile(token: &str, directory: &Path, profile: Profile) -> Result<Self, String> {
        if profile==Profile::Seeds {return Err("MX+ seed profile unavailable".into());}
        if token.len() != 32 || !token.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("Invalid MX+ owner token".into());
        }
        Ok(Self::from_link_profile(Link::open(token, directory, profile)?, profile))
    }
    fn from_link(link: Link) -> Self {
        Self::from_link_profile(link, Profile::Read)
    }
    fn from_link_profile(mut link: Link, profile: Profile) -> Self {
        let (input, requests) = mpsc::sync_channel::<(usize, CanTransmission)>(32);
        let (events, output) = mpsc::sync_channel(4096);
        let (error, errors) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let cancelled = stop.clone();
        let worker = thread::spawn(move || {
            let result = (|| -> Result<(), String> {
                link.setup()?;
                let mut pending = None;
                while !cancelled.load(Ordering::Relaxed) {
                    if link.started.elapsed() > Duration::from_secs(1800) {
                        return Err("Private MX+ firmware session reached its 30-minute limit".into());
                    }
                    if pending.is_none() { pending = requests.try_recv().ok(); }
                    if pending.as_ref().is_some_and(|(controller, _)|
                        link.controller == 0 && *controller != 0
                        && link.hs_reply_until.is_some_and(|until| Instant::now() < until)) {
                        // Preserve original request order while collecting late functional
                        // HS replies. No new request is sent, retried or synthesized here.
                        link.receive(&events)?;
                        continue;
                    }
                    if let Some((controller, tx)) = pending.take() {
                        link.interrupt_monitor(&events, "next-guest-request")?;
                        link.active_tx = Some(FailureContext {
                            tx: crate::adapter_failure::TxContext::new(controller, &tx),
                            ticket: tx.ticket,
                            phase: "bus-setup",
                        });
                        let switched = link.controller != controller;
                        if switched {
                            link.select_bus(controller)?;
                        }
                        let route = electrical_route(controller, &tx)?;
                        // FRPM F8.14: STPX opens the selected protocol itself.
                        // SW mode changes still require a separately checked open;
                        // ordinary HS STPX needs neither STPO nor a cached open.
                        if !switched && controller == 2 { link.ensure_protocol_open()?; }
                        if controller == 2 && (!switched || route.mode == ElectricalMode::SingleWireWake) {
                            link.exact(
                                if route.mode == ElectricalMode::SingleWireWake {
                                    "STCSWM2"
                                } else {
                                    "STCSWM3"
                                },
                                "OK",
                            )?;
                        }
                        // STPX always supplies H explicitly (FRPM F8.6), so ATSH
                        // is redundant. Keep the previously verified wake setup.
                        if route.mode == ElectricalMode::SingleWireWake {
                            link.exact(&format!("ATSH{:03X}", tx.frame.id), "OK")?;
                        }
                        let command = encode(controller, &tx)?;
                        link.record(serde_json::json!({"guest_tx":{"controller":controller,"ticket":tx.ticket,"id":tx.frame.id,"dlc":tx.frame.dlc,"data":tx.frame.data}}))?;
                        link.active_tx.as_mut().unwrap().phase = "can-request";
                        if route.mode == ElectricalMode::SingleWireWake {
                            let lines = link.reply(&command)?;
                            if lines.iter().any(|s| !matches!(s.trim(), "OK" | "NO DATA")) {
                                return Err(format!("STN zero-DLC wake rejected: {lines:?}"));
                            }
                            link.exact("STCSWM3", "OK")?;
                            let counters = link.reply("ATCS")?;
                            link.record(
                                serde_json::json!({"wake_CAN_counters_without_reopen":counters}),
                            )?;
                            // Candidate pacing taken from the pinned Saab J2534 reference;
                            // not a vendor-specified delay or proof of electrical wake.
                            thread::sleep(Duration::from_millis(500));
                            link.record(serde_json::json!({"wake_settle_candidate_ms":500,"protocol_closed_after_wake":false,"guest_frame_altered":false}))?;
                            link.record(serde_json::json!({"native_zero_DLC_wake_command_accepted":true,"electrical_ACK_verified":false}))?;
                        } else {
                            // FRPM F 16.1: indefinite STM is permitted at the end
                            // of a batch. Avoid the Bluetooth round trip during
                            // which preview.12 lost the first VIN continuation.
                            // The pipe validates STPX completion; it is neither
                            // an electrical ACK nor an ECU operation result.
                            link.send(&format!("{command}|STM"))?;
                            link.finish_transmit_batch(&events)?;
                            link.record(serde_json::json!({"ordinary_request_batch_boundary":{"controller":controller,"ticket":tx.ticket},"transmit_only_response_count":0,"original_CANdi_flow_control_owner":true,"electrical_ACK_verified":false}))?;
                        }
                        events
                            .try_send(Event::Completed {
                                controller,
                                ticket: tx.ticket,
                                source: if route.mode == ElectricalMode::SingleWireWake { CompletionSource::StnRfcommWriteCompatibility } else { CompletionSource::StnBatchBoundaryWriteCompatibility },
                            })
                            .map_err(|_| "STN event queue full".to_string())?;
                        link.hs_reply_until = if functional_hs_reply_request(controller, &tx) {
                            let until = Instant::now() + Duration::from_millis(100);
                            link.record(serde_json::json!({"functional_HS_reply_window_ms":100,"ticket":tx.ticket,
                                "guest_frame_altered":false,"scope":"defer only queued cross-bus work; preserve FIFO order"}))?;
                            Some(until)
                        } else if controller == 0 { link.hs_reply_until } else { None };
                        link.receive_preference.transmitted(controller, &tx);
                        link.active_tx = None;
                    } else if link.restore_idle_hs_receive(&events)? {
                        // Queued original requests take priority over idle parking.
                    } else if !link.receiving {
                        link.send("STM")?;
                    } else {
                        link.receive(&events)?;
                    }
                }
                Ok(())
            })();
            if let Err(e) = result {
                let context = link.active_tx.clone();
                let _=link.record(serde_json::json!({"failure":e,"in_flight":context.as_ref().map(|c|serde_json::json!({"controller":c.tx.controller,"ticket":c.ticket,"id":c.tx.id,"dlc":c.tx.dlc,"phase":c.phase}))}));
                let _ = error.send(WorkerFailure {
                    message: e,
                    context,
                });
            }
            link.cleanup(&events);
        });
        Self {
            input: Some(input),
            output,
            errors,
            failure: None,
            stop,
            worker: Some(worker),
            gate: CommandGate::default(),
            profile,
            // This private read preview supports the exact captured native
            // Service > Check Ignition Key Status query, at most three times
            // in this owned session. Other DeviceControl remains rejected.
            key_status: KeyStatusGate {
                enabled: true,
                sent: 0,
            },
            confirmations: 0,
        }
    }
}
impl Backend for Bridge {
    fn poll(&mut self) -> Result<Vec<Event>, String> {
        if let Ok(e) = self.errors.try_recv() {
            self.failure = e.context;
            return Err(e.message);
        }
        let events: Vec<_> = self.output.try_iter().collect();
        self.confirmations += events
            .iter()
            .filter(|e| matches!(e, Event::Completed { .. }))
            .count() as u64;
        Ok(events)
    }
    fn transmit(&mut self, controller: usize, tx: CanTransmission) -> Result<(), String> {
        encode(controller, &tx)?;
        if !self
            .gate
            .allowed(controller, &tx, self.profile, Instant::now())
            && !self.key_status.allowed(controller, &tx)
        {
            return Err("STN original-firmware session policy rejected request".into());
        }
        self.input
            .as_ref()
            .ok_or("MX+ bridge closed")?
            .try_send((controller, tx))
            .map_err(|_| "STN transmit queue full/closed".into())
    }
    fn confirmations(&self) -> u64 {
        self.confirmations
    }
    fn failure_context(&self) -> Option<FailureContext> {
        self.failure.clone()
    }
    fn close(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.input = None;
        if let Some(w) = self.worker.take() {
            let _ = w.join();
        }
    }
}
impl Drop for Bridge {
    fn drop(&mut self) {
        self.close();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::candi_cpu::{CanElectricalState, CanFrame};
    #[test]
    fn preserves_guest_bytes_and_rejects_other_buses_and_empty_frames() {
        let mut tx = CanTransmission {
            frame: CanFrame {
                id: 0x7e0,
                extended: false,
                rtr: false,
                dlc: 8,
                data: vec![2, 0x1a, 0x90, 0, 0, 0, 0, 0],
            },
            ticket: 1,
            btr0: 0xc1,
            btr1: 0x36,
            electrical: CanElectricalState::StandardCan,
        };
        assert_eq!(encode(0, &tx).unwrap(), "STPXH:7E0,D:021A900000000000,R:0");
        assert!(encode(2, &tx).is_err());
        tx.frame.dlc = 3;
        tx.frame.data = vec![0x30, 0, 0];
        assert_eq!(encode(0, &tx).unwrap(), "STPXH:7E0,D:300000,R:0");
        // Raw first/consecutive/flow-control PCI bytes belong to original CANdi.
        tx.frame.data = vec![0x21, 0xaa, 0xbb];
        assert_eq!(encode(0, &tx).unwrap(), "STPXH:7E0,D:21AABB,R:0");
        tx.frame.data = vec![0x30, 0, 0];
        assert!(CommandGate::default().allowed(0, &tx, Profile::Read, Instant::now()));
        tx.frame.dlc = 0;
        tx.frame.data.clear();
        assert!(encode(0, &tx).is_err());
        tx.frame.id = 0x100;
        tx.btr0 = 0xdd;
        tx.electrical = CanElectricalState::SingleWireGpio {
            latch: 2,
            assignment: 0,
            direction: 3,
        };
        assert_eq!(encode(2, &tx).unwrap(), "STPXH:100,D:,R:0");
        tx.frame.dlc = 1;
        tx.frame.data = vec![0];
        assert!(encode(2, &tx).is_err());
    }
}

#[cfg(test)]
#[path = "obdlink_guest_tests.rs"]
mod byte_peer_tests;

#[cfg(test)]
mod profile_tests {
    use super::*;
    use crate::candi_cpu::{CanElectricalState, CanFrame};
    fn guarded(profile: Profile) -> (Bridge, Receiver<(usize, CanTransmission)>) {
        let (input, requests) = mpsc::sync_channel(32);
        let (_, output) = mpsc::sync_channel(1);
        let (_, errors) = mpsc::channel();
        (Bridge { input: Some(input), output, errors, failure: None,
            stop: Arc::new(AtomicBool::new(false)), worker: None,
            gate: CommandGate::default(), profile,
            key_status: KeyStatusGate {enabled:true, sent:0}, confirmations:0 }, requests)
    }
    fn request(data: [u8;8]) -> CanTransmission {
        CanTransmission {ticket:1,frame:CanFrame {id:0x7e0,extended:false,rtr:false,dlc:8,data:data.to_vec()},
            btr0:0xc1,btr1:0x36,electrical:CanElectricalState::StandardCan}
    }
    #[test]
    fn dtc_clear_uses_shared_profile_and_preserves_frames_on_both_buses() {
        for controller in [0,2] {
            let mut tx=request([1,4,0,0,0,0,0,0]);tx.frame.id=0x245;
            if controller==2 {tx.btr0=0xdd;tx.electrical=CanElectricalState::SingleWireGpio {latch:3,assignment:0,direction:3};}
            let (mut read,read_queue)=guarded(Profile::Read);assert!(read.transmit(controller,tx.clone()).is_err());assert!(read_queue.try_recv().is_err());
            let (mut clear,queue)=guarded(Profile::ClearDtc);clear.transmit(controller,tx.clone()).unwrap();
            let sent=queue.try_recv().unwrap();assert_eq!(sent.0,controller);assert_eq!(sent.1.frame.data,tx.frame.data);assert_eq!(sent.1.frame.id,tx.frame.id);
            tx.frame.data=vec![3,0xa9,0x81,0x0a,0,0,0,0];clear.transmit(controller,tx.clone()).unwrap();assert_eq!(queue.try_recv().unwrap().1.frame.data,tx.frame.data);
        }
        let (mut clear,queue)=guarded(Profile::ClearDtc);
        for d in [[2,0xae,0x14,0,0,0,0,0],[2,0x27,1,0,0,0,0,0],[2,0x27,2,0,0,0,0,0],[3,0x2e,0,1,0,0,0,0],[1,0x34,0,0,0,0,0,0],[2,4,1,0,0,0,0,0]] {
            assert!(clear.transmit(0,request(d)).is_err());assert!(queue.try_recv().is_err());
        }
    }
    #[test]
    fn counter_reset_is_explicit_exact_and_does_not_open_other_writes() {
        let reset=request([2,0xae,0x14,0,0,0,0,0]);
        let (mut read, read_queue)=guarded(Profile::Read);
        assert!(read.transmit(0,reset.clone()).is_err());
        assert!(read_queue.try_recv().is_err());
        let (mut maintenance, queue)=guarded(Profile::ResetEngineCounters);
        maintenance.transmit(0,reset).unwrap();
        assert_eq!(queue.try_recv().unwrap().1.frame.data,[2,0xae,0x14,0,0,0,0,0]);
        for forbidden in [[2,0xae,0x15,0,0,0,0,0],[2,0xae,0x14,1,0,0,0,0],
            [1,4,0,0,0,0,0,0],[2,0x27,1,0,0,0,0,0],[2,0x27,2,0,0,0,0,0]] {
            assert!(maintenance.transmit(0,request(forbidden)).is_err());
            assert!(queue.try_recv().is_err());
        }
        assert!(maintenance.transmit(2,request([2,0xae,0x14,0,0,0,0,0])).is_err());
        maintenance.transmit(0,request([2,0x1a,0x90,0,0,0,0,0])).unwrap();
        assert_eq!(queue.try_recv().unwrap().1.frame.data,[2,0x1a,0x90,0,0,0,0,0]);
    }
}
