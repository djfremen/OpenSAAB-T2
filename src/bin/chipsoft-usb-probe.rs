// SPDX-License-Identifier: MPL-2.0
//! Android-owned USB, native Rust framing. This stage sends GET_INFO only.
use std::{
    fs::OpenOptions,
    io::{BufReader, Write},
    net::TcpStream,
    time::{Duration, Instant},
};
use tech2_emu::{
    chipsoft::{Decoder, Frame},
    nano_backend::SocketUsb,
    nano_channel::UsbTransport,
};

fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let vin_check = args.len() == 3 && args[2] == "--vin-check";
    let receive = vin_check || (args.len() == 3 && args[2] == "--receive-test");
    if !(args.len() == 2 || receive)
        || args[0].len() != 32
        || !args[0].bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err("Expected session token and fresh result path".into());
    }
    let mut result_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args[1])
        .map_err(|e| e.to_string())?;
    let stream =
        TcpStream::connect_timeout(&"127.0.0.1:35674".parse().unwrap(), Duration::from_secs(2))
            .map_err(|e| e.to_string())?;
    stream.set_nodelay(true).map_err(|e| e.to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_secs(1)))
        .map_err(|e| e.to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(1)))
        .map_err(|e| e.to_string())?;
    let mut usb = SocketUsb(BufReader::new(stream));
    if usb.command(&format!("HELLO {}", args[0]))?
        != if vin_check {
            "READY chipsoft-vin"
        } else if receive {
            "READY chipsoft-receive"
        } else {
            "READY chipsoft-identity"
        }
    {
        return Err("Wrong app transport mode".into());
    }
    let started = Instant::now();
    let mut operation = (|| {
        usb.write(&[1, 0, 0, 0, 0, 0, 0, 0])?;
        let mut decoder = Decoder::default();
        while started.elapsed() < Duration::from_secs(3) {
            let bytes = usb.read()?;
            if bytes.is_empty() {
                continue;
            }
            let mut frames: Vec<Frame> = Vec::new();
            decoder
                .feed(&bytes, |f| frames.push(f))
                .map_err(|e| e.to_string())?;
            if frames.is_empty() {
                continue;
            }
            decoder.finish().map_err(|e| e.to_string())?;
            if frames.len() != 1 || frames[0].opcode != 1 || frames[0].status != 0 {
                return Err("Unexpected GET_INFO response".into());
            }
            let info = String::from_utf8(frames.remove(0).payload).map_err(|e| e.to_string())?;
            if !info.starts_with("CHIPSOFT J2534 Pro v. ")
                || !info.bytes().all(|b| (0x20..=0x7e).contains(&b))
            {
                return Err("Unsupported device identity".into());
            }
            return Ok(info);
        }
        Err(format!("GET_INFO deadline expired; {:?}", decoder.finish()))
    })();
    let mut vin_result = None;
    let mut vehicle_tx = 0u32;
    let mut counts = [0u64; 2];
    let mut empty_statuses = 0;
    let mut channel_cleanup = true;
    if receive && operation.is_ok() {
        use tech2_emu::chipsoft_channel::{command, decode_read, setup, words, Client, PROTOCOLS};
        let mut c = Client::new(usb);
        let mut opened = [false; 2];
        let channel_result = (|| -> Result<(), String> {
            c.checked(&command(8, vec![]))?;
            for (i, protocol) in PROTOCOLS
                .iter()
                .take(if vin_check { 1 } else { 2 })
                .enumerate()
            {
                opened[i] = true;
                for f in setup(*protocol)? {
                    let r = c.checked(&f)?;
                    println!(
                        "CHANNEL_REPLY protocol={protocol:04X} opcode={:04X} payload={:02X?}",
                        r.opcode, r.payload
                    );
                }
            }
            let mut vin_reply = tech2_emu::vin_probe::VinReply::default();
            let vin_tx = |data: Vec<u8>| {
                use tech2_emu::candi_cpu::{CanElectricalState, CanFrame, CanTransmission};
                tech2_emu::chipsoft_channel::transmit(
                    0,
                    &CanTransmission {
                        ticket: 1,
                        frame: CanFrame {
                            id: 0x7e0,
                            extended: false,
                            rtr: false,
                            dlc: 8,
                            data,
                        },
                        btr0: 0xc1,
                        btr1: 0x36,
                        electrical: CanElectricalState::StandardCan,
                    },
                )
            };
            if vin_check {
                println!("HOST_VIN_REQUEST 7E0: 02 1A 90 00 00 00 00 00; startup discovery, not guest traffic");
                c.checked(&vin_tx(vec![2, 0x1a, 0x90, 0, 0, 0, 0, 0])?)?;
                vehicle_tx += 1;
            }
            let until = Instant::now() + Duration::from_secs(if vin_check { 4 } else { 8 });
            while Instant::now() < until {
                for protocol in PROTOCOLS.iter().take(if vin_check { 1 } else { 2 }) {
                    let r = c.exchange(&command(0x10, words(&[*protocol, 10])))?;
                    if r.status == 0x85 && r.payload.is_empty() {
                        empty_statuses += 1;
                        continue;
                    }
                    for frame in decode_read(&r)? {
                        // The OEM read wrapper routes each returned record. A
                        // single reply can contain both active protocols.
                        let i = PROTOCOLS
                            .iter()
                            .position(|p| *p == frame.protocol)
                            .ok_or("Unopened raw channel")?;
                        counts[i] += 1;
                        if vin_check && frame.protocol == 5 && frame.id == 0x7e8 && frame.flags == 0
                        {
                            if vin_reply.feed(&frame.data)? {
                                c.checked(&vin_tx(vec![0x30, 0, 0, 0, 0, 0, 0, 0])?)?;
                                vehicle_tx += 1;
                            }
                            if let Some(v) = vin_reply.vin()? {
                                vin_result = Some(v);
                            }
                        }

                        println!("RAW_RX protocol={:04X} id={:03X} flags={:08X} timestamp={} data={:02X?}",frame.protocol,frame.id,frame.flags,frame.timestamp,frame.data);
                    }
                }
                if vin_result.is_some() {
                    break;
                }
            }
            Ok(())
        })();
        for i in (0..2).rev() {
            if opened[i] && c.checked(&command(5, words(&[PROTOCOLS[i]]))).is_err() {
                channel_cleanup = false;
            }
        }
        if c.checked(&command(0x20, vec![])).is_err() {
            channel_cleanup = false;
        }
        usb = c.transport;
        if let Err(e) = channel_result {
            operation = Err(e);
        }
    }
    // Always release USB; failures request the app's emergency channel cleanup.
    let cleanup = usb.command(if channel_cleanup { "QUIT" } else { "ABORT" });
    let identity = operation?;
    if cleanup? != "CLOSED" {
        return Err("USB cleanup not confirmed".into());
    }
    let report = serde_json::json!({"status":if vin_check {if vin_result.is_some(){"vin_received"}else{"vin_no_reply"}}else if receive{"raw_receive_complete"}else{"identified"},"identity":identity,"elapsed_ms":started.elapsed().as_millis(),"channel_rx":counts,"empty_status_0085":empty_statuses,"vehicle_commands_sent":vehicle_tx,"vin":vin_result,"model_year":vin_result.as_deref().and_then(|v|tech2_emu::vin_probe::saab_tech2_year(v).ok()),"request_origin":if vin_check{"host-startup-discovery"}else{"none"},"usb_closed":true});
    writeln!(result_file, "{}", report).map_err(|e| e.to_string())?;
    println!("CHIPSOFT_RESULT {report}");
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("CHIPSOFT_ERROR {e}");
        std::process::exit(1);
    }
}
