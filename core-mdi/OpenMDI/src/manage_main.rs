// SPDX-License-Identifier: MPL-2.0
use std::{
    fs::OpenOptions,
    io::Write,
    net::IpAddr,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
fn run() -> Result<bool, String> {
    let a: Vec<_> = std::env::args_os().skip(1).collect();
    if a.len() != 5 {
        return Err("Usage: openmdi-manage MDI_IP SERIAL PRIVATE_BASE_KEY.bin query|start|reboot NEW_PRIVATE_OUTPUT.json".into());
    }
    let ip: IpAddr = a[0]
        .to_str()
        .ok_or("IP encoding")?
        .parse()
        .map_err(|_| "Explicit IP required")?;
    let serial: u32 = a[1]
        .to_str()
        .ok_or("Serial encoding")?
        .parse()
        .map_err(|_| "Explicit serial required")?;
    let mode = a[3].to_str().ok_or("Mode encoding")?;
    let key = std::fs::read(Path::new(&a[2])).map_err(|e| format!("Private key read: {e}"))?;
    if key.len() != 56 || serial == 0 || !matches!(mode, "query" | "start" | "reboot") {
        return Err("Key/serial/mode boundary".into());
    }
    let mut opts = OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut output = opts
        .open(Path::new(&a[4]))
        .map_err(|e| format!("Exclusive private result before adapter access: {e}"))?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let report = openmdi::management::attempt(
        ip,
        serial,
        &key,
        mode,
        &format!("OpenMDI-{now}-{}", std::process::id()),
    );
    output
        .write_all(&serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?)
        .and_then(|_| output.write_all(b"\n"))
        .map_err(|e| e.to_string())?;
    println!(
        "{}",
        serde_json::json!({"mode":mode,"success":report["success"],"elapsed_ms":report["elapsed_ms"],"acknowledged":report["acknowledged"],"management_released":report["management_released"],"vehicle_requests":0,"errors":report["errors"]})
    );
    Ok(report["success"] == true)
}
fn main() {
    match run() {
        Ok(true) => {}
        Ok(false) => std::process::exit(1),
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2)
        }
    }
}
