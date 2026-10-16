// SPDX-License-Identifier: MPL-2.0
//! Desktop support reports, ported from Android SupportReports/ConnectionAttempt/PerformanceReport.
//! Shared by every desktop shell: the shell supplies host facts and performs HTTPS; this module
//! decides what is collected, how it is bounded, and what counts as a confirmed receipt.
//! Never exports raw logs, bus payloads, VINs, SSA/security files, credentials, firmware or screenshots.
use serde_json::{json, Map, Value};
use std::{
    fs,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub const ENDPOINT: &str = "https://www.opensaab.com/api/support/reports";
pub const CONSENT: &str = "support-report-v1";
pub const APP: &str = "com.opensaab.desktop";
pub const MAX_UPLOAD: usize = 65536;
const TAIL: u64 = 262_144;
const EVENTS: [&str; 12] = [
    "USB_OPEN",
    "USB_CLOSED",
    "USB_TX",
    "USB_RX",
    "CAN_TX",
    "CAN_RX",
    "TIMEOUT",
    "DISCONNECT",
    "PANIC",
    "ERROR",
    "FAILED",
    "NEGATIVE RESPONSE",
];
const LOGS: [&str; 2] = ["console.log", "tech2.log"];
const STATUSES: [&str; 5] = [
    "success",
    "guest_failure",
    "incomplete",
    "output_failure",
    "cancelled",
];
const FAILURE_KINDS: [&str; 6] = ["single_wire_wake_failed", "can_write_timeout", "command_blocked", "adapter_disconnected", "adapter_stopped", "unknown"];
const PLATFORMS: [&str; 3] = ["macos", "windows", "linux"];
const ARCHES: [&str; 2] = ["arm64", "x86_64"];
pub const PRIVACY: &str = "Raw logs, CAN payloads, VIN, SSA, security responses, credentials, firmware and screenshots are excluded. Description and error messages are user-reviewed; VIN-like text is redacted. Event counts cover bounded log tails only.";

fn now() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap()
}
fn utc(t: SystemTime) -> Option<String> {
    let secs = t.duration_since(UNIX_EPOCH).ok()?.as_secs() as i64;
    time::OffsetDateTime::from_unix_timestamp(secs)
        .ok()?
        .format(&time::format_description::well_known::Rfc3339)
        .ok()
}
fn modified(p: &Path) -> Option<SystemTime> {
    fs::metadata(p).and_then(|m| m.modified()).ok()
}
fn small_json(p: &Path, limit: u64) -> Option<Value> {
    let m = fs::symlink_metadata(p).ok()?;
    if !m.is_file() || m.len() > limit {
        return None;
    }
    serde_json::from_slice(&fs::read(p).ok()?).ok()
}
fn utc_shape(t: &str) -> bool {
    (20..=40).contains(&t.len())
        && t.as_bytes()[4] == b'-'
        && t.contains('T')
        && t.bytes()
            .all(|b| b.is_ascii_digit() || b"-:T.Z+".contains(&b))
}
fn truncate(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

/// Replace 17-character VIN-shaped tokens (ISO 3779 alphabet, no I/O/Q, at least one digit).
pub fn redact_vins(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let ok =
        |c: char| c.is_ascii_digit() || (c.is_ascii_uppercase() && !matches!(c, 'I' | 'O' | 'Q'));
    let word = |c: char| c.is_ascii_alphanumeric();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        if (i == 0 || !word(chars[i - 1])) && i + 17 <= chars.len() {
            let w = &chars[i..i + 17];
            let bounded = i + 17 == chars.len() || !word(chars[i + 17]);
            if bounded
                && w.iter().all(|&c| ok(c))
                && w.iter().any(|c| c.is_ascii_digit())
                && w.iter().any(|c| c.is_ascii_uppercase())
            {
                out.push_str("[VIN removed]");
                i += 17;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

pub fn log_summary(file: &Path) -> Value {
    let name = file
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("log")
        .to_string();
    let read = || -> std::io::Result<Value> {
        let mut f = fs::File::open(file)?;
        let len = f.metadata()?.len();
        let start = len.saturating_sub(TAIL);
        f.seek(SeekFrom::Start(start))?;
        let mut bytes = Vec::new();
        f.take(TAIL).read_to_end(&mut bytes)?;
        let mut text = String::from_utf8_lossy(&bytes).into_owned();
        if start > 0 {
            text = text
                .split_once('\n')
                .map(|(_, t)| t.to_string())
                .unwrap_or_default();
        }
        let mut counts = Map::new();
        for line in text.lines() {
            let upper = line.to_ascii_uppercase();
            for e in EVENTS {
                if upper.contains(e) {
                    let n = counts.get(e).and_then(Value::as_u64).unwrap_or(0) + 1;
                    counts.insert(e.into(), json!(n));
                }
            }
        }
        Ok(json!({"source":name,"file_bytes":len,"tail_only":start>0,"event_counts":counts}))
    };
    read().unwrap_or_else(|_| json!({"source":name,"unavailable":true}))
}

fn numbers(src: &Value, keys: &[&str]) -> Map<String, Value> {
    let mut out = Map::new();
    for k in keys {
        if let Some(n) = src.get(*k).and_then(Value::as_u64) {
            out.insert((*k).into(), json!(n));
        }
    }
    out
}

pub fn performance(dir: &Path) -> Value {
    let Some(raw) = small_json(&dir.join("host-performance.json"), 16384) else {
        return json!({"unavailable":true});
    };
    if raw["performance_schema"] != 1 {
        return json!({"unavailable":true});
    }
    let mut safe = numbers(
        &raw,
        &[
            "performance_schema",
            "sample_interval_ms",
            "sample_count",
            "first_frame_observed_ms",
        ],
    );
    if let Some(b) = raw["complete"].as_bool() {
        safe.insert("complete".into(), json!(b));
    }
    let samples: Vec<Value> = raw["samples"]
        .as_array()
        .map(|a| {
            a.iter()
                .rev()
                .take(12)
                .rev()
                .map(|s| {
                    numbers(
                        s,
                        &[
                            "elapsed_ms",
                            "cpu_ms",
                            "rss_kib",
                            "peak_rss_kib",
                            "minor_faults",
                            "major_faults",
                            "ram_total_kib",
                            "ram_available_kib",
                            "swap_free_kib",
                            "frame_age_ms",
                        ],
                    )
                })
                .filter(|m| m.contains_key("elapsed_ms"))
                .map(Value::Object)
                .collect()
        })
        .unwrap_or_default();
    safe.insert("samples".into(), json!(samples));
    Value::Object(safe)
}

fn session(dir: &Path) -> Value {
    let adapter = small_json(&dir.join("dtc-context.json"), 8192)
        .and_then(|c| {
            c["adapter"].as_str().map(|a| {
                if a.starts_with("Chipsoft") {
                    "chipsoft"
                } else {
                    "offline"
                }
            })
        })
        .unwrap_or("unknown");
    let mut s = Map::new();
    s.insert("adapter".into(), json!(adapter));
    if let Some(t) = modified(dir).and_then(utc) {
        s.insert("modified_utc".into(), json!(t));
    }
    let logs: Vec<Value> = LOGS
        .iter()
        .map(|n| dir.join(n))
        .filter(|p| {
            fs::symlink_metadata(p)
                .map(|m| m.is_file())
                .unwrap_or(false)
        })
        .map(|p| log_summary(&p))
        .collect();
    s.insert("logs".into(), json!(logs));
    if let Some(raw) = small_json(&dir.join("report.json"), 1_048_576) {
        let mut o = numbers(
            &raw,
            &[
                "exit_code",
                "instructions",
                "pc",
                "host_cancelled_operations",
            ],
        );
        if let Some(st) = raw["status"].as_str().filter(|s| STATUSES.contains(s)) {
            o.insert("status".into(), json!(st));
        }
        // Shared adapter-stop classification (kind only; never the raw adapter text).
        if let Some(kind) = raw["adapter_failure"]["kind"].as_str().filter(|k| FAILURE_KINDS.contains(k)) {
            o.insert("reason".into(), json!(kind));
        }
        s.insert("emulator_outcome".into(), Value::Object(o));
    }
    if let Some(raw) = small_json(&dir.join("startup.json"), 4096) {
        s.insert(
            "startup".into(),
            Value::Object(numbers(&raw, &["native_boot_ms"])),
        );
    }
    if let Some(raw) = small_json(&dir.join("session-end.json"), 4096) {
        let mut o = numbers(&raw, &["exit_code", "elapsed_ms"]);
        if let Some(r) = raw["reason"].as_str().filter(|r| {
            [
                "user_stop",
                "forced_stop",
                "exited",
                "adapter_stopped",
                "wake_rejected",
                "unsupported_command",
                "app_quit",
            ]
            .contains(r)
        }) {
            o.insert("reason".into(), json!(r));
        }
        s.insert("session_end".into(), Value::Object(o));
    }
    s.insert(
        "guest_crash".into(),
        json!(dir.join("crash.json").is_file()),
    );
    s.insert("native_performance".into(), performance(dir));
    Value::Object(s)
}

pub fn sessions(root: &Path) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = fs::read_dir(root.join("runs"))
        .map(|r| r.filter_map(Result::ok).map(|e| e.path()).collect())
        .unwrap_or_default();
    dirs.retain(|p| fs::symlink_metadata(p).map(|m| m.is_dir()).unwrap_or(false));
    dirs.sort_by_key(|p| std::cmp::Reverse(modified(p)));
    dirs.truncate(5);
    dirs
}

const STAGES: [&str; 8] = [
    "DETECTION",
    "IDENTIFICATION",
    "VIN_REQUEST",
    "FIRMWARE_START",
    "SESSION",
    "CLEANUP",
    "USB_OPEN",
    "CHANNEL_OPEN",
];
const OUTCOMES: [&str; 4] = ["IN_PROGRESS", "COMPLETED", "FAILED", "CANCELLED"];
const REASONS: [&str; 9] = [
    "NONE",
    "NO_ADAPTER",
    "DISCONNECTED",
    "TIMEOUT",
    "IO_ERROR",
    "PROTOCOL_OR_PROCESS_ERROR",
    "VIN_UNAVAILABLE",
    "CLEANUP_FAILED",
    "USER_STOP",
];

fn attempt(raw: &Value) -> Option<Value> {
    let id = raw["attempt_id"]
        .as_str()
        .filter(|s| s.len() == 36 && s.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-'))?;
    let pick = |k: &str, allowed: &[&str]| {
        raw[k]
            .as_str()
            .filter(|v| allowed.contains(v))
            .map(String::from)
    };
    let mut o = Map::new();
    o.insert("attempt_id".into(), json!(id.to_ascii_lowercase()));
    if let Some(t) = raw["created_utc"].as_str().filter(|t| utc_shape(t)) {
        o.insert("created_utc".into(), json!(t));
    }
    o.insert(
        "adapter".into(),
        json!(pick("adapter", &["CHIPSOFT", "SELECTION"]).unwrap_or("CHIPSOFT".into())),
    );
    o.insert("stage".into(), json!(pick("stage", &STAGES)?));
    o.insert("outcome".into(), json!(pick("outcome", &OUTCOMES)?));
    o.insert(
        "reason".into(),
        json!(pick("reason", &REASONS).unwrap_or("NONE".into())),
    );
    if let Some(n) = raw["elapsed_ms"].as_u64() {
        o.insert("elapsed_ms".into(), json!(n));
    }
    let stages: Vec<Value> = raw["stages"]
        .as_array()
        .map(|a| {
            a.iter()
                .take(24)
                .filter_map(|s| {
                    let st = s["stage"].as_str().filter(|v| STAGES.contains(v))?;
                    Some(json!({"stage":st,"elapsed_ms":s["elapsed_ms"].as_u64().unwrap_or(0)}))
                })
                .collect()
        })
        .unwrap_or_default();
    o.insert("stages".into(), json!(stages));
    Some(Value::Object(o))
}

pub fn attempts(root: &Path) -> Vec<Value> {
    let mut files: Vec<PathBuf> = fs::read_dir(root.join("connection-attempts"))
        .map(|r| {
            r.filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == "json"))
                .collect()
        })
        .unwrap_or_default();
    files.sort_by_key(|p| std::cmp::Reverse(modified(p)));
    files
        .iter()
        .filter_map(|p| small_json(p, 16384))
        .filter_map(|v| attempt(&v))
        .take(20)
        .collect()
}

fn host(h: &Value) -> Result<Map<String, Value>, String> {
    let mut o = Map::new();
    let platform = h["platform"]
        .as_str()
        .filter(|p| PLATFORMS.contains(p))
        .ok_or("Unsupported platform")?;
    let arch = h["arch"]
        .as_str()
        .filter(|a| ARCHES.contains(a))
        .ok_or("Unsupported architecture")?;
    o.insert("platform".into(), json!(platform));
    o.insert("arch".into(), json!(arch));
    for k in ["version", "build", "os_version"] {
        let v = h[k].as_str().unwrap_or("").trim();
        if !v.is_empty() {
            o.insert(k.into(), json!(truncate(v, 120)));
        }
    }
    o.insert(
        "device_resources".into(),
        Value::Object(numbers(
            h,
            &[
                "ram_total_bytes",
                "storage_free_bytes",
                "runtime_cpu_count",
                "display_width_px",
                "display_height_px",
            ],
        )),
    );
    Ok(o)
}

fn last_error(root: &Path) -> Option<Value> {
    let raw = small_json(&root.join("last-app-error.json"), 8192)?;
    let mut o = Map::new();
    if let Some(t) = raw["utc"].as_str().filter(|t| utc_shape(t)) {
        o.insert("utc".into(), json!(t));
    }
    o.insert(
        "kind".into(),
        json!(raw["kind"]
            .as_str()
            .filter(|k| ["handled_error", "session_error", "connection_error"].contains(k))
            .unwrap_or("handled_error")),
    );
    if let Some(m) = raw["message"].as_str() {
        o.insert("message".into(), json!(redact_vins(&truncate(m, 600))));
    }
    Some(Value::Object(o))
}

pub fn collect(root: &Path, description: &str, host_facts: &Value) -> Result<Value, String> {
    let mut r = Map::new();
    r.insert("format".into(), json!(1));
    r.insert("created_utc".into(), json!(now()));
    r.insert(
        "description".into(),
        json!(redact_vins(&truncate(description, 2000))),
    );
    r.insert("app".into(), json!(APP));
    r.extend(host(host_facts)?);
    r.insert("privacy".into(), json!(PRIVACY));
    let mut runs: Vec<Value> = sessions(root).iter().map(|d| session(d)).collect();
    r.insert("recent_sessions".into(), json!(runs));
    r.insert("connection_attempts".into(), json!(attempts(root)));
    if let Some(e) = last_error(root) {
        r.insert("last-app-error.json".into(), e);
    }
    // Stay under the server limit: drop oldest sessions rather than truncating fields.
    let mut report = Value::Object(r);
    while serde_json::to_vec_pretty(&report)
        .map(|b| b.len())
        .unwrap_or(usize::MAX)
        > MAX_UPLOAD - 2048
    {
        if runs.pop().is_none() {
            return Err("Report is too large".into());
        }
        report["recent_sessions"] = json!(runs);
    }
    Ok(report)
}

fn valid_id(id: &str) -> bool {
    id.len() == 27
        && id.starts_with("OS-")
        && id[3..]
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// Saves the frozen report the user reviewed; returns its id and the exact text shown and uploaded.
pub fn save(root: &Path, report: &Value) -> Result<Value, String> {
    let dir = root.join("support-reports");
    fs::create_dir_all(&dir).map_err(|_| "Cannot create report folder")?;
    let text = serde_json::to_string_pretty(report).map_err(|_| "Cannot encode report")?;
    let id = format!("desktop_support_{}", unique());
    let path = dir.join(format!("{id}.json"));
    let tmp = dir.join(format!(".{id}.tmp"));
    fs::write(&tmp, &text)
        .and_then(|_| fs::rename(&tmp, &path))
        .map_err(|_| {
            let _ = fs::remove_file(&tmp);
            "Could not save the report. Check free space."
        })?;
    let mut old: Vec<PathBuf> = fs::read_dir(&dir)
        .map(|r| {
            r.filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| {
                    p.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.starts_with("desktop_support_") && n.ends_with(".json"))
                })
                .collect()
        })
        .unwrap_or_default();
    old.sort_by_key(|p| modified(p));
    while old.len() > 8 {
        let _ = fs::remove_file(old.remove(0));
    }
    Ok(json!({"id":id,"path":path.to_string_lossy(),"text":text,"bytes":text.len()}))
}

fn unique() -> String {
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{:x}{:x}", n, std::process::id())
}

/// Same rules as Android SupportUpload: only HTTP 201 with a well-formed stored receipt is success.
pub fn receipt(status: u64, body: &str, network: &str) -> Value {
    if !network.is_empty() {
        return json!({"stored":false,"retry":true,"message":match network {
            "offline" | "dns" => "Could not reach OpenSAAB. Check your internet connection and try again. Your report is kept on this computer.",
            "timeout" => "OpenSAAB did not answer in time. Your report is kept; try again.",
            "redirect" => "Unexpected server redirect; the report was not sent. Use Other options instead.",
            _ => "Could not send the report. Your local copy is kept; please try again.",
        }});
    }
    if status != 201 {
        let message = match status {
            429 => "Too many reports have been sent recently. Please try again later.",
            400 | 413 | 415 => "The server could not accept this report. Use Other options to copy or save it instead.",
            _ => "OpenSAAB could not confirm storage. Your report is kept here; try again later.",
        };
        return json!({"stored":false,"retry":status != 400 && status != 413 && status != 415,"message":message,"status":status});
    }
    if body.len() > 4096 {
        return json!({"stored":false,"retry":true,"message":"Unexpected server reply; keep your local report."});
    }
    match serde_json::from_str::<Value>(body) {
        Ok(v) if v["stored"] == true && v["report_id"].as_str().is_some_and(valid_id) => {
            json!({"stored":true,"report_id":v["report_id"]})
        }
        _ => {
            json!({"stored":false,"retry":true,"message":"Storage was not confirmed. Keep your report and try again."})
        }
    }
}

pub fn dispatch(v: &Value) -> Result<Value, String> {
    let root = Path::new(v["root"].as_str().ok_or("Missing report root")?);
    match v["op"].as_str().unwrap_or("") {
        "support_collect" => {
            let report = collect(root, v["description"].as_str().unwrap_or(""), &v["host"])?;
            let mut saved = save(root, &report)?;
            saved["endpoint"] = json!(ENDPOINT);
            saved["consent"] = json!(CONSENT);
            Ok(saved)
        }
        "support_receipt" => {
            let r = receipt(
                v["status"].as_u64().unwrap_or(0),
                v["body"].as_str().unwrap_or(""),
                v["network"].as_str().unwrap_or(""),
            );
            if r["stored"] == true {
                let dir = root.join("support-reports");
                let _ = fs::create_dir_all(&dir);
                let _ = fs::write(
                    dir.join("last-support-upload.json"),
                    json!({"report_id":r["report_id"],"submitted_utc":now()}).to_string(),
                );
            }
            Ok(r)
        }
        "support_last" => Ok(small_json(
            &root.join("support-reports/last-support-upload.json"),
            4096,
        )
        .filter(|l| l["report_id"].as_str().is_some_and(valid_id))
        .unwrap_or(json!({}))),
        _ => Err("Unsupported support operation".into()),
    }
}

#[cfg(test)]
mod tests;
