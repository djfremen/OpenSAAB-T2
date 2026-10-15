// SPDX-License-Identifier: MPL-2.0
//! Nano probe using the shared protocol over Android USB or native macOS CDC.
use std::{cell::RefCell, collections::BTreeMap, time::Instant};
use std::{
    fs::OpenOptions,
    io::{BufRead, BufReader, Read, Write},
    net::{SocketAddr, TcpStream},
    time::Duration,
};
use tech2_emu::nano_channel::{self, Client, RawCan, UsbTransport};
use tech2_emu::nano_usb::{identity, Frame, ReplyDeadline};

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect()
}
fn dlc_voltage(reply: &Frame) -> Result<u32, String> {
    if reply.header != [0x80, 0, 0x86, 0]
        || reply.payload.len() != 6
        || reply.payload[0] != 0
        || reply.payload[5] != 0x10
    {
        return Err(format!(
            "Unexpected DLC voltage reply: header={} payload={}",
            hex(&reply.header),
            hex(&reply.payload)
        ));
    }
    Ok(u32::from_be_bytes(reply.payload[1..5].try_into().unwrap()))
}
fn unhex(s: &str) -> Result<Vec<u8>, String> {
    if s.len() > 32768 || s.len() % 2 != 0 || !s.is_ascii() {
        return Err("Invalid USB hex length".into());
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|_| "Invalid USB hex".into()))
        .collect()
}
fn line(r: &mut BufReader<TcpStream>) -> Result<String, String> {
    let mut s = String::new();
    let n = r
        .by_ref()
        .take(32775)
        .read_line(&mut s)
        .map_err(|e| e.to_string())?;
    if n == 0 || n >= 32775 || !s.ends_with('\n') {
        return Err("USB transport closed or exceeded line bound".into());
    }
    Ok(s.trim_end().to_owned())
}
fn command(r: &mut BufReader<TcpStream>, text: &str) -> Result<String, String> {
    r.get_mut()
        .write_all(format!("{text}\n").as_bytes())
        .map_err(|e| e.to_string())?;
    line(r)
}
struct SocketUsb(BufReader<TcpStream>);
impl UsbTransport for SocketUsb {
    fn write(&mut self, bytes: &[u8]) -> Result<(), String> {
        println!("USB_TX {} completion=usb-transfer-only", hex(bytes));
        if command(&mut self.0, &format!("TX {}", hex(bytes)))? != "TXOK" {
            return Err("USB write rejected or incomplete".into());
        }
        Ok(())
    }
    fn read(&mut self) -> Result<Vec<u8>, String> {
        let reply = command(&mut self.0, "READ")?;
        if reply == "EMPTY" {
            return Ok(vec![]);
        }
        unhex(
            reply
                .strip_prefix("RX ")
                .ok_or_else(|| format!("Unexpected USB response: {reply}"))?,
        )
    }
}
enum ProbeTransport {
    Android(SocketUsb),
    #[cfg(target_os = "macos")]
    Mac(tech2_emu::adapters::vcx_nano::serial_macos::Port),
}
impl ProbeTransport {
    fn close(&mut self) -> Result<(), String> {
        match self {
            Self::Android(usb) => {
                if command(&mut usb.0, "QUIT")? == "CLOSED" {
                    Ok(())
                } else {
                    Err("USB controller did not confirm close".into())
                }
            }
            #[cfg(target_os = "macos")]
            Self::Mac(port) => port.close().map_err(|e| e.to_string()),
        }
    }
}
impl UsbTransport for ProbeTransport {
    fn write(&mut self, bytes: &[u8]) -> Result<(), String> {
        match self {
            Self::Android(usb) => usb.write(bytes),
            #[cfg(target_os = "macos")]
            Self::Mac(port) => {
                println!("USB_TX {} completion=serial-write-only", hex(bytes));
                port.write(bytes)
            }
        }
    }
    fn read(&mut self) -> Result<Vec<u8>, String> {
        match self {
            Self::Android(usb) => usb.read(),
            #[cfg(target_os = "macos")]
            Self::Mac(port) => {
                let bytes = port.read()?;
                if !bytes.is_empty() {
                    println!("SERIAL_RX {}", hex(&bytes));
                }
                Ok(bytes)
            }
        }
    }
}
// The bridge can cancel at every bounded transport operation. Cleanup retains
// the underlying port and runs without the cancellation gate before release.
struct ControlledTransport {
    inner: ProbeTransport,
    cancelled: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
}
impl ControlledTransport {
    fn check(&self) -> Result<(), String> {
        if self.cancelled.as_ref().is_some_and(|v| v.load(std::sync::atomic::Ordering::SeqCst)) {
            Err("App cancelled the fresh VIN attempt".into())
        } else { Ok(()) }
    }
    fn close(&mut self) -> Result<(), String> { self.inner.close() }
}
impl UsbTransport for ControlledTransport {
    fn write(&mut self, bytes: &[u8]) -> Result<(), String> { self.check()?; self.inner.write(bytes) }
    fn read(&mut self) -> Result<Vec<u8>, String> { self.check()?; self.inner.read() }
}
use tech2_emu::vin_probe::VinReply;
fn host_hs_frame(sequence: u8, id: u32, data: &[u8]) -> Result<Frame, String> {
    use tech2_emu::candi_cpu::{CanElectricalState, CanFrame, CanTransmission};
    // The same read-only gate, electrical route and wire encoder as the guest
    // Nano backend; this helper cannot introduce a separate raw CAN dialect.
    tech2_emu::adapters::vcx_nano::native::encode(0, &CanTransmission {
        ticket: u64::from(sequence),
        frame: CanFrame { id, extended: false, rtr: false, dlc: data.len() as u8, data: data.to_vec() },
        btr0: 0xc1, btr1: 0x36, electrical: CanElectricalState::StandardCan,
    }, sequence)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gm_keepalive_uses_the_shared_read_gate_and_native_hs_encoder() {
        let keepalive = host_hs_frame(1, 0x101, &tech2_emu::vin_probe::SAAB_9440_TESTER_PRESENT).unwrap();
        assert_eq!(keepalive.header, [0x80, 1, 0, 0]);
        assert_eq!(keepalive.payload, [0,0,0,0,0,12,0,0,1,1,0xfe,1,0x3e,0,0,0,0,0]);
        let vin = host_hs_frame(2, 0x7e0, &[2,0x1a,0x90,0,0,0,0,0]).unwrap();
        assert_eq!(vin.header, [0x80, 2, 0, 0]);
        assert_eq!(&vin.payload[6..], &[0,0,7,0xe0,2,0x1a,0x90,0,0,0,0,0]);
        assert!(host_hs_frame(3, 0x7e0, &[0x30,0,0,0,0,0,0,0]).is_ok());
        assert!(host_hs_frame(1, 0x7e0, &[2,0x27,2,0,0,0,0,0]).is_err());
        assert!(host_hs_frame(1, 0x101, &[1,0x04,0,0,0,0,0,0]).is_err());
    }
    #[test]
    fn captured_dlc_voltage_and_invalid_replies() {
        let good = Frame {
            header: [0x80, 0, 0x86, 0],
            payload: vec![0, 0, 0, 0x2e, 0x43, 0x10],
        };
        assert_eq!(dlc_voltage(&good).unwrap(), 11843);
        for index in [0, 5] {
            let mut bad = good.clone();
            bad.payload[index] ^= 1;
            assert!(dlc_voltage(&bad).is_err());
        }
        let mut bad = good.clone();
        bad.payload.pop();
        assert!(dlc_voltage(&bad).is_err());
        let mut bad = good;
        bad.header[2] = 0x8c;
        assert!(dlc_voltage(&bad).is_err());
    }
    #[test]
    fn vin_multiframe_requires_order_and_preserves_all_characters() {
        let mut r = VinReply::default();
        assert!(r
            .feed(&[0x10, 19, 0x5a, 0x90, b'Y', b'S', b'3', b'F'])
            .unwrap());
        assert!(!r
            .feed(&[0x21, b'H', b'4', b'6', b'U', b'6', b'8', b'1'])
            .unwrap());
        assert!(!r
            .feed(&[0x22, b'0', b'0', b'0', b'0', b'0', b'1', 0])
            .unwrap());
        assert_eq!(r.vin().unwrap().as_deref(), Some("YS3FH46U681000001"));
        assert!(r.feed(&[0x22, 0, 0, 0, 0, 0, 0, 0]).is_err());
        assert!(VinReply::default()
            .feed(&[0x21, 0, 0, 0, 0, 0, 0, 0])
            .is_err());
    }
}
#[path = "nano_bridge/mod.rs"]
mod ios_bridge;
fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--ios-bridge-once") {
        return ios_bridge::serve(&args[1..]);
    }
    run_probe(args, None)
}
fn run_probe(mut args: Vec<String>, cancelled: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>) -> Result<(), String> {
    let direct = args.first().map(String::as_str) == Some("--port");
    if direct {
        args.remove(0);
    }
    if !(args.len() == 2
        || (args.len() == 3
            && [
                "--channel-test",
                "--channel-test-sw",
                "--receive-test",
                "--hs-vin-check",
                "--hs-vin-gm-check",
                "--voltage-check",
                "--hardware-reboot",
            ]
            .contains(&args[2].as_str())))
        || (!direct && (args[0].len() != 32 || !args[0].bytes().all(|b| b.is_ascii_hexdigit())))
        || (!direct && args.get(2).map(String::as_str) == Some("--hardware-reboot"))
    {
        return Err(
            "Usage: nano-usb-probe SESSION_TOKEN NEW_RESULT_PATH [MODE] or nano-usb-probe --port /dev/cu.usbmodem... NEW_RESULT_PATH [--channel-test|--channel-test-sw|--receive-test|--hs-vin-check|--hs-vin-gm-check|--voltage-check|--hardware-reboot]"
                .into(),
        );
    }
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args[1])
        .map_err(|e| e.to_string())?;
    let transport = if direct {
        #[cfg(target_os = "macos")]
        {
            ProbeTransport::Mac(
                tech2_emu::adapters::vcx_nano::serial_macos::Port::open(std::path::Path::new(
                    &args[0],
                ))
                .map_err(|e| e.to_string())?,
            )
        }
        #[cfg(not(target_os = "macos"))]
        {
            return Err("Direct CDC port mode currently requires macOS".into());
        }
    } else {
        let stream = TcpStream::connect_timeout(
            &SocketAddr::from(([127, 0, 0, 1], 35673)),
            Duration::from_secs(3),
        )
        .map_err(|e| e.to_string())?;
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .map_err(|e| e.to_string())?;
        stream
            .set_write_timeout(Some(Duration::from_secs(2)))
            .map_err(|e| e.to_string())?;
        stream.set_nodelay(true).map_err(|e| e.to_string())?;
        let mut r = BufReader::new(stream);
        if command(&mut r, &format!("HELLO {}", args[0]))? != "READY adapter-only" {
            return Err("USB app did not accept this session".into());
        }
        ProbeTransport::Android(SocketUsb(r))
    };
    let gm_vin = args.get(2).map(String::as_str) == Some("--hs-vin-gm-check");
    let hs_vin = gm_vin || args.get(2).map(String::as_str) == Some("--hs-vin-check");
    let voltage_check = args.get(2).map(String::as_str) == Some("--voltage-check");
    let hardware_reboot = args.get(2).map(String::as_str) == Some("--hardware-reboot");
    let probe_channel = u8::from(args.get(2).map(String::as_str) == Some("--channel-test-sw"));
    let mut dlc_voltage_mv = None;
    let receive_test = hs_vin || args.get(2).map(String::as_str) == Some("--receive-test");
    let vin_frames = RefCell::new(Vec::<Vec<u8>>::new());
    let mut startup = tech2_emu::adapters::vcx_nano::init_handshake::Progress::default();
    let mut vehicle_tx = 0;
    let mut gm_tester_present_tx = 0;
    let mut vin_result = None;
    let mut client = Client::new(ControlledTransport { inner: transport, cancelled });
    let mut counts = [0u64; 2];
    let mut ids = [BTreeMap::<u32, u64>::new(), BTreeMap::<u32, u64>::new()];
    let mut capture = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(format!("{}.frames.jsonl", args[1]))
        .map_err(|e| e.to_string())?;
    let start = Instant::now();
    let mut capture_bytes = 0;
    let mut observed = |f: &Frame| -> Result<(), String> {
        let row=serde_json::json!({"elapsed_us":start.elapsed().as_micros(),"direction":"rx","header":hex(&f.header),"payload":hex(&f.payload)}).to_string()+"\n";
        capture_bytes += row.len();
        if capture_bytes > 8 * 1024 * 1024 {
            return Err("Decoded capture size limit reached".into());
        }
        capture
            .write_all(row.as_bytes())
            .map_err(|e| e.to_string())?;
        if f.header[2] == 0 {
            let raw = RawCan::decode(f)?;
            let c = raw.channel as usize;
            if hs_vin && c == 0 && raw.id == 0x7e8 && raw.flags == 0 {
                let mut frames = vin_frames.borrow_mut();
                if frames.len() >= 16 {
                    return Err("VIN response queue limit".into());
                }
                frames.push(raw.data.clone());
            }
            counts[c] += 1;
            if ids[c].len() >= 512 && !ids[c].contains_key(&raw.id) {
                return Err("CAN ID diversity limit exceeded".into());
            }
            *ids[c].entry(raw.id).or_default() += 1;
            if counts[c] <= 8 {
                println!(
                    "CAN_RX channel={} id={:08X} flags={:08X} data={} sequence={:02X}",
                    c,
                    raw.id,
                    raw.flags,
                    hex(&raw.data),
                    raw.sequence
                );
            }
        } else {
            println!(
                "CONTROL_RX header={} payload={}",
                hex(&f.header),
                hex(&f.payload)
            );
        }
        Ok(())
    };
    let mut attempted = [false; 2];
    let result: Result<_, String> = (|| {
        let echo = Frame::command(0x80, b"\0TEST");
        if client.exchange(&echo, &mut observed)? != echo {
            return Err("Nano echo mismatch".into());
        }
        let identity_reply = client.exchange(&Frame::command(0x8c, &[]), &mut observed)?;
        let info = identity(&identity_reply)?;
        println!(
            "IDENTIFIED hardware={} firmware={} vehicle_tx=0",
            info.hardware, info.firmware
        );
        if direct && !hardware_reboot && !voltage_check && args.len() == 3 {
            tech2_emu::adapters::vcx_nano::init_handshake::init_handshake(&mut client, &identity_reply, &mut observed, &mut startup)?;
        }
        let mut status = "identified";
        if hardware_reboot {
            // Exact adapter-only VCX_DEV_Reset command observed in the original
            // SDK on this Nano. Acknowledgment does not establish cold readiness
            // or loss of OBD power; re-open and test the channel independently.
            let reply = client.exchange(&Frame::command(0x8d, &[]), &mut observed)?;
            if reply.payload != [0] {
                return Err(format!("Nano reboot rejected: {}", hex(&reply.payload)));
            }
            status = "adapter_reboot_acknowledged";
        } else if voltage_check {
            // OEM VCX_DEV_GetDlcVol for OBD pin 16, captured with its decoded log.
            // This queries the adapter ADC; it does not transmit on the vehicle bus.
            let reply = client.exchange(&Frame::command(0x86, &[0x10]), &mut observed)?;
            let mv = dlc_voltage(&reply)?;
            dlc_voltage_mv = Some(mv);
            println!("DLC_VOLTAGE pin=16 millivolts={mv} source=adapter-adc vehicle_tx=0");
            status = "dlc_voltage_read";
        } else if receive_test {
            for c in 0..if hs_vin { 1 } else { 2 } {
                attempted[c as usize] = true;
                for request in nano_channel::setup(c)? {
                    let reply = client.exchange(&request, &mut observed)?;
                    if reply.payload != [0] {
                        return Err(format!(
                            "Channel {c} opcode {:02X} rejected: {}",
                            request.header[2],
                            hex(&reply.payload)
                        ));
                    }
                }
                println!("RAW_CHANNEL_STARTED channel={c} diagnostic_tx=0 mode=normal-can");
            }
            let deadline = ReplyDeadline::new(Duration::from_secs(if hs_vin { 2 } else { 8 }));
            while !deadline.expired() {
                client.receive(&mut observed)?;
            }
            status = "channels_receive_test_completed";
            if hs_vin {
                if gm_vin {
                    client.transport_mut().write(&host_hs_frame(1, 0x101, &tech2_emu::vin_probe::SAAB_9440_TESTER_PRESENT)?.encode()?)?;
                    vehicle_tx += 1;
                    gm_tester_present_tx += 1;
                    println!("HOST_PROBE functional tester-present 101: FE 01 3E 00 00 00 00 00; USB write returned; bus acknowledgment not inferred");
                    // Captured Tech2Win keep-alive precedes VIN. Receive during
                    // a bounded settling interval; never replay the VIN request.
                    let settle = ReplyDeadline::new(Duration::from_millis(100));
                    while !settle.expired() { client.receive(&mut observed)?; }
                }
                vin_frames.borrow_mut().clear();
                println!("HOST_PROBE VIN request 7E0: 02 1A 90 00 00 00 00 00; not guest firmware traffic");
                client
                    .transport_mut()
                    .write(&host_hs_frame(if gm_vin {2} else {1}, 0x7e0, &[2, 0x1a, 0x90, 0, 0, 0, 0, 0])?.encode()?)?;
                vehicle_tx += 1;
                let mut reply = VinReply::default();
                let deadline = ReplyDeadline::new(Duration::from_secs(4));
                while !deadline.expired() {
                    client.receive(&mut observed)?;
                    for data in vin_frames.borrow_mut().drain(..) {
                        println!("VIN_WIRE_RX 7E8 {}", hex(&data));
                        if reply.feed(&data)? {
                            client
                                .transport_mut()
                                .write(&host_hs_frame(if gm_vin {3} else {2}, 0x7e0, &[0x30, 0, 0, 0, 0, 0, 0, 0])?.encode()?)?;
                            vehicle_tx += 1;
                        }
                        if let Some(vin) = reply.vin()? {
                            vin_result = Some(vin);
                            break;
                        }
                    }
                    if vin_result.is_some() {
                        break;
                    }
                }
                status = if vin_result.is_some() {
                    "hs_vin_received"
                } else {
                    "hs_vin_no_reply"
                };
                println!("HOST_PROBE VIN status={status} diagnostic_tx={vehicle_tx}");
            }
        } else if args.len() == 3 {
            attempted[probe_channel as usize] = true;
            let reply = client.exchange(
                &nano_channel::control(probe_channel, 0x40, &[0, 0, 0x81, 1]),
                &mut observed,
            )?;
            if reply.payload != [0] {
                return Err(format!(
                    "Raw channel {probe_channel} OPEN rejected: status {}",
                    hex(&reply.payload)
                ));
            }
            status = "channel_open_close_verified";
        }
        Ok((status, info))
    })();
    // Best-effort channel shutdown even when initialization or reception fails.
    // USB release alone does not establish that the adapter stopped its channels.
    client.transport_mut().cancelled = None; // Cancellation cannot interrupt adapter cleanup.
    let mut cleanup_errors = vec![];
    for c in (0..2).rev() {
        if !attempted[c] {
            continue;
        }
        let requests = if receive_test {
            nano_channel::shutdown(c as u8).to_vec()
        } else {
            vec![nano_channel::control(c as u8, 0x41, &[])]
        };
        for request in requests {
            match client.exchange(&request, &mut observed) {
                Ok(reply) if reply.payload == [0] => {}
                other => cleanup_errors.push(format!(
                    "channel {c} cleanup {:02X}: {other:?}",
                    request.header[2]
                )),
            }
        }
    }
    let cleanup = client.transport_mut().close();
    drop(observed);
    // Save failures too: an empty result file hides the exact boundary where a
    // headless probe stopped. Keep the original error and cleanup separately.
    let usb_closed = cleanup.is_ok();
    let cleanup_ok = cleanup_errors.is_empty() && usb_closed;
    let error = result.as_ref().err().cloned().or_else(|| {
        if !cleanup_errors.is_empty() {
            Some(cleanup_errors.join("; "))
        } else if !usb_closed {
            Some(format!("USB cleanup was not confirmed: {cleanup:?}"))
        } else {
            None
        }
    });
    let (status, info) = match &result {
        Ok((status, info)) => (*status, Some(info)),
        Err(_) => ("failed", None),
    };
    println!(
        "CAN_RX_SUMMARY hs={} sw={} diagnostic_tx={vehicle_tx}",
        counts[0], counts[1]
    );
    let json=serde_json::json!({"startup_complete":startup.installed_record_verified,"query_record_verified":startup.query_record_verified,"installed_record_verified":startup.installed_record_verified,"fresh_dh_exchanges":startup.fresh_dh_exchanges,"status":if error.is_some(){"failed"}else{status},"error":error,"hardware":info.map(|v| &v.hardware),"firmware":info.map(|v| &v.firmware),"vehicle_tx":vehicle_tx,"gm_tester_present_tx":gm_tester_present_tx,"vin_profile":if gm_vin {tech2_emu::vin_probe::SAAB_9440_VIN_PROFILE}else{"basic-1a90"},"vin":vin_result,"dlc_voltage_mv":dlc_voltage_mv,"request_origin":if hs_vin {"host-transport-probe"}else{"none"},"origin":if direct {"macos-native-cdc-serial"}else{"android-direct-usb"},"channel_rx":counts,"channel_ids":ids,"channel_cleanup":cleanup_ok,"cleanup_errors":cleanup_errors,"usb_closed":usb_closed}).to_string()+"\n";
    output
        .write_all(json.as_bytes())
        .map_err(|e| e.to_string())?;
    error.map_or(Ok(()), Err)
}
fn main() {
    if let Err(e) = run() {
        eprintln!("Nano USB probe failed: {e}");
        std::process::exit(1);
    }
}
