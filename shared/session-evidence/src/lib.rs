// SPDX-License-Identifier: MPL-2.0
//! Fixed-vocabulary runtime evidence. Never export the source reason or screen.
use serde_json::{json, Map, Value};

pub fn stop_code(status: &str, reason: &str) -> &'static str {
    if status == "output_failure" {
        return "output_failure";
    }
    if reason.starts_with("native CANdi stopped") {
        return "candi_stopped";
    }
    if reason.starts_with("adapter failure:") {
        return "adapter_failure";
    }
    if reason.starts_with("instruction budget exhausted") {
        return "instruction_budget";
    }
    if reason.starts_with("interactive session stopped by operator or deadline") {
        return "operator_or_deadline";
    }
    if reason.starts_with("Native Android session stopped by operator")
        || reason.starts_with("native DTC operator stopped")
    {
        return "operator_stop";
    }
    if reason.starts_with("guest reported processor halt or pSOS bootstrap failure") {
        return "guest_bootstrap_failure";
    }
    if status == "guest_failure" {
        return "guest_cpu_fault";
    }
    if reason.starts_with("Guest reports: No CANDI Communication Established") {
        return "link_unavailable";
    }
    if status == "success" {
        return "completed";
    }
    "unknown"
}

pub fn screen_stage(screen: &str) -> &'static str {
    let s = screen.to_ascii_lowercase();
    if s.contains("no candi communication established") {
        "link_unavailable"
    } else if s.contains("program not found") || s.contains("card not present") {
        "firmware_missing"
    } else if s.contains("checking") && s.contains("working") {
        "vehicle_link_wait"
    } else if s.contains("main menu") && s.contains("f0:") {
        "main_menu"
    } else if s.contains("model year") {
        "model_year"
    } else if s.contains("diagnostic trouble codes") {
        "dtc_menu"
    } else if s.contains("diagnostics") {
        "diagnostics_menu"
    } else {
        "other_unknown"
    }
}

pub fn snapshot(status: &str, reason: &str, screen: &str, candi: Option<&Value>) -> Value {
    let mut out =
        json!({"schema":1,"reason":stop_code(status, reason),"stage":screen_stage(screen)});
    if let Some(raw) = candi.filter(|v| v.is_object()) {
        let r = raw["reason"].as_str().unwrap_or("");
        let code = if r.starts_with("unsupported read ") || r.starts_with("unsupported write ") {
            "unsupported_access"
        } else if r.starts_with("CPU stop:") {
            "cpu_fault"
        } else if r == "instruction budget reached" {
            "instruction_budget"
        } else if r == "host cancelled" {
            "host_cancelled"
        } else if r.is_empty() {
            "none"
        } else {
            "unknown"
        };
        let mut safe = Map::new();
        safe.insert("reason".into(), json!(code));
        if code == "unsupported_access" {
            safe.insert(
                "access".into(),
                json!(if r.starts_with("unsupported write ") {
                    "write"
                } else {
                    "read"
                }),
            );
            for part in r.split_whitespace() {
                if let Some(hex) = part.strip_prefix("address=0x") {
                    if let Ok(address) = u32::from_str_radix(hex, 16) {
                        safe.insert("address".into(), json!(address));
                    }
                }
                if let Some(width) = part
                    .strip_prefix("width=")
                    .and_then(|s| s.parse::<u8>().ok())
                    .filter(|n| [1, 2, 4].contains(n))
                {
                    safe.insert("width".into(), json!(width));
                }
            }
        }
        for key in [
            "attempted",
            "completed",
            "cycles",
            "pc",
            "serial_rx_bytes",
            "serial_rx_breaks",
            "serial_tx_bytes",
            "adapter_tx_confirmations",
        ] {
            if let Some(v) = raw[key].as_u64() {
                safe.insert(key.into(), json!(v));
            }
        }
        for key in ["native_uart", "external_tx"] {
            if let Some(v) = raw[key].as_bool() {
                safe.insert(key.into(), json!(v));
            }
        }
        out["candi"] = Value::Object(safe);
    }
    out
}

/// First two and last ten numerical samples; extrema cover every sampled observation.
#[derive(Default)]
pub struct PerformanceWindow {
    history: std::collections::VecDeque<Value>,
    count: u64,
    max_frame_age: Option<u64>,
    min_ram: Option<u64>,
}
impl PerformanceWindow {
    pub fn push(&mut self, sample: Value) {
        if let Some(v) = sample["frame_age_ms"].as_u64() {
            self.max_frame_age = Some(self.max_frame_age.map_or(v, |n| n.max(v)));
        }
        if let Some(v) = sample["ram_available_kib"].as_u64() {
            self.min_ram = Some(self.min_ram.map_or(v, |n| n.min(v)));
        }
        self.count += 1;
        self.history.push_back(sample);
        if self.history.len() > 12 {
            self.history.remove(2);
        }
    }
    pub fn report(&self, interval: u64, complete: bool) -> Value {
        let mut v = json!({"performance_schema":1,"sample_interval_ms":interval,
            "sample_count":self.count,"complete":complete,"samples":self.history,
            "retained_sample_count":self.history.len(),"samples_truncated":self.count > self.history.len() as u64});
        if let Some(n) = self.max_frame_age {
            v["max_frame_age_ms"] = json!(n);
        }
        if let Some(n) = self.min_ram {
            v["min_ram_available_kib"] = json!(n);
        }
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_performance_preserves_start_end_and_mid_session_extrema() {
        let mut w = PerformanceWindow::default();
        for i in 0..42 {
            w.push(json!({"elapsed_ms":i,"frame_age_ms":if i==10 {9000}else{5},"ram_available_kib":if i==11 {10}else{100}}));
        }
        let v = w.report(5000, true);
        let samples = v["samples"].as_array().unwrap();
        assert_eq!(samples.len(), 12);
        assert_eq!(samples[0]["elapsed_ms"], 0);
        assert_eq!(samples[1]["elapsed_ms"], 1);
        assert_eq!(samples[2]["elapsed_ms"], 32);
        assert_eq!(samples[11]["elapsed_ms"], 41);
        assert_eq!(v["sample_count"], 42);
        assert_eq!(v["samples_truncated"], true);
        assert_eq!(v["max_frame_age_ms"], 9000);
        assert_eq!(v["min_ram_available_kib"], 10);
        assert!(PerformanceWindow::default()
            .report(5000, false)
            .get("min_ram_available_kib")
            .is_none());
    }
    #[test]
    fn incomplete_is_not_a_crash_and_raw_reasons_never_escape() {
        for (reason, expected) in [
            ("native CANdi stopped; secret trace", "candi_stopped"),
            (
                "instruction budget exhausted before requested milestone; waiting for secret",
                "instruction_budget",
            ),
            (
                "interactive session stopped by operator or deadline",
                "operator_or_deadline",
            ),
            ("private arbitrary payload", "unknown"),
        ] {
            let s = snapshot("incomplete", reason, "private screenshot", None);
            assert_eq!(s["reason"], expected);
            assert_eq!(s["stage"], "other_unknown");
            assert!(!s.to_string().contains("private"));
            assert!(!s.to_string().contains("secret"));
        }
        assert_eq!(stop_code("guest_failure", "opaque"), "guest_cpu_fault");
        assert_eq!(
            stop_code(
                "output_failure",
                "native CANdi stopped; failed capture /private"
            ),
            "output_failure"
        );
    }
    #[test]
    fn candi_missing_emulated_register_is_distinct_from_budget_or_cancellation() {
        for (reason, expected) in [
            (
                "unsupported write address=0x100 width=4 value=0xdeadbeef",
                "unsupported_access",
            ),
            ("CPU stop: private registers", "cpu_fault"),
            ("instruction budget reached", "instruction_budget"),
            ("host cancelled", "host_cancelled"),
            ("opaque secret", "unknown"),
        ] {
            let s = snapshot(
                "incomplete",
                "native CANdi stopped",
                "Main Menu F0: Diagnostics",
                Some(
                    &json!({"reason":reason,"pc":20,"completed":42,"external_tx":false,"payload":"SECRET"}),
                ),
            );
            assert_eq!(s["candi"]["reason"], expected);
            assert_eq!(s["candi"]["pc"], 20);
            if expected == "unsupported_access" {
                assert_eq!(s["candi"]["address"], 256);
                assert_eq!(s["candi"]["width"], 4);
                assert_eq!(s["candi"]["access"], "write");
            }
            assert_eq!(s["stage"], "main_menu");
            assert!(!s.to_string().contains("SECRET"));
            assert!(!s.to_string().contains("deadbeef"));
        }
    }
}
