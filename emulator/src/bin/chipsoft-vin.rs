// SPDX-License-Identifier: MPL-2.0
//! Native macOS Chipsoft VIN read: GET_INFO, one raw HS-CAN channel, one
//! ISO-TP `1A 90` request to 0x7E0, reply from 0x7E8, then channel teardown.
//! Same wire sequence as `chipsoft-usb-probe --vin-check` on Android, over the
//! exclusively opened USB CDC port (macOS `/dev/cu.usbmodem*`, Windows `COMn`
//! under the inbox usbser driver) instead of the app-owned USB socket. No
//! vendor J2534 library on any platform.
//! Default VIN-only path does not change session. Optional --dtc-read uses
//! Android's T8 diagnostic-session/read/stop sequence. No clears, security or programming.

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn run() -> Result<(), Box<dyn std::error::Error>> {
    use std::time::{Duration, Instant};
    use tech2_emu::{
        candi_cpu::{CanElectricalState, CanFrame, CanTransmission},
        chipsoft::Frame,
        chipsoft_channel::{command, decode_read, setup, transmit, words, Client},
        chipsoft_serial, vin_probe,
    };

    let mut args: Vec<_> = std::env::args().skip(1).collect();
    let dtc_read = args.last().is_some_and(|a| a == "--dtc-read");
    if dtc_read { args.pop(); }
    if args.as_slice() == ["--help"] || args.as_slice() == ["-h"] {
        println!("Usage: chipsoft-vin --port /dev/cu.usbmodemNAME|COMn [--timeout-ms 8000] [--dtc-read]\n\nIdentifies Chipsoft Pro (GET_INFO), opens raw HS-CAN 500 kbit/s, sends one\nISO 15765 request 7E0: 1A 90 and reads the 7E8 reply. Sends one flow-control\nframe. Closes the channel and device before exit. With --dtc-read: fresh VIN then Android's T8 session start / A9-81 current-history read / session stop. No clearing or security access.");
        return Ok(());
    }
    if !((args.len() == 2 || args.len() == 4 && args[2] == "--timeout-ms") && args[0] == "--port") {
        return Err("Usage: chipsoft-vin --port /dev/cu.usbmodemNAME|COMn [--timeout-ms 8000] [--dtc-read]".into());
    }
    let timeout_ms = if args.len() == 4 { args[3].parse::<u64>()? } else { 8000 };
    if !(1000..=30000).contains(&timeout_ms) {
        return Err("timeout must be 1000..30000 milliseconds".into());
    }
    let started = Instant::now();
    let deadline = started + Duration::from_millis(timeout_ms);
    let log = |line: String| eprintln!("{:8.3}ms {line}", started.elapsed().as_secs_f64() * 1000.0);
    eprintln!(
        "VIN_START port={} deadline_ms={timeout_ms} request=7E0:1A90 protocol=HS-CAN-500k scope=read-only",
        args[1]
    );
    let port = chipsoft_serial::Port::open(std::path::Path::new(&args[1]))?;
    let mut c = Client::new(port);

    // 1. Identity, exactly as chipsoft-probe.
    let info = c.checked(&command(1, vec![]))?;
    let identity = String::from_utf8(info.payload).map_err(|e| e.to_string())?;
    if !identity.starts_with("CHIPSOFT J2534 Pro v. ")
        || !identity.bytes().all(|b| (0x20..=0x7e).contains(&b))
    {
        return Err("Unsupported device identity".into());
    }
    log(format!("IDENTIFIED {identity}"));

    // 2. Open device and one raw HS-CAN channel. Teardown always runs below.
    let mut device_open = false;
    let mut channel_open = false;
    let mut vin_result: Option<String> = None;
    let mut dtc_result = None;
    let mut vehicle_tx = 0u32;
    let mut raw_rx = 0u64;
    let mut empty_statuses = 0u64;
    let vin_tx = |data: Vec<u8>| -> Result<Frame, String> {
        transmit(
            0,
            &CanTransmission {
                ticket: 1,
                frame: CanFrame { id: 0x7e0, extended: false, rtr: false, dlc: 8, data },
                btr0: 0xc1,
                btr1: 0x36,
                electrical: CanElectricalState::StandardCan,
            },
        )
    };
    let operation = (|| -> Result<(), String> {
        c.checked(&command(8, vec![]))?;
        device_open = true;
        for f in setup(5)? {
            let r = c.checked(&f)?;
            channel_open = true;
            log(format!(
                "CHANNEL_REPLY opcode={:04X} ({}) payload={:02X?}",
                r.opcode,
                tech2_emu::chipsoft::opcode_name(r.opcode),
                r.payload
            ));
        }
        log("HOST_VIN_REQUEST 7E0: 02 1A 90 00 00 00 00 00".into());
        c.checked(&vin_tx(vec![2, 0x1a, 0x90, 0, 0, 0, 0, 0])?)?;
        vehicle_tx += 1;
        let mut reply = vin_probe::VinReply::default();
        while Instant::now() < deadline {
            let r = c.exchange(&command(0x10, words(&[5, 10])))?;
            if r.status == 0x85 && r.payload.is_empty() {
                empty_statuses += 1;
                continue;
            }
            for frame in decode_read(&r)? {
                raw_rx += 1;
                log(format!(
                    "RAW_RX id={:03X} flags={:08X} timestamp={} data={:02X?}",
                    frame.id, frame.flags, frame.timestamp, frame.data
                ));
                if frame.protocol == 5 && frame.id == 0x7e8 && frame.flags == 0 {
                    if reply.feed(&frame.data)? {
                        log("HOST_FLOW_CONTROL 7E0: 30 00 00 00 00 00 00 00".into());
                        c.checked(&vin_tx(vec![0x30, 0, 0, 0, 0, 0, 0, 0])?)?;
                        vehicle_tx += 1;
                    }
                    if let Some(v) = reply.vin()? {
                        vin_result = Some(v);
                    }
                }
            }
            if vin_result.is_some() {
                break;
            }
        }
        if vin_result.is_none() {
            return Err(format!(
                "No complete VIN reply from 7E8 within {timeout_ms} ms (raw_rx={raw_rx}); check ignition, P-bus wiring and ECU power"
            ));
        }
        if dtc_read {
            dtc_result = Some(tech2_emu::chipsoft_dtc::read(&mut c)?);
            vehicle_tx += 3;
        }
        Ok(())
    })();

    // 3. Teardown in reverse, reported separately from the read result.
    let mut cleanup = true;
    if channel_open && c.checked(&command(5, words(&[5]))).is_err() {
        cleanup = false;
    }
    if device_open && c.checked(&command(0x20, vec![])).is_err() {
        cleanup = false;
    }
    drop(c);
    log(format!(
        "SERIAL_CLOSED channel_closed={cleanup} raw_rx={raw_rx} empty_status_0085={empty_statuses} vehicle_commands_sent={vehicle_tx}"
    ));
    operation?;
    if !cleanup {
        return Err("Read succeeded but channel/device teardown failed; reconnect the adapter before continuing".into());
    }
    let vin = vin_result.expect("checked above");
    let model_year = vin_probe::saab_tech2_year(&vin).ok();
    println!("IDENTITY={identity}");
    println!("VIN={vin}");
    match model_year {
        Some(year) => println!("MODEL_YEAR={year}"),
        None => println!("MODEL_YEAR=unknown"),
    }
    println!(
        "CHIPSOFT_VIN_RESULT {}",
        serde_json::json!({
            "status": "vin_received",
            "identity": identity,
            "vin": vin,
            "model_year": model_year,
            "elapsed_ms": started.elapsed().as_millis(),
            "raw_rx": raw_rx,
            "empty_status_0085": empty_statuses,
            "vehicle_commands_sent": vehicle_tx,
            "channel_closed": cleanup,
            "request_origin": "host-startup-discovery",
        })
    );
    if let Some(records) = dtc_result {
        let observed = time::OffsetDateTime::now_utc().format(&time::format_description::well_known::Rfc3339)?;
        let report = tech2_emu::chipsoft_dtc::startup_report(&vin, &identity, &observed, &records);
        println!("CHIPSOFT_STARTUP_RESULT {}", report);
    }
    Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn run() -> Result<(), Box<dyn std::error::Error>> {
    Err("chipsoft-vin supports macOS USB CDC ports and Windows COM ports only".into())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("VIN_FAILED {error}");
        std::process::exit(1);
    }
}
