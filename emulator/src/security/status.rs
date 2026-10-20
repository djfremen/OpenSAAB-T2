// SPDX-License-Identifier: MPL-2.0
//! Display-only Android preview64 VehicleHistoryStatus/SsaState/receipt rules.
//! Source: matching-source commit9d0498bd4150e03bfc39ebfe2c2e3f02cbe21a49.
//! Status never grants access, invokes a transport/API, writes a file, restores
//! data, or changes the processing/collection/cleanup rules. Session selection
//! adapts Android's card-baseline binding to the desktop's selected image and
//! latest-session pointer, comparing unchanged bytes outside the SSA region.
use super::{hash, read, read_json, valid_vin, vin, Result, CARD_SIZE, OFFSET, SIZE};
use serde_json::{json, Value};
use std::{collections::BTreeMap, fs, path::{Path, PathBuf}};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

const AGE_EXPLANATION: &str = "Fresh means written less than 3 hours ago; Stale means 3 hours or older. This is an age reminder, not a confirmed vehicle-access expiry. Follow the firmware if it requests security access again.";

fn instant(s: &str) -> Option<OffsetDateTime> {
    if s.len() > 64 { return None; }
    OffsetDateTime::parse(s, &Rfc3339).ok()
}
fn classify(b: Option<&[u8]>) -> &'static str {
    let Some(b) = b.filter(|b| b.len() == SIZE) else { return "UNAVAILABLE"; };
    let all = |p: usize, n: usize, v: u8| b[p..p+n].iter().all(|c| *c == v);
    let seed = u16::from_be_bytes([b[0x30], b[0x31]]);
    if all(0x14, 17, 255) && all(0x26, 8, 255) && matches!(seed, 0 | 65535) {
        return "INIT_AUTH";
    }
    let chars = b[0x14..0x25].iter().copied().filter(|c| !matches!(c, 0 | 255)).collect::<Vec<_>>();
    if !chars.is_empty() && chars.iter().all(|c| b"ABCDEFGHJKLMNPRSTUVWXYZ0123456789".contains(c)) {
        if all(0x26, 8, 255) { return "PRE_AUTH"; }
        if !all(0x26, 8, 0) { return "POST_AUTH"; }
    }
    "INVALID"
}
fn label(state: &str) -> &'static str {
    match state {
        "INIT_AUTH" => "[INIT_AUTH]", "PRE_AUTH" => "[PRE-AUTH]",
        "POST_AUTH" => "[POST-AUTH]", "INVALID" => "[INVALID]", _ => "[N/A]",
    }
}
fn description(state: &str) -> (&'static str, &'static str) {
    match state {
        "INIT_AUTH" => ("Security data cleared. Collect fresh data from the vehicle.", "#9ed8ff"),
        "PRE_AUTH" => ("Vehicle data collected. Security response has not been written.", "#ffd77c"),
        "POST_AUTH" => ("Processed security data is present. Continue in the firmware.", "#9fe0b5"),
        "INVALID" => ("Security data does not match a recognized state.", "#ffb59a"),
        _ => ("No readable security data yet.", "#bdcbd8"),
    }
}
fn exact(path: &Path, length: usize) -> Option<Vec<u8>> {
    read(path, length).ok().filter(|b| b.len() == length)
}
fn receipt(path: &Path) -> Option<BTreeMap<String, String>> {
    let bytes = read(path, 8192).ok()?;
    let text = std::str::from_utf8(&bytes).ok()?;
    let mut values = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.starts_with('!') || line.is_empty() { continue; }
        let (key, value) = line.split_once('=')?;
        // Properties emitted by both bindings escape colons. Unknown escape
        // forms remain unknown rather than inventing a trusted timestamp.
        let value = value.trim().replace("\\:", ":");
        if key.contains('\\') || value.contains('\\') || values.insert(key.trim().to_owned(), value).is_some() { return None; }
    }
    let get = |k: &str| values.get(k).map(String::as_str);
    let is_hash = |s: Option<&str>| s.is_some_and(|v| v.len() == 64 && v.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)));
    if get("schema") != Some("1") || !is_hash(get("vehicle_hash"))
        || get("started_utc").and_then(instant).is_none()
        || !matches!(get("stage"), Some("processing" | "processed" | "imported" | "failed")) {
        return None;
    }
    for key in ["pre_auth_utc", "processed_utc", "imported_utc", "failed_utc"] {
        if get(key).is_some_and(|v| instant(v).is_none()) { return None; }
    }
    if get("stage") == Some("imported") && (get("processed_utc").is_none() || get("imported_utc").is_none() || !is_hash(get("ssa_hash"))) { return None; }
    Some(values)
}

struct Selected {
    data: Option<Vec<u8>>,
    run: Option<PathBuf>,
    kind: &'static str,
}
fn unavailable() -> Selected { Selected { data: None, run: None, kind: "unavailable" } }
fn firmware_id(s: &str) -> bool {
    !s.is_empty() && s.len() <= 160 && s.bytes().all(|c| c.is_ascii_alphanumeric() || b"_.-".contains(&c))
}
fn select(state: &Path, selected_card: &Path, firmware: &str, active_run: Option<&str>) -> Selected {
    if !firmware_id(firmware) { return unavailable(); }
    let Ok(card_path) = fs::canonicalize(selected_card) else { return unavailable(); };
    let Ok(firmware_dir) = fs::canonicalize(state.join("firmware")) else { return unavailable(); };
    if card_path.parent() != Some(firmware_dir.as_path()) || card_path.file_name().and_then(|s| s.to_str()) != Some(format!("{firmware}.bin").as_str()) {
        return unavailable();
    }
    let Some(card) = exact(&card_path, CARD_SIZE) else { return unavailable(); };
    let latest_path = state.join("latest-session.json");
    if !latest_path.exists() && active_run.is_none() {
        return Selected { data: Some(card[OFFSET..OFFSET+SIZE].to_vec()), run: None, kind: "selected-card" };
    }
    let Ok(latest) = read_json(&latest_path) else { return unavailable(); };
    if latest["firmware"] != firmware {
        // A different latest session cannot supply this selection's SSA or its
        // receipt. The selected immutable image remains the only source.
        return if active_run.is_none() { Selected { data: Some(card[OFFSET..OFFSET+SIZE].to_vec()), run: None, kind: "selected-card" } } else { unavailable() };
    }
    let Some(pointer) = latest["run"].as_str() else { return unavailable(); };
    let Ok(run) = fs::canonicalize(pointer) else { return unavailable(); };
    let Ok(runs) = fs::canonicalize(state.join("runs")) else { return unavailable(); };
    if run.parent() != Some(runs.as_path()) || active_run.is_some_and(|p| fs::canonicalize(p).ok().as_deref() != Some(run.as_path())) { return unavailable(); }
    let Some(original) = exact(&run.join("card.bin"), CARD_SIZE) else { return unavailable(); };
    if original[..OFFSET] != card[..OFFSET] || original[OFFSET+SIZE..] != card[OFFSET+SIZE..] { return unavailable(); }
    let active = active_run.is_some();
    let working = run.join("working-ssa.bin");
    let after = run.join("ssa-card-after.bin");
    let (data, kind) = if working.exists() {
        (exact(&working, SIZE), if active { "active-working" } else { "saved-working" })
    } else if after.exists() || run.join("native-security-snapshot.json").exists() {
        let valid = read_json(&run.join("native-security-snapshot.json")).ok().is_some_and(|m| m["origin"] == "original-guest-memory" && m["card_offset"] == OFFSET && m["bytes"] == SIZE);
        (if valid { exact(&after, SIZE) } else { None }, if active { "active-snapshot" } else { "saved-snapshot" })
    } else {
        (Some(original[OFFSET..OFFSET+SIZE].to_vec()), if active { "active-card" } else { "saved-card" })
    };
    Selected { data, run: Some(run), kind }
}

/// Read-only worker endpoint. Returned timestamps are the exact stored strings;
/// the OS may localize them, but must never replace them with render/mtime time.
pub(super) fn dispatch(v: &Value) -> Result<Value> {
    if v.as_object().is_none_or(|o| o.keys().any(|k| !["schema","op","state","firmware","card","run","offline","now_utc"].contains(&k.as_str()))) {
        return Err("Unsupported display status fields".into());
    }
    let state_dir = Path::new(super::text(v, "state")?);
    let firmware = v["firmware"].as_str().filter(|s| firmware_id(s)).unwrap_or("");
    let clock = match v["now_utc"].as_str() {
        Some(s) => instant(s).ok_or("Invalid status clock")?,
        None => OffsetDateTime::now_utc(),
    };
    let observation = read_json(&state_dir.join("vehicle-status.json")).ok().filter(|o|
        o["vin"].as_str().is_some_and(|s| s.starts_with("YS3") && valid_vin(s)) && o["observed_utc"].as_str().and_then(instant).is_some())
        .map(|o| {
            // A saved observation is untrusted input. Return only bounded local
            // vehicle metadata, never unrelated credential/SSA fields.
            let mut clean = serde_json::Map::new();
            for key in ["vin","observed_utc","source","adapter","firmware","description","lookup_status","platform","engine","color","gearbox","body"] {
                if let Some(s) = o[key].as_str() {
                    clean.insert(key.to_owned(), json!(s.chars().filter(|c| !c.is_control()).take(160).collect::<String>()));
                }
            }
            if let Some(year) = o["model_year"].as_u64().filter(|y| *y <= 9999) { clean.insert("model_year".to_owned(),json!(year)); }
            Value::Object(clean)
        });
    let observed_vin = observation.as_ref().and_then(|o| o["vin"].as_str());
    let observed_utc = observation.as_ref().and_then(|o| o["observed_utc"].as_str());
    let offline = v["offline"] == true;
    let selected = if offline { unavailable() } else { select(state_dir, Path::new(v["card"].as_str().unwrap_or("")), firmware, v["run"].as_str()) };
    let bytes = selected.data.as_deref();
    let raw_state = classify(bytes);
    let state = if observed_vin.is_none() || (raw_state != "INIT_AUTH" && bytes.and_then(|b| vin(b).ok()) != observed_vin) { "UNAVAILABLE" } else { raw_state };
    let mut auth = format!("auth_status: {}", label(state));
    if state == "INIT_AUTH" { auth.push_str(" · cleared"); }
    let (message, mut color) = description(state);
    let mut freshness = "Age unknown";
    let mut status_utc: Option<String> = None;
    let mut timestamp = "Timestamp unavailable".to_owned();
    let local = selected.run.as_ref().map(|r| r.join("security-processing-status.properties"));
    let path = local.filter(|p| p.exists()).unwrap_or_else(|| state_dir.join("security-processing-status.properties"));
    let receipt = receipt(&path);
    if let (Some(r), Some(vehicle)) = (receipt.as_ref(), observed_vin) {
        if r.get("vehicle_hash") == Some(&hash(vehicle.as_bytes())) {
            if state == "POST_AUTH" && r.get("stage").map(String::as_str) == Some("imported") && bytes.is_some_and(|b| r.get("ssa_hash") == Some(&hash(b))) {
                let imported = &r["imported_utc"];
                let age = clock - instant(imported).expect("validated receipt time");
                if !age.is_negative() {
                    freshness = if age < time::Duration::hours(3) { "Fresh" } else { "Stale" };
                    if freshness == "Stale" { color = "#ffd77c"; }
                }
                auth.push_str(" · "); auth.push_str(freshness);
                status_utc = Some(imported.clone());
                timestamp = format!("Post-auth written · {imported}");
            } else if state == "PRE_AUTH" && r.get("stage").map(String::as_str) != Some("imported") {
                if let Some(collected) = r.get("pre_auth_utc") {
                    status_utc = Some(collected.clone());
                    timestamp = format!("Pre-auth collected · {collected}");
                }
            }
        }
    }
    let summary = auth.replacen("auth_status:", "Security:", 1);
    let connection = format!("Last connection · {}", observed_utc.unwrap_or("Date unknown"));
    let vehicle_summary = observation.as_ref().map(|o| {
        let detail = o["description"].as_str().filter(|s| !s.is_empty()).unwrap_or("");
        if detail.is_empty() { "Last vehicle · Saved VIN observation".to_owned() }
        else { format!("Last vehicle · {}", detail.chars().filter(|c| !c.is_control()).take(160).collect::<String>()) }
    }).unwrap_or_else(|| "No vehicle connected · tap for details".to_owned());
    // Active workflow wording is an observation of the original firmware,
    // separate from saved card classification. A partial PRE_AUTH card alone
    // must never produce a ready-to-process label or bypass processing gates.
    let workflow_summary = if !offline && state != "UNAVAILABLE" && v["run"].as_str().is_some() {
        selected.run.as_ref().and_then(|run| read(&run.join("screen.txt"), 8192).ok().map(|s| (run, s)))
            .and_then(|(run, screen)| {
                let observed = super::observe_run(run, &screen, super::prompt::Cache::Read);
                if observed["transfer"] == true && bytes.is_some_and(|b| super::validate_input(b).is_ok()) {
                    Some("Security status · Ready to process")
                } else if observed["collecting"] == true {
                    Some("Security status · Collecting pre-auth")
                } else { None }
            })
    } else { None };
    // Vehicle identity is local UI history. No SSA, keys, seeds, card hashes,
    // receipt IDs or service credentials are returned or persisted.
    Ok(json!({"schema":1, "display_only":true, "history_only":true,
        "vehicle_access_verified":false, "state":state, "auth_status":auth,
        "summary":summary, "freshness":freshness, "status_utc":status_utc,
        "timestamp":timestamp, "color":color,
        "details":format!("{auth}\n{message}\n{timestamp}\n\n{AGE_EXPLANATION}\nSecurity status describes saved emulator data, not confirmation of current vehicle authorization."),
        "connection":connection, "connection_observed_utc":observed_utc,
        "vehicle_summary":vehicle_summary, "compact_vehicle":vehicle_summary,
        "workflow_summary":workflow_summary,
        "observed_vehicle":observation, "source_kind":if offline {"offline"} else {selected.kind},
        "source_run":selected.run, "source_card_firmware_id":firmware,
        "offline":offline}))
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture(PathBuf);
    impl Drop for Fixture { fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); } }
    const VIN: &str = "YS3FF49Y541000000";
    const OTHER: &str = "YS3FF49Y541000001";
    const NOW: &str = "2026-10-09T19:00:00Z";
    fn pre() -> Vec<u8> { let mut b=vec![255;SIZE];b[20..37].copy_from_slice(VIN.as_bytes());b[0]=0xb1;b }
    fn post() -> Vec<u8> { let mut b=pre();b[0x26..0x2e].fill(b'A');b }
    fn fixture() -> Fixture {
        let p=std::env::temp_dir().join(format!("display-status-{}",super::super::unique()));
        fs::create_dir_all(p.join("runs/current")).unwrap();fs::create_dir(p.join("firmware")).unwrap();
        let card=vec![255;CARD_SIZE];fs::write(p.join("firmware/test.bin"),&card).unwrap();fs::write(p.join("runs/current/card.bin"),&card).unwrap();
        fs::write(p.join("vehicle-status.json"),json!({"vin":VIN,"observed_utc":"2026-10-08T17:00:00Z","adapter":"Nano"}).to_string()).unwrap();
        fs::write(p.join("latest-session.json"),json!({"firmware":"test","run":p.join("runs/current")}).to_string()).unwrap();Fixture(p)
    }
    fn request(p: &Path) -> Value { json!({"schema":1,"op":"status","state":p,"firmware":"test","card":p.join("firmware/test.bin"),"now_utc":NOW}) }
    fn set_working(p:&Path,b:&[u8]) { fs::write(p.join("runs/current/working-ssa.bin"),b).unwrap(); }
    fn save_receipt(p:&Path,b:&[u8],vehicle:&str, imported:&str) {
        fs::write(p.join("security-processing-status.properties"),format!("schema=1\nvehicle_hash={}\nstage=imported\nstarted_utc=2026-10-09T10\\:00\\:00Z\nprocessed_utc=2026-10-09T11\\:00\\:00Z\nimported_utc={}\nssa_hash={}\n",hash(vehicle.as_bytes()),imported,hash(b))).unwrap();
    }
    #[test] fn android_states_and_history_are_not_ecu_grants() {
        let f=fixture();let p=&f.0;let req=request(p);
        for (b, expected) in [(vec![255;SIZE],"INIT_AUTH"),(pre(),"PRE_AUTH"),(post(),"POST_AUTH")] {
            set_working(p,&b);let out=dispatch(&req).unwrap();assert_eq!(out["state"],expected);assert_eq!(out["vehicle_access_verified"],false);assert_eq!(out["connection_observed_utc"],"2026-10-08T17:00:00Z");assert_eq!(out["source_kind"],"saved-working");
        }
        let mut invalid=pre();invalid[0x26..0x2e].fill(0);set_working(p,&invalid);assert_eq!(dispatch(&req).unwrap()["state"],"INVALID");
        fs::remove_file(p.join("vehicle-status.json")).unwrap();assert_eq!(dispatch(&req).unwrap()["state"],"UNAVAILABLE");
    }
    #[test] fn android_receipt_binding_three_hours_future_and_changed_ssa() {
        let f=fixture();let p=&f.0;let b=post();set_working(p,&b);let req=request(p);
        for (t, expected) in [("2026-10-09T16:00:01Z","Fresh"),("2026-10-09T16:00:00Z","Stale"),("2026-10-09T19:00:01Z","Age unknown")] {
            save_receipt(p,&b,VIN,t);let out=dispatch(&req).unwrap();assert_eq!(out["freshness"],expected);assert_eq!(out["status_utc"],t);assert_eq!(out["vehicle_access_verified"],false);
        }
        save_receipt(p,&b,OTHER,"2026-10-09T18:00:00Z");let out=dispatch(&req).unwrap();assert_eq!(out["freshness"],"Age unknown");assert!(out["status_utc"].is_null());
        save_receipt(p,&b,VIN,"2026-10-09T18:00:00Z");let mut changed=b;changed[99]^=1;set_working(p,&changed);let out=dispatch(&req).unwrap();assert_eq!(out["state"],"POST_AUTH");assert_eq!(out["freshness"],"Age unknown");assert!(out["status_utc"].is_null());
    }
    #[test] fn active_working_precedes_card_and_malformed_preferred_data_stays_unknown() {
        let f=fixture();let p=&f.0;set_working(p,&pre());let mut req=request(p);req["run"]=json!(p.join("runs/current"));assert_eq!(dispatch(&req).unwrap()["source_kind"],"active-working");
        set_working(p,&pre()[..713]);assert_eq!(dispatch(&req).unwrap()["state"],"UNAVAILABLE");
        let mut wrong=pre();wrong[20..37].copy_from_slice(OTHER.as_bytes());set_working(p,&wrong);assert_eq!(dispatch(&req).unwrap()["state"],"UNAVAILABLE");
        req["offline"]=json!(true);set_working(p,&post());let out=dispatch(&req).unwrap();assert_eq!(out["state"],"UNAVAILABLE");assert_eq!(out["source_kind"],"offline");assert_eq!(out["connection_observed_utc"],"2026-10-08T17:00:00Z");
    }
    #[test] fn latest_session_firmware_path_and_baseline_never_borrow_other_data() {
        let f=fixture();let p=&f.0;set_working(p,&post());let mut req=request(p);req["run"]=json!(p.join("runs/other"));assert_eq!(dispatch(&req).unwrap()["state"],"UNAVAILABLE");
        req=request(p);let mut card=fs::read(p.join("runs/current/card.bin")).unwrap();card[0]^=1;fs::write(p.join("runs/current/card.bin"),card).unwrap();assert_eq!(dispatch(&req).unwrap()["state"],"UNAVAILABLE");
        fs::write(p.join("latest-session.json"),json!({"firmware":"other","run":p.join("runs/current")}).to_string()).unwrap();let out=dispatch(&req).unwrap();assert_eq!(out["source_kind"],"selected-card");assert_eq!(out["state"],"INIT_AUTH");assert!(out["status_utc"].is_null());
        fs::write(p.join("latest-session.json"),json!({"firmware":"test","run":p.join("firmware")}).to_string()).unwrap();assert_eq!(dispatch(&req).unwrap()["state"],"UNAVAILABLE");
    }
    #[test] fn legacy_snapshot_requires_metadata_and_does_not_claim_collection_complete() {
        let f=fixture();let p=&f.0;let run=p.join("runs/current");fs::write(run.join("ssa-card-after.bin"),pre()).unwrap();assert_eq!(dispatch(&request(p)).unwrap()["state"],"UNAVAILABLE");
        fs::write(run.join("native-security-snapshot.json"),json!({"origin":"original-guest-memory","card_offset":OFFSET,"bytes":SIZE,"ssa_erases":0,"ssa_programmed_bytes":0}).to_string()).unwrap();let out=dispatch(&request(p)).unwrap();assert_eq!(out["state"],"PRE_AUTH");assert_eq!(out["source_kind"],"saved-snapshot");assert_eq!(out["vehicle_access_verified"],false);
    }
    #[test] fn local_receipt_invalid_global_newer_is_not_borrowed() {
        let f=fixture();let p=&f.0;let b=post();set_working(p,&b);save_receipt(p,&b,VIN,"2026-10-09T18:00:00Z");fs::write(p.join("runs/current/security-processing-status.properties"),"schema=garbage").unwrap();let out=dispatch(&request(p)).unwrap();assert_eq!(out["freshness"],"Age unknown");assert!(out["status_utc"].is_null());
    }
    #[test] fn preauth_original_timestamp_not_render_time_or_mtime() {
        let f=fixture();let p=&f.0;set_working(p,&pre());let t="2026-10-07T08:00:00.123Z";fs::write(p.join("security-processing-status.properties"),format!("schema=1\nvehicle_hash={}\nstage=processing\nstarted_utc=2026-10-07T07:59:00Z\npre_auth_utc={t}\n",hash(VIN.as_bytes()))).unwrap();let out=dispatch(&request(p)).unwrap();assert_eq!(out["status_utc"],t);assert_eq!(out["freshness"],"Age unknown");assert_eq!(out["connection_observed_utc"],"2026-10-08T17:00:00Z");
    }
    #[test] fn malformed_clock_receipt_observation_and_no_selection_stay_bounded() {
        let f=fixture();let p=&f.0;set_working(p,&post());let mut req=request(p);req["now_utc"]=json!("bad");assert!(dispatch(&req).is_err());req=request(p);save_receipt(p,&post(),VIN,"bad");assert_eq!(dispatch(&req).unwrap()["freshness"],"Age unknown");
        fs::write(p.join("vehicle-status.json"),json!({"vin":VIN,"observed_utc":"bad"}).to_string()).unwrap();let out=dispatch(&req).unwrap();assert_eq!(out["state"],"UNAVAILABLE");assert!(out["connection_observed_utc"].is_null());req["firmware"]=json!("");req["card"]=json!("");assert_eq!(dispatch(&req).unwrap()["state"],"UNAVAILABLE");
    }
    #[test] fn display_dispatch_does_not_write_and_never_returns_ssa_or_receipt_secrets() {
        let f=fixture();let p=&f.0;let b=post();set_working(p,&b);save_receipt(p,&b,VIN,"2026-10-09T18:00:00Z");let files=["vehicle-status.json","latest-session.json","security-processing-status.properties","firmware/test.bin","runs/current/card.bin","runs/current/working-ssa.bin"];let before=files.map(|name|hash(&fs::read(p.join(name)).unwrap()));let result=super::super::dispatch(&request(p)).unwrap();assert_eq!(result["display_only"],true);assert_eq!(result["vehicle_access_verified"],false);for (name,h) in files.into_iter().zip(before){assert_eq!(hash(&fs::read(p.join(name)).unwrap()),h);}
        let text=result.to_string();for secret in [hash(&b),"SSA_DATA".to_owned(),"source_session".to_owned(),"request_id".to_owned(),"password".to_owned()] {assert!(!text.contains(&secret));}
    }
    #[test] fn observation_whitelist_and_status_request_fields_cannot_echo_secrets() {
        let f=fixture();let p=&f.0;set_working(p,&pre());fs::write(p.join("vehicle-status.json"),json!({"vin":VIN,"observed_utc":"2026-10-08T17:00:00Z","password":"private-marker","ssa":"private-marker","description":"A".repeat(1000)}).to_string()).unwrap();
        let out=dispatch(&request(p)).unwrap();assert!(!out.to_string().contains("private-marker"));assert_eq!(out["observed_vehicle"]["description"].as_str().unwrap().len(),160);
        let mut req=request(p);req["password"]=json!("private-marker");assert!(dispatch(&req).is_err());
    }
    #[test] fn workflow_labels_require_active_original_screen_and_real_input_readiness() {
        let f=fixture();let p=&f.0;let run=p.join("runs/current");set_working(p,&pre());let mut req=request(p);req["run"]=json!(&run);
        assert!(dispatch(&req).unwrap()["workflow_summary"].is_null());
        fs::write(run.join("screen.txt"),"Checking security access\nReading all vehicle VINs\nReading all vehicle seeds").unwrap();
        assert_eq!(dispatch(&req).unwrap()["workflow_summary"],"Security status · Collecting pre-auth");
        fs::write(run.join("screen.txt"),"You need security access from TIS2000\nDisconnect Tech 2 from vehicle").unwrap();
        // A recognized transfer prompt plus an incomplete PRE_AUTH card does
        // not manufacture a ready state.
        assert!(dispatch(&req).unwrap()["workflow_summary"].is_null());
        let mut ready=pre();ready[0x132..0x13a].copy_from_slice(&[0x02,0x41,0x03,0x01,0x12,0x34,0xff,0xff]);set_working(p,&ready);
        assert_eq!(dispatch(&req).unwrap()["workflow_summary"],"Security status · Ready to process");
        assert_eq!(dispatch(&req).unwrap()["vehicle_access_verified"],false);
        req["offline"]=json!(true);assert!(dispatch(&req).unwrap()["workflow_summary"].is_null());
        req=request(p);assert!(dispatch(&req).unwrap()["workflow_summary"].is_null());
        req["run"]=json!(p.join("runs/wrong"));assert!(dispatch(&req).unwrap()["workflow_summary"].is_null());
    }
    #[test] fn localized_card_transfer_prompt_is_ready_without_display_writes() {
        use super::super::prompt::tests::{message, screen, same, DE_STEPS, DE_TITLE};
        let f=fixture();let p=&f.0;let run=p.join("runs/current");
        let block=message([2,0x53,0xe3],&[DE_TITLE],&DE_STEPS);
        for name in ["firmware/test.bin","runs/current/card.bin"] {
            let mut card=fs::read(p.join(name)).unwrap();card[0x1025000..0x1025000+block.len()].copy_from_slice(&block);fs::write(p.join(name),card).unwrap();
        }
        let mut ready=pre();ready[0x132..0x13a].copy_from_slice(&[0x02,0x41,0x03,0x01,0x12,0x34,0xff,0xff]);set_working(p,&ready);
        let mut req=request(p);req["run"]=json!(&run);
        let dashes=[b'-';39];
        fs::write(run.join("screen.txt"),screen(&[DE_TITLE,&dashes,DE_STEPS[0],DE_STEPS[1]],same)).unwrap();
        assert_eq!(dispatch(&req).unwrap()["workflow_summary"],"Security status · Ready to process");
        assert!(!run.join("security-prompt.json").exists());
        fs::write(run.join("screen.txt"),screen(&[DE_TITLE,&dashes],same)).unwrap();
        assert!(dispatch(&req).unwrap()["workflow_summary"].is_null());
    }
}
