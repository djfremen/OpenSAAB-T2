// SPDX-License-Identifier: MPL-2.0
//! Chipsoft's VIN-first startup coordinator, with adapter-specific discovery kept separate.
//! Reference: Android ChipsoftUsbActivity, VehicleSession, VehicleIdentity and ConnectionAttempt.
pub use crate::ownership::active;
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

mod f {
    use serde_json::Value;
    use std::{
        path::Path,
        sync::atomic::{AtomicU64, Ordering},
    };
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    pub fn utc() -> String {
        time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_default()
    }
    pub fn unique() -> String {
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let raw = format!(
            "{:032x}",
            n.wrapping_shl(32)
                | ((std::process::id() as u128) << 16)
                | SEQUENCE.fetch_add(1, Ordering::SeqCst) as u128
        );
        format!(
            "{}-{}-{}-{}-{}",
            &raw[..8],
            &raw[8..12],
            &raw[12..16],
            &raw[16..20],
            &raw[20..]
        )
    }
    pub fn write_json(path: &Path, value: &Value) -> Result<(), String> {
        let directory = path.parent().ok_or("Missing connection record directory")?;
        std::fs::create_dir_all(directory).map_err(|e| e.to_string())?;
        let temp = directory.join(format!(
            ".{}-{}.tmp",
            path.file_name().unwrap().to_string_lossy(),
            unique()
        ));
        std::fs::write(
            &temp,
            serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        std::fs::rename(&temp, path).map_err(|e| e.to_string())
    }
    #[cfg(test)]
    pub fn read_json(path: &Path) -> Result<Value, String> {
        serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())
    }
}

const ENDPOINT: &str = "https://relevant-diann-djfremen2-c013cdc3.koyeb.app/api/lookup/";
fn text(value: &Value, key: &str) -> String {
    value[key]
        .as_str()
        .unwrap_or("")
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .trim()
        .chars()
        .take(160)
        .collect()
}
fn valid_vin(vin: &str) -> bool {
    vin.len() == 17
        && vin.starts_with("YS3")
        && vin
            .bytes()
            .all(|b| b.is_ascii_digit() || b.is_ascii_uppercase() && !b"IOQ".contains(&b))
}
pub fn from_probe(adapter: &str, report: &Value) -> Result<Value, String> {
    let (closed, origin, vin) = match adapter {
        "vcx_nano_bridge" => {
            if report["channel_cleanup"] != true || report["hardware"] != "VCX-NANO"
                || report["origin"] != "macos-native-cdc-serial"
                || report["vehicle_tx"] != 2 || report["host_vehicle_requests_written"] != 1
                || report["host_vehicle_request_attempts"] != 1 || report["automatic_replay"] != false {
                return Err("Nano discovery or channel cleanup was not verified".into());
            }
            (report["usb_closed"] == true, &report["request_origin"], &report["vin"])
        }
        "chipsoft" => (
            report["usb_closed"] == true,
            &report["request_origin"],
            &report["vin"],
        ),
        "vlinker_mc_plus" | "bluetooth_mic" => {
            if report["host_vehicle_request_attempts"] != 1
                || report["host_vehicle_requests_written"] != 1
                || report["automatic_replay"] != false
            {
                return Err("Vehicle discovery did not complete exactly one request".into());
            }
            (
                report["bluetooth_closed"] == true,
                &report["result"]["request_origin"],
                &report["result"]["vin"],
            )
        }
        _ => return Err("Adapter startup transport is unavailable".into()),
    };
    if !closed || report["status"] != "vin_received" || origin != "host-startup-discovery" {
        return Err("No verified fresh VIN reply with confirmed transport release".into());
    }
    let vin = vin
        .as_str()
        .filter(|v| valid_vin(v))
        .ok_or("Expected a complete Saab VIN from the vehicle")?;
    Ok(
        json!({"schema":1,"vin":vin,"observed_utc":f::utc(),"source":if adapter=="chipsoft" {"Chipsoft P-bus ECU VIN reply"}else if adapter=="vcx_nano_bridge" {"Nano USB bridge P-bus ECU VIN reply"}else{"Bluetooth P-bus ECU VIN reply"},
        "adapter_model":report["result"]["adapter_model"],"adapter_label":report["result"]["adapter_label"],"adapter_profile":report["result"]["profile_id"],"adapter_product":report["result"]["product"],"adapter_firmware":report["result"]["chip_firmware"],"adapter_voltage":report["result"]["voltage"],"model_year":crate::vin_probe::saab_tech2_year(vin).unwrap_or(0),"lookup_status":"pending","platform":"","engine":"","color":"","gearbox":"","body":""}),
    )
}
pub fn apply_lookup(vehicle: &Value, payload: &Value) -> Result<Value, String> {
    if payload["vin"] != vehicle["vin"] {
        return Err("Vehicle lookup returned a different VIN".into());
    }
    let mut result = vehicle.clone();
    if payload["success"] != true {
        result["lookup_status"] = json!("not_found");
        return Ok(result);
    }
    let car = payload["vehicle"]
        .as_object()
        .ok_or("Missing vehicle lookup fields")?;
    let car = Value::Object(car.clone());
    let year = car["model_year"]
        .as_i64()
        .unwrap_or(vehicle["model_year"].as_i64().unwrap_or(0));
    if !(0..=9999).contains(&year)
        || vehicle["model_year"].as_i64().unwrap_or(0) > 0
            && car["model_year"].as_i64().unwrap_or(year)
                != vehicle["model_year"].as_i64().unwrap_or(0)
    {
        return Err("Vehicle lookup returned a different model year".into());
    }
    result["model_year"] = json!(year);
    result["lookup_status"] = json!("available");
    for (destination, key) in [
        ("platform", "platform"),
        ("engine", "engine"),
        ("gearbox", "gearbox"),
        ("body", "body_display"),
    ] {
        result[destination] = json!(text(&car, key));
    }
    result["color"] = json!(text(&car["color"], "english"));
    Ok(result)
}
fn description(v: &Value) -> String {
    let mut parts = Vec::new();
    if v["model_year"].as_u64().unwrap_or(0) > 0 {
        parts.push(v["model_year"].to_string());
    }
    for key in ["platform", "engine", "color"] {
        if let Some(s) = v[key].as_str().filter(|s| !s.is_empty()) {
            parts.push(s.to_owned());
        }
    }
    if parts.is_empty() {
        "Vehicle identified".into()
    } else {
        parts.join(" · ")
    }
}

pub struct Session {
    root: PathBuf,
    id: String,
    born: Instant,
    created_utc: String,
    stopping_since: Option<Instant>,
    timed_out: bool,
    phase: &'static str,
    adapter: String,
    vehicle: Value,
    message: String,
    lease: Option<crate::ownership::Lease>,
    steps: Vec<Value>,
}
impl Session {
    pub fn root_path(&self) -> String { self.root.to_string_lossy().into_owned() }
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            id: f::unique(),
            born: Instant::now(),
            created_utc: f::utc(),
            stopping_since: None,
            timed_out: false,
            phase: "idle",
            adapter: String::new(),
            vehicle: Value::Null,
            message: String::new(),
            lease: None,
            steps: Vec::new(),
        }
    }
    fn release(&mut self) {
        self.lease = None;
    }
    fn busy(&self) -> bool {
        ["selecting", "discovering", "lookup", "stopping"].contains(&self.phase)
    }
    fn record(&mut self, stage: &str, outcome: &str, reason: &str) {
        if self.steps.len() < 24 {
            self.steps
                .push(json!({"stage":stage,"elapsed_ms":self.born.elapsed().as_millis()}));
        }
        // Connection/support records contain typed progress only, never VIN or payloads.
        let _ = f::write_json(
            &self
                .root
                .join("connection-attempts")
                .join(format!("{}.json", self.id)),
            &json!({"format":1,"attempt_id":self.id,"created_utc":self.created_utc,"adapter":if self.adapter=="chipsoft"{"CHIPSOFT"}else if self.adapter=="vcx_nano_bridge"{"NANO_BRIDGE"}else{"VLINKER"},"stage":stage,"outcome":outcome,"reason":reason,"elapsed_ms":self.born.elapsed().as_millis(),"stages":self.steps}),
        );
        if let Ok(entries) = std::fs::read_dir(self.root.join("connection-attempts")) {
            let mut files: Vec<_> = entries
                .flatten()
                .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("json"))
                .collect();
            files.sort_by_key(|e| e.file_name());
            for old in files.iter().take(files.len().saturating_sub(20)) {
                let _ = std::fs::remove_file(old.path());
            }
        }
    }
    fn snapshot(&self, effect: &str) -> Value {
        json!({"schema":1,"state":self.phase,"busy":self.busy(),"effect":effect,"attempt_id":self.id,"adapter":self.adapter,"vehicle":self.vehicle,
            "description":if self.vehicle.is_null(){String::new()}else{description(&self.vehicle)},"message":self.message,
            "vehicle_connected":false,"firmware_running":false,"firmware_transport_available":false,"automatic_replay":false})
    }
    fn fail(&mut self, message: impl Into<String>, reason: &str) -> Value {
        self.phase = "failed";
        self.vehicle = Value::Null;
        self.message = message.into();
        self.record("CLEANUP", "FAILED", reason);
        self.release();
        self.snapshot("none")
    }
    pub fn event(&mut self, event: &Value) -> Value {
        let op = event["op"].as_str().unwrap_or("");
        if self.phase == "stopping" {
            if op == "transport_closed" {
                if (self.adapter == "vcx_nano_bridge" && event["usb_closed"] != true) || (self.adapter != "vcx_nano_bridge" && event["bluetooth_closed"] != true) {
                    return self.fail(
                        "Connection stopped · Adapter release was not confirmed",
                        "CLEANUP_FAILED",
                    );
                }
                if self.timed_out {
                    return self.fail(
                        "Vehicle connection timed out; reconnect for a fresh VIN",
                        "TIMEOUT",
                    );
                }
                self.phase = "cancelled";
                self.message = "Connection stopped · No vehicle connection".into();
                self.record("CLEANUP", "CANCELLED", "USER_STOP");
                self.release();
            } else if self
                .stopping_since
                .is_some_and(|t| t.elapsed() > Duration::from_secs(if self.adapter == "vcx_nano_bridge" {10} else {6}))
            {
                return self.fail(
                    "Connection stopped · Adapter release was not confirmed",
                    "CLEANUP_FAILED",
                );
            }
            return self.snapshot("none");
        }
        if op == "cancel" {
            if self.phase == "discovering" {
                return self.stop_transport(false);
            }
            self.phase = "cancelled";
            self.vehicle = Value::Null;
            self.message = "Connection stopped · No vehicle connection".into();
            self.record("CLEANUP", "CANCELLED", "USER_STOP");
            self.release();
            return self.snapshot("cancel_transport");
        }
        if self.busy() && self.born.elapsed() > Duration::from_secs(45) {
            if self.phase == "discovering" {
                return self.stop_transport(true);
            }
            return self.fail(
                "Vehicle connection timed out; reconnect for a fresh VIN",
                "TIMEOUT",
            );
        }
        match (self.phase, op) {
            ("idle", "begin") => {
                if event["firmware_ready"] != true {
                    self.phase = "setup_required";
                    self.message = "Firmware setup is required before connecting".into();
                    return self.snapshot("setup");
                }
                self.lease = if event["other_workflow_running"] == true { None } else { crate::ownership::Lease::acquire(&self.id) };
                if self.lease.is_none() {
                    return self.fail("Finish the active workflow before connecting", "IO_ERROR");
                }
                self.phase = "selecting";
                self.message = "Select adapter".into();
                self.record("DETECTION", "IN_PROGRESS", "NONE");
                self.snapshot("select_adapter")
            }
            ("selecting", "select_adapter") => {
                let adapter = event["adapter"].as_str().unwrap_or("");
                if !["vlinker_mc_plus", "bluetooth_mic", "vcx_nano_bridge"].contains(&adapter) || event["transport_available"] != true {
                    return self.fail(
                        "No supported adapter connection is available in this build",
                        "NO_ADAPTER",
                    );
                }
                self.adapter = adapter.into();
                self.phase = "discovering";
                self.message = "Opening selected adapter · Reading a fresh vehicle VIN…".into();
                self.record("VIN_REQUEST", "IN_PROGRESS", "NONE");
                self.snapshot("discover_vin")
            }
            ("discovering", "probe_result") => {
                let report = &event["report"];
                if self.adapter == "vcx_nano_bridge" && report["attempt_id"] != self.id {
                    return self.fail("Bridge reply belongs to another connection attempt", "VIN_UNAVAILABLE");
                }
                match from_probe(&self.adapter, report) {
                    Ok(vehicle) => {
                        self.vehicle = vehicle;
                        self.phase = "lookup";
                        self.message =
                            "Vehicle identified · Loading OpenSAAB vehicle details…".into();
                        self.record("CLEANUP", "IN_PROGRESS", "NONE");
                        let mut result = self.snapshot("lookup");
                        result["url"]=json!(format!("{ENDPOINT}{}?include_rpo=true&include_options=false&include_recalls=false",self.vehicle["vin"].as_str().unwrap()));
                        result["maximum_bytes"] = json!(256 * 1024);
                        result["timeout_seconds"] = json!(10);
                        result
                    }
                    Err(e) => self.fail(
                        format!("Could not identify vehicle · {e}"),
                        "VIN_UNAVAILABLE",
                    ),
                }
            }
            ("lookup", "lookup_result") | ("lookup", "lookup_unavailable") => {
                let updated = if op == "lookup_result" && event["http_status"] == 200 {
                    event["body"]
                        .as_str()
                        .filter(|s| s.len() <= 256 * 1024)
                        .and_then(|s| serde_json::from_str::<Value>(s).ok())
                        .and_then(|p| apply_lookup(&self.vehicle, &p).ok())
                } else {
                    None
                };
                if let Some(vehicle) = updated {
                    self.vehicle = vehicle;
                } else {
                    self.vehicle["lookup_status"] = json!("unavailable");
                }
                let run = self.root.join("connection-startups").join(&self.id);
                if let Err(e) = f::write_json(&run.join("vehicle-session.json"), &self.vehicle)
                    .and_then(|_| {
                        f::write_json(&self.root.join("last-vehicle.json"), &self.vehicle)
                    })
                {
                    return self.fail(
                        format!("Vehicle identity could not be saved · {e}"),
                        "IO_ERROR",
                    );
                }
                self.phase = "identified";
                self.message=if self.adapter == "vcx_nano_bridge" {"Vehicle identified · USB bridge firmware transport is not implemented in this build"}else{"Vehicle identified · Bluetooth firmware transport is not implemented in this build"}.into();
                self.record("FIRMWARE_START", "FAILED", "PROTOCOL_OR_PROCESS_ERROR");
                let result = self.snapshot("none");
                let _ = f::write_json(&run.join("startup-result.json"), &result);
                self.release();
                result
            }
            (_, "tick") => self.snapshot("none"),
            _ => self.snapshot("none"), // Ignore old callbacks; never start another probe automatically.
        }
    }
    fn stop_transport(&mut self, timed_out: bool) -> Value {
        self.phase = "stopping";
        self.stopping_since = Some(Instant::now());
        self.timed_out = timed_out;
        self.vehicle = Value::Null;
        self.message = "Stopping connection · Waiting for adapter release…".into();
        self.record(
            "CLEANUP",
            "IN_PROGRESS",
            if timed_out { "TIMEOUT" } else { "USER_STOP" },
        );
        self.snapshot("cancel_transport")
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        self.release();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    static OWNERSHIP_TEST: std::sync::Mutex<()> = std::sync::Mutex::new(());
    #[test]
    fn startup_and_probe_share_owner_before_transport_and_through_cleanup() {
        let _serial = OWNERSHIP_TEST.lock().unwrap();
        let mut startup = session();
        begin(&mut startup);
        let mut probe = crate::vlinker::Session::probe(true).owned();
        assert_eq!(probe.event(&json!({"op":"claim"}))["state"], "failed");
        let mut discovery = crate::vlinker::Session::startup().owned();
        assert_eq!(discovery.event(&json!({"op":"claim","owner":startup.id}))["state"], "idle");
        startup.event(&json!({"op":"cancel"}));
        startup.event(&json!({"op":"transport_closed","bluetooth_closed":true}));
        assert!(active(), "The native transport must retain its lease until released");
        drop(discovery);
        assert!(!active());
        let mut fresh = crate::vlinker::Session::probe(false).owned();
        assert_eq!(fresh.event(&json!({"op":"claim"}))["state"], "idle");
        assert_eq!(fresh.event(&json!({"op":"cancel"}))["state"], "failed");
        assert!(active(), "Cancellation must not let another transport start before native teardown");
        drop(fresh);
        assert!(!active());
        std::fs::remove_dir_all(&startup.root).unwrap();
    }
    #[test]
    fn nano_bridge_requires_fresh_request_and_complete_native_cleanup() {
        let good=json!({"status":"vin_received","usb_closed":true,"channel_cleanup":true,"hardware":"VCX-NANO","origin":"macos-native-cdc-serial","vehicle_tx":2,"host_vehicle_request_attempts":1,"host_vehicle_requests_written":1,"automatic_replay":false,"request_origin":"host-startup-discovery","vin":"YS3FD49YX41000001"});
        assert!(from_probe("vcx_nano_bridge",&good).is_ok());
        for (field,value) in [("usb_closed",json!(false)),("channel_cleanup",json!(false)),("hardware",json!("CHIPSOFT")),("origin",json!("saved")),("vehicle_tx",json!(3)),("host_vehicle_requests_written",json!(0)),("host_vehicle_request_attempts",json!(2)),("automatic_replay",json!(true)),("request_origin",json!("host-transport-probe")),("vin",json!("YS3PARTIAL"))] {
            let mut bad=good.clone();bad[field]=value;assert!(from_probe("vcx_nano_bridge",&bad).is_err(),"{field}");
        }
    }
    fn probe() -> Value {
        json!({"status":"vin_received","bluetooth_closed":true,"automatic_replay":false,"host_vehicle_request_attempts":1,"host_vehicle_requests_written":1,"result":{"request_origin":"host-startup-discovery","vin":"YS3FD49YX41000001"}})
    }
    fn session() -> Session {
        Session::new(std::env::temp_dir().join(format!("opensaab-startup-test-{}", f::unique())))
    }
    fn begin(s: &mut Session) {
        assert_eq!(
            s.event(&json!({"op":"begin","firmware_ready":true}))["effect"],
            "select_adapter"
        );
        assert_eq!(s.event(&json!({"op":"select_adapter","adapter":"vlinker_mc_plus","transport_available":true}))["effect"],"discover_vin");
    }
    #[test]
    fn fresh_vin_requires_release_and_startup_origin() {
        let p = probe();
        let v = from_probe("vlinker_mc_plus", &p).unwrap();
        assert_eq!(v["model_year"], 2004);
        for field in ["bluetooth_closed", "automatic_replay"] {
            let mut bad = p.clone();
            bad[field] = json!(field == "automatic_replay");
            assert!(from_probe("vlinker_mc_plus", &bad).is_err());
        }
        let mut bad = p;
        bad["result"]["request_origin"] = json!("host-bench-qualification");
        assert!(from_probe("vlinker_mc_plus", &bad).is_err());
    }
    #[test]
    fn lookup_matches_vin_year_and_allowlists_fields() {
        let v = from_probe("vlinker_mc_plus", &probe()).unwrap();
        let p = json!({"vin":v["vin"],"success":true,"security_code":"DO_NOT_SAVE","vehicle":{"model_year":2004,"platform":"9440","engine":"B207R","color":{"english":"Silver Metallic"}}});
        let saved = apply_lookup(&v, &p).unwrap();
        assert_eq!(saved["engine"], "B207R");
        assert!(!saved.to_string().contains("DO_NOT_SAVE"));
        let mut bad = p.clone();
        bad["vin"] = json!("YS3FH46U681000002");
        assert!(apply_lookup(&v, &bad).is_err());
        let mut bad = p;
        bad["vehicle"]["model_year"] = json!(2008);
        assert!(apply_lookup(&v, &bad).is_err());
    }
    #[test]
    fn startup_saves_identity_but_never_starts_offline_as_live() {
        let _serial = OWNERSHIP_TEST.lock().unwrap();
        let mut s = session();
        begin(&mut s);
        assert_eq!(
            s.event(&json!({"op":"probe_result","report":probe()}))["effect"],
            "lookup"
        );
        let v = s.event(&json!({"op":"lookup_unavailable"}));
        assert_eq!(v["state"], "identified");
        assert_eq!(v["vehicle"]["vin"], "YS3FD49YX41000001");
        assert_eq!(v["vehicle_connected"], false);
        assert_eq!(v["firmware_running"], false);
        assert_eq!(
            f::read_json(&s.root.join("last-vehicle.json")).unwrap()["model_year"],
            2004
        );
        assert!(!f::read_json(
            &s.root
                .join("connection-attempts")
                .join(format!("{}.json", s.id))
        )
        .unwrap()
        .to_string()
        .contains("YS3"));
        assert_eq!(
            s.event(&json!({"op":"probe_result","report":probe()}))["effect"],
            "none"
        );
        std::fs::remove_dir_all(&s.root).unwrap();
    }
    #[test]
    fn cancel_rejects_late_lookup_and_clears_current_identity() {
        let _serial = OWNERSHIP_TEST.lock().unwrap();
        let mut s = session();
        begin(&mut s);
        s.event(&json!({"op":"probe_result","report":probe()}));
        assert_eq!(s.event(&json!({"op":"cancel"}))["vehicle"], Value::Null);
        s.event(&json!({"op":"lookup_unavailable"}));
        assert!(!s.root.join("last-vehicle.json").exists());
        let mut fresh = Session::new(s.root.clone());
        assert!(fresh.event(&json!({"op":"tick"}))["vehicle"].is_null());
        std::fs::remove_dir_all(&s.root).unwrap();
    }
    #[test]
    fn failed_probe_missing_firmware_and_watchdog_send_no_replay() {
        let _serial = OWNERSHIP_TEST.lock().unwrap();
        let mut s = session();
        assert_eq!(
            s.event(&json!({"op":"begin","firmware_ready":false}))["effect"],
            "setup"
        );
        let mut s = session();
        begin(&mut s);
        let mut p = probe();
        p["bluetooth_closed"] = json!(false);
        assert_eq!(
            s.event(&json!({"op":"probe_result","report":p}))["state"],
            "failed"
        );
        assert!(s.event(&json!({"op":"probe_result","report":probe()}))["vehicle"].is_null());
        let mut s = session();
        begin(&mut s);
        s.born -= Duration::from_secs(46);
        assert_eq!(s.event(&json!({"op":"tick"}))["state"], "stopping");
        assert_eq!(
            s.event(&json!({"op":"transport_closed","bluetooth_closed":true}))["state"],
            "failed"
        );
    }
    #[test]
    fn discovery_cancel_owns_adapter_until_release_and_rejects_late_vin() {
        let _serial = OWNERSHIP_TEST.lock().unwrap();
        let mut s = session();
        begin(&mut s);
        let stopped = s.event(&json!({"op":"cancel"}));
        assert_eq!(stopped["state"], "stopping");
        assert_eq!(stopped["busy"], true);
        assert!(active());
        let mut other = session();
        assert_eq!(
            other.event(&json!({"op":"begin","firmware_ready":true}))["state"],
            "failed"
        );
        assert!(s.event(&json!({"op":"probe_result","report":probe()}))["vehicle"].is_null());
        assert_eq!(
            s.event(&json!({"op":"transport_closed","bluetooth_closed":true}))["state"],
            "cancelled"
        );
        assert!(!active());
        assert!(!s.root.join("last-vehicle.json").exists());
        let mut timeout = session();
        begin(&mut timeout);
        timeout.event(&json!({"op":"cancel"}));
        timeout.stopping_since = Some(Instant::now() - Duration::from_secs(7));
        assert_eq!(timeout.event(&json!({"op":"tick"}))["state"], "failed");
        assert!(!active());
        std::fs::remove_dir_all(&s.root).unwrap();
        std::fs::remove_dir_all(&other.root).unwrap();
        std::fs::remove_dir_all(&timeout.root).unwrap();
    }
}
