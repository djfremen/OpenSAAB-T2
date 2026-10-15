// SPDX-License-Identifier: MPL-2.0
//! Durable ECM-only reports. Native apps provide storage roots and transport receipts.
use serde_json::{json, Value};
use std::{ffi::{c_char, CStr, CString}, fs, io::{Read, Write}, path::{Path, PathBuf}};
const MAX_BYTES: u64 = 131072;
pub fn valid_id(id: &str) -> bool {
    id.starts_with("hscan_ecm_") && id.len() <= 100
        && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}
fn path(root: &Path, id: &str, ext: &str) -> Result<PathBuf, String> {
    if !valid_id(id) { return Err("Invalid report selection".into()); }
    Ok(root.join("dtc-reports").join(format!("{id}.{ext}")))
}
fn load(p: &Path) -> Result<Value, String> {
    if fs::symlink_metadata(p).map_err(|_| "Saved report unavailable")?.file_type().is_symlink() { return Err("Invalid report location".into()); }
    let mut bytes = Vec::new();
    fs::File::open(p).map_err(|_| "Saved report unavailable")?.take(MAX_BYTES + 1)
        .read_to_end(&mut bytes).map_err(|_| "Cannot read report")?;
    if bytes.len() as u64 > MAX_BYTES { return Err("Saved report too large".into()); }
    serde_json::from_slice(&bytes).map_err(|_| "Invalid report JSON".into())
}
fn atomic(p: &Path, bytes: &[u8]) -> Result<(), String> {
    let tmp = p.with_extension("tmp");
    let mut options = fs::OpenOptions::new(); options.write(true).create_new(true);
    #[cfg(unix)] { use std::os::unix::fs::OpenOptionsExt; options.mode(0o600); }
    let mut file = options.open(&tmp).map_err(|_| "Cannot save report")?;
    let result = file.write_all(bytes).and_then(|_| file.sync_all()).and_then(|_| fs::rename(&tmp, p));
    if result.is_err() { let _ = fs::remove_file(tmp); }
    result.map_err(|_| "Cannot finish saving report".into())
}
fn text<'a>(v: &'a Value, key: &str) -> &'a str { v[key].as_str().unwrap_or("") }
fn bounded(v: &Value) -> bool { v.as_str().is_some_and(|s| s.len() <= 200 && !s.chars().any(char::is_control)) }
fn validate(result: &Value) -> Result<(), String> {
    let vin = text(result, "vin");
    if vin.len() != 17 || !vin.starts_with("YS3") || !vin.bytes().all(|b| b.is_ascii_digit() || b.is_ascii_uppercase() && !b"IOQ".contains(&b))
        || result["request_origin"] != "host-hscan-probe" || result["firmware_emulation"] != false {
        return Err("No verified ECM probe identity".into());
    }
    match text(result, "mode") {
        "ecm_info" => {
            if result["ecm_identifiers"].as_object().is_none_or(|m| m.len() != 7) { return Err("Incomplete ECM information".into()); }
            for (_, _, label) in crate::vlinker::INFO_READS {
                let v = &result["ecm_identifiers"][label];
                if !bounded(&v["value"]) || v["raw_hex"].as_str().is_none_or(|s| s.len() > 1536 || !s.bytes().all(|b| b.is_ascii_hexdigit() || b == b' ')) { return Err("Invalid ECM identifier".into()); }
            }
        }
        "ecm_dtc" => {
            let dtc = &result["dtc_report"];
            if dtc["complete"] != true || dtc["mask"] != "12" || dtc["codes_cleared"] != false || result["diagnostic_session_stopped"] != true { return Err("Incomplete engine-code report".into()); }
            let records = dtc["records"].as_array().ok_or("Missing engine-code records")?;
            if records.len() > 128 { return Err("Engine-code report too large".into()); }
            for d in records {
                let c = text(d, "code");
                let hex = |s: &str| s.len() == 2 && s.bytes().all(|b| b.is_ascii_digit() || (b'A'..=b'F').contains(&b));
                if c.len() != 5 || !matches!(c.as_bytes()[0], b'P'|b'C'|b'B'|b'U') || !c.as_bytes()[1..].iter().all(|b| b.is_ascii_hexdigit())
                    || !hex(text(d, "failure_type")) || !hex(text(d, "status")) { return Err("Invalid engine-code record".into()); }
            }
        }
        _ => return Err("Unsupported ECM report scope".into()),
    }
    for field in ["product", "chip_firmware", "voltage"] { if !bounded(&result[field]) { return Err("Invalid adapter identification".into()); } }
    Ok(())
}
pub fn title(report: &Value) -> String {
    format!("{} · {}", if report["result"]["mode"] == "ecm_dtc" { "Engine codes — HS-CAN" } else { "ECM information — HS-CAN" }, text(report, "observed_utc"))
}
pub fn render(report: &Value) -> Result<String, String> {
    let r = &report["result"]; validate(r)?;
    let mut out = format!("OpenSAAB saved diagnostic report\n{}\nSaved observation; reconnect and read again for current results.\nVIN: {}\nModule: Trionic 8 ECM · HS-CAN 500 kbit/s\nCoverage: {}\nRead at (UTC): {}\nApp: {} ({})\nAdapter: {} · firmware {} · voltage {}\n\n",
        title(report), text(r,"vin"), if r["mode"] == "ecm_dtc" { "Complete ECM current/history report · mask 12" } else { "Seven ECM identifiers" },
        text(report,"observed_utc"), text(report,"app_version"), text(report,"app_build"), text(r,"product"), text(r,"chip_firmware"), text(r,"voltage"));
    if r["mode"] == "ecm_info" {
        for (_, _, label) in crate::vlinker::INFO_READS { out += &format!("{label}: {}\n", text(&r["ecm_identifiers"][label],"value")); }
    } else {
        let codes = r["dtc_report"]["records"].as_array().unwrap();
        out += &format!("{} code records\n",codes.len());
        if codes.is_empty() { out += "No codes reported for mask 12.\n"; }
        for d in codes { out += &format!("{} · failure {} · status {}\n",text(d,"code"),text(d,"failure_type"),text(d,"status")); }
        out += "\nStatus bytes retained as reported; these results do not establish whether the physical CEL is lit.\nECM identifiers were not requested in this engine-code read.\n";
    }
    if report["transport"] == "synthetic-fixture" { out += "\nSynthetic fixture replay · no vehicle connected.\n"; }
    out += "\nBluetooth release confirmed. No codes cleared. ECM-only coverage; other modules were not scanned.\n";
    Ok(out)
}
fn save(root: &Path, v: &Value) -> Result<Value, String> {
    if v["bluetooth_closed"] != true || v["automatic_replay"] != false || v["host_vehicle_request_attempts"] != 1 || v["host_vehicle_requests_written"] != 1 { return Err("No completed probe with confirmed transport release".into()); }
    validate(&v["result"])?;
    let observed = v["observed_utc"].as_str().ok_or("Missing read time")?;
    time::OffsetDateTime::parse(observed, &time::format_description::well_known::Rfc3339).map_err(|_| "Invalid read time")?;
    for field in ["app_version","app_build","transport"] { if !bounded(&v[field]) { return Err("Invalid report metadata".into()); } }
    let id = format!("hscan_ecm_{}", crate::ownership::unique().replace('-', "_"));
    let report = json!({"schema":1,"source":"host-hscan-probe","observed_utc":observed,"saved_utc":time::OffsetDateTime::now_utc().format(&time::format_description::well_known::Rfc3339).unwrap_or_default(),
        "shared_workflow":crate::manifest(),"app_version":v["app_version"],"app_build":v["app_build"],"transport":v["transport"],"bluetooth_closed":true,"automatic_replay":false,"result":v["result"]});
    let bytes = serde_json::to_vec_pretty(&report).map_err(|_| "Cannot encode report")?;
    if bytes.len() as u64 > MAX_BYTES { return Err("ECM report too large".into()); }
    let p = path(root, &id,"json")?; fs::create_dir_all(p.parent().unwrap()).map_err(|_| "Cannot create report folder")?;
    atomic(&p, &bytes)?;
    if let Err(error) = atomic(&path(root,&id,"txt")?, render(&report)?.as_bytes()) { let _ = fs::remove_file(p); return Err(error); }
    Ok(json!({"id":id,"title":title(&report),"saved":true}))
}
pub fn dispatch(v: &Value) -> Result<Value, String> {
    if v["schema"] != 1 { return Err("Unsupported ECM report schema".into()); }
    let root = Path::new(v["root"].as_str().ok_or("Missing report root")?);
    match text(v,"op") {
        "save" => save(root,v),
        "read" => { let report = load(&path(root,text(v,"id"),"json")?)?; Ok(json!({"id":v["id"],"text":render(&report)?,"title":title(&report)})) }
        _ => Err("Unknown ECM report operation".into()),
    }
}
#[no_mangle]
pub unsafe extern "C" fn opensaab_hscan_report_json(input: *const c_char) -> *mut c_char {
    let outcome = std::panic::catch_unwind(|| {
        if input.is_null() { return json!({"error":"Missing report request"}); }
        let bytes = CStr::from_ptr(input).to_bytes();
        if bytes.len() as u64 > MAX_BYTES { return json!({"error":"Report request too large"}); }
        match serde_json::from_slice(bytes).map_err(|_| "Invalid report request".to_owned()).and_then(|v| dispatch(&v)) {
            Ok(result) => result, Err(error) => json!({"error":error}),
        }
    }).unwrap_or_else(|_| json!({"error":"Report operation unavailable"}));
    CString::new(outcome.to_string()).map(CString::into_raw).unwrap_or(std::ptr::null_mut())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request(root: &Path) -> Value {
        json!({"schema":1,"op":"save","root":root,"bluetooth_closed":true,"automatic_replay":false,
            "host_vehicle_request_attempts":1,"host_vehicle_requests_written":1,
            "observed_utc":"2026-10-02T18:00:00Z","app_version":"test","app_build":"1","transport":"synthetic-fixture",
            "result":{"mode":"ecm_dtc","request_origin":"host-hscan-probe","firmware_emulation":false,
                "vin":"YS3FD49YX41000001","product":"vLinker MC+","chip_firmware":"test","voltage":"12.0V",
                "diagnostic_session_stopped":true,"dtc_report":{"mask":"12","complete":true,"codes_cleared":false,
                "records":[{"code":"P0107","failure_type":"00","status":"6F"}]}}})
    }
    #[test]
    fn saved_report_reopens_without_live_identity_and_preserves_scope_and_status() {
        let root=std::env::temp_dir().join(crate::ownership::unique());
        let saved=dispatch(&request(&root)).unwrap();
        let reopened=dispatch(&json!({"schema":1,"op":"read","root":root,"id":saved["id"]})).unwrap();
        let text=reopened["text"].as_str().unwrap();
        for field in ["Saved observation", "mask 12", "P0107 · failure 00 · status 6F", "No codes cleared", "2026-10-02T18:00:00Z"] { assert!(text.contains(field)); }
        assert!(!root.join("last-vehicle.json").exists());
        let json=load(&path(&root,saved["id"].as_str().unwrap(),"json").unwrap()).unwrap();
        assert_eq!(json["shared_workflow"],crate::manifest());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn incomplete_cancelled_wrong_identity_and_bad_selection_never_become_reports() {
        let root=std::env::temp_dir().join(crate::ownership::unique());
        let original=request(&root);
        for (pointer,value) in [("/bluetooth_closed",json!(false)),("/automatic_replay",json!(true)),
            ("/host_vehicle_requests_written",json!(0)),("/result/diagnostic_session_stopped",json!(false)),
            ("/result/dtc_report/complete",json!(false)),("/result/dtc_report/codes_cleared",json!(true)),
            ("/result/dtc_report/mask",json!("FF")),("/result/vin",json!("UNKNOWN"))] {
            let mut bad=original.clone();*bad.pointer_mut(pointer).unwrap()=value;
            assert!(dispatch(&bad).is_err(),"{pointer}");
        }
        for id in ["../../last-vehicle", "hscan_ecm_../../bad", "macos_dtc_1"] {
            assert!(dispatch(&json!({"schema":1,"op":"read","root":root,"id":id})).is_err());
        }
        assert!(!root.exists());
    }
    #[test]
    fn completed_empty_report_is_distinct_from_silence() {
        let root=std::env::temp_dir().join(crate::ownership::unique());
        let mut v=request(&root);v["result"]["dtc_report"]["records"]=json!([]);
        let saved=dispatch(&v).unwrap();
        let read=dispatch(&json!({"schema":1,"op":"read","root":root,"id":saved["id"]})).unwrap();
        assert!(read["text"].as_str().unwrap().contains("No codes reported for mask 12"));
        fs::remove_dir_all(root).unwrap();
    }
}
