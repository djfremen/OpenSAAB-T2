// SPDX-License-Identifier: MPL-2.0
//! Offline file inspection only; never accesses an adapter or vendor DLL.
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
};
use tech2_emu::adapters::mongoose::trace::{review, MAX_CAPTURE_BYTES};
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 2 {
        return Err("usage: mongoose-trace-decode USB.pcap NEW-SAFE-REPORT.json".into());
    }
    let mut bytes = Vec::new();
    File::open(&args[0])?
        .take((MAX_CAPTURE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    let report = review(&bytes).map_err(|s| format!("Capture review failed: {s}"))?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&args[1])?;
    file.write_all(&serde_json::to_vec_pretty(&report)?)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    println!("Reviewed {} RPC records, {} matched replies, {} complete VIN responses. Report excludes VIN values and raw payloads.",
        report["decoded_rpc_records"],report["matched_rpc_replies"],report["complete_vin_responses"]);
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
