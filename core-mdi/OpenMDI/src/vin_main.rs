// SPDX-License-Identifier: MPL-2.0
use std::{
    fs::OpenOptions,
    io::Write,
    net::{IpAddr, SocketAddr},
    path::Path,
};
fn run() -> Result<bool, String> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 2 {
        return Err(
            "Usage: openmdi-vin MDI_IP NEW_PRIVATE_OUTPUT.json (one read-only Saab HS VIN attempt)"
                .into(),
        );
    }
    let ip: IpAddr = args[0]
        .to_str()
        .ok_or("Invalid IP argument")?
        .parse()
        .map_err(|_| "Expected an explicit MDI IP address")?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(Path::new(&args[1]))
        .map_err(|e| format!("Create new private result before connecting: {e}"))?;
    let attempt = openmdi::client::read_vin(SocketAddr::new(ip, 10123));
    file.write_all(&serde_json::to_vec_pretty(&attempt.report).map_err(|e| e.to_string())?)
        .and_then(|_| file.write_all(b"\n"))
        .map_err(|e| e.to_string())?;
    println!(
        "{}",
        serde_json::json!({"status":attempt.report["status"],"elapsed_ms":attempt.report["elapsed_ms"],"vehicle_requests":attempt.report["vehicle_requests"],"cleanup_ok":attempt.report["cleanup_ok"],"error":attempt.report["error"]})
    );
    Ok(attempt.passed)
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
