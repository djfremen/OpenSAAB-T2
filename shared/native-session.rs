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
        let mut last = None;
        for line in std::io::BufReader::new(file).lines().map_while(Result::ok) {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) {
                if let Some(cleanup) = value["cleanup"].as_str() { last = Some(cleanup.to_owned()); }
            }
        }
        if last.as_deref().is_some_and(|value| value != "confirmed") {
            failures.push("bluetooth_protocol_or_socket");
        }
    }
    failures
}
#[cfg(test)]
mod tests {
    use super::*;
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
        let dir = std::env::temp_dir().join(format!("native-cleanup-proof-{}",std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(cleanup_failures(&dir).is_empty());
        std::fs::write(dir.join("adapter-diagnostics.json"),r#"{"protocol_cleanup":"unknown","usb_release":"succeeded"}"#).unwrap();
        assert!(cleanup_failures(&dir).is_empty());
        std::fs::write(dir.join("adapter-diagnostics.json"),r#"{"protocol_cleanup":"failed","usb_release":"failed"}"#).unwrap();
        std::fs::write(dir.join("obdlink-native-transcript-private.jsonl"),"{\"cleanup\":\"unconfirmed: missing prompt\"}\n").unwrap();
        assert_eq!(cleanup_failures(&dir), vec!["protocol","usb","bluetooth_protocol_or_socket"]);
        std::fs::remove_dir_all(dir).unwrap();
    }
}

