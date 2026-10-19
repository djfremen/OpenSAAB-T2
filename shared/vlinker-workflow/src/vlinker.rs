// SPDX-License-Identifier: MPL-2.0
//! MIC-family host workflow with model-specific qualification and common VIN decoding.
//! Transport bindings deliver bytes; this module owns commands, framing and VIN validation.
use serde_json::{json, Value};
use std::{
    ffi::{c_char, CStr, CString},
    time::{Duration, Instant},
};

pub use crate::adapters::mic::VIN_REQUEST;
// Compatibility export; identified models select their own startup profile below.
pub const COMMANDS: [&str; 13] = crate::adapters::mic::STARTUP;
pub fn allowed(command: &str) -> bool {
    [crate::adapter_identity::Model::VlinkerMcPlus, crate::adapter_identity::Model::CaristaEvo, crate::adapter_identity::Model::ObdlinkMxp]
        .iter().any(|model| model.profile().startup.contains(&command))
        || PROBE_COMMANDS.contains(&command)
}

// Bounded, explicit host reads derived from Trionic8.cs; no clear/write/security services.
pub const INFO_READS: [(&str, u8, &str); 7] = [
    ("021A080000000000", 0x08, "ECM software"),
    ("021A710000000000", 0x71, "ECM hardware"),
    ("021A720000000000", 0x72, "ECM description"),
    ("021A730000000000", 0x73, "Codefile version"),
    ("021A740000000000", 0x74, "Calibration set"),
    ("021A950000000000", 0x95, "ECM software version ID"),
    ("021A7C0000000000", 0x7c, "Saab part number"),
];
pub const DTC_READ: &str = "03A9811200000000";
pub const SESSION_START: &str = "0210020000000000";
pub const SESSION_STOP: &str = "0120000000000000";
const PROBE_COMMANDS: [&str; 14] = [
    "021A080000000000",
    "021A710000000000",
    "021A720000000000",
    "021A730000000000",
    "021A740000000000",
    "021A950000000000",
    "021A7C0000000000",
    SESSION_START,
    DTC_READ,
    SESSION_STOP,
    "ATCFC0",
    "ATCF5E8",
    "ATCM5FF",
    "ATSTFF",
];
pub fn vehicle_command(command: &str) -> bool {
    command == VIN_REQUEST
        || INFO_READS.iter().any(|r| r.0 == command)
        || [SESSION_START, DTC_READ, SESSION_STOP].contains(&command)
}
/// Raw header-on CAF0 frame parsing, with no guessed DLC or synthetic payload.
fn frames(reply: &str) -> Result<Vec<(u32, [u8; 8])>, String> {
    reply
        .split(['\r', '\n'])
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let compact = line.split_whitespace().collect::<String>();
            if compact.len() != 19 || !compact.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err("Unexpected or truncated CAN frame".into());
            }
            let id = u32::from_str_radix(&compact[..3], 16).map_err(|_| "Invalid CAN ID")?;
            let mut data = [0; 8];
            for (i, b) in data.iter_mut().enumerate() {
                *b = u8::from_str_radix(&compact[3 + i * 2..5 + i * 2], 16)
                    .map_err(|_| "Invalid CAN bytes")?;
            }
            Ok((id, data))
        })
        .collect()
}
/// Strict ISO-TP readIdentifier reply; padding is excluded using the declared length.
fn identifier(reply: &str, pid: u8) -> Result<Vec<u8>, String> {
    let mut receiver = opensaab_adapter_translation::isotp::Receiver::new(512)?;
    let mut payload = None;
    for (id, data) in frames(reply)? {
        if id == 0x7e0 && data == [0x30, 0, 0, 0, 0, 0, 0, 0] { continue; }
        if id != 0x7e8 { return Err("Unexpected ECM identifier CAN ID".into()); }
        if data[..3] == [3, 0x7f, 0x1a] {
            if data[3] == 0x78 && !receiver.started() { continue; }
            return Err(format!("ECM identifier {pid:02X} rejected: NRC {:02X}", data[3]));
        }
        if let opensaab_adapter_translation::isotp::Step::Complete(bytes) = receiver.push(&data)? {
            payload = Some(bytes);
        }
    }
    let payload = payload.ok_or("Incomplete ECM identifier reply")?;
    if payload.len() < 3 || payload.get(..2) != Some(&[0x5a, pid]) {
        return Err("Incomplete or mismatched ECM identifier reply".into());
    }
    Ok(payload[2..].to_vec())
}
fn identifier_value(bytes: &[u8], pid: u8) -> Value {
    let hex = bytes
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ");
    let value = if pid == 0x7c && bytes.len() == 4 {
        u32::from_be_bytes(bytes.try_into().unwrap()).to_string()
    } else if bytes.iter().all(|b| *b == 0 || (32..=126).contains(b))
        && bytes.iter().any(|b| (33..=126).contains(b))
    {
        String::from_utf8(bytes.iter().copied().filter(|b| *b != 0).collect())
            .unwrap()
            .trim()
            .to_owned()
    } else {
        format!("Hex: {hex}")
    };
    json!({"value":value,"raw_hex":hex,"identifier":format!("1A {pid:02X}")})
}
fn dtc_report(reply: &str) -> Result<Value, String> {
    let mut report = crate::t8_dtc::Report::default();
    for (id, data) in frames(reply)? {
        if ![0x7e8, 0x5e8].contains(&id) {
            return Err("Unexpected DTC report CAN ID".into());
        }
        if id == 0x7e8 && !(data[..3] == [3, 0x7f, 0xa9]) {
            return Err("Unexpected DTC report control response".into());
        }
        if id == 0x5e8 && data[0] != 0x81 {
            return Err("Unexpected DTC report record".into());
        }
        if id == 0x5e8 && data[1..4] == [0, 0, 0] && data[4] != 0xff {
            return Err("Invalid DTC report end marker".into());
        }
        report.receive(id, &data)?;
    }
    let records=report.finish()?.into_iter().map(|d|json!({"code":d.code,"failure_type":format!("{:02X}",d.failure_type),"status":format!("{:02X}",d.status),"warning_indicator_requested":d.status&0x80!=0,"current":d.status&2!=0,"history":d.status&0x10!=0})).collect::<Vec<_>>();
    Ok(
        json!({"scope":"Trionic 8 ECM current/history","mask":"12","complete":true,"codes_cleared":false,"records":records}),
    )
}

pub fn checked(command: &str, raw: &str) -> Result<String, String> {
    let compact = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
    let echo = compact(command);
    let lines: Vec<_> = raw
        .split(['\r', '\n'])
        .map(str::trim)
        .filter(|s| !s.is_empty() && !compact(s).eq_ignore_ascii_case(&echo))
        .collect();
    if lines.is_empty() {
        return Err(format!("{command}: empty reply"));
    }
    for line in &lines {
        let s = line.to_ascii_uppercase();
        if ["?", "NO DATA", "STOPPED", "UNABLE TO CONNECT"].contains(&s.as_str())
            || ["ERROR", "BUFFER FULL", "BUS INIT"]
                .iter()
                .any(|token| s.contains(token))
        {
            return Err(format!("{command}: {line}"));
        }
    }
    Ok(lines.join("\n"))
}

pub fn parse_vin(reply: &str) -> Result<String, String> {
    let mut receiver = opensaab_adapter_translation::isotp::VinReply::default();
    for (id, data) in frames(reply)? {
        if id == 0x7e0 && data == [0x30, 0, 0, 0, 0, 0, 0, 0] { continue; }
        if id != 0x7e8 { return Err("Unexpected CAN ID in VIN reply".into()); }
        if data[..3] == [3, 0x7f, 0x1a] {
            if data[3] == 0x78 { continue; }
            return Err(format!("ECM rejected VIN read: NRC {:02X}", data[3]));
        }
        receiver.feed(&data)?;
    }
    let vin = receiver.vin()?.ok_or("Incomplete ECM VIN reply")?;
    if !vin.starts_with("YS3") { return Err("Invalid Saab VIN characters".into()); }
    Ok(vin)
}

#[derive(Default)]
struct Prompt {
    bytes: Vec<u8>,
    done: bool,
}
impl Prompt {
    fn feed(&mut self, bytes: &[u8]) -> Result<(), String> {
        for &byte in bytes {
            if self.done {
                if !b"\r\n ".contains(&byte) {
                    return Err("Unexpected bytes after command prompt".into());
                }
            } else if byte == b'>' {
                self.done = true;
            } else {
                if ![9, 10, 13].contains(&byte) && !(32..=126).contains(&byte) {
                    return Err("Non-ASCII adapter reply".into());
                }
                if self.bytes.len() >= 16384 {
                    return Err("Adapter reply exceeded 16 KiB".into());
                }
                self.bytes.push(byte);
            }
        }
        Ok(())
    }
}

pub struct Session {
    read_vin: bool,
    owned: bool,
    lease: Option<crate::ownership::Lease>,
    commands: Vec<&'static str>,
    mode: &'static str,
    ecm: serde_json::Map<String, Value>,
    dtc: Option<Value>,
    diagnostic_active: bool,
    cleanup_error: Option<String>,
    origin: &'static str,
    index: usize,
    started: bool,
    born: Instant,
    command_at: Instant,
    prompt: Prompt,
    terminal: Option<Value>,
    model: Option<crate::adapter_identity::Model>,
    product: String,
    firmware: String,
    voltage: String,
    vin: String,
}
impl Session {
    // Pure protocol sessions can replay offline fixtures; native bindings must claim ownership.
    pub fn owned(mut self) -> Self { self.owned = true; self }
    fn claim(&mut self, event: &Value) -> bool {
        if !self.owned || self.lease.is_some() { return true; }
        let owner = event["owner"].as_str().map(str::to_owned).unwrap_or_else(crate::ownership::unique);
        self.lease = crate::ownership::Lease::acquire(&owner);
        self.lease.is_some()
    }
    pub fn startup() -> Self {
        let mut session = Self::new(true);
        session.origin = "host-startup-discovery";
        session
    }
    pub fn new(read_vin: bool) -> Self {
        Self {
            read_vin,
            owned: false,
            lease: None,
            commands: COMMANDS[..if read_vin { 13 } else { 5 }].to_vec(),
            mode: "vin",
            ecm: serde_json::Map::new(),
            dtc: None,
            diagnostic_active: false,
            cleanup_error: None,
            origin: "host-bench-qualification",
            index: 0,
            started: false,
            born: Instant::now(),
            command_at: Instant::now(),
            prompt: Prompt::default(),
            terminal: None,
            model: None,
            product: String::new(),
            firmware: String::new(),
            voltage: String::new(),
            vin: String::new(),
        }
    }
    pub fn probe(codes: bool) -> Self {
        let mut session = Self::new(true);
        session.origin = "host-hscan-probe";
        session.mode = if codes { "ecm_dtc" } else { "ecm_info" };
        session.commands = COMMANDS[..12].to_vec();
        if codes {
            session.commands.extend([
                "ATCFC0",
                "ATCF5E8",
                "ATCM5FF",
                "ATSTFF",
                SESSION_START,
                DTC_READ,
                SESSION_STOP,
            ]);
        } else {
            session.commands.extend(INFO_READS.iter().map(|r| r.0));
        }
        session.commands.push("VTPC");
        session
    }
    fn close_command(&self) -> &'static str {
        if self.commands.contains(&"STDI") { "STPC" } else { "VTPC" }
    }
    fn select_obdlink(&mut self) -> Result<(), String> {
        if self.started { return Err("Cannot change an active adapter profile".into()); }
        self.commands = crate::adapters::obdlink_mxp::STARTUP[..if self.read_vin {22} else {5}].to_vec();
        if self.mode == "ecm_dtc" {
            self.commands.extend(["ATCFC0", "ATCF5E8", "ATCM5FF", "ATSTFF", SESSION_START, DTC_READ, SESSION_STOP]);
        } else if self.mode == "ecm_info" { self.commands.extend(INFO_READS.iter().map(|r|r.0)); }
        if self.read_vin { self.commands.push("STPC"); }
        Ok(())
    }
    fn fail(&mut self, reason: impl Into<String>) -> Value {
        let reason = reason.into();
        // When a report fails while the diagnostic operation is active, stop once,
        // then close the adapter protocol. A failed cleanup can never publish codes.
        if self.mode == "ecm_dtc" && self.diagnostic_active && self.cleanup_error.is_none() {
            self.cleanup_error = Some(reason);
            self.vin.clear();
            self.dtc = None;
            self.commands = if self.commands[self.index] == SESSION_STOP {
                self.diagnostic_active = false;
                vec![self.close_command()]
            } else {
                vec![SESSION_STOP, self.close_command()]
            };
            self.index = 0;
            return self.emit();
        }
        if self.cleanup_error.is_some() && self.commands[self.index] == SESSION_STOP {
            self.diagnostic_active = false;
            self.index += 1;
            return self.emit();
        }
        let result = json!({"state":"failed", "error":self.cleanup_error.clone().unwrap_or(reason), "automatic_replay":false});
        self.vin.clear();
        self.terminal = Some(result.clone());
        result
    }
    fn emit(&mut self) -> Value {
        self.command_at = Instant::now();
        if self.commands[self.index] == SESSION_START {
            self.diagnostic_active = true;
        }
        self.prompt = Prompt::default();
        json!({"state":"send", "command":self.commands[self.index]})
    }
    fn validate(&mut self, reply: &str) -> Result<(), String> {
        let command = self.commands[self.index];
        match command {
            "ATZ" => {}
            "VTI" => {
                let model = crate::adapter_identity::Model::from_product(reply)?;
                if model == crate::adapter_identity::Model::ObdlinkMxp { return Err("MX+ requires its explicit STN profile".into()); }
                // Adopt the identified model's startup, preserving pending probe
                // reads and cleanup. Identity-only sessions retain their five steps.
                let count = if self.mode == "vin" { self.commands.len() } else { 12 };
                self.commands[..count].copy_from_slice(&model.profile().startup[..count]);
                self.model = Some(model);
                self.product = reply.into();
            }
            "STDI" => {
                let model = crate::adapter_identity::Model::from_product(reply)?;
                if model != crate::adapter_identity::Model::ObdlinkMxp { return Err("Selected MX+ returned a different product".into()); }
                self.model = Some(model); self.product = reply.into();
            }
            "STI" | "VTVERS" => {
                let model = self.model.ok_or("Missing adapter model")?;
                let qualified = if self.mode == "vin" { model.vin_firmware(reply) } else { model.probe_firmware(reply) };
                if self.read_vin && !qualified {
                    return Err(format!("{} profile is not qualified for {} firmware: {reply}", self.mode, model.label()));
                }
                self.firmware = reply.into();
            }
            "ATRV" => {
                let v = reply
                    .strip_suffix(['V', 'v'])
                    .ok_or("Invalid adapter voltage reply")?;
                let (whole, fraction) = v.split_once('.').ok_or("Invalid adapter voltage reply")?;
                if !(1..=2).contains(&whole.len())
                    || fraction.is_empty()
                    || !whole
                        .bytes()
                        .chain(fraction.bytes())
                        .all(|b| b.is_ascii_digit())
                {
                    return Err("Invalid adapter voltage reply".into());
                }
                self.voltage = reply.into();
            }
            "STPR" | "VTPRON" => {
                if reply != "6"
                    && reply
                        .chars()
                        .filter(|c| !c.is_whitespace())
                        .collect::<String>()
                        != "6[AT]"
                {
                    return Err("Adapter did not select 11-bit HS-CAN protocol 6".into());
                }
            }
            "STPBRR" | "VTPBRD" => {
                if reply != "500000" {
                    return Err("Adapter CAN baud rate is not 500000".into());
                }
            }
            VIN_REQUEST => self.vin = parse_vin(reply)?,
            SESSION_START => {
                if !frames(reply)?
                    .iter()
                    .any(|(id, data)| *id == 0x7e8 && data[..2] == [1, 0x50])
                {
                    return Err("ECM diagnostic session start was not confirmed".into());
                }
                self.diagnostic_active = true;
            }
            SESSION_STOP => {
                if !frames(reply)?
                    .iter()
                    .any(|(id, data)| *id == 0x7e8 && data[..2] == [1, 0x60])
                {
                    return Err("ECM diagnostic session stop was not confirmed".into());
                }
                self.diagnostic_active = false;
            }
            DTC_READ => self.dtc = Some(dtc_report(reply)?),
            command if INFO_READS.iter().any(|r| r.0 == command) => {
                let (_, pid, label) = INFO_READS.iter().find(|r| r.0 == command).unwrap();
                self.ecm.insert(
                    (*label).into(),
                    identifier_value(&identifier(reply, *pid)?, *pid),
                );
            }
            _ => {
                if reply != "OK" {
                    return Err(format!("{command}: expected OK"));
                }
            }
        }
        Ok(())
    }
    pub fn event(&mut self, event: &Value) -> Value {
        if let Some(terminal) = &self.terminal {
            return terminal.clone();
        }
        let op = event["op"].as_str().unwrap_or("");
        if op == "select_profile" {
            return if event["model"] == "obdlink_mxp" {
                match self.select_obdlink() {Ok(()) => json!({"state":"idle"}), Err(e) => self.fail(e)}
            } else {self.fail("Unknown explicit adapter profile")};
        }
        if op == "claim" {
            return if self.claim(event) { json!({"state":"idle"}) }
                else { self.fail("Finish the active workflow before connecting") };
        }
        if op == "cancel" || op == "disconnected" || op == "error" {
            return self.fail(
                event["message"]
                    .as_str()
                    .unwrap_or("Bluetooth test stopped"),
            );
        }
        if self.born.elapsed() >= Duration::from_secs(30) {
            return self.fail("Session watchdog expired; no automatic replay");
        }
        if self.started && self.command_at.elapsed() >= Duration::from_secs(4) {
            return self.fail("Command prompt timeout; no automatic replay");
        }
        match op {
            "start" => {
                if !self.claim(event) { return self.fail("Finish the active workflow before connecting"); }
                if self.started {
                    return self.fail("Session already started");
                }
                self.started = true;
                self.emit()
            }
            "tick" => json!({"state":"waiting"}),
            "rx" if self.started => {
                let Some(input) = event["bytes"].as_array() else {
                    return self.fail("Missing response bytes");
                };
                let bytes: Option<Vec<u8>> = input
                    .iter()
                    .map(|v| v.as_u64().and_then(|n| u8::try_from(n).ok()))
                    .collect();
                let Some(bytes) = bytes else {
                    return self.fail("Invalid response byte");
                };
                if let Err(error) = self.prompt.feed(&bytes) {
                    return self.fail(error);
                }
                if !self.prompt.done {
                    return json!({"state":"waiting"});
                }
                let raw = String::from_utf8_lossy(&self.prompt.bytes);
                let reply = match checked(self.commands[self.index], &raw) {
                    Ok(reply) => reply,
                    Err(error) => return self.fail(error),
                };
                if let Err(error) = self.validate(&reply) {
                    return self.fail(error);
                }
                let completed = json!({"command":self.commands[self.index], "reply":reply});
                self.index += 1;
                if self.index == self.commands.len() {
                    if let Some(error) = self.cleanup_error.clone() {
                        let result = json!({"state":"failed","error":error,"automatic_replay":false,"diagnostic_stop_attempted":true});
                        self.terminal = Some(result.clone());
                        return result;
                    }
                    let mut display = if self.read_vin { format!("{} · 500 kbit/s\nVIN: {}\n", if self.mode == "vin" { "Vehicle identification · HS-CAN" } else { "HS-CAN ECM probe" }, self.vin) } else { "Bluetooth adapter identification\n".to_owned() };
                    display += &format!("Adapter: {}\n", self.model.unwrap().label());
                    if self.mode == "ecm_info" {
                        for (_, _, label) in INFO_READS {
                            if let Some(v) = self.ecm.get(label) {
                                display +=
                                    &format!("{label}: {}\n", v["value"].as_str().unwrap_or(""));
                            }
                        }
                    }
                    if let Some(dtc) = &self.dtc {
                        display += "Engine codes · current/history · complete report\n";
                        let records = dtc["records"].as_array().unwrap();
                        if records.is_empty() {
                            display += "No codes reported for mask 12\n";
                        }
                        for d in records {
                            display += &format!(
                                "{} · failure {} · status {}\n",
                                d["code"].as_str().unwrap(),
                                d["failure_type"].as_str().unwrap(),
                                d["status"].as_str().unwrap()
                            );
                        }
                        display += "Codes not cleared · ECM only\n";
                    }
                    display += &format!(
                        "Adapter FW: {}\nAdapter voltage: {}\nAdapter connection closed",
                        self.firmware, self.voltage
                    );
                    let result = json!({"state":"complete", "completed":completed,
                        "result":{"mode":self.mode,"ecm_identifiers":self.ecm,"dtc_report":self.dtc,"display":display,"diagnostic_session_stopped":!self.diagnostic_active,"adapter_model":self.model.unwrap().id(),"adapter_label":self.model.unwrap().label(),"command_family":if self.model == Some(crate::adapter_identity::Model::ObdlinkMxp) {"STN"} else {"MIC"},"profile_id":if self.mode == "vin" { self.model.unwrap().vin_profile_id() } else { self.model.unwrap().probe_profile_id(self.mode == "ecm_dtc") },"product":self.product,"chip_firmware":self.firmware,"voltage":self.voltage,
                            "vin":if self.read_vin {Some(&self.vin)} else {None},
                            "request_origin":self.origin, "firmware_emulation":false,
                            "flow_control_owner":if self.read_vin {"adapter"} else {"none"}}});
                    self.terminal = Some(result.clone());
                    result
                } else {
                    let mut result = self.emit();
                    result["completed"] = completed;
                    result
                }
            }
            _ => self.fail("Unexpected workflow event"),
        }
    }
}

fn c_json(value: Value) -> *mut c_char {
    CString::new(value.to_string())
        .map(CString::into_raw)
        .unwrap_or(std::ptr::null_mut())
}
#[no_mangle]
pub extern "C" fn opensaab_vlinker_create(read_vin: i32) -> *mut Session {
    Box::into_raw(Box::new(Session::new(read_vin != 0).owned()))
}
#[no_mangle]
pub extern "C" fn opensaab_vlinker_create_startup() -> *mut Session {
    Box::into_raw(Box::new(Session::startup().owned()))
}
#[no_mangle]
pub extern "C" fn opensaab_vlinker_create_probe(codes: i32) -> *mut Session {
    Box::into_raw(Box::new(Session::probe(codes != 0).owned()))
}
/// Calls for one session must remain on one serial queue. Returned strings are caller-owned.
#[no_mangle]
pub unsafe extern "C" fn opensaab_vlinker_event(
    session: *mut Session,
    input: *const c_char,
) -> *mut c_char {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let (Some(session), false) = (session.as_mut(), input.is_null()) else {
            return json!({"state":"failed","error":"Missing workflow handle/input"});
        };
        let input = CStr::from_ptr(input).to_bytes();
        if input.len() > 131072 {
            return session.fail("Workflow event exceeds limit");
        }
        match serde_json::from_slice(input) {
            Ok(event) => session.event(&event),
            Err(_) => session.fail("Invalid workflow JSON"),
        }
    }));
    c_json(result.unwrap_or_else(|_| json!({"state":"failed","error":"Workflow failed safely"})))
}
#[no_mangle]
pub unsafe extern "C" fn opensaab_vlinker_destroy(session: *mut Session) {
    if !session.is_null() {
        drop(Box::from_raw(session));
    }
}
#[no_mangle]
pub unsafe extern "C" fn opensaab_vlinker_free_string(value: *mut c_char) {
    if !value.is_null() {
        drop(CString::from_raw(value));
    }
}
#[no_mangle]
pub unsafe extern "C" fn opensaab_vlinker_allowed(command: *const c_char) -> i32 {
    if command.is_null() {
        return 0;
    }
    CStr::from_ptr(command)
        .to_str()
        .map(|s| i32::from(allowed(s)))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    const VIN: &str =
        "7E8 10 13 5A 90 59 53 33 46\r7E8 21 44 34 39 59 58 34 31\r7E8 22 30 30 30 30 30 31 00\r";
    fn reply(command: &str) -> String {
        match command {
            "ATZ" => "ATZ\rELM327 v2.2\r",
            "ATE0" => "ATE0\rOK\r",
            "VTI" => "vLinker MC v2.2.92\r",
            "VTVERS" => "MIC3313 V2.0.92\r",
            "ATRV" => "12.1V\r",
            "VTPRON" => "6 [AT]\r",
            "VTPBRD" => "500000\r",
            VIN_REQUEST => VIN,
            _ => "OK\r",
        }
        .into()
    }
    fn feed(session: &mut Session, text: &str) -> Value {
        session.event(&json!({"op":"rx","bytes":text.as_bytes()}))
    }
    #[test]
    fn live_fixture_sequence_and_fragmentation() {
        for read_vin in [false, true] {
            let mut session = Session::new(read_vin);
            let mut event = session.event(&json!({"op":"start"}));
            let mut sent = Vec::new();
            while event["state"] == "send" {
                let command = event["command"].as_str().unwrap().to_owned();
                sent.push(command.clone());
                for byte in reply(&command).bytes() {
                    assert_eq!(
                        session.event(&json!({"op":"rx","bytes":[byte]}))["state"],
                        "waiting"
                    );
                }
                event = feed(&mut session, ">\r\n");
            }
            assert_eq!(event["state"], "complete");
            assert_eq!(event["result"]["chip_firmware"], "MIC3313 V2.0.92");
            assert_eq!(
                event["result"]["vin"],
                if read_vin {
                    json!("YS3FD49YX41000001")
                } else {
                    Value::Null
                }
            );
            assert_eq!(sent, COMMANDS[..if read_vin { 13 } else { 5 }]);
            assert_eq!(
                sent.iter().filter(|c| *c == VIN_REQUEST).count(),
                usize::from(read_vin)
            );
        }
    }
    #[test]
    fn strict_vin_and_negative_responses() {
        assert_eq!(parse_vin(VIN).unwrap(), "YS3FD49YX41000001");
        assert!(parse_vin(&VIN.replace(' ', "")).is_ok());
        assert!(parse_vin(&format!(
            "7E8 03 7F 1A 78 00 00 00 00\r7E0 30 00 00 00 00 00 00 00\r{VIN}"
        ))
        .is_ok());
        for reply in [
            VIN.replace("7E8 21", "7E8 22"),
            VIN.repeat(2),
            VIN.replace("7E8", "7E9"),
            VIN.replace("5A 90", "5A 91"),
            VIN.replace("59 53 33", "49 53 33"),
            "7E8 03 7F 1A 31 00 00 00 00".into(),
            VIN.lines().next().unwrap_or("").chars().take(28).collect(),
            VIN_REQUEST.into(),
            "NO DATA".into(),
        ] {
            assert!(parse_vin(&reply).is_err(), "{reply}");
        }
    }
    #[test]
    fn failures_stop_without_vehicle_request_or_replay() {
        for bad_command in ["ATSP6", "VTPBRD", VIN_REQUEST] {
            let mut session = Session::new(true);
            let mut event = session.event(&json!({"op":"start"}));
            let mut sent = Vec::new();
            while event["state"] == "send" {
                let command = event["command"].as_str().unwrap().to_owned();
                sent.push(command.clone());
                event = feed(
                    &mut session,
                    &(if command == bad_command {
                        "NO DATA".into()
                    } else {
                        reply(&command)
                    } + ">"),
                );
            }
            assert_eq!(event["state"], "failed");
            assert_eq!(
                sent.iter().filter(|c| *c == VIN_REQUEST).count(),
                usize::from(bad_command == VIN_REQUEST)
            );
            assert_eq!(
                feed(&mut session, &(VIN.to_owned() + ">"))["state"],
                "failed"
            );
        }
    }
    #[test]
    fn bounds_cancel_and_late_fragments() {
        for text in ["OK>>".to_string(), "A".repeat(16385)] {
            let mut session = Session::new(true);
            session.event(&json!({"op":"start"}));
            assert_eq!(feed(&mut session, &text)["state"], "failed");
        }
        let mut session = Session::new(true);
        session.event(&json!({"op":"start"}));
        assert_eq!(
            session.event(&json!({"op":"rx","bytes":[255]}))["state"],
            "failed"
        );
        let mut session = Session::new(true);
        session.event(&json!({"op":"start"}));
        session.command_at -= Duration::from_secs(4);
        assert_eq!(session.event(&json!({"op":"tick"}))["state"], "failed");
        let mut session = Session::new(true);
        session.event(&json!({"op":"start"}));
        assert_eq!(session.event(&json!({"op":"cancel"}))["state"], "failed");
        assert_eq!(feed(&mut session, "OK>")["state"], "failed");
        for command in ["04", "2701", "VTP101"] {
            assert!(!allowed(command));
        }
    }
    #[test]
    fn cleanup_failure_withholds_vin_and_cancellation_never_replays() {
        for cancel in [false, true] {
            let mut session = Session::new(true);
            let mut event = session.event(&json!({"op":"start"}));
            let mut requests = 0;
            while event["state"] == "send" {
                let command = event["command"].as_str().unwrap().to_owned();
                if command == VIN_REQUEST {
                    requests += 1;
                }
                if cancel && command == VIN_REQUEST {
                    event = session.event(&json!({"op":"cancel"}));
                    break;
                }
                let response = if command == "VTPC" {
                    "?".to_owned()
                } else {
                    reply(&command)
                };
                event = feed(&mut session, &(response + ">"));
            }
            assert_eq!(requests, 1);
            assert_eq!(event["state"], "failed");
            assert!(event.get("result").is_none());
            assert_eq!(session.event(&json!({"op":"tick"}))["state"], "failed");
        }
    }
    #[test]
    fn evo_identity_is_distinct_and_unqualified_profiles_send_no_vehicle_request() {
        for (read_vin, probe) in [(false, false), (true, false), (true, true)] {
            let mut session = if probe { Session::probe(false) } else { Session::new(read_vin) };
            let mut event = session.event(&json!({"op":"start"}));
            let mut commands = Vec::new();
            while event["state"] == "send" {
                let command = event["command"].as_str().unwrap().to_owned();
                commands.push(command.clone());
                let response = if command == "VTI" { "Carista EVO v2.3.0".to_owned() }
                    else if command == "VTVERS" { "Unknown firmware".to_owned() } else { reply(&command) };
                event = feed(&mut session, &(response + ">"));
            }
            assert!(!commands.contains(&"ATSP6".to_owned()));
            assert!(!commands.iter().any(|c| vehicle_command(c)));
            if read_vin { assert_eq!(event["state"], "failed"); }
            else {
                assert_eq!(event["state"], "complete");
                assert_eq!(event["result"]["adapter_model"], "carista_evo");
                assert_eq!(event["result"]["adapter_label"], "Carista EVO");
                assert!(event["result"]["vin"].is_null());
            }
        }
    }
    #[test]
    fn evo_vin_profile_preserves_model_and_requires_can_readback() {
        for (baud, expected) in [("500000", "complete"), ("33333", "failed")] {
            let mut session = Session::startup();
            let mut event = session.event(&json!({"op":"start"}));
            let mut commands = Vec::new();
            while event["state"] == "send" {
                let command = event["command"].as_str().unwrap().to_owned();
                commands.push(command.clone());
                let response = match command.as_str() {
                    "VTI" => "Carista EVO v2.2.84".to_owned(),
                    "VTVERS" => "MIC3413 V2.0.84".to_owned(),
                    "VTPBRD" => baud.to_owned(),
                    _ => reply(&command),
                };
                event = feed(&mut session, &(response + ">"));
            }
            assert_eq!(event["state"], expected);
            if expected == "complete" {
                assert_eq!(event["result"]["adapter_model"], "carista_evo");
                assert_eq!(event["result"]["chip_firmware"], "MIC3413 V2.0.84");
                assert_eq!(event["result"]["request_origin"], "host-startup-discovery");
                assert_eq!(commands.iter().filter(|c| c.as_str() == VIN_REQUEST).count(), 1);
                assert_eq!(commands.last().unwrap(), "VTPC");
            } else { assert!(!commands.iter().any(|c| vehicle_command(c))); }
        }
    }
    #[test]
    fn evo_ecm_profiles_validate_firmware_and_bus_before_vehicle_requests() {
        for codes in [false, true] {
            for (firmware, baud, expected) in [
                ("MIC3413 V2.0.84", "500000", "complete"),
                ("MIC3413 V2.1.0", "500000", "failed"),
                ("MIC3313 V2.0.84", "500000", "failed"),
                ("MIC3413 V2.0.84", "33333", "failed"),
            ] {
                let mut session = Session::probe(codes);
                let mut value = session.event(&json!({"op":"start"}));
                let mut sent = Vec::new();
                while value["state"] == "send" {
                    let command = value["command"].as_str().unwrap().to_owned();
                    sent.push(command.clone());
                    let response = match command.as_str() {
                        "VTI" => "Carista EVO v2.2.84".into(),
                        "VTVERS" => firmware.into(),
                        "VTPBRD" => baud.into(),
                        _ => probe_reply(&command),
                    };
                    value = feed(&mut session, &(response + ">"));
                }
                assert_eq!(value["state"], expected, "{value}");
                if expected == "failed" {
                    assert!(!sent.iter().any(|c| vehicle_command(c)));
                    assert!(value.get("result").is_none());
                } else {
                    assert_eq!(value["result"]["adapter_model"], "carista_evo");
                    assert_eq!(value["result"]["profile_id"], crate::adapter_identity::Model::CaristaEvo.probe_profile_id(codes));
                    assert_eq!(sent.iter().filter(|c| c.as_str() == VIN_REQUEST).count(), 1);
                    assert_eq!(sent.last().unwrap(), "VTPC");
                    if codes {
                        assert_eq!(value["result"]["dtc_report"]["records"][0]["code"], "P0107");
                        assert_eq!(value["result"]["diagnostic_session_stopped"], true);
                        assert_eq!(sent.iter().filter(|c| c.as_str() == DTC_READ).count(), 1);
                        assert_eq!(sent.iter().filter(|c| c.as_str() == SESSION_STOP).count(), 1);
                    } else { assert_eq!(value["result"]["ecm_identifiers"].as_object().unwrap().len(), 7); }
                }
            }
        }
    }
    #[test]
    fn identity_validation_and_firmware_versions_are_not_pinned_forever() {
        for (bad_command, bad_reply) in [
            ("VTI", "vLinker FD"),
            ("VTVERS", "MIC3322 V2.3"),
            ("ATRV", "abc"),
        ] {
            let mut session = Session::new(true);
            let mut event = session.event(&json!({"op":"start"}));
            while event["state"] == "send" {
                let command = event["command"].as_str().unwrap().to_owned();
                let text = if command == bad_command {
                    bad_reply.to_owned()
                } else {
                    reply(&command)
                };
                event = feed(&mut session, &(text + ">"));
            }
            assert_eq!(event["state"], "failed");
            assert!(session.index < 5);
        }
        let mut session = Session::new(false);
        let mut event = session.event(&json!({"op":"start"}));
        while event["state"] == "send" {
            let command = event["command"].as_str().unwrap().to_owned();
            event = feed(
                &mut session,
                &(reply(&command)
                    .replace("2.2.92", "2.2.93")
                    .replace("2.0.92", "2.0.93")
                    + ">"),
            );
        }
        assert_eq!(event["state"], "complete");
        let mut session = Session::new(true);
        session.born -= Duration::from_secs(30);
        assert_eq!(session.event(&json!({"op":"tick"}))["state"], "failed");
    }
    fn info_fixture(pid: u8, bytes: &[u8]) -> String {
        let mut payload = vec![0x5a, pid];
        payload.extend(bytes);
        let mut frames = Vec::new();
        if payload.len() <= 7 {
            let mut f = [0u8; 8];
            f[0] = payload.len() as u8;
            f[1..1 + payload.len()].copy_from_slice(&payload);
            frames.push(f);
        } else {
            let mut f = [0u8; 8];
            f[0] = 0x10 | ((payload.len() >> 8) as u8);
            f[1] = payload.len() as u8;
            f[2..].copy_from_slice(&payload[..6]);
            frames.push(f);
            let mut next = 1;
            for chunk in payload[6..].chunks(7) {
                let mut f = [0u8; 8];
                f[0] = 0x20 | next;
                f[1..1 + chunk.len()].copy_from_slice(chunk);
                frames.push(f);
                next = (next + 1) & 15;
            }
        }
        frames
            .iter()
            .map(|f| {
                format!(
                    "7E8 {}\r",
                    f.iter()
                        .map(|b| format!("{b:02X}"))
                        .collect::<Vec<_>>()
                        .join(" ")
                )
            })
            .collect()
    }
    fn probe_reply(command: &str) -> String {
        if let Some((_, pid, _)) = INFO_READS.iter().find(|r| r.0 == command) {
            return info_fixture(
                *pid,
                if *pid == 0x95 {
                    &[0, 11]
                } else if *pid == 0x7c {
                    &[0, 1, 2, 3]
                } else {
                    b"FA5B_C_FMEP_46_FIEF_82d\0"
                },
            );
        }
        match command {
            SESSION_START=>"7E8 01 50 00 00 00 00 00 00\r".into(),
            SESSION_STOP=>"7E8 01 60 00 00 00 00 00 00\r".into(),
            DTC_READ=>"7E8 03 7F A9 78 00 00 00 00\r5E8 81 01 07 00 6F 00 00 00\r5E8 81 00 00 00 FF 00 00 00\r".into(),
            _=>reply(command)
        }
    }
    #[test]
    fn probe_identifiers_preserve_binary_and_exact_isotp_length() {
        for size in [1, 5, 6, 20, 120, 250] {
            let bytes = vec![b'X'; size];
            assert_eq!(identifier(&info_fixture(8, &bytes), 8).unwrap(), bytes);
        }
        let text = info_fixture(8, b"ABCDEF123456");
        for broken in [
            text.replace("7E8 21", "7E8 22"),
            text.repeat(2),
            text.replace("5A 08", "5A 09"),
            text.replace("7E8", "7E9"),
            "7E8 03 7F 1A 31 00 00 00 00".into(),
            "7E8 03 7F 1A 78 00 00 00 00".into(),
        ] {
            assert!(identifier(&broken, 8).is_err());
        }
        assert_eq!(identifier_value(&[0, 11], 0x95)["value"], "Hex: 00 0B");
        assert_eq!(identifier_value(&[0, 1, 2, 3], 0x7c)["value"], "66051");
    }
    #[test]
    fn probe_modes_finish_once_and_never_include_vehicle_writes() {
        for codes in [false, true] {
            let mut session = Session::probe(codes);
            let mut value = session.event(&json!({"op":"start"}));
            let mut sent = Vec::new();
            while value["state"] == "send" {
                let command = value["command"].as_str().unwrap().to_owned();
                assert!(allowed(&command));
                sent.push(command.clone());
                value = feed(&mut session, &(probe_reply(&command) + ">"));
            }
            assert_eq!(value["state"], "complete", "{value}");
            assert_eq!(sent.iter().filter(|c| c.as_str() == VIN_REQUEST).count(), 1);
            for command in sent.iter().filter(|c| vehicle_command(c)) {
                assert_eq!(sent.iter().filter(|c| *c == command).count(), 1);
            }
            assert_eq!(value["result"]["diagnostic_session_stopped"], true);
            if codes {
                assert_eq!(value["result"]["dtc_report"]["records"][0]["code"], "P0107");
                assert_eq!(value["result"]["dtc_report"]["records"][0]["status"], "6F");
            } else {
                assert_eq!(
                    value["result"]["ecm_identifiers"]["ECM software version ID"]["raw_hex"],
                    "00 0B"
                );
            }
            assert_eq!(session.event(&json!({"op":"start"})), value);
        }
        for command in [
            "0104000000000000",
            "2701",
            "0211010000000000",
            "03AE180000000000",
            "021A250000000000",
        ] {
            assert!(!allowed(command));
        }
    }
    #[test]
    fn probe_dtc_requires_end_and_confirmed_stop_then_protocol_close() {
        for broken in [
            "5E8 81 01 07 00 6F 00 00 00",
            "7E8 03 7F A9 78 00 00 00 00",
            "5E8 81 00 00 00 00 00 00 00",
            "5E8 81 01 07 00",
            "NO DATA",
        ] {
            let mut session = Session::probe(true);
            let mut value = session.event(&json!({"op":"start"}));
            let mut sent = Vec::new();
            while value["state"] == "send" {
                let command = value["command"].as_str().unwrap().to_owned();
                sent.push(command.clone());
                value = feed(
                    &mut session,
                    &(if command == DTC_READ {
                        broken.into()
                    } else {
                        probe_reply(&command)
                    } + ">"),
                );
            }
            assert_eq!(value["state"], "failed");
            assert!(value.get("result").is_none());
            assert_eq!(sent.iter().filter(|c| c.as_str() == DTC_READ).count(), 1);
            assert_eq!(
                sent.iter().filter(|c| c.as_str() == SESSION_STOP).count(),
                1
            );
            assert_eq!(sent.last().unwrap(), "VTPC");
        }
        let empty = dtc_report("5E8 81 00 00 00 FF 00 00 00").unwrap();
        assert!(empty["records"].as_array().unwrap().is_empty());
        assert!(dtc_report("5E8 81 00 00 00 FF 00 00 00\n5E8 81 01 07 00 6F 00 00 00").is_err());
    }
    #[test]
    fn probe_failed_stop_is_never_replayed() {
        let mut session = Session::probe(true);
        let mut value = session.event(&json!({"op":"start"}));
        let mut stops = 0;
        while value["state"] == "send" {
            let command = value["command"].as_str().unwrap().to_owned();
            if command == SESSION_STOP {
                stops += 1;
                value = feed(&mut session, "NO DATA>");
            } else {
                value = feed(&mut session, &(probe_reply(&command) + ">"));
            }
        }
        assert_eq!(stops, 1);
        assert_eq!(value["state"], "failed");
        assert!(value.get("result").is_none());
    }
    #[test]
    fn probe_cancel_during_diagnostic_operation_runs_bounded_cleanup() {
        let mut session = Session::probe(true);
        let mut value = session.event(&json!({"op":"start"}));
        while value["command"] != DTC_READ {
            let command = value["command"].as_str().unwrap();
            value = feed(&mut session, &(probe_reply(command) + ">"));
        }
        value = session.event(&json!({"op":"cancel"}));
        assert_eq!(value["command"], SESSION_STOP);
        value = feed(&mut session, "NO DATA>");
        assert_eq!(value["command"], "VTPC");
        value = feed(&mut session, "OK>");
        assert_eq!(value["state"], "failed");
        assert_eq!(feed(&mut session, &(probe_reply(DTC_READ) + ">")), value);
    }
}
