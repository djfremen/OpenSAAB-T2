// SPDX-License-Identifier: MPL-2.0
//! Shared native process outcomes. A clean operator stop is not a diagnostic pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Success,
    Stopped,
    GuestFailure,
    Incomplete,
    OutputFailure,
    CleanupFailure,
}
impl Outcome {
    pub fn code(self) -> u8 {
        match self {
            Self::Success | Self::Stopped => 0,
            Self::GuestFailure => 1,
            Self::Incomplete => 3,
            Self::OutputFailure => 4,
            Self::CleanupFailure => 5,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Stopped => "stopped",
            Self::GuestFailure => "guest_failure",
            Self::Incomplete => "incomplete",
            Self::OutputFailure => "output_failure",
            Self::CleanupFailure => "cleanup_failure",
        }
    }
    pub fn operator_stop(adapter_already_failed: bool) -> Self {
        if adapter_already_failed { Self::Incomplete } else { Self::Stopped }
    }
    pub fn after_cleanup(self, failed: bool) -> Self {
        if failed && matches!(self, Self::Stopped | Self::Success | Self::Incomplete) {
            Self::CleanupFailure
        } else { self }
    }
}
/// Read final native cleanup evidence only after the backend worker is joined.
/// No command is sent and no missing/unknown evidence is promoted to success.
pub fn cleanup_failures(dir: &std::path::Path) -> Vec<&'static str> {
    let read = |name: &str| std::fs::read(dir.join(name)).ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .unwrap_or(serde_json::Value::Null);
    let mut failures = Vec::new();
    let adapter = read("adapter-diagnostics.json");
    for (key, label) in [("protocol_cleanup", "protocol"), ("usb_release", "usb")] {
        if adapter[key] == "failed" { failures.push(label); }
    }
    let mdi = read("mdi-cleanup-private.json");
    if mdi["cleanup_ok"] == false { failures.push("mdi_protocol"); }
    // The local native transcript is bounded by the session's existing capture
    // policy. Stream it rather than loading a full session into memory.
    use std::io::BufRead;
    if let Ok(file) = std::fs::File::open(dir.join("obdlink-native-transcript-private.jsonl")) {
        let mut failed = false;
        for line in std::io::BufReader::new(file).lines().map_while(Result::ok) {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) {
                // Link::record wraps actual observations in `event`. Accept
                // legacy top-level records too, but never let a later confirmed
                // summary erase an explicit failure already observed.
                for event in [&value, &value["event"]] {
                    if let Some(cleanup) = event["cleanup"].as_str() {
                        failed |= cleanup != "confirmed";
                    }
                    if event.get("protocol_cleanup").is_some()
                        || event.get("transport_disposition").is_some() {
                        failed |= event["protocol_cleanup"] != "succeeded"
                            || !matches!(event["transport_disposition"].as_str(), Some("closed" | "parked"))
                            || !event["protocol_cleanup_error"].is_null()
                            || !event["transport_handoff_error"].is_null();
                    }
                }
            }
        }
        if failed {
            failures.push("bluetooth_protocol_or_socket");
        }
    }
    failures
}
#[cfg(test)]
mod tests {
    use super::*;
    static CLEANUP_SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    fn cleanup_directory() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("native-cleanup-proof-{}-{}", std::process::id(),
            CLEANUP_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
        std::fs::create_dir(&dir).unwrap();
        dir
    }
    fn transcript_failures(lines: &str) -> Vec<&'static str> {
        let dir = cleanup_directory();
        std::fs::write(dir.join("obdlink-native-transcript-private.jsonl"), lines).unwrap();
        let failures = cleanup_failures(&dir);
        std::fs::remove_dir_all(dir).unwrap();
        failures
    }
    #[test]
    fn final_failure_reason_has_priority_over_requested_stop() {
        assert_eq!(opensaab_session_evidence::stop_code("cleanup_failure", "Native Android session stopped by operator; native cleanup failed: usb"), "cleanup_failure");
        assert_eq!(opensaab_session_evidence::stop_code("stopped", "Native Android session stopped by operator"), "operator_stop");
    }
    #[test]
    fn stop_never_qualifies_work_and_never_hides_an_existing_failure() {
        assert_eq!(Outcome::operator_stop(false), Outcome::Stopped);
        assert_eq!(Outcome::Stopped.code(), 0);
        assert_ne!(Outcome::Stopped.name(), "success");
        assert_eq!(Outcome::operator_stop(true), Outcome::Incomplete);
        assert_eq!(Outcome::Stopped.after_cleanup(true), Outcome::CleanupFailure);
        assert_ne!(Outcome::CleanupFailure.code(), 0);
        assert_eq!(Outcome::GuestFailure.after_cleanup(true), Outcome::GuestFailure);
        assert_eq!(Outcome::OutputFailure.after_cleanup(true), Outcome::OutputFailure);
    }
    #[test]
    fn final_cleanup_failure_is_read_after_join_and_unknown_stays_unknown() {
        let dir = cleanup_directory();
        assert!(cleanup_failures(&dir).is_empty());
        std::fs::write(dir.join("adapter-diagnostics.json"),r#"{"protocol_cleanup":"unknown","usb_release":"succeeded"}"#).unwrap();
        assert!(cleanup_failures(&dir).is_empty());
        std::fs::write(dir.join("adapter-diagnostics.json"),r#"{"protocol_cleanup":"failed","usb_release":"failed"}"#).unwrap();
        std::fs::write(dir.join("obdlink-native-transcript-private.jsonl"),"{\"cleanup\":\"unconfirmed: missing prompt\"}\n").unwrap();
        assert_eq!(cleanup_failures(&dir), vec!["protocol","usb","bluetooth_protocol_or_socket"]);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn nested_native_cleanup_failure_cannot_be_hidden_by_a_later_summary() {
        assert_eq!(transcript_failures(concat!(
            "{\"elapsed_ms\":1,\"event\":{\"cleanup\":\"unconfirmed: missing prompt\"}}\n",
            "{\"cleanup\":\"confirmed\"}\n"
        )), vec!["bluetooth_protocol_or_socket"]);
    }
    #[test]
    fn structured_protocol_and_transport_failures_override_confirmed_text() {
        for record in [
            r#"{"event":{"cleanup":"confirmed","protocol_cleanup":"failed","transport_disposition":"closed"}}"#,
            r#"{"event":{"cleanup":"confirmed","protocol_cleanup":"succeeded","transport_disposition":"unknown"}}"#,
            r#"{"event":{"cleanup":"confirmed","protocol_cleanup":"succeeded","transport_disposition":"closed","transport_handoff_error":"ambiguous finish acknowledgement"}}"#,
            r#"{"event":{"cleanup":"confirmed","protocol_cleanup":"succeeded","transport_disposition":"parked","protocol_cleanup_error":"STPC rejected"}}"#,
            r#"{"event":{"cleanup":"confirmed","transport_disposition":"parked"}}"#,
        ] {
            assert_eq!(transcript_failures(record), vec!["bluetooth_protocol_or_socket"], "{record}");
        }
    }
    #[test]
    fn verified_parking_is_protocol_cleanup_only_and_does_not_qualify_work() {
        for disposition in ["closed", "parked"] {
            assert!(transcript_failures(&serde_json::json!({"event":{
                "cleanup":"confirmed", "protocol_cleanup":"succeeded", "transport_disposition":disposition,
                "protocol_cleanup_error":null, "transport_handoff_error":null
            }}).to_string()).is_empty());
        }
        assert_eq!(Outcome::GuestFailure.after_cleanup(false), Outcome::GuestFailure);
        assert_eq!(Outcome::Stopped.after_cleanup(false), Outcome::Stopped);
        assert_ne!(Outcome::Stopped.name(), "success");
    }
}

