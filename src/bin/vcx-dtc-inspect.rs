// SPDX-License-Identifier: MPL-2.0
//! Validate an existing Windows DTC helper transcript. Does not transmit.
fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 1 {
        eprintln!("Usage: vcx-dtc-inspect HELPER_STDOUT_LOG");
        std::process::exit(2);
    }
    let result = std::fs::metadata(&args[0])
        .map_err(|e| e.to_string())
        .and_then(|m| {
            if m.len() > 65536 {
                return Err("DTC log exceeds 64 KiB".into());
            }
            std::fs::read_to_string(&args[0]).map_err(|e| e.to_string())
        })
        .and_then(|log| tech2_emu::t8_dtc::parse_log(&log));
    match result {
        Ok(records) => {
            println!(
                "VERIFIED_RECORDED_DTC_REPORT records={} live_tx=false",
                records.len()
            );
            for r in records {
                println!(
                    "{} failure_type={:02X} status={:02X}",
                    r.code, r.failure_type, r.status
                );
            }
        }
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
