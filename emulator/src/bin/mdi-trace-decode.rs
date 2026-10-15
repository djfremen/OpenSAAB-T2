// SPDX-License-Identifier: MPL-2.0
//! Offline private trace tool. Does not access adapters or load vendor DLLs.
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::Path,
};
use tech2_emu::adapters::mdi::trace::{decode_stream, MAX_STREAM_BYTES};

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 2 {
        return Err("usage: mdi-trace-decode REASSEMBLED-DIRECTION.bin PRIVATE-OUTPUT.json".into());
    }
    let mut bytes = Vec::new();
    File::open(&args[0])?
        .take((MAX_STREAM_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    let records =
        decode_stream(&bytes).map_err(|e| format!("decode offset {}: {}", e.offset, e.reason))?;
    let output = serde_json::to_vec(&serde_json::json!({"schema": 1,
        "profile": "classic-mdi-8.5.33.154-observed", "records": records}))?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    // Output contains raw private bytes; never overwrite or print it to stdout.
    let mut file = options.open(Path::new(&args[1]))?;
    file.write_all(&output)?;
    file.sync_all()?;
    println!(
        "Decoded {} records from {} bytes to private output.",
        records.len(),
        bytes.len()
    );
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
