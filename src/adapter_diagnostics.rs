// SPDX-License-Identifier: MPL-2.0
//! Passive, bounded adapter evidence. The same source is used by Android and desktop.
//! Identification is observation, never permission to use an unsupported adapter.
use serde_json::{json, Value};
use std::{fs, path::{Path, PathBuf}};

pub fn version(s: &str) -> bool {
    if s.len() > 48 { return false; }
    let base = s.split(['-', '+']).next().unwrap_or("");
    let parts: Vec<_> = base.split('.').collect();
    (2..=4).contains(&parts.len()) && parts.iter().all(|p| !p.is_empty() && p.len() <= 3 && p.bytes().all(|b| b.is_ascii_digit()))
        && s.bytes().all(|b| b.is_ascii_alphanumeric() || b".-+_".contains(&b))
}
fn token(v: &Value, max: usize) -> Option<&str> {
    v.as_str().filter(|s| !s.is_empty() && s.len() <= max && s.bytes().all(|b| b.is_ascii_alphanumeric() || b".-+_".contains(&b)))
}
fn read(dir: &Path, name: &str) -> Value {
    let p = dir.join(name);
    match fs::symlink_metadata(&p) {
        Ok(m) if m.is_file() && m.len() <= 16_384 => fs::read(&p).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or(Value::Null),
        _ => Value::Null,
    }
}
fn label<'a>(v: &'a Value, choices: &[&str]) -> Option<&'a str> { v.as_str().filter(|s| choices.contains(s)) }

pub struct Recorder { path: PathBuf, value: Value }
impl Recorder {
    pub fn new(dir: &Path, family: &str, transport: &str) -> Self {
        Self { path: dir.join("adapter-diagnostics.json"), value: json!({"schema":1,"family":family,"transport":transport,
            "model":null,"firmware_version":null,"hardware_version":null,"driver_version":null,
            "compatibility":"unknown","state":"initializing","stage":"identification","usb_release":"unknown","protocol_cleanup":"unknown"}) }
    }
    pub fn chipsoft_reply(&mut self, bytes: &[u8], accepted: bool) {
        // Only model and version tokens survive; serials and arbitrary reply text never do.
        if let Ok(s) = std::str::from_utf8(bytes) {
            if let Some(rest) = s.strip_prefix("CHIPSOFT J2534 ") {
                if let Some((model, rest)) = rest.split_once(" v. ") {
                    if ["Pro", "Mid", "Lite"].contains(&model) {
                        self.value["model"] = json!(format!("Chipsoft J2534 {model}"));
                        let fw = rest.split_whitespace().next().unwrap_or("").trim_end_matches('\0');
                        if version(fw) { self.value["firmware_version"] = json!(fw); }
                        if let Some(hw)=rest.split("HW: ").nth(1).and_then(|s|s.split_whitespace().next()).filter(|s|s.len()<=8 && s.bytes().all(|b|b.is_ascii_hexdigit())) {self.value["hardware_version"]=json!(hw);}

                    }
                }
            }
        }
        self.value["compatibility"] = json!(if accepted { "accepted" } else { "rejected" });
        self.save();
    }
    pub fn nano_reply(&mut self, firmware: &str) {
        self.value["model"] = json!("VCX Nano");
        if version(firmware) { self.value["firmware_version"] = json!(firmware); }
        self.save();
    }
    pub fn stage(&mut self, stage: &str) { self.value["stage"] = json!(stage); self.save(); }
    pub fn ready(&mut self) {
        self.value["compatibility"] = json!("accepted"); self.value["state"] = json!("ready"); self.stage("session");
    }
    pub fn finish(&mut self, failed: bool, protocol: bool, usb: Option<bool>) {
        self.value["state"] = json!(if failed { if self.value["state"] == "ready" { "session_failed" } else { "initialization_failed" } } else { "stopped" });
        self.value["protocol_cleanup"] = json!(if protocol { "succeeded" } else { "failed" });
        self.value["usb_release"] = json!(match usb { Some(true)=>"succeeded",Some(false)=>"failed",None=>"unknown" });
        self.save();
    }
    fn save(&self) {
        let tmp = self.path.with_extension("json.tmp");
        if fs::write(&tmp, self.value.to_string()).and_then(|_| fs::rename(&tmp, &self.path)).is_err() {
            println!("ADAPTER_DIAGNOSTICS evidence_write=failed");
        }
    }
}

pub fn snapshot(dir: &Path) -> Value {
    let binding = read(dir,"connection-evidence.json");
    let observed = read(dir,"adapter-diagnostics.json");
    let startup = read(dir,"candi-startup.json");
    let mut out = json!({"schema":1,"family":"unknown","model":null,"firmware_version":null,"hardware_version":null,"driver_version":null,
        "transport":"unknown","compatibility":"unknown","state":"unknown","stage":"unknown","usb_release":"unknown","protocol_cleanup":"unknown",
        "app_version":null,"architecture":null,"build":null,"attempt_id":null,"session_mode":"unknown","last_operation":"unknown","failure_kind":"unknown","terminal_pc":null,"process_exit_code":null});
    for (key, choices) in [
        ("family", &["chipsoft","nano","mdi","mongoose","obdlink","vlinker","offline","unknown"][..]),
        ("transport", &["android_usb","usb_cdc","j2534","bluetooth","offline","unknown"][..]),
        ("compatibility", &["accepted","rejected","unknown"][..]),
        ("state", &["initializing","ready","initialization_failed","session_failed","stopped","unknown"][..]),
        ("stage", &["identification","channel_open","session","unknown"][..]),
        ("usb_release", &["succeeded","failed","unknown","not_attempted"][..]),
        ("protocol_cleanup", &["succeeded","failed","unknown","not_attempted"][..])
    ] { if let Some(s)=label(&observed[key],choices).or_else(||label(&binding[key],choices)) { out[key]=json!(s); } }
    for key in ["firmware_version","hardware_version","driver_version"] {
        if let Some(s)=observed[key].as_str().or_else(||binding[key].as_str()).filter(|s|version(s) || key=="hardware_version" && !s.is_empty() && s.len()<=8 && s.bytes().all(|b|b.is_ascii_hexdigit())) {out[key]=json!(s);}
    }
    if let Some(s)=label(&observed["model"],&["Chipsoft J2534 Pro","Chipsoft J2534 Mid","Chipsoft J2534 Lite","VCX Nano","Classic MDI","Mongoose","OBDLink MX+","vLinker"]) {out["model"]=json!(s);}
    for (key,max) in [("app_version",48),("architecture",24),("build",48)] {
        if let Some(s)=token(&binding[key],max) {out[key]=json!(s);}
    }
    if let Some(s)=binding["attempt_id"].as_str().filter(|s|s.len()==36 && s.bytes().enumerate().all(|(i,b)|if [8,13,18,23].contains(&i){b==b'-'}else{b.is_ascii_hexdigit()})) {out["attempt_id"]=json!(s);}
    for (key,choices) in [("session_mode",&["offline","read_only","full_native","security_seeds","unknown"][..]),("last_operation",&["connection","vin","dtc_read","dtc_clear","ecu_info","engine_data","security","original_menus","manual_input","unknown"][..])] {
        if let Some(s)=label(&binding[key],choices) {out[key]=json!(s);}
    }
    if startup["status"]=="failed" {out["state"]=json!("initialization_failed");}
    let report=read(dir,"report.json");
    if let Some(n)=report["pc"].as_u64().filter(|n|*n<=u32::MAX as u64) {out["terminal_pc"]=json!(n);}
    if let Some(n)=binding["cleanup"]["child_exit_code"].as_i64().or_else(||report["exit_code"].as_i64()).filter(|n|i32::try_from(*n).is_ok()) {out["process_exit_code"]=json!(n);}
    let reason=startup["error"].as_str().or_else(||report["reason"].as_str()).unwrap_or("");
    if reason.contains("Unvalidated Chipsoft firmware version") {out["failure_kind"]=json!("adapter_firmware_not_validated");}
    else if let Some(s)=binding["primary_reason"].as_str().filter(|s|["NONE","NO_ADAPTER","MULTIPLE_ADAPTERS","PERMISSION_DENIED","DISCONNECTED","TIMEOUT","IO_ERROR","PROTOCOL_OR_PROCESS_ERROR","VIN_UNAVAILABLE","CLEANUP_FAILED","USER_STOP","SETUP_REQUIRED"].contains(s)) {out["failure_kind"]=json!(s);}
    for key in ["usb_release","protocol_cleanup"] {if let Some(s)=label(&binding["cleanup"][key],&["succeeded","failed","not_attempted"]) {out[key]=json!(s);}}
    if binding["primary_reason"]!="NONE" && binding["primary_reason"]!="USER_STOP" && !binding["primary_reason"].is_null() && out["state"]=="unknown" {out["state"]=json!("session_failed");}
    if observed.is_null() {
        let identity=read(dir,"adapter-identity.json");
        if let Some(s)=identity["firmware_version"].as_str().filter(|s|version(s)) {out["firmware_version"]=json!(s);}
        let result=read(dir,"result.json");
        if let Some(s)=result["firmware"].as_str().filter(|s|version(s)) {out["firmware_version"]=json!(s);}
    }
    out
}
pub fn console(v: &Value) -> String {
    let s=|key:&str|v[key].as_str().unwrap_or("unavailable");
    let pc=v["terminal_pc"].as_u64().map(|n|format!("0x{n:08X}")).unwrap_or("unavailable".into());
    let exit=v["process_exit_code"].as_i64().map(|n|n.to_string()).unwrap_or("unavailable".into());
    format!("CRASH DETAILS\n  App: OpenSAAB {} {} | build {}\n  Connection: {}\n  Adapter: {} | model {}\n  Firmware: {} | hardware {} | driver {}\n  Transport: {} | state {} | stage {}\n  Compatibility: {} | failure {}\n  Mode: {} | operation {}\n  Cleanup: USB {} | protocol {}\n  Terminal PC: {pc} | process exit {exit}",
        s("app_version"),s("architecture"),s("build"),s("attempt_id"),s("family"),s("model"),s("firmware_version"),s("hardware_version"),s("driver_version"),
        s("transport"),s("state"),s("stage"),s("compatibility"),s("failure_kind"),s("session_mode"),s("last_operation"),s("usb_release"),s("protocol_cleanup"))
}

/// Refresh after host cleanup as well, without opening any transport or firmware.
pub fn persist(dir: &Path) -> Result<Value,String> {
    let v=snapshot(dir);
    let path=dir.join("crash-diagnostics.json");
    fs::write(path.with_extension("json.tmp"),v.to_string()).and_then(|_|fs::rename(path.with_extension("json.tmp"),&path)).map_err(|e|e.to_string())?;
    for name in ["report.json","crash.json"] {
        let mut report=read(dir,name);
        if report.is_object() {
            report["crash_diagnostics"]=v.clone();let path=dir.join(name);
            fs::write(path.with_extension("json.tmp"),report.to_string()).and_then(|_|fs::rename(path.with_extension("json.tmp"),&path)).map_err(|e|e.to_string())?;
        }
    }
    Ok(v)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejected_mid_is_retained_without_serial_or_new_capabilities() {
        let dir=std::env::temp_dir().join(format!("adapter-diagnostics-mid-{}",std::process::id()));fs::create_dir_all(&dir).unwrap();
        let mut r=Recorder::new(&dir,"chipsoft","android_usb");
        r.chipsoft_reply(b"CHIPSOFT J2534 Mid v. 1.5.2 HW: 00000003 SN: PRIVATE",false);
        r.finish(true,true,Some(true));let v=snapshot(&dir);
        assert_eq!(v["model"],"Chipsoft J2534 Mid");assert_eq!(v["firmware_version"],"1.5.2");assert_eq!(v["compatibility"],"rejected");
        assert_eq!(v["state"],"initialization_failed");assert_eq!(v["usb_release"],"succeeded");
        assert!(!v.to_string().contains("PRIVATE"));assert_eq!(v["hardware_version"],"00000003");assert!(console(&v).contains("driver unavailable"));fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn unavailable_and_injected_fields_never_become_evidence() {
        let dir=std::env::temp_dir().join(format!("adapter-diagnostics-unknown-{}",std::process::id()));fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("connection-evidence.json"),json!({"app_version":"x\nINJECT","family":"chipsoft","attempt_id":"private","session_mode":"full_native"}).to_string()).unwrap();
        let v=snapshot(&dir);assert!(v["app_version"].is_null());assert!(v["firmware_version"].is_null());assert!(v["attempt_id"].is_null());
        assert_eq!(v["state"],"unknown");assert_eq!(v["family"],"chipsoft");fs::remove_dir_all(dir).unwrap();
    }
}
