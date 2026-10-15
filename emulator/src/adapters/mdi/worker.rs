// SPDX-License-Identifier: MPL-2.0
//! Emulator orchestration: bounded x86 worker, new private logs, cleanup gate.
use super::profile;
use std::{
    fs::{self, OpenOptions},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

pub fn parse_cli(args: &[String]) -> Result<PathBuf, String> {
    match args {
        [mode,output,path] if mode=="--mdi-vin" && output=="--output-dir" && !path.is_empty()=>Ok(PathBuf::from(path)),
        _=>Err("Usage: tech2-emu --mdi-vin --output-dir NEW_PRIVATE_DIRECTORY (Windows classic MDI host VIN operation)".into()),
    }
}
pub fn validate_result(log: &str, exit_ok: bool) -> Result<serde_json::Value, String> {
    if !exit_ok {
        return Err("MDI Rust worker failed; see private driver logs".into());
    }
    let reports = log
        .lines()
        .filter_map(|line| line.strip_prefix("MDI_RESULT="))
        .collect::<Vec<_>>();
    if reports.len() != 1 {
        return Err("Missing or duplicate MDI worker result".into());
    }
    let result: serde_json::Value =
        serde_json::from_str(reports[0]).map_err(|_| "Malformed MDI worker result")?;
    let vin = result["fresh_vin"]
        .as_str()
        .ok_or("MDI result has no fresh VIN")?;
    let mut payload = vec![0x5a, 0x90];
    payload.extend_from_slice(vin.as_bytes());
    crate::vin_probe::assembled_saab_vin(&payload)?;
    if result["schema"] != 1
        || result["adapter"] != "bosch-mdi-classic"
        || result["status"] != "vin_received"
        || result["request_origin"] != "host-startup-discovery"
        || result["vehicle_requests"] != 1
        || result["cleanup_ok"] != true
        || !result["error"].is_null()
        || result["process_bits"] != 32
        || result["driver_sha256"] != profile::DRIVER_SHA
        || result["runtime_sha256"] != profile::RUNTIME_SHA
        || result["pdu_sha256"] != profile::PDU_SHA
        || result["version"][1] != "2.5.33.154"
        || result["version"][2] != "04.04"
        || result["voltage_mv"].as_u64().is_none()
        || result["rx_records"].as_u64().is_none_or(|v| v == 0)
    {
        return Err(
            "MDI result lacks matching driver identity, fresh reply or clean release".into(),
        );
    }
    for required in [
        "PassThruStopMsgFilter rc=0x00000000",
        "PassThruDisconnect rc=0x00000000",
        "PassThruClose rc=0x00000000",
        "CLEAR_RX_BUFFER rc=0x00000000",
    ] {
        if log.lines().filter(|line| *line == required).count() != 1 {
            return Err(format!("Incomplete MDI transaction: {required}"));
        }
    }
    if log
        .lines()
        .filter(|line| line.starts_with("REQUEST CAN="))
        .count()
        != 1
    {
        return Err("MDI request count mismatch".into());
    }
    Ok(result)
}
pub fn run(directory: &Path) -> Result<serde_json::Value, String> {
    if !cfg!(windows) {
        return Err("Classic MDI requires the Windows x86 vendor driver".into());
    }
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    let helper = executable
        .parent()
        .ok_or("Missing emulator directory")?
        .join(profile::HELPER);
    let image = fs::read(&helper).map_err(|e| format!("Missing Rust MDI worker: {e}"))?;
    use sha2::{Digest, Sha256};
    let helper_sha = format!("{:x}", Sha256::digest(&image));
    profile::validate_image(&image, &helper_sha)?;
    let parent = directory
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    fs::create_dir(directory).map_err(|e| format!("Use a new private MDI run directory: {e}"))?;
    let stdout_path = directory.join("mdi-driver.log");
    let stderr_path = directory.join("mdi-driver-stderr.log");
    let stdout = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&stdout_path)
        .map_err(|e| e.to_string())?;
    let stderr = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&stderr_path)
        .map_err(|e| e.to_string())?;
    let started = Instant::now();
    let mut command = Command::new(&helper);
    command
        .arg("--vin")
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(stderr);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command.spawn().map_err(|e| e.to_string())?;
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            break Some(status);
        }
        if started.elapsed() >= Duration::from_secs(25)
            || fs::metadata(&stdout_path).is_ok_and(|m| m.len() > 1024 * 1024)
            || fs::metadata(&stderr_path).is_ok_and(|m| m.len() > 256 * 1024)
        {
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        thread::sleep(Duration::from_millis(20));
    };
    let log = fs::read_to_string(&stdout_path).map_err(|e| e.to_string())?;
    let validated = status.as_ref().map_or_else(
        || Err("MDI worker watchdog stopped the process; native cleanup unconfirmed".into()),
        |s| validate_result(&log, s.success()),
    );
    let mut report = match &validated {
        Ok(result) => result.clone(),
        Err(error) => {
            let mut failure = if status.is_some() {
                let entries = log
                    .lines()
                    .filter_map(|line| line.strip_prefix("MDI_RESULT="))
                    .collect::<Vec<_>>();
                if entries.len() == 1 {
                    serde_json::from_str(entries[0]).unwrap_or(serde_json::Value::Null)
                } else {
                    serde_json::Value::Null
                }
            } else {
                serde_json::Value::Null
            };
            if !failure.is_object() {
                failure = serde_json::json!({"schema":1,"adapter":"bosch-mdi-classic","cleanup_ok":false});
            }
            failure["status"] = "failed".into();
            failure["validation_error"] = error.clone().into();
            failure
        }
    };
    report["emulator_sha256"] = format!(
        "{:x}",
        Sha256::digest(fs::read(executable).map_err(|e| e.to_string())?)
    )
    .into();
    report["worker_sha256"] = helper_sha.into();
    report["worker_exit_code"] = serde_json::json!(status.and_then(|s| s.code()));
    report["elapsed_ms"] = serde_json::json!(started.elapsed().as_millis());
    report["observed_utc"] = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .map_err(|e| e.to_string())?
        .into();
    fs::write(
        directory.join("mdi-result.json"),
        serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    validated.map(|_| report)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn log() -> String {
        let outcome = super::super::init::Outcome {
            vin: Some("YS3FD49YX41000001".into()),
            version: Some(["8.5.33".into(), "2.5.33.154".into(), "04.04".into()]),
            voltage_mv: Some(14000),
            vehicle_requests: 1,
            rx_records: 1,
            cleanup_ok: true,
            error: None,
        };
        format!("CLEAR_RX_BUFFER rc=0x00000000\nREQUEST CAN=7E0 payload=1A90\nPassThruStopMsgFilter rc=0x00000000\nPassThruDisconnect rc=0x00000000\nPassThruClose rc=0x00000000\nMDI_RESULT={}\n",outcome.json())
    }
    #[test]
    fn no_success_from_worker_exit_timeout_missing_cleanup_or_changed_identity() {
        let log = log();
        assert!(validate_result(&log, true).is_ok());
        assert!(validate_result(&log, false).is_err());
        for bad in [
            log.replace("PassThruClose rc=0x00000000", "PassThruClose rc=0x00000001"),
            log.replace(profile::DRIVER_SHA, "changed"),
            log.replace("\"vehicle_requests\":1", "\"vehicle_requests\":2"),
            format!("{log}{log}"),
            log.replace("\"cleanup_ok\":true", "\"cleanup_ok\":false"),
        ] {
            assert!(validate_result(&bad, true).is_err());
        }
    }
    #[test]
    fn host_vin_mode_cannot_mix_original_firmware_or_other_adapters() {
        assert!(parse_cli(&["--mdi-vin".into(), "--output-dir".into(), "new-run".into()]).is_ok());
        assert!(parse_cli(&["--mdi-vin".into(), "card.bin".into()]).is_err());
        assert!(parse_cli(&[
            "--mdi-vin".into(),
            "--output-dir".into(),
            "new-run".into(),
            "--candi-j2534-adapter".into(),
            "nano".into()
        ])
        .is_err());
    }
}
