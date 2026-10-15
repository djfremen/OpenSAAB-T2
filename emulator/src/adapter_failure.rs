// SPDX-License-Identifier: MPL-2.0
//! Shared classification of a live adapter stop, so every desktop shell shows the same
//! explanation instead of a blank or stale firmware frame. Input is the adapter error the
//! native link already received plus the last CAN transmission it submitted; nothing here
//! talks to hardware, retries, or invents an ECU reply. An unknown error stays "adapter_stopped".
use serde_json::{json, Value};

/// The last frame the emulated CANdi handed to the adapter before the stop.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TxContext {
    pub controller: usize,
    pub bus: &'static str,
    pub wake: bool,
    pub id: u32,
    pub dlc: u8,
}

impl TxContext {
    pub fn new(controller: usize, tx: &crate::candi_cpu::CanTransmission) -> Self {
        let bus = match crate::candi_transport::ROUTES
            .get(controller)
            .map(|r| r.bus)
        {
            Some(crate::candi_transport::PhysicalBus::HighSpeed) => "HighSpeed",
            Some(crate::candi_transport::PhysicalBus::Middle) => "Middle",
            Some(crate::candi_transport::PhysicalBus::SingleWire) => "SingleWire",
            None => "unknown",
        };
        TxContext {
            controller,
            bus,
            wake: tx.electrical.wake_control_selected(),
            id: tx.frame.id,
            dlc: tx.frame.dlc,
        }
    }
    fn is_single_wire_wake(&self) -> bool {
        self.bus == "SingleWire" && self.wake && self.id == 0x100 && self.dlc == 0
    }
}

const SW_WAKE_MESSAGE: &str = "The adapter could not complete the single-wire CAN wake-up request. The adapter connection worked, but this vehicle-network step did not complete.\n\nOn an ECM-only bench, a high-speed CAN VIN read can work while the original menus still need the low-speed bus. Check the single-wire CAN connection at diagnostic pin 1. ECU information and DTC reading have not been confirmed.";

fn transport_detail(raw: &str) -> Option<&'static str> {
    if raw.contains("Chipsoft opcode=000F status=001D") {
        Some("Chipsoft status 001D")
    } else if raw.contains("ERR_CAN_WRITE_TIMEOUT") {
        Some("J2534 ERR_CAN_WRITE_TIMEOUT")
    } else {
        None
    }
}

fn disconnected(raw: &str) -> bool {
    let l = raw.to_ascii_lowercase();
    [
        "err_device_not_connected",
        "no such device",
        "device not configured",
        "device disconnected",
        "device removed",
        "serial port closed",
    ]
    .iter()
    .any(|s| l.contains(s))
}

/// Classify one adapter stop. `raw` is the error text after "Native adapter stopped: ".
pub fn classify(raw: &str, last_tx: Option<&TxContext>) -> Value {
    let raw_short: String = raw.chars().take(300).collect();
    let tx = last_tx.map(|t| json!({"controller":t.controller,"bus":t.bus,"wake":t.wake,"id":format!("{:03X}",t.id),"dlc":t.dlc}));
    let out = |kind: &str, title: &str, message: String| json!({"schema":1,"kind":kind,"title":title,"message":message,"adapter_error":raw_short,"last_tx":tx,"ecu_rejected":false});
    if let Some(rest) = raw
        .split_once("command guard blocked before transport:")
        .or_else(|| raw.split_once("command guard blocked before USB:"))
        .map(|(_, rest)| rest)
    {
        let message = if rest.contains("controller=2 CAN_ID=241 service=AE profile=Read;") {
            "The ignition-key test requires Full native control. This session was read-only, so OpenSAAB stopped before sending the request. The ECU did not reject it.\n\nChoose Connect and start, select Full native control, then retry Check Ignition Key Status. The previous session and its logs have been saved."
        } else if rest.contains(" service=27 ") && rest.contains(" profile=Read;") {
            "Security seed collection was requested in a read-diagnostics session. OpenSAAB stopped before sending that request; the ECU did not reject it.\n\nReconnect with Collect security seeds selected, then use the original Security Access menu. Seed collection does not process security data or submit an authorization key."
        } else {
            "The original software requested a command outside this session's supported operations. OpenSAAB stopped before sending it; this is not an ECU rejection. See Console for the command and session profile."
        };
        return out(
            "command_blocked",
            "Unsupported command for this session",
            message.into(),
        );
    }
    if disconnected(raw) {
        return out("adapter_disconnected", "Adapter disconnected",
            "The adapter stopped responding or was disconnected during the session. Reconnect the adapter, then choose Connect and start for a fresh VIN. No vehicle result from this session should be relied on.".into());
    }
    if let (Some(detail), Some(t)) = (transport_detail(raw), last_tx) {
        if t.is_single_wire_wake() {
            return out(
                "single_wire_wake_failed",
                "Single-wire CAN wake-up failed · Check pin 1",
                format!("{SW_WAKE_MESSAGE}\n\nAdapter detail: {detail}."),
            );
        }
        if raw.contains("ERR_CAN_WRITE_TIMEOUT") {
            return out("can_write_timeout", "CAN transmit not acknowledged",
                format!("The adapter could not transmit a frame on the {} CAN bus: no node acknowledged it in time. Check that the ECU is powered and the CAN wiring for that bus is connected. The original firmware stopped; no ECU reply was assumed.\n\nAdapter detail: {detail}.", t.bus));
        }
    }
    out("adapter_stopped", "Live adapter stopped",
        "The adapter reported an error and the session stopped. The operation did not complete; no ECU reply was assumed. See Console for the adapter detail.".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn wake() -> TxContext {
        TxContext {
            controller: 2,
            bus: "SingleWire",
            wake: true,
            id: 0x100,
            dlc: 0,
        }
    }
    const CHIPSOFT_001D: &str = "Chipsoft opcode=000F status=001D";
    const J2534_TIMEOUT: &str = "Native J2534 ERROR WRITE failed 00000009 last_error_rc=00000000 detail=ERR_CAN_WRITE_TIMEOUT";

    #[test]
    fn single_wire_wake_on_both_transports() {
        for raw in [CHIPSOFT_001D, J2534_TIMEOUT] {
            let v = classify(raw, Some(&wake()));
            assert_eq!(v["kind"], "single_wire_wake_failed", "{raw}");
            assert!(v["message"].as_str().unwrap().contains("pin 1"));
            assert_eq!(v["ecu_rejected"], false);
            assert_eq!(v["last_tx"]["bus"], "SingleWire");
        }
    }

    #[test]
    fn same_error_without_matching_wake_is_not_a_wake_failure() {
        // Ported from the Mac shell's negative cases: no TX, HS bus, other status, other frame.
        let hs = TxContext {
            bus: "HighSpeed",
            controller: 0,
            wake: false,
            id: 0x7e0,
            dlc: 8,
        };
        let not_wake = TxContext {
            wake: false,
            ..wake()
        };
        let other_frame = TxContext {
            id: 0x241,
            dlc: 8,
            ..wake()
        };
        assert_eq!(classify(CHIPSOFT_001D, None)["kind"], "adapter_stopped");
        assert_eq!(
            classify(CHIPSOFT_001D, Some(&hs))["kind"],
            "adapter_stopped"
        );
        assert_eq!(
            classify("Chipsoft opcode=000F status=0001", Some(&wake()))["kind"],
            "adapter_stopped"
        );
        assert_eq!(
            classify(CHIPSOFT_001D, Some(&not_wake))["kind"],
            "adapter_stopped"
        );
        assert_eq!(
            classify(CHIPSOFT_001D, Some(&other_frame))["kind"],
            "adapter_stopped"
        );
        let v = classify(J2534_TIMEOUT, Some(&hs));
        assert_eq!(v["kind"], "can_write_timeout");
        assert!(v["message"].as_str().unwrap().contains("HighSpeed"));
    }

    #[test]
    fn command_guard_messages_match_mac_shell() {
        let seeds = "Chipsoft command guard blocked before USB: controller=2 CAN_ID=257 service=27 profile=Read; no ECU rejection";
        let v = classify(seeds, Some(&wake()));
        assert_eq!(v["kind"], "command_blocked");
        assert!(v["message"]
            .as_str()
            .unwrap()
            .contains("Collect security seeds"));
        let already = classify(&seeds.replace("profile=Read", "profile=Seeds"), None);
        assert!(!already["message"]
            .as_str()
            .unwrap()
            .contains("Collect security seeds"));
        let ignition = "Chipsoft command guard blocked before USB: controller=2 CAN_ID=241 service=AE profile=Read; no ECU rejection";
        assert!(classify(ignition, None)["message"]
            .as_str()
            .unwrap()
            .contains("ignition-key test requires Full native control"));
        assert_ne!(classify(CHIPSOFT_001D, None)["kind"], "command_blocked");
    }

    #[test]
    fn mdi_policy_denial_is_distinct_from_transport_failure() {
        let ignition = "MDI command guard blocked before transport: controller=2 CAN_ID=241 service=AE profile=Read; no ECU rejection";
        let failure = classify(ignition, None);
        assert_eq!(failure["kind"], "command_blocked");
        assert!(failure["message"].as_str().unwrap().contains("ignition-key test requires Full native control"));
        assert_eq!(failure["ecu_rejected"], false);
        assert_eq!(classify("MDI USB IN: IOKit 0xE00002C0", None)["kind"], "adapter_stopped");
        assert_eq!(classify("MDI TCP device disconnected", None)["kind"], "adapter_disconnected");
    }

    #[test]
    fn disconnect_and_unknown_are_explicit_and_bounded() {
        assert_eq!(
            classify("Chipsoft read: No such device (os error 6)", Some(&wake()))["kind"],
            "adapter_disconnected"
        );
        assert_eq!(
            classify("J2534 ERR_DEVICE_NOT_CONNECTED", None)["kind"],
            "adapter_disconnected"
        );
        let long = "x".repeat(1000);
        let v = classify(&long, None);
        assert_eq!(v["kind"], "adapter_stopped");
        assert_eq!(v["adapter_error"].as_str().unwrap().len(), 300);
        assert!(v["last_tx"].is_null());
    }
}
