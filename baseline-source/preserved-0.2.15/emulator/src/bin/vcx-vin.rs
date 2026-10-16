// SPDX-License-Identifier: MPL-2.0
//! One host-originated live VIN read using the bounded Windows helper.
use std::path::PathBuf;
use std::time::Duration;
use tech2_emu::vcx::{Connection, VinJob};

fn main() {
    let mut args: Vec<_> = std::env::args().skip(1).collect();
    let identity = args.last().is_some_and(|arg| arg == "--identity");
    if identity {
        args.pop();
    }
    if !(2..=3).contains(&args.len()) {
        eprintln!(
            "Usage: vcx-vin USER@WINDOWS_HOST OUTPUT_DIRECTORY [SSH_CONTROL_SOCKET] [--identity]"
        );
        std::process::exit(2);
    }
    let connection = Connection {
        target: args[0].clone(),
        control_path: args.get(2).map(PathBuf::from),
        ecu_profile: Default::default(),
    };
    let result =
        VinJob::start_profile(connection, PathBuf::from(&args[1]), identity).and_then(|mut job| {
            loop {
                let result = job.poll();
                for line in job.take_logs() {
                    println!("{line}");
                }
                if let Some(result) = result {
                    break result;
                }
                std::thread::sleep(Duration::from_millis(25));
            }
        });
    match result {
        Ok(result) => println!(
            "LIVE ECU VIN={} source=VCX-J2534 origin=host-diagnostic log={}",
            result.vin,
            result.log_path.display()
        ),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
