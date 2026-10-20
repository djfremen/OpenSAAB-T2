// SPDX-License-Identifier: MPL-2.0
//! Read-only ADB health preflight for the selected paired phone. No install/restart/key commands.
use crate::app_health;
use serde_json::{json, Value};
use std::{
    io::Read,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
const APP: &str = "com.opensaab.tech2";
fn capture(serial: &str, args: &[&str]) -> Result<String, String> {
    let adb = std::env::var_os("OPENSAAB_ADB").unwrap_or_else(|| "adb".into());
    let mut child = Command::new(adb)
        .args(["-s", serial])
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "ADB unavailable; set OPENSAAB_ADB to platform-tools/adb")?;
    let pipe = child.stdout.take().unwrap();
    let reader = thread::spawn(move || {
        let mut data = Vec::new();
        let result = pipe.take(262145).read_to_end(&mut data);
        (result, data)
    });
    let deadline = Instant::now() + Duration::from_secs(4);
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break Ok(s),
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break Err("ADB request timed out or disconnected".to_owned());
            }
        }
    };
    let (read, data) = reader.join().map_err(|_| "ADB output worker failed")?;
    read.map_err(|_| "Cannot read ADB output")?;
    if data.len() > 262144 {
        return Err("ADB output exceeded its size limit".into());
    }
    let status = status?;
    if !status.success() {
        return Err(format!(
            "ADB command returned status {}",
            status
                .code()
                .map(|c| c.to_string())
                .unwrap_or_else(|| "unknown".into())
        ));
    }
    Ok(String::from_utf8_lossy(&data).into_owned())
}
fn pid(text: &str) -> Option<u32> {
    text.split_whitespace()
        .find_map(|s| s.parse::<u32>().ok().filter(|p| *p > 0))
}
pub fn parse_exits(text: &str) -> Value {
    let mut records = Vec::new();
    for block in text.split("ApplicationExitInfo #").skip(1).take(16) {
        let mut record = json!({"historical":true,"correlation":"OS exit history; not a current-session verdict","time_basis":"device local; UTC offset unavailable"});
        for line in block.lines().map(str::trim) {
            if let Some(rest) = line.strip_prefix("timestamp=") {
                let timestamp = rest
                    .split_whitespace()
                    .take(2)
                    .collect::<Vec<_>>()
                    .join(" ");
                if timestamp.len() <= 32
                    && timestamp
                        .bytes()
                        .all(|b| b.is_ascii_digit() || b"-:. ".contains(&b))
                {
                    record["timestamp_local"] = json!(timestamp);
                }
                for word in rest.split_whitespace() {
                    if let Some(n) = word
                        .strip_prefix("pid=")
                        .and_then(|s| s.parse::<u32>().ok())
                    {
                        record["pid"] = json!(n);
                    }
                }
            }
            if line.starts_with("process=") {
                let process = line
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .trim_start_matches("process=");
                if process == APP
                    || process.starts_with("com.opensaab.tech2:")
                    || process == "libtech2_emu.so"
                {
                    record["process"] = json!(process);
                }
                for word in line.split_whitespace() {
                    if let Some(reason) = word
                        .strip_prefix("reason=")
                        .and_then(|s| s.parse::<u64>().ok())
                    {
                        record["reason_code"] = json!(reason);
                        record["reason"] = json!(app_health::exit_reason(reason));
                    }
                    if let Some(status) = word
                        .strip_prefix("status=")
                        .and_then(|s| s.parse::<i32>().ok())
                    {
                        record["status"] = json!(status);
                    }
                }
            }
        }
        if record["process"].is_string() && record["reason_code"].is_number() {
            records.push(record);
        }
    }
    json!(records)
}
fn history_supported(text: &str) -> bool {
    text.contains("ACTIVITY MANAGER PROCESS EXIT INFO")
}
pub fn collect(serial: &str, details: bool) -> Result<Value, String> {
    if serial.is_empty()
        || serial.len() > 160
        || serial.starts_with('-')
        || !serial
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._:-".contains(&b))
    {
        return Err("Use the exact selected paired ADB device serial".into());
    }
    let started = Instant::now();
    let connection = capture(serial, &["get-state"]);
    if connection
        .as_ref()
        .is_err_and(|s| s.starts_with("ADB unavailable"))
    {
        return Err(connection.unwrap_err());
    }
    if connection.as_deref().map(str::trim) != Ok("device") {
        return Ok(app_health::classify(
            &json!({"platform":"android","device_reachable":false,"collector":"adb_read_only"}),
        ));
    }
    let (process, package, exits) = thread::scope(|scope| {
        let p = scope.spawn(|| capture(serial, &["shell", "pidof", APP]));
        let v = scope.spawn(|| capture(serial, &["shell", "dumpsys", "package", APP]));
        let e =
            scope.spawn(|| capture(serial, &["shell", "dumpsys", "activity", "exit-info", APP]));
        (p.join().unwrap(), v.join().unwrap(), e.join().unwrap())
    });
    let first = process.as_ref().ok().and_then(|s| pid(s));
    let second = capture(serial, &["shell", "pidof", APP])
        .ok()
        .and_then(|s| pid(&s));
    if capture(serial, &["get-state"]).as_deref().map(str::trim) != Ok("device") {
        return Ok(app_health::classify(
            &json!({"platform":"android","device_reachable":false,"collector":"adb_read_only","snapshot_consistent":false}),
        ));
    }
    // pidof exit status 1 means absent. Other error types stay unknown.
    let alive = if first != second {
        Value::Null
    } else if first.is_some() {
        json!(true)
    } else if process
        .as_ref()
        .is_err_and(|s| s == "ADB command returned status 1")
    {
        json!(false)
    } else if process.is_ok() {
        json!(false)
    } else {
        Value::Null
    };
    let mut identity = json!({"version":null,"version_code":null,"abi":null});
    if let Ok(text) = &package {
        for line in text.lines().map(str::trim) {
            if let Some(v) = line.strip_prefix("versionName=").filter(|v| {
                !v.is_empty()
                    && v.len() <= 80
                    && v.bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b".-+_".contains(&b))
            }) {
                identity["version"] = json!(v);
            }
            if let Some(v) = line
                .strip_prefix("versionCode=")
                .and_then(|s| s.split_whitespace().next())
                .and_then(|s| s.parse::<u64>().ok())
            {
                identity["version_code"] = json!(v);
            }
            if let Some(v) = line
                .strip_prefix("primaryCpuAbi=")
                .filter(|v| ["arm64-v8a", "armeabi-v7a", "x86_64", "x86"].contains(v))
            {
                identity["abi"] = json!(v);
            }
        }
    }
    let records = exits.as_ref().map(|s| parse_exits(s)).unwrap_or(json!([]));
    let mut result = app_health::classify(
        &json!({"platform":"android","app":APP,"device_reachable":true,"app_alive":alive,"app_pid":first,
        "identity":identity,"ui_heartbeat_age_ms":null,"emulator_active":null,"emulator_alive":null,"collector":"adb_read_only","snapshot_consistent":first==second}),
    );
    result["historical_exit_count"] = json!(records.as_array().unwrap().len());
    result["latest_known_exit"] = records
        .as_array()
        .unwrap()
        .first()
        .cloned()
        .unwrap_or(Value::Null);
    result["exit_history_available"] = json!(exits.as_ref().is_ok_and(|s| history_supported(s)));
    result["collection_ms"] = json!(started.elapsed().as_millis());
    result["phone_control_binding"] = json!(
        "pending; this release reads OS health only, without changing the installed Android app"
    );
    if details {
        result["historical_exits"] = records;
        result["app_private_reports"] = json!({"available":false,"reason":"Installed production APK needs its authenticated app-side MCP/report endpoint; no run-as/debuggable assumption"});
    }
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn force_stop_and_old_native_crash_are_labelled_history_and_raw_fields_removed() {
        let v=parse_exits("ApplicationExitInfo #0:\ntimestamp=2026-10-14 01:00:00.123 pid=7 realUid=123\nprocess=com.opensaab.tech2 reason=10 (USER REQUESTED) status=0\ndescription=SECRET\nApplicationExitInfo #1:\ntimestamp=2026-10-13 01:00:00.000 pid=6\nprocess=com.opensaab.tech2 reason=5 (NATIVE CRASH) status=11\n");
        assert_eq!(v[0]["reason"], "user_requested");
        assert_eq!(v[1]["reason"], "native_crash");
        assert_eq!(v[1]["historical"], true);
        assert!(!v.to_string().contains("SECRET"));
    }
    #[test]
    fn unrelated_and_malformed_exit_records_do_not_become_crashes() {
        assert_eq!(
            parse_exits("ApplicationExitInfo #0:\nprocess=other.app reason=5 status=11\n"),
            json!([])
        );
        assert_eq!(parse_exits("unavailable"), json!([]));
    }
    #[test]
    fn invalid_serial_does_not_spawn_adb() {
        assert!(collect("-s invalid", false).is_err());
        assert!(collect("device; echo secret", false).is_err());
    }
    #[test]
    fn older_android_without_exit_info_reports_unavailable_not_empty_history() {
        assert!(!history_supported("Unknown command: exit-info"));
        assert!(history_supported("ACTIVITY MANAGER PROCESS EXIT INFO (dumpsys activity exit-info)\n  package: com.opensaab.tech2\n"));
    }
}
