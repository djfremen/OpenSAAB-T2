// SPDX-License-Identifier: MPL-2.0
#[cfg(target_os = "macos")]
fn main() {
    use std::{
        fs,
        os::unix::fs::{symlink, PermissionsExt},
        path::PathBuf,
    };
    let result = (|| -> Result<serde_json::Value, String> {
        let mut args = std::env::args().skip(1);
        let mut values = std::collections::BTreeMap::new();
        while let Some(k) = args.next() {
            if !["--target", "--directory", "--runtime", "--card", "--mode"].contains(&k.as_str())
                || values.contains_key(&k)
            {
                return Err("Invalid MDI worker option".into());
            }
            values.insert(k, args.next().ok_or("Missing MDI argument")?);
        }
        let get = |k: &str| values.get(k).ok_or_else(|| format!("Missing {k}"));
        let dir = PathBuf::from(get("--directory")?);
        let runtime = PathBuf::from(get("--runtime")?);
        let card = PathBuf::from(get("--card")?);
        if !dir.is_absolute()
            || !runtime.is_absolute()
            || card != dir.join("card.bin")
            || !dir.is_dir()
        {
            return Err("MDI run boundary".into());
        }
        let mode = get("--mode")?;
        if !["read", "seeds", "full"].contains(&mode.as_str()) {
            return Err("Invalid explicit MDI mode".into());
        }
        let staging = dir.join("mdi-runtime");
        fs::create_dir(&staging).map_err(|e| e.to_string())?;
        fs::set_permissions(&staging, fs::Permissions::from_mode(0o700))
            .map_err(|e| e.to_string())?;
        for name in ["eprom.bin", "opsys.dwn", "candi.bin"] {
            symlink(runtime.join(name), staging.join(name)).map_err(|e| e.to_string())?;
        }
        symlink(&card, staging.join("card.bin")).map_err(|e| e.to_string())?;
        let executable = std::env::current_exe()
            .map_err(|e| e.to_string())?
            .with_file_name("tech2-emu");
        if dir.join("mdi-stop").exists() {
            return Err("Stopped before MDI USB open".into());
        }
        let usb = opensaab_mdi_workflow::macos::MacUsb::open(get("--target")?)?;
        opensaab_mdi_workflow::run(Box::new(usb), 6, &dir, &executable, &staging, mode, false)
    })();
    let report = result.unwrap_or_else(|e| serde_json::json!({"success":false,"error":e}));
    let args: Vec<_> = std::env::args().collect();
    if let Some(index) = args.iter().position(|a| a == "--directory") {
        if let Some(path) = args.get(index + 1) {
            let dir = PathBuf::from(path);
            let _ = fs::write(
                dir.join("mdi-result-private.json"),
                serde_json::to_vec_pretty(&report).unwrap(),
            );
            let mut evidence:serde_json::Value=fs::read(dir.join("adapter-diagnostics.json")).ok().and_then(|b|serde_json::from_slice(&b).ok()).unwrap_or_else(||serde_json::json!({"schema":1,"family":"mdi","transport":"usb_rndis","compatibility":"unknown","stage":"identification"}));
            evidence["state"] = serde_json::json!(if report["success"] == true {
                "stopped"
            } else if report["workflow"]["native_firmware"].is_object() {
                "session_failed"
            } else {
                "initialization_failed"
            });
            evidence["protocol_cleanup"] = serde_json::json!(if report["workflow"]
                ["native_firmware"]["transport_cleanup_ok"]
                == true
            {
                "succeeded"
            } else {
                "failed"
            });
            evidence["usb_release"] = serde_json::json!(if report["usb_closed"] == true {
                "succeeded"
            } else {
                "unknown"
            });
            let _ = fs::write(dir.join("adapter-diagnostics.json"), evidence.to_string());
        }
    }
    if let Some(error) = report["error"]
        .as_str()
        .or_else(|| report["carrier_error"].as_str())
        .or_else(|| report["workflow"]["error"].as_str())
    {
        eprintln!("MDI connection/session failed: {error}");
    }
    println!("MDI session finished; private receipt retained.");
    std::process::exit(if report["success"] == true { 0 } else { 1 });
}
#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("Native Mac MDI USB binding required");
    std::process::exit(2);
}
