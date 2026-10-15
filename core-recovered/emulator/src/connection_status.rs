// SPDX-License-Identifier: MPL-2.0
//! Evidence-based connection reporting; screen text describes firmware work,
//! never proves a transport disconnect or a successful diagnostic operation.
#[derive(Clone, Copy, Debug, Default)]
pub struct Evidence {
    pub live_session: bool,
    pub transport_open: bool,
    pub tx_confirmations: u64,
    pub transport_failed: bool,
}
fn checking_key(screen: &str) -> bool {
    screen.find("checking key position").is_some_and(|at| {
        let rest = &screen[at + "checking key position".len()..];
        let end = rest.find("checking").unwrap_or(rest.len());
        rest[..end].contains("working")
    })
}
pub fn snapshot(screen: &str, e: Evidence) -> serde_json::Value {
    let text = screen.to_ascii_lowercase();
    let guest_error = text.contains("no candi communication established");
    let (state, label) = if e.transport_failed {
        ("disconnected", "Adapter disconnected")
    } else if guest_error {
        (
            "guest_link_unavailable",
            "Firmware reports no CANdi communication",
        )
    } else if checking_key(&text) {
        (
            "waiting_key_position",
            "Firmware waiting for key-position input",
        )
    } else if text.contains("checking") && text.contains("working") {
        ("firmware_checking", "Firmware checks in progress")
    } else if e.transport_open {
        ("transport_open", "Adapter transport open")
    } else if e.live_session {
        ("opening", "Opening adapter")
    } else {
        ("offline", "Run without an adapter")
    };
    serde_json::json!({"state":state,"label":label,"transport_open":e.transport_open&&!e.transport_failed,"adapter_tx_confirmations":e.tx_confirmations,"transmission_observed":e.tx_confirmations>0,"electrical_ack":"unverified","diagnostic_completion":"unverified","live_session":e.live_session})
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn transmitting_key_wait_is_not_disconnected_or_completed() {
        let s = snapshot(
            "Checking Key Position Working",
            Evidence {
                live_session: true,
                transport_open: true,
                tx_confirmations: 2,
                ..Evidence::default()
            },
        );
        assert_eq!(s["state"], "waiting_key_position");
        assert_eq!(s["transmission_observed"], true);
        assert_eq!(s["diagnostic_completion"], "unverified");
    }
    #[test]
    fn completed_key_check_does_not_hide_other_pending_checks() {
        assert_eq!(
            snapshot(
                "Checking Key Position OK\nChecking Vehicle Selection Working",
                Evidence::default()
            )["state"],
            "firmware_checking"
        );
    }
    #[test]
    fn only_real_transport_failure_reports_disconnect() {
        assert_eq!(
            snapshot(
                "Checking Key Position Working",
                Evidence {
                    transport_failed: true,
                    ..Evidence::default()
                }
            )["state"],
            "disconnected"
        );
        assert_eq!(
            snapshot("Main Menu", Evidence::default())["state"],
            "offline"
        );
        assert_eq!(
            snapshot("No CANDI Communication Established", Evidence::default())["state"],
            "guest_link_unavailable"
        );
    }
}
