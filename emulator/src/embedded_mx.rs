// SPDX-License-Identifier: MPL-2.0
//! Common embedded MX+ attachment. Host owns radio bytes; Rust owns CAN policy.
use std::path::Path;
pub fn validate_token(token: &str) -> Result<(), String> {
    if token.len() != 32 || !token.bytes().all(|v| v.is_ascii_hexdigit()) {
        return Err("MX+ requires a valid app-owned read-session token".into());
    }
    Ok(())
}
pub fn attach(bus: &mut crate::bus::Tech2Bus, directory: &Path, output: &Path, token: &str) -> Result<(), String> {
    validate_token(token)?;
    let metadata = std::fs::metadata(output.join("mx-discovery.json")).map_err(|_| "Missing same-session MX+ discovery")?;
    if !metadata.is_file() || metadata.len() > 8192 { return Err("Invalid MX+ discovery file".into()); }
    let proof: serde_json::Value = serde_json::from_slice(&std::fs::read(output.join("mx-discovery.json")).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
    if proof["schema"] != 1 || proof["vehicle"]["adapter_model"] != "obdlink_mxp"
        || proof["discovery_closed"] != true || proof["token_sha256"] != crate::security::hash(token.as_bytes())
        || !proof["vehicle"]["vin"].as_str().is_some_and(crate::security::valid_vin) {
        return Err("MX+ requires its matching fresh discovery and confirmed discovery release".into());
    }
    let mut link = crate::candi::native_link::NativeLink::new(&directory.join("candi.bin"))?;
    link.attach_obdlink(token, output)?;
    bus.pending_candi = None;
    bus.candi_link = Some(link);
    let context = serde_json::json!({"platform":if cfg!(target_os="ios") {"ios"} else {"macos"},"adapter":"OBDLink MX+","vin":proof["vehicle"]["vin"],"observed_utc":proof["vehicle"]["observed_utc"]});
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new(); options.create(true).write(true).truncate(true);
    #[cfg(unix)] { use std::os::unix::fs::OpenOptionsExt; options.mode(0o600); }
    let mut file = options.open(output.join("dtc-context.json")).map_err(|e|e.to_string())?;
    file.write_all(context.to_string().as_bytes()).map_err(|e|e.to_string())?;
    Ok(())
}
#[cfg(test)] mod tests {
    #[test] fn invalid_owner_is_rejected_before_any_firmware_or_radio_access() {
        for token in ["", "short", "0123456789abcdef0123456789abcdeg", "0123456789abcdef0123456789abcdef00"] {
            assert!(super::validate_token(token).is_err());
        }
        assert!(super::validate_token("0123456789abcdef0123456789abcdef").is_ok());
    }
}

/// A native stream close alone cannot qualify the STN protocol cleanup.
pub fn protocol_cleanup_confirmed(output: &Path) -> bool {
    let path = output.join("obdlink-native-transcript-private.jsonl");
    let Ok(meta) = std::fs::metadata(&path) else { return false; };
    if meta.len() > 16*1024*1024 { return false; }
    let Ok(text) = std::fs::read_to_string(path) else { return false; };
    text.lines().filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter_map(|v|v["event"]["cleanup"].as_str().map(str::to_owned)).last().as_deref() == Some("confirmed")
}

#[cfg(test)] mod cleanup_tests {
    #[test] fn latest_cleanup_controls_the_result_and_missing_is_not_success() {
        let root=std::env::temp_dir().join(format!("mx-cleanup-{}-{:?}",std::process::id(),std::thread::current().id()));
        std::fs::create_dir_all(&root).unwrap();
        assert!(!super::protocol_cleanup_confirmed(&root));
        let path=root.join("obdlink-native-transcript-private.jsonl");
        std::fs::write(&path,"{\"event\":{\"cleanup\":\"confirmed\"}}\n").unwrap();
        assert!(super::protocol_cleanup_confirmed(&root));
        std::fs::write(&path,"{\"event\":{\"cleanup\":\"confirmed\"}}\n{\"event\":{\"cleanup\":\"unconfirmed: timeout\"}}\n").unwrap();
        assert!(!super::protocol_cleanup_confirmed(&root));
        std::fs::remove_dir_all(root).unwrap();
    }
}
