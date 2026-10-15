// SPDX-License-Identifier: MPL-2.0
//! Internal x86 driver process, invoked by the Windows Rust emulator.
fn main() -> std::process::ExitCode {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if let [flag, value] = args.as_slice() {
        if flag == "--readiness-profile" {
            use tech2_emu::adapters::mdi::readiness;
            let stage = match readiness::Stage::parse(value) {
                Ok(stage) => stage,
                Err(error) => {
                    eprintln!("{error}");
                    return 2.into();
                }
            };
            #[cfg(all(windows, target_arch = "x86"))]
            {
                use tech2_emu::adapters::mdi::native;
                let mut driver = match native::Native::new() {
                    Ok(driver) => driver,
                    Err(error) => {
                        eprintln!("{error}");
                        return 1.into();
                    }
                };
                let result = readiness::run(&mut driver, stage);
                println!("MDI_READINESS={}", result.json());
                return if result.success() { 0.into() } else { 1.into() };
            }
            #[cfg(not(all(windows, target_arch = "x86")))]
            {
                let _ = stage;
                eprintln!("MDI readiness controls require Windows x86");
                return 2.into();
            }
        }
    }
    #[cfg(all(windows, target_arch = "x86"))]
    if args == ["--ibus-probe"] {
        return tech2_emu::adapters::mdi::server::probe().into();
    }
    let raw_port = match args.as_slice() {
        [mode] if mode == "--raw-listen" => Some(None),
        [mode, flag, port] if mode == "--bridge" && flag == "--port" => match port.parse::<u16>() {
            Ok(p) if p != 0 => Some(Some(p)),
            _ => {
                eprintln!("Invalid MDI bridge port");
                return 2.into();
            }
        },
        [mode] if mode == "--vin" => None,
        _ => {
            eprintln!("Internal MDI worker requires --vin, --raw-listen or --bridge --port PORT");
            return 2.into();
        }
    };
    #[cfg(all(windows, target_arch = "x86"))]
    if let Some(port) = raw_port {
        return tech2_emu::adapters::mdi::server::main(port).into();
    }
    #[cfg(not(all(windows, target_arch = "x86")))]
    let _ = raw_port;
    if args != ["--vin"] {
        eprintln!("MDI raw worker requires Windows x86");
        return 2.into();
    }
    #[cfg(all(windows, target_arch = "x86"))]
    {
        use tech2_emu::adapters::mdi::{init, native};
        let result = match native::Native::new() {
            Ok(mut driver) => init::read_vin(&mut driver),
            Err(error) => init::Outcome {
                cleanup_ok: true,
                error: Some(error),
                ..Default::default()
            },
        };
        println!("MDI_RESULT={}", result.json());
        if result.success() {
            0.into()
        } else {
            1.into()
        }
    }
    #[cfg(not(all(windows, target_arch = "x86")))]
    {
        eprintln!("Classic MDI worker requires Windows x86");
        2.into()
    }
}
