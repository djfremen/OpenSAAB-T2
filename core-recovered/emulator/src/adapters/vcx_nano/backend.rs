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
    fn start_at(
        token: &str,
        directory: &Path,
        profile: Profile,
        address: std::net::SocketAddr,
    ) -> Result<Self, String> {
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
        if usb.command(&format!("HELLO {token}"))? != "READY native-firmware" {
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
            let mut rx = 0usize;
            let mut ledger = TxLedger::default();
            let mut observe = |frame: &crate::nano_usb::Frame| -> Result<(), String> {
                if frame.header[2] != 0 {
                    return Ok(());
                }
                rx += 1;
                if rx > 300_000 {
                    return Err("Native RX capture/queue budget exceeded".into());
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
                for channel in 0..2 {
                    // Mark before allocation so a failed reply still triggers cleanup.
                    opened[channel as usize] = true;
                    for request in nano_channel::setup(channel)? {
                        if cancelled.load(Ordering::Relaxed) {
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
                    if started_at.elapsed() > Duration::from_secs(300) {
                        return Err("Native USB session deadline expired".into());
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
            profile,
        };
        if started.recv_timeout(Duration::from_secs(15)).is_err() {
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
        if !self
            .gate
            .allowed(controller, &tx, self.profile, Instant::now())
        {
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
            assert_eq!(opcodes, [0x40, 0x48, 0x43, 0x41]);
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
            loop {
                let mut line = String::new();
                r.read_line(&mut line).unwrap();
                assert!(!line.is_empty());
                let response = if line.starts_with("HELLO ") {
                    "READY native-firmware".into()
                } else if line.starts_with("TX ") {
                    let h = line[3..].trim();
                    let wire: Vec<_> = (0..h.len())
                        .step_by(2)
                        .map(|i| u8::from_str_radix(&h[i..i + 2], 16).unwrap())
                        .collect();
                    for frame in frames.feed(&wire).unwrap() {
                        if frame.header[2] == 0 {
                            diagnostic += 1;
                            assert_eq!(&frame.payload[10..], &[2, 0x27, 1, 0, 0, 0, 0, 0]);
                            let rx = Frame {
                                header: [0x80, 0x45, 0, 1],
                                payload: vec![
                                    0, 0, 0, 0, 0, 12, 0, 0, 6, 0x41, 4, 0x67, 1, 0x12, 0x34, 0, 0,
                                    0,
                                ],
                            };
                            pending.push_back(rx.encode().unwrap());
                        } else {
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
                    "TXOK".into()
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
        });
        let directory =
            std::env::temp_dir().join(format!("nano-worker-test-{}", std::process::id()));
        std::fs::create_dir(&directory).unwrap();
        let mut bridge = Bridge::start_at(
            "0123456789abcdef0123456789abcdef",
            &directory,
            Profile::Seeds,
            address,
        )
        .unwrap();
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
                        data: vec![2, 0x27, 1, 0, 0, 0, 0, 0],
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
        while Instant::now() < end && !(got_rx && got_done) {
            for event in bridge.poll().unwrap() {
                match event {
                    Event::Completed {
                        controller: 2,
                        ticket: 1,
                        source: crate::can_adapter::CompletionSource::VcxUsbWriteCompatibility,
                    } => got_done = true,
                    Event::Received(2, f)
                        if f.id == 0x641 && f.data == [4, 0x67, 1, 0x12, 0x34, 0, 0, 0] =>
                    {
                        got_rx = true
                    }
                    _ => panic!("Unexpected native event"),
                }
            }
            thread::sleep(Duration::from_millis(1));
        }
        assert!(got_rx && got_done);
        bridge.close();
        server.join().unwrap();
        std::fs::remove_dir_all(directory).unwrap();
    }
}
