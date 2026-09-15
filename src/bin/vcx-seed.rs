// SPDX-License-Identifier: MPL-2.0
//! One VIN-guarded BCM level-1 seed request over the bounded Windows helper.
//! No key derivation/submission, automatic retries, or GUI dependency.
use std::{path::PathBuf, time::Duration};
use tech2_emu::vcx::{parse_bcm_seed, Connection, EcuProfile, VinJob};

fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if !(2..=3).contains(&args.len()) {
        return Err("Usage: vcx-seed USER@WINDOWS_HOST NEW_OUTPUT_DIRECTORY [SSH_CONTROL_SOCKET]\nTarget: vehicle 1367 BCM, I-bus 242/642, level 01 only".into());
    }
    let directory = PathBuf::from(&args[1]);
    // Every invocation keeps distinct evidence and must not overwrite a prior result.
    std::fs::create_dir(&directory)
        .map_err(|e| format!("Create fresh seed output directory: {e}"))?;
    let connection = Connection {
        target: args[0].clone(),
        control_path: args.get(2).map(PathBuf::from),
        ecu_profile: EcuProfile::IbusBcm1367,
    };
    let mut job = VinJob::start_bcm_seed(connection, directory.clone())?;
    let result = loop {
        let result = job.poll();
        for line in job.take_logs() {
            println!("{line}");
        }
        if let Some(result) = result {
            break result?;
        }
        std::thread::sleep(Duration::from_millis(25));
    };
    let log = std::fs::read_to_string(&result.log_path).map_err(|e| e.to_string())?;
    let seed = parse_bcm_seed(&log)?;
    let hex = format!("{:02X}{:02X}", seed[0], seed[1]);
    let record = format!("{{\n  \"status\": \"seed_received\",\n  \"origin\": \"host-diagnostic\",\n  \"vin\": \"YS3FH46U681000002\",\n  \"module\": \"BCM\",\n  \"protocol\": 32775,\n  \"baud\": 33333,\n  \"tx_id\": \"242\",\n  \"rx_id\": \"642\",\n  \"level\": \"01\",\n  \"seed_hex\": \"{hex}\",\n  \"zero_seed\": {},\n  \"key_submitted\": false,\n  \"native_guest_transport\": false\n}}\n", seed == [0,0]);
    std::fs::write(directory.join("seed-result.json"), record).map_err(|e| e.to_string())?;
    println!(
        "LIVE BCM SEED={hex} LEVEL=01 VIN={} key_submitted=false log={}",
        result.vin,
        result.log_path.display()
    );
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
