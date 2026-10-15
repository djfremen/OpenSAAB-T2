// SPDX-License-Identifier: MPL-2.0
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;

fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 2 {
        return Err(
            "Usage: openmdi-trace REASSEMBLED_DIRECTION.bin NEW_PRIVATE_OUTPUT.json".into(),
        );
    }
    let input = Path::new(&args[0]);
    let output = Path::new(&args[1]);
    let file = File::open(input).map_err(|e| format!("Open input: {e}"))?;
    if !file
        .metadata()
        .map_err(|e| format!("Inspect input: {e}"))?
        .is_file()
    {
        return Err("Input must be a regular offline file".into());
    }
    let mut bytes = Vec::new();
    file.take((openmdi::trace::MAX_STREAM_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Read input: {e}"))?;
    if bytes.is_empty() {
        return Err("Input contains no records".into());
    }
    let records = openmdi::trace::decode_stream(&bytes)
        .map_err(|e| format!("Decode stopped at byte {}: {}", e.offset, e.reason))?;
    let encoded = serde_json::to_vec_pretty(&records).map_err(|e| format!("Encode output: {e}"))?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(output)
        .map_err(|e| format!("Create a new output file: {e}"))?;
    file.write_all(&encoded)
        .and_then(|_| file.write_all(b"\n"))
        .map_err(|e| format!("Write output: {e}"))?;
    println!(
        "Decoded {} complete records. Output retains source payloads; keep it private.",
        records.len()
    );
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
