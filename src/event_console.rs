// SPDX-License-Identifier: MPL-2.0
//! Human-readable summaries of important observations, not a wire-protocol decoder.
use crate::logger::LogLevel;

fn field<'a>(detail: &'a str, key: &str) -> &'a str {
    detail
        .split_whitespace()
        .find_map(|part| part.strip_prefix(key))
        .unwrap_or("unknown")
}

pub fn screen_summary(screen: &str) -> String {
    screen
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take(3)
        .collect::<Vec<_>>()
        .join(" / ")
}

pub fn summarize(kind: &str, detail: &str) -> Option<(LogLevel, &'static str, String)> {
    use LogLevel::{Error, Info, Warn};
    let f = |key| field(detail, key);
    Some(match kind {
        "candi_native_link" if detail.starts_with("virtual application inventory") => {
            (Info, "CANDI", detail.to_owned())
        }
        "candi_native_link" if detail.starts_with("CAN ") => (
            if detail.contains("backend absent") || detail.contains("unsupported") {
                Warn
            } else {
                Info
            },
            "CANDI",
            detail.to_owned(),
        ),
        "candi_uart_timer" if detail.starts_with("expired ") => (
            Warn,
            "CANDI",
            "UART reply timeout; guest decides retry or recovery.".into(),
        ),
        "candi_native_link" if detail.starts_with("FRAME ") => (
            if detail.contains("checksum=OK") {
                Info
            } else {
                Warn
            },
            "CANDI",
            detail.trim_start_matches("FRAME ").to_owned(),
        ),
        "candi_native_link" if detail.contains("stopped") || detail.contains("overflow") => {
            (Warn, "CANDI", detail.to_owned())
        }
        "candi_link_status" => (Info, "CANDI", detail.to_owned()),
        "menu_highlight" => (Info, "SELECT", detail.to_owned()),
        "guest_request_contract" => (
            Info,
            "REQUEST",
            format!(
                "Internal request: arg={} key={}",
                f("argument="),
                f("request_key=")
            ),
        ),
        "guest_queue_submit" => (
            Info,
            "QUEUE",
            format!(
                "Internal send: queue={} words={} (not vehicle TX)",
                f("queue="),
                f("envelope_words=")
            ),
        ),
        "guest_queue_submit_return" => (
            Info,
            "QUEUE",
            format!(
                "Submission returned {}. ECU response not established.",
                f("helper_d0=")
            ),
        ),
        "guest_queue_receive_return" if f("envelope_valid=") == "true" => (
            Info,
            "RECV",
            format!("Internal message: {}", f("envelope_words=")),
        ),
        "guest_queue_receive_return" => (
            Warn,
            "RECV",
            format!("No valid response. Queue status={}", f("helper_d0=")),
        ),
        "guest_wait_timeout" => (
            Warn,
            "TIMEOUT",
            "Guest wait expired / receive status 1.".into(),
        ),
        "guest_request_complete" => (
            Info,
            "RESULT",
            format!(
                "Internal status={} result={}",
                f("status_low16="),
                f("descriptor_result=")
            ),
        ),
        "host_wait_start" => (
            Warn,
            "WAIT",
            format!(
                "Guest paused; J2534 is unimplemented. Back in 5s / Esc now. Saved menu: {}",
                f("checkpoint_available=")
            ),
        ),
        "menu_restore" => (
            Info,
            "BACK",
            "Operation cancelled; previous guest menu restored.".into(),
        ),
        "menu_checkpoint_failed" => (
            Warn,
            "RECOVERY",
            "Could not save menu; Restart/Quit remains available.".into(),
        ),
        "host_recovery_action" => (Info, "RECOVERY", detail.into()),
        "guest_assertion" => (Error, "GUEST", detail.into()),
        "host_recovery" => (Info, "RECOVERY", detail.into()),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn console_does_not_call_internal_messages_vehicle_transmissions() {
        let (_, _, message) = summarize(
            "guest_queue_submit",
            "queue=0x80000 envelope_words=[1,7,3,0]",
        )
        .unwrap();
        assert!(message.contains("[1,7,3,0]"));
        assert!(message.contains("not vehicle TX"));
        let (_, _, message) = summarize(
            "guest_queue_receive_return",
            "envelope_valid=false helper_d0=0x1 envelope_words=[stale]",
        )
        .unwrap();
        assert!(!message.contains("stale"));
        assert!(summarize("qspi_transfer", "RTC echo").is_none());
        assert!(summarize("progress", "1000000").is_none());
    }
}
