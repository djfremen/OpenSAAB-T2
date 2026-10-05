// SPDX-License-Identifier: MPL-2.0
//! Original CANdi traffic over the Android app's USB connection.
//! The worker owns blocking I/O. There are no host-generated ECU requests.
use crate::{
    can_adapter::{Backend, Event},
    candi_cpu::CanTransmission,
    nano_channel::{self, Client, UsbTransport},
    nano_native::{CommandGate, Profile, TxLedger},
};
use std::{
    fs::OpenOptions,
    io::{BufReader, Write},
    net::TcpStream,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender},
        Arc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use super::{init_handshake, native::{full_native_allowed, KeyStatusGate}, protocol::Frame};
use crate::adapters::common::usb::hex;
pub use crate::adapters::common::usb::SocketUsb;
#[cfg(test)]
use std::io::BufRead;

pub struct Bridge {
    input: Option<SyncSender<(usize, CanTransmission)>>,
    output: Receiver<Event>,
    error: Receiver<String>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    confirmations: u64,
    profile: Profile,
    gate: CommandGate,
    key_status: KeyStatusGate,
    full_native: bool,
}
// Original Add may wait for an operator. Total research-session budgets do not
// belong to that explicit mode; I/O deadlines, queues and cleanup stay bounded.
fn session_budget_error(full_native: bool, elapsed: Duration, received: u64) -> Option<&'static str> {
    if full_native {
        None
    } else if elapsed > Duration::from_secs(300) {
        Some("Native USB session deadline expired")
    } else if received > 300_000 {
        Some("Native RX capture/queue budget exceeded")
    } else {
        None
    }
}
impl Bridge {
    pub fn start(token: &str, directory: &Path, profile: Profile) -> Result<Self, String> {
        Self::start_at(
            token,
            directory,
            profile,
            "127.0.0.1:35673".parse().unwrap(),
        )
    }
    pub fn start_key_status(token: &str, directory: &Path) -> Result<Self, String> {
        Self::start_at_mode(
            token,
            directory,
            Profile::Read,
            true,
            "127.0.0.1:35673".parse().unwrap(),
        )
    }
    pub fn start_full_native(token: &str, directory: &Path) -> Result<Self, String> {
        Self::start_at_modes(token, directory, Profile::Read, false, true,
            "127.0.0.1:35673".parse().unwrap())
    }
    pub fn start_seeds(token: &str, directory: &Path) -> Result<Self, String> {
        Self::start_at_permissions(token, directory, Profile::Seeds, false, false, true,
            "127.0.0.1:35673".parse().unwrap())
    }
    fn start_at(
        token: &str,
        directory: &Path,
        profile: Profile,
        address: std::net::SocketAddr,
    ) -> Result<Self, String> {
        Self::start_at_mode(token, directory, profile, false, address)
    }
    fn start_at_mode(
        token: &str,
        directory: &Path,
        profile: Profile,
        key_status: bool,
        address: std::net::SocketAddr,
    ) -> Result<Self, String> {
        Self::start_at_modes(token, directory, profile, key_status, false, address)
    }
    fn start_at_modes(
        token: &str,
        directory: &Path,
        profile: Profile,
        key_status: bool,
        full_native: bool,
        address: std::net::SocketAddr,
    ) -> Result<Self, String> {
        Self::start_at_permissions(token, directory, profile, key_status, full_native, false, address)
    }
    fn start_at_permissions(
        token: &str,
        directory: &Path,
        profile: Profile,
        key_status: bool,
        full_native: bool,
        manual_seeds: bool,
        address: std::net::SocketAddr,
    ) -> Result<Self, String> {
        if (key_status && full_native) || ((key_status || full_native) && profile != Profile::Read) {
            return Err("Nano native permissions require exclusive Read-based native-manual mode".into());
        }
        if manual_seeds && (key_status || full_native || profile != Profile::Seeds) {
            return Err("Nano manual seed collection requires exclusive Seeds mode".into());
        }
        let stream = TcpStream::connect_timeout(&address, Duration::from_secs(3))
            .map_err(|e| e.to_string())?;
        stream.set_nodelay(true).map_err(|e| e.to_string())?;
        stream
            .set_read_timeout(Some(Duration::from_secs(1)))
            .map_err(|e| e.to_string())?;
        stream
            .set_write_timeout(Some(Duration::from_secs(1)))
            .map_err(|e| e.to_string())?;
        let mut usb = SocketUsb(BufReader::new(stream));
        if usb.command(&format!("HELLO {token}"))?
            != if manual_seeds {
                "READY nano-seeds"
            } else if full_native {
                "READY nano-full-native"
            } else if key_status {
                "READY native-key-status"
            } else {
                "READY native-firmware"
            }
        {
            return Err("App not in native firmware mode".into());
        }
        let mut log = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(directory.join("native-android-nano.log"))
            .map_err(|e| e.to_string())?;
        let (input, commands) = mpsc::sync_channel::<(usize, CanTransmission)>(16);
        let (events, output) = mpsc::sync_channel(4096);
        let (errors, error) = mpsc::sync_channel(1);
        let (ready, started) = mpsc::sync_channel(1);
        let stop = Arc::new(AtomicBool::new(false));
        let cancelled = stop.clone();
        let worker = thread::spawn(move || {
            let mut client = Client::new(usb);
            let mut opened = [false; 2];
            let started_at = Instant::now();
            let mut rx = 0u64;
            let mut ledger = if full_native {
                TxLedger::with_full_native()
            } else {
                TxLedger::with_key_status(key_status)
            };
            let mut observe = |frame: &crate::nano_usb::Frame| -> Result<(), String> {
                if frame.header[2] != 0 {
                    return Ok(());
                }
                rx = rx.saturating_add(1);
                // The original RX budget did not impose a second elapsed-time
                // check during startup; retain that behavior for restricted modes.
                if let Some(error) = session_budget_error(full_native, Duration::ZERO, rx) {
                    return Err(error.into());
                }
                writeln!(
                    log,
                    "RX t_us={} header={} payload={}",
                    started_at.elapsed().as_micros(),
                    hex(&frame.header),
                    hex(&frame.payload)
                )
                .map_err(|e| e.to_string())?;
                // Log before decoding so a rejected/short adapter reply is
                // retained alongside the original CANdi request.
                let event = crate::nano_native::receive(frame)?;
                events
                    .try_send(event)
                    .map_err(|_| "Native RX event queue full/closed".to_string())
            };
            let result = (|| -> Result<(), String> {
                if cancelled.load(Ordering::Relaxed) {
                    return Err("Native USB cancelled before initialization".into());
                }
                let identity_reply = client.exchange(&Frame::command(0x8c, &[]), &mut |frame| {
                    if frame.header[2] == 0 {
                        Err("Unexpected CAN before Nano initialization".into())
                    } else {
                        Ok(())
                    }
                })?;
                let mut startup = init_handshake::Progress::default();
                init_handshake::init_handshake(
                    &mut client,
                    &identity_reply,
                    &mut |_| Ok(()),
                    &mut startup,
                )?;
                println!("NATIVE_USB_INIT startup_complete={} query_record_verified={} installed_record_verified={} fresh_dh_exchanges={} vehicle_tx=0", startup.installed_record_verified, startup.query_record_verified, startup.installed_record_verified, startup.fresh_dh_exchanges);
                for channel in 0..2 {
                    // Mark before allocation so a failed reply still triggers cleanup.
                    opened[channel as usize] = true;
                    for request in nano_channel::setup(channel)? {
                        if cancelled.load(Ordering::Relaxed)
                            || started_at.elapsed() > Duration::from_secs(60)
                        {
                            return Err("Native USB cancelled during setup".into());
                        }
                        let reply = client.exchange(&request, &mut observe)?;
                        println!(
                            "NATIVE_USB_SETUP channel={channel} opcode={:02X} reply={}",
                            request.header[2],
                            hex(&reply.payload)
                        );
                        if reply.payload != [0] {
                            return Err(format!(
                                "Nano rejected channel {channel} setup opcode {:02X}: status {}",
                                request.header[2],
                                hex(&reply.payload)
                            ));
                        }
                    }
                }
                ready.try_send(()).map_err(|_| "Native startup cancelled")?;
                while !cancelled.load(Ordering::Relaxed) {
                    if let Some(error) = session_budget_error(full_native, started_at.elapsed(), 0) {
                        return Err(error.into());
                    }
                    for _ in 0..16 {
                        match commands.try_recv() {
                            Ok((controller, tx)) => {
                                let p = ledger.prepare_for_profile(
                                    controller,
                                    &tx,
                                    Instant::now(),
                                    profile,
                                )?;
                                println!("NATIVE_USB_TX controller={controller} ticket={} id={:03X} data={} origin=original-candi",tx.ticket,tx.frame.id,hex(&tx.frame.data));
                                let result =
                                    client.transport_mut().write(&p.wire).map(|_| p.wire.len());
                                let done = ledger.usb_write_finished(
                                    controller,
                                    tx.ticket,
                                    result,
                                    Instant::now(),
                                )?;
                                events
                                    .try_send(done)
                                    .map_err(|_| "Native completion event queue full/closed")?;
                            }
                            Err(mpsc::TryRecvError::Empty) => break,
                            Err(mpsc::TryRecvError::Disconnected) => return Ok(()),
                        }
                    }
                    client.receive(&mut observe)?;
                }
                Ok(())
            })();
            // Stop creating guest traffic before closing both physical channels.
            ledger.close();
            let mut cleanup = true;
            for channel in [1, 0] {
                if opened[channel as usize] {
                    for request in nano_channel::shutdown(channel) {
                        // Cleanup consumes real traffic but never queues it back to a stopped guest.
                        // A rejected filter-disable must not skip stop/close.
                        // Each exchange remains bounded; never claim that an
                        // unacknowledged cleanup succeeded.
                        match client.exchange(&request, &mut |_| Ok(())) {
                            Ok(reply) => {
                                println!(
                                    "NATIVE_USB_CLEANUP channel={channel} opcode={:02X} reply={}",
                                    request.header[2],
                                    hex(&reply.payload)
                                );
                                cleanup &= reply.payload == [0];
                            }
                            Err(error) => {
                                println!("NATIVE_USB_CLEANUP channel={channel} opcode={:02X} error={error}", request.header[2]);
                                cleanup = false;
                            }
                        }
                    }
                }
            }
            if client.transport_mut().command("QUIT").as_deref() != Ok("CLOSED") {
                cleanup = false;
            }
            println!("NATIVE_USB_CLOSED cleanup_ok={cleanup} generated_diagnostic_requests=0");
            if let Err(e) = result {
                let _ = errors.try_send(e);
            } else if !cleanup {
                let _ = errors.try_send("Native USB cleanup incomplete".into());
            }
        });
        let mut bridge = Self {
            input: Some(input),
            output,
            error,
            stop,
            worker: Some(worker),
            confirmations: 0,
            gate: CommandGate::default(),
            key_status: KeyStatusGate::new(key_status),
            full_native,
            profile,
        };
        // Six initialization and fourteen channel controls each keep the
        // existing3s reply bound. Cancellation still closes attempted channels.
        if started.recv_timeout(Duration::from_secs(65)).is_err() {
            bridge.close();
            return Err(bridge
                .error
                .try_recv()
                .unwrap_or_else(|_| "Native USB startup deadline/worker failure".into()));
        }
        Ok(bridge)
    }
}
impl Backend for Bridge {
    fn poll(&mut self) -> Result<Vec<Event>, String> {
        if let Ok(error) = self.error.try_recv() {
            self.close();
            return Err(error);
        }
        if self.stop.load(Ordering::Relaxed) {
            return Err("Native USB bridge closed".into());
        }
        let mut batch = Vec::new();
        for _ in 0..64 {
            match self.output.try_recv() {
                Ok(event) => {
                    if matches!(event, Event::Completed { .. }) {
                        self.confirmations += 1;
                    }
                    batch.push(event);
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(_) => return Err("Native USB worker disconnected".into()),
            }
        }
        Ok(batch)
    }
    fn transmit(&mut self, controller: usize, tx: CanTransmission) -> Result<(), String> {
        let allowed = if self.full_native {
            full_native_allowed(controller, &tx)
        } else {
            self.gate.allowed(controller, &tx, self.profile, Instant::now())
                || self.key_status.allowed(controller, &tx)
        };
        if !allowed {
            return Err("Native USB command rejected by collection profile".into());
        }
        self.input
            .as_ref()
            .ok_or("Native USB bridge closed")?
            .try_send((controller, tx))
            .map_err(|_| "Native USB TX queue full/closed".into())
    }
    fn confirmations(&self) -> u64 {
        self.confirmations
    }
    fn close(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.input.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
impl Drop for Bridge {
    fn drop(&mut self) {
        self.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        candi_cpu::{CanElectricalState, CanFrame},
        nano_usb::{Decoder, Frame},
    };
    /// Synthetic Nano peer for the real shared initializer. No adapter opened.
    fn handshake_reply(
        frame: &Frame,
        stage: &mut usize,
        key: &mut Option<[u8; 16]>,
    ) -> Option<Frame> {
        if *stage >= init_handshake::EXPECTED.len() {
            return None;
        }
        let (op, width) = init_handshake::EXPECTED[*stage];
        assert_eq!(frame.header, [0x80, 0, op, 0]);
        assert_eq!(frame.payload.len(), width);
        let mut payload = vec![0];
        match op {
            0x8c => {
                let mut info = [0; 64];
                info[16..24].copy_from_slice(&[9; 8]);
                info[28..38].copy_from_slice(b"VCX-NANO\0\0");
                info[52..56].copy_from_slice(&[2, 4, 9, 1]);
                payload.extend_from_slice(&info);
            }
            0xa0 => {
                let private = 21575960585 + *stage as u64;
                let peer = init_handshake::Dh::generate(&mut || Ok(private), None).unwrap();
                let host = u64::from_le_bytes(frame.payload.as_slice().try_into().unwrap());
                *key = Some(peer.key(&[9; 8], host).unwrap());
                payload.extend_from_slice(&peer.public.to_le_bytes());
            }
            0x84 | 0xa2 => {
                let mut request = frame.payload.clone();
                init_handshake::crypt(&mut request, key.as_ref().unwrap(), true).unwrap();
                assert_eq!(&request[..16], &init_handshake::query());
                payload.extend(
                    init_handshake::seal(
                        &init_handshake::query(),
                        key.as_ref().unwrap(),
                        init_handshake::Trailer {
                            filetime: 123456789,
                            ticks: 123,
                        },
                    )
                    .unwrap(),
                );
            }
            0xa1 => {
                let mut request = frame.payload.clone();
                init_handshake::crypt(&mut request, key.as_ref().unwrap(), true).unwrap();
                assert_eq!(&request[..144], &init_handshake::selector());
            }
            _ => unreachable!(),
        }
        *stage += 1;
        Some(Frame {
            header: frame.header,
            payload,
        })
    }
    #[test]
    fn unsupported_identity_releases_usb_without_auth_channel_or_can() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut r = BufReader::new(stream);
            let mut decoder = Decoder::default();
            let mut reply = None;
            let mut writes = 0;
            loop {
                let mut line = String::new();
                r.read_line(&mut line).unwrap();
                assert!(!line.is_empty());
                let response = if line.starts_with("HELLO ") {
                    "READY native-firmware".into()
                } else if let Some(hex) = line.strip_prefix("TX ") {
                    let hex = hex.trim();
                    let bytes = (0..hex.len())
                        .step_by(2)
                        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
                        .collect::<Vec<_>>();
                    let frames = decoder.feed(&bytes).unwrap();
                    assert_eq!(frames.len(), 1);
                    assert_eq!(frames[0].header, [0x80, 0, 0x8c, 0]);
                    assert_eq!(writes, 0, "No auth/channel after unknown identity");
                    let mut stage = 0;
                    let mut info = handshake_reply(&frames[0], &mut stage, &mut None).unwrap();
                    info.payload[53] = 3;
                    reply = Some(info.encode().unwrap());
                    writes += 1;
                    "TXOK".into()
                } else if line.trim() == "READ" {
                    format!("RX {}", hex(&reply.take().unwrap()))
                } else if line.trim() == "QUIT" {
                    writeln!(r.get_mut(), "CLOSED").unwrap();
                    break;
                } else {
                    panic!("Unexpected request");
                };
                writeln!(r.get_mut(), "{response}").unwrap();
            }
            assert_eq!(writes, 1);
        });
        let directory =
            std::env::temp_dir().join(format!("nano-bad-identity-test-{}", std::process::id()));
        std::fs::create_dir(&directory).unwrap();
        let error = match Bridge::start_at("test-token", &directory, Profile::Seeds, address) {
            Ok(_) => panic!("Unknown firmware must fail"),
            Err(e) => e,
        };
        assert!(error.contains("firmware differs"), "{error}");
        server.join().unwrap();
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn failed_allocation_and_filter_cleanup_still_stop_and_close_channel() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut r = BufReader::new(stream);
            let mut decoder = Decoder::default();
            let mut pending = std::collections::VecDeque::new();
            let mut opcodes = Vec::new();
            let mut auth_stage = 0;
            let mut auth_key = None;
            loop {
                let mut line = String::new();
                r.read_line(&mut line).unwrap();
                assert!(!line.is_empty());
                let response = if line.starts_with("HELLO ") {
                    "READY native-firmware".to_string()
                } else if let Some(h) = line.strip_prefix("TX ") {
                    let h = h.trim();
                    let bytes: Vec<_> = (0..h.len())
                        .step_by(2)
                        .map(|i| u8::from_str_radix(&h[i..i + 2], 16).unwrap())
                        .collect();
                    for frame in decoder.feed(&bytes).unwrap() {
                        assert_eq!(frame.header[3], 0);
                        let opcode = frame.header[2];
                        opcodes.push(opcode);
                        if let Some(reply) = handshake_reply(&frame, &mut auth_stage, &mut auth_key)
                        {
                            pending.push_back(reply.encode().unwrap());
                            continue;
                        }
                        let status = match opcode {
                            0x40 => 0xfe,
                            0x48 => 0x96,
                            0x43 | 0x41 => 0,
                            _ => panic!("Unexpected command after setup rejection: {opcode:02X}"),
                        };
                        pending.push_back(
                            Frame {
                                header: frame.header,
                                payload: vec![status],
                            }
                            .encode()
                            .unwrap(),
                        );
                    }
                    "TXOK".to_string()
                } else if line.trim() == "READ" {
                    format!("RX {}", hex(&pending.pop_front().expect("reply queued")))
                } else if line.trim() == "QUIT" {
                    writeln!(r.get_mut(), "CLOSED").unwrap();
                    break;
                } else {
                    panic!("Unexpected command: {line}");
                };
                writeln!(r.get_mut(), "{response}").unwrap();
            }
            assert_eq!(
                opcodes,
                [0x8c, 0xa0, 0x84, 0xa0, 0xa1, 0xa2, 0x40, 0x48, 0x43, 0x41]
            );
        });
        let directory =
            std::env::temp_dir().join(format!("nano-rejected-setup-test-{}", std::process::id()));
        std::fs::create_dir(&directory).unwrap();
        let result = Bridge::start_at("test-token", &directory, Profile::Seeds, address);
        let error = match result {
            Ok(_) => panic!("Rejected setup must fail"),
            Err(e) => e,
        };
        assert!(error.contains("opcode 40: status FE"), "{error}");
        server.join().unwrap();
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn fake_usb_runs_native_tx_rx_and_acknowledged_cleanup() {
        scripted_worker(false, false, false);
    }
    #[test]
    fn rejected_queued_key_status_write_has_no_completion_and_six_cleanup_replies() {
        scripted_worker(true, false, false);
    }
    #[test]
    fn explicit_full_native_forwards_original_key_through_frontend_and_worker() {
        scripted_worker(false, true, false);
    }
    #[test]
    fn full_native_total_budget_exemption_keeps_restricted_boundaries() {
        assert_eq!(session_budget_error(false, Duration::from_secs(300), 300_000), None);
        assert!(session_budget_error(false, Duration::from_secs(301), 0).is_some());
        assert!(session_budget_error(false, Duration::ZERO, 300_001).is_some());
        assert_eq!(session_budget_error(true, Duration::MAX, u64::MAX), None);
    }
    #[test]
    fn full_native_rejects_wrong_ready_or_permission_mix_before_init() {
        let nowhere = "127.0.0.1:1".parse().unwrap();
        for (profile, key_status) in [(Profile::Read, true), (Profile::Seeds, false), (Profile::ClearDtc, false)] {
            assert!(Bridge::start_at_modes("token", Path::new("unused"), profile, key_status, true, nowhere)
                .err().unwrap().contains("exclusive"));
        }
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            stream.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
            let mut r = BufReader::new(stream);
            let mut line = String::new();
            r.read_line(&mut line).unwrap();
            assert_eq!(line.trim(), "HELLO token");
            writeln!(r.get_mut(), "READY native-firmware").unwrap();
            line.clear();
            assert_eq!(r.read_line(&mut line).unwrap(), 0, "No initialization in mismatched mode");
        });
        assert!(Bridge::start_at_modes("token", Path::new("unused"), Profile::Read, false, true, address)
            .err().unwrap().contains("App not in native"));
        server.join().unwrap();
    }
    #[test]
    fn explicit_manual_seeds_forward_only_original_seed_and_reject_keys() {
        scripted_worker(false, false, true);
    }
    #[test]
    fn manual_seed_mode_rejects_wrong_ready_and_permission_mix_before_init() {
        let nowhere = "127.0.0.1:1".parse().unwrap();
        for (profile,key,full) in [(Profile::Read,false,false),(Profile::ClearDtc,false,false),
            (Profile::Seeds,true,false),(Profile::Seeds,false,true)] {
            assert!(Bridge::start_at_permissions("token",Path::new("unused"),profile,key,full,true,nowhere).is_err());
        }
        let listener=std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address=listener.local_addr().unwrap();
        let server=thread::spawn(move || {
            let(stream,_)=listener.accept().unwrap();
            stream.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
            let mut r=BufReader::new(stream);let mut line=String::new();
            r.read_line(&mut line).unwrap();assert_eq!(line.trim(),"HELLO token");
            writeln!(r.get_mut(),"READY native-firmware").unwrap();line.clear();
            assert_eq!(r.read_line(&mut line).unwrap(),0,"No init after wrong seed READY");
        });
        assert!(Bridge::start_at_permissions("token",Path::new("unused"),Profile::Seeds,false,false,true,address).is_err());
        server.join().unwrap();
    }
    fn scripted_worker(stopping_key: bool, full_native: bool, manual_seeds: bool) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut r = BufReader::new(stream);
            let mut pending = std::collections::VecDeque::new();
            let mut frames = Decoder::default();
            let mut shutdown = 0;
            let mut diagnostic = 0;
            let mut auth_stage = 0;
            let mut auth_key = None;
            let mut stopped = false;
            let mut cleanup = Vec::new();
            loop {
                let mut line = String::new();
                r.read_line(&mut line).unwrap();
                assert!(!line.is_empty());
                let response = if line.starts_with("HELLO ") {
                    if manual_seeds {
                        "READY nano-seeds"
                    } else if full_native {
                        "READY nano-full-native"
                    } else if stopping_key {
                        "READY native-key-status"
                    } else {
                        "READY native-firmware"
                    }
                    .into()
                } else if line.starts_with("TX ") {
                    let h = line[3..].trim();
                    let wire: Vec<_> = (0..h.len())
                        .step_by(2)
                        .map(|i| u8::from_str_radix(&h[i..i + 2], 16).unwrap())
                        .collect();
                    let mut reject_write = false;
                    for frame in frames.feed(&wire).unwrap() {
                        if let Some(reply) = handshake_reply(&frame, &mut auth_stage, &mut auth_key)
                        {
                            pending.push_back(reply.encode().unwrap());
                            continue;
                        }
                        assert_eq!(auth_stage, 6, "No channel/CAN before full initialization");
                        if frame.header[2] == 0 {
                            diagnostic += 1;
                            assert_eq!(
                                &frame.payload[10..],
                                if full_native {
                                    &[4, 0x27, 2, 0x12, 0x34, 0, 0, 0]
                                } else if stopping_key {
                                    &[3, 0xae, 3, 2, 0, 0, 0, 0]
                                } else {
                                    &[2, 0x27, 1, 0, 0, 0, 0, 0]
                                }
                            );
                            if stopping_key {
                                assert_eq!(frame.header[3], 1);
                                assert_eq!(&frame.payload[..4], &[0; 4]);
                                stopped = true;
                                reject_write = true;
                                continue;
                            }
                            let rx = Frame {
                                header: [0x80, 0x45, 0, 1],
                                payload: vec![
                                    0, 0, 0, 0, 0, 12, 0, 0, 6, 0x41, 4, 0x67, if full_native {2} else {1}, 0x12, 0x34, 0, 0,
                                    0,
                                ],
                            };
                            pending.push_back(rx.encode().unwrap());
                        } else {
                            if stopped {
                                assert!([0x48, 0x43, 0x41].contains(&frame.header[2]));
                                assert_eq!(
                                    frame.payload,
                                    if frame.header[2] == 0x48 {
                                        vec![1, 0, 1, 1, 0]
                                    } else {
                                        vec![]
                                    }
                                );
                                cleanup.push((frame.header[3], frame.header[2]));
                            }
                            if frame.header[2] == 0x41 {
                                shutdown += 1;
                            }
                            pending.push_back(
                                Frame {
                                    header: frame.header,
                                    payload: vec![0],
                                }
                                .encode()
                                .unwrap(),
                            );
                        }
                    }
                    if reject_write {
                        "ERROR stopped"
                    } else {
                        "TXOK"
                    }
                    .into()
                } else if line.trim() == "READ" {
                    if let Some(bytes) = pending.pop_front() {
                        format!("RX {}", hex(&bytes))
                    } else {
                        thread::sleep(Duration::from_millis(1));
                        "EMPTY".into()
                    }
                } else if line.trim() == "QUIT" {
                    writeln!(r.get_mut(), "CLOSED").unwrap();
                    break;
                } else {
                    panic!("Unexpected socket request");
                };
                writeln!(r.get_mut(), "{response}").unwrap();
            }
            assert_eq!(diagnostic, 1);
            assert_eq!(shutdown, 2);
            if stopping_key {
                assert_eq!(
                    cleanup,
                    [
                        (1, 0x48),
                        (1, 0x43),
                        (1, 0x41),
                        (0, 0x48),
                        (0, 0x43),
                        (0, 0x41)
                    ]
                );
            }
        });
        let directory = std::env::temp_dir().join(format!(
            "nano-worker-test-{}-{stopping_key}-{full_native}-{manual_seeds}",
            std::process::id()
        ));
        std::fs::create_dir(&directory).unwrap();
        let mut bridge = Bridge::start_at_permissions(
            "0123456789abcdef0123456789abcdef",
            &directory,
            if stopping_key || full_native {
                Profile::Read
            } else {
                Profile::Seeds
            },
            stopping_key,
            full_native,
            manual_seeds,
            address,
        )
        .unwrap();
        if manual_seeds {
            for data in [vec![4,0x27,2,0x12,0x34,0,0,0], vec![2,0x27,0x0c,0,0,0,0,0],
                vec![2,0xae,1,0,0,0,0,0], vec![4,0x3b,0x60,0x12,0x34,0,0,0]] {
                assert!(bridge.transmit(2, CanTransmission {
                    ticket:1,
                    frame:CanFrame {id:0x241, extended:false, rtr:false, dlc:8, data},
                    btr0:0xdd, btr1:0x36,
                    electrical:CanElectricalState::SingleWireGpio {latch:3, assignment:0, direction:3},
                }).is_err(), "Manual seed frontend must never queue a key/write");
            }
        }
        bridge
            .transmit(
                2,
                CanTransmission {
                    ticket: 1,
                    frame: CanFrame {
                        id: 0x241,
                        extended: false,
                        rtr: false,
                        dlc: 8,
                        data: if full_native {
                            vec![4, 0x27, 2, 0x12, 0x34, 0, 0, 0]
                        } else if stopping_key {
                            vec![3, 0xae, 3, 2, 0, 0, 0, 0]
                        } else {
                            vec![2, 0x27, 1, 0, 0, 0, 0, 0]
                        },
                    },
                    btr0: 0xdd,
                    btr1: 0x36,
                    electrical: CanElectricalState::SingleWireGpio {
                        latch: 3,
                        assignment: 0,
                        direction: 3,
                    },
                },
            )
            .unwrap();
        let end = Instant::now() + Duration::from_secs(2);
        let mut got_rx = false;
        let mut got_done = false;
        let mut got_error = false;
        while Instant::now() < end && !(got_rx && got_done) {
            let events = match bridge.poll() {
                Ok(events) => events,
                Err(error) if stopping_key => {
                    assert!(error.contains("USB write failed or was short"), "{error}");
                    got_error = true;
                    break;
                }
                Err(error) => panic!("{error}"),
            };
            for event in events {
                match event {
                    Event::Completed {
                        controller: 2,
                        ticket: 1,
                        source: crate::can_adapter::CompletionSource::VcxUsbWriteCompatibility,
                    } => got_done = true,
                    Event::Received(2, f)
                        if f.id == 0x641 && f.data == [4, 0x67, if full_native {2} else {1}, 0x12, 0x34, 0, 0, 0] =>
                    {
                        got_rx = true
                    }
                    _ => panic!("Unexpected native event"),
                }
            }
            thread::sleep(Duration::from_millis(1));
        }
        if stopping_key {
            assert!(got_error && !got_rx && !got_done);
            assert_eq!(bridge.confirmations(), 0);
        } else {
            assert!(got_rx && got_done);
        }
        bridge.close();
        server.join().unwrap();
        std::fs::remove_dir_all(directory).unwrap();
    }
}
