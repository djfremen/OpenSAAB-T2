// SPDX-License-Identifier: MPL-2.0
//! Evidence-based health states. A static LCD, old crash, or timeout is not a new crash.
//! UI/guest warning deadlines match Android EmulatorHealthState (20s / 30s / 180s startup).
use serde_json::{json, Value};
pub fn classify(facts: &Value) -> Value {
    let ui = facts["ui_heartbeat_age_ms"].as_u64();
    let guest = facts["emulator_heartbeat_age_ms"].as_u64();
    let status = if facts["device_reachable"] == false {
        "device_unreachable"
    } else if facts["endpoint_enabled"] == false {
        "mcp_disabled"
    } else if facts["app_alive"] == false {
        "app_not_running"
    } else if facts["app_alive"] != true {
        "app_state_unknown"
    } else if ui.is_some_and(|n| n >= 20_000) {
        "ui_unresponsive_suspected"
    } else if ui.is_some_and(|n| n > 2_000) {
        "ui_heartbeat_delayed"
    } else if facts["busy"] == true {
        "operation_in_progress"
    } else if facts["emulator_active"] == true && facts["emulator_alive"] == false {
        "emulator_exited"
    } else if facts["emulator_active"] == true
        && (guest.is_some_and(|n| n >= 30_000)
            || guest.is_none()
                && facts["session_age_ms"]
                    .as_u64()
                    .is_some_and(|n| n >= 180_000))
    {
        "emulator_unresponsive_suspected"
    } else if ui.is_none() {
        "process_running_ui_unknown"
    } else if facts["emulator_active"] == true {
        "responsive_session"
    } else {
        "responsive_idle"
    };
    json!({"schema":1,"state":status,"facts":facts,"crash_confirmed":false,
        "meaning":"Current process/heartbeat observations. Historical exits and saved failures are separate evidence; suspected hangs require confirmation.",
        "static_screen_is_failure":false,"warning_deadlines_ms":{"ui":20000,"guest":30000,"guest_startup":180000}})
}
pub fn exit_reason(reason: u64) -> &'static str {
    match reason {
        1 => "exit_self",
        2 => "signaled",
        3 => "low_memory",
        4 => "java_crash",
        5 => "native_crash",
        6 => "anr",
        7 => "initialization_failure",
        8 => "permission_change",
        9 => "excessive_resource_use",
        10 => "user_requested",
        11 => "user_stopped",
        12 => "dependency_died",
        13 => "other",
        14 => "freezer",
        15 => "package_state_change",
        16 => "package_updated",
        _ => "unknown",
    }
}
/// Only numeric outcome fields and fixed classifications; never register/stack dumps or LCD text.
pub fn safe_outcome(report: &Value, guest_crash: &Value) -> Value {
    let mut result = json!({"status":"unknown","guest_crash_artifact_present":guest_crash.is_object(),"failure_kind":"unknown"});
    if let Some(status) = report["status"].as_str().filter(|s| {
        [
            "success",
            "complete",
            "incomplete",
            "guest_failure",
            "output_failure",
            "cancelled",
            "error",
            "failed",
        ]
        .contains(s)
    }) {
        result["status"] = json!(status);
    }
    for key in ["exit_code", "instructions", "pc"] {
        if let Some(n) = report[key].as_u64().or_else(|| guest_crash[key].as_u64()) {
            result[key] = json!(n);
        }
    }
    if let Some(kind) = report["adapter_failure"]["kind"].as_str().filter(|s| {
        [
            "single_wire_wake_failed",
            "can_write_timeout",
            "command_blocked",
            "adapter_disconnected",
            "adapter_stopped",
            "unknown",
        ]
        .contains(s)
    }) {
        result["failure_kind"] = json!(kind);
    }
    result["failure_layer"] = json!(if result["status"] == "guest_failure" {
        "guest_firmware"
    } else if result["failure_kind"] != "unknown" {
        "adapter_or_policy"
    } else {
        "unknown"
    });
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn static_menu_and_historical_crash_do_not_mark_current_app_crashed() {
        let result = classify(
            &json!({"app_alive":true,"ui_heartbeat_age_ms":50,"emulator_active":true,"emulator_alive":true,"emulator_heartbeat_age_ms":200,"frame_age_ms":300000,"historical_crash":true}),
        );
        assert_eq!(result["state"], "responsive_session");
        assert_eq!(result["crash_confirmed"], false);
    }
    #[test]
    fn android_deadlines_and_unknown_states_preserved() {
        assert_eq!(
            classify(&json!({"app_alive":true,"ui_heartbeat_age_ms":20000}))["state"],
            "ui_unresponsive_suspected"
        );
        assert_eq!(
            classify(
                &json!({"app_alive":true,"ui_heartbeat_age_ms":1,"emulator_active":true,"emulator_alive":false})
            )["state"],
            "emulator_exited"
        );
        assert_eq!(
            classify(
                &json!({"app_alive":true,"ui_heartbeat_age_ms":1,"emulator_active":true,"emulator_alive":true,"session_age_ms":179999})
            )["state"],
            "responsive_session"
        );
        assert_eq!(
            classify(
                &json!({"app_alive":true,"ui_heartbeat_age_ms":1,"emulator_active":true,"emulator_alive":true,"session_age_ms":180000})
            )["state"],
            "emulator_unresponsive_suspected"
        );
        assert_eq!(
            classify(&json!({"app_alive":true}))["state"],
            "process_running_ui_unknown"
        );
        assert_eq!(
            classify(&json!({"device_reachable":false}))["state"],
            "device_unreachable"
        );
    }
    #[test]
    fn normal_user_stop_and_package_update_are_not_crashes() {
        assert_eq!(exit_reason(10), "user_requested");
        assert_eq!(exit_reason(16), "package_updated");
        assert_eq!(exit_reason(5), "native_crash");
        assert_eq!(exit_reason(6), "anr");
    }
    #[test]
    fn guest_failure_and_adapter_failure_are_distinct_and_sensitive_data_excluded() {
        let a = safe_outcome(
            &json!({"status":"guest_failure","pc":123}),
            &json!({"screen":"PRIVATE","stack_bytes":"SECRET"}),
        );
        assert_eq!(a["failure_layer"], "guest_firmware");
        assert!(!a.to_string().contains("PRIVATE"));
        let b = safe_outcome(
            &json!({"status":"incomplete","adapter_failure":{"kind":"single_wire_wake_failed","message":"SECRET"}}),
            &Value::Null,
        );
        assert_eq!(b["failure_layer"], "adapter_or_policy");
        assert!(!b.to_string().contains("SECRET"));
    }
}
