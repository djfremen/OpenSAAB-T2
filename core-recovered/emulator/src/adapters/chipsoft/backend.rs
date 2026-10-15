// SPDX-License-Identifier: MPL-2.0
//! Original CANdi requests over Android-owned Chipsoft USB. No ECU client here.
use crate::{
    adapters::common::policy::{CommandGate, Profile},
    adapters::common::usb::{SocketUsb, UsbTransport},
    can_adapter::{Backend, CompletionSource, Event},
    candi_cpu::{CanFrame, CanTransmission},
    chipsoft_channel::{command, decode_read, disconnect, setup, terminate, words, Client, PROTOCOLS},
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
// The wire protocol and CAN worker are shared; only ownership of USB differs.
enum Transport {
    Android(SocketUsb),
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    Serial(crate::chipsoft_serial::Port),
}
impl UsbTransport for Transport {
    fn write(&mut self, bytes: &[u8]) -> Result<(), String> {
        match self {
            Self::Android(t) => t.write(bytes),
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            Self::Serial(t) => UsbTransport::write(t, bytes),
        }
    }
    fn read(&mut self) -> Result<Vec<u8>, String> {
        match self {
            Self::Android(t) => t.read(),
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            Self::Serial(t) => UsbTransport::read(t),
        }
    }
}
impl Transport {
    fn finish(&mut self, clean: bool) -> bool {
        match self {
            Self::Android(t) => t.command(if clean { "QUIT" } else { "ABORT" }).as_deref() == Ok("CLOSED"),
            // Channel/device teardown already ran; Port::drop restores and closes CDC.
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            Self::Serial(_) => true,
        }
    }
}
use crate::adapters::common::policy::KeyStatusGate;
pub struct Bridge {
    input: Option<SyncSender<(usize, CanTransmission)>>,
    output: Receiver<Event>,
    errors: Receiver<String>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    confirmations: u64,
    gate: CommandGate,
    full_native: bool,
    key_status: KeyStatusGate,
    profile: Profile,
    policy: Option<Arc<std::sync::Mutex<crate::seatbelt::Policy>>>,
}
// Interactive firmware must be allowed to wait for the operator. These total
// budgets belong to bounded research profiles, not an in-progress module Add.
fn session_budget_error(
    full_native: bool,
    elapsed: Duration,
    received: u64,
) -> Option<&'static str> {
    if full_native {
        None
    } else if elapsed > Duration::from_secs(300) {
        Some("Chipsoft native session expired")
    } else if received > 400000 {
        Some("Chipsoft receive budget exceeded")
    } else {
        None
    }
}
// Session policy is shared with Android. CDC changes device ownership only;
// it must not silently replace the selected original-firmware command policy.
fn validate_modes(serial: bool, symbol: bool, seeds: bool, audible: bool,
                  key_status: bool, full_native: bool) -> Result<(), String> {
    if (symbol as u8 + seeds as u8 + audible as u8 + key_status as u8 + full_native as u8) > 1 {
        return Err("Chipsoft session modes cannot be combined".into());
    }
    if serial && (symbol || audible) {
        return Err("BCM research profiles require the Android policy workspace".into());
    }
    Ok(())
}
impl Bridge {
    pub fn start(
        token: &str,
        directory: &Path,
        symbol_only: bool,
        seeds: bool,
        audible: bool,
    ) -> Result<Self, String> {
        Self::start_inner(token, None, directory, symbol_only, seeds, audible)
    }
    pub fn start_serial(port: &Path, directory: &Path, seeds: bool) -> Result<Self, String> {
        Self::start_inner("", Some(port), directory, false, seeds, false)
    }
    fn start_inner(token: &str, serial_port: Option<&Path>, directory: &Path,
                   symbol_only: bool, seeds: bool, audible: bool) -> Result<Self, String> {
        let full_native = match std::env::var("TECH2_CHIPSOFT_FULL_NATIVE") {
            Err(std::env::VarError::NotPresent) => false,
            Ok(v) if v == "0" => false,
            Ok(v) if v == "1" => true,
            _ => return Err("TECH2_CHIPSOFT_FULL_NATIVE must be 0 or 1".into()),
        };
        let key_status = match std::env::var("TECH2_CHIPSOFT_KEY_STATUS") {
            Err(std::env::VarError::NotPresent) => false,
            Ok(v) if v == "0" => false,
            Ok(v) if v == "1" => true,
            _ => return Err("TECH2_CHIPSOFT_KEY_STATUS must be 0 or 1".into()),
        };
        validate_modes(serial_port.is_some(), symbol_only, seeds, audible, key_status, full_native)?;
        let policy = if audible {
            Some(Arc::new(std::sync::Mutex::new(
                crate::seatbelt::Policy::load_audible(
                    &directory
                        .parent()
                        .ok_or("Missing app files directory")?
                        .join("chipsoft-audible-authority.json"),
                )?,
            )))
        } else if symbol_only {
            Some(Arc::new(std::sync::Mutex::new(
                crate::seatbelt::Policy::load(
                    &directory
                        .parent()
                        .ok_or("Missing app files directory")?
                        .join("chipsoft-symbol-authority.json"),
                )?,
            )))
        } else {
            None
        };
        let usb = if let Some(port) = serial_port {
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            { Transport::Serial(crate::chipsoft_serial::Port::open(port).map_err(|e| e.to_string())?) }
            #[cfg(not(any(target_os = "macos", target_os = "windows")))]
            { let _ = port; return Err("Native Chipsoft CDC is unavailable on this platform".into()); }
        } else {
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
        if usb.command(&format!("HELLO {token}"))?
            != if full_native {
                "READY chipsoft-full-native"
            } else if key_status {
                "READY chipsoft-key-status"
            } else if symbol_only {
                "READY chipsoft-symbol-only"
            } else if audible {
                "READY chipsoft-audible"
            } else if seeds {
                "READY chipsoft-seeds"
            } else {
                "READY chipsoft-native"
            }
        {
            return Err("App not in Chipsoft native mode".into());
        }
        Transport::Android(usb)
        };
        let mut log = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(directory.join("native-chipsoft.log"))
            .map_err(|e| e.to_string())?;
        writeln!(
            log,
            "SESSION command_policy={} requests=original-firmware",
            if full_native {
                "full-native"
            } else {
                "restricted"
            }
        )
        .map_err(|e| e.to_string())?;
        let (input, commands) = mpsc::sync_channel::<(usize, CanTransmission)>(16);
        let (events, output) = mpsc::sync_channel(4096);
        let (failed, errors) = mpsc::sync_channel(1);
        let (ready, started) = mpsc::sync_channel(1);
        let stop = Arc::new(AtomicBool::new(false));
        let cancelled = stop.clone();
        let worker_policy = policy.clone();
        let worker = thread::spawn(move || {
            let mut c = Client::new(usb);
            let mut opened = [false; 2];
            let began = Instant::now();
            let mut rx_count = 0u64;
            let operation = (|| -> Result<(), String> {
                let id = c.checked(&command(1, vec![]))?;
                if !id.payload.starts_with(b"CHIPSOFT J2534 Pro v. 1.5.2") {
                    return Err("Unvalidated Chipsoft firmware version".into());
                }
                c.checked(&command(8, vec![]))?;
                for (i, p) in PROTOCOLS.iter().enumerate() {
                    opened[i] = true;
                    for f in setup(*p)? {
                        c.checked(&f)?;
                    }
                }
                ready.try_send(()).map_err(|_| "Native startup cancelled")?;
                while !cancelled.load(Ordering::Relaxed) {
                    if let Some(error) =
                        session_budget_error(full_native, began.elapsed(), rx_count)
                    {
                        return Err(error.into());
                    }
                    for _ in 0..16 {
                        match commands.try_recv() {
                            Ok((controller, tx)) => {
                                if let Some(p) = &worker_policy {
                                    p.lock()
                                        .map_err(|_| "BCM policy lock failed")?
                                        .before(controller, &tx)?;
                                }
                                let f = crate::chipsoft_channel::transmit(controller, &tx)?;
                                writeln!(log,"TX t_us={} controller={controller} ticket={} id={:03X} data={:02X?} origin=original-candi",began.elapsed().as_micros(),tx.ticket,tx.frame.id,tx.frame.data).map_err(|e|e.to_string())?;
                                let reply = c.checked(&f)?;
                                if reply.payload.len() != 8 || reply.payload[4..] != [0; 4] {
                                    return Err("Unexpected Chipsoft transmit result".into());
                                }
                                println!("NATIVE_USB_TX adapter=chipsoft controller={controller} ticket={} id={:03X} data={:02X?} device_reply=accepted electrical_ack=unverified",tx.ticket,tx.frame.id,tx.frame.data);
                                events
                                    .try_send(Event::Completed {
                                        controller,
                                        ticket: tx.ticket,
                                        source: CompletionSource::ChipsoftDeviceReplyCompatibility,
                                    })
                                    .map_err(|_| "Chipsoft completion queue full")?;
                            }
                            Err(mpsc::TryRecvError::Empty) => break,
                            Err(mpsc::TryRecvError::Disconnected) => return Ok(()),
                        }
                    }
                    for p in PROTOCOLS {
                        let reply = c.exchange(&command(0x10, words(&[p, 10])))?;
                        if reply.status == 0x85 && reply.payload.is_empty() {
                            continue;
                        }
                        for raw in decode_read(&reply)? {
                            rx_count = rx_count.saturating_add(1);
                            if let Some(error) =
                                session_budget_error(full_native, began.elapsed(), rx_count)
                            {
                                return Err(error.into());
                            }
                            let controller = if raw.protocol == 5 { 0 } else { 2 };
                            if raw.flags & 1 != 0 {
                                continue;
                            } // never feed an echo back as ECU traffic
                            if raw.flags & !0x100 != 0 {
                                return Err(format!(
                                    "Unverified Chipsoft RX flags {:08X}",
                                    raw.flags
                                ));
                            }
                            let frame = CanFrame {
                                id: raw.id,
                                extended: raw.flags & 0x100 != 0,
                                rtr: false,
                                dlc: raw.data.len() as u8,
                                data: raw.data,
                            };
                            frame.validate()?;
                            // Format once, then write one record. Formatting a
                            // File directly performs many small writes per frame
                            // on a busy bus. Preserve immediate capture and both
                            // clock domains for receive-delay analysis.
                            log.write_all(format!("RX controller={controller} id={:03X} flags={:08X} timestamp={} data={:02X?} host_t_us={}\n",frame.id,raw.flags,raw.timestamp,frame.data,began.elapsed().as_micros()).as_bytes()).map_err(|e|e.to_string())?;
                            if let Some(p) = &worker_policy {
                                let mut p = p.lock().map_err(|_| "BCM policy lock failed")?;
                                p.observe(controller, frame.id, &frame.data)?;
                                if controller == 2 && frame.id == 0x642 {
                                    println!("BCM_REMINDER_POLICY write_acknowledged={} desired_readback={} symbol_only_readback={} rejection={:?}",p.acknowledged,p.desired_readback,p.symbol_only,p.rejection);
                                }
                            }
                            events
                                .try_send(Event::Received(controller, frame))
                                .map_err(|_| "Chipsoft receive queue full")?;
                        }
                    }
                }
                Ok(())
            })();
            let mut clean = true;
            for i in (0..2).rev() {
                if opened[i] && c.checked(&disconnect(PROTOCOLS[i])).is_err() {
                    clean = false;
                }
            }
            if c.checked(&terminate()).is_err() {
                clean = false;
            }
            if !c.transport.finish(clean) {
                clean = false;
            }
            println!("NATIVE_USB_CLOSED adapter=chipsoft cleanup_ok={clean} rx={rx_count}");
            if let Err(e) = operation {
                let _ = failed.try_send(e);
            } else if !clean {
                let _ = failed.try_send("Chipsoft cleanup incomplete".into());
            }
        });
        let mut bridge = Self {
            input: Some(input),
            output,
            errors,
            stop,
            worker: Some(worker),
            confirmations: 0,
            gate: CommandGate::default(),
            full_native,
            key_status: KeyStatusGate {
                enabled: key_status,
                sent: 0,
            },
            profile: Profile::collection(seeds),
            policy,
        };
        if started.recv_timeout(Duration::from_secs(15)).is_err() {
            bridge.close();
            return Err(bridge
                .errors
                .try_recv()
                .unwrap_or("Chipsoft startup failed".into()));
        }
        Ok(bridge)
    }
}
impl Backend for Bridge {
    fn poll(&mut self) -> Result<Vec<Event>, String> {
        if let Ok(e) = self.errors.try_recv() {
            self.close();
            return Err(e);
        }
        let mut out = Vec::new();
        for _ in 0..128 {
            match self.output.try_recv() {
                Ok(e) => {
                    if matches!(e, Event::Completed { .. }) {
                        self.confirmations += 1;
                    }
                    out.push(e)
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(_) => return Err("Chipsoft worker ended".into()),
            }
        }
        Ok(out)
    }
    fn transmit(&mut self, controller: usize, tx: CanTransmission) -> Result<(), String> {
        // Full native mode removes the diagnostic-service allowlist only.
        // Keep physical route, frame-size and wire-encoding validation.
        crate::chipsoft_channel::transmit(controller, &tx)?;
        if !self.full_native
            && !self
                .gate
                .allowed(controller, &tx, self.profile, Instant::now())
            && !self.key_status.allowed(controller, &tx)
            && !self
                .policy
                .as_ref()
                .is_some_and(|p| p.lock().is_ok_and(|p| p.allows(controller, &tx)))
        {
            let offset = usize::from(tx.frame.data.first() == Some(&0xfe));
            let service = tx.frame.data.get(offset + 1).copied();
            return Err(format!(
                "Chipsoft command guard blocked before USB: controller={} CAN_ID={:03X} service={} profile={:?}; no ECU rejection",
                controller,
                tx.frame.id,
                service.map(|s| format!("{s:02X}")).unwrap_or_else(|| "unknown".into()),
                self.profile
            ));
        }
        self.input
            .as_ref()
            .ok_or("Chipsoft closed")?
            .try_send((controller, tx))
            .map_err(|_| "Chipsoft transmit queue full".into())
    }
    fn confirmations(&self) -> u64 {
        self.confirmations
    }
    fn close(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.input.take();
        if let Some(w) = self.worker.take() {
            let _ = w.join();
        }
    }
}
impl Drop for Bridge {
    fn drop(&mut self) {
        self.close();
    }
}

#[cfg(test)]
mod key_status_tests {
    use super::*;
    #[test]
    fn cdc_and_android_share_session_modes_without_combining_policies() {
        for serial in [false, true] {
            for mask in 0u8..32 {
                let symbol = mask & 1 != 0;
                let seeds = mask & 2 != 0;
                let audible = mask & 4 != 0;
                let key = mask & 8 != 0;
                let full = mask & 16 != 0;
                assert_eq!(validate_modes(serial, symbol, seeds, audible, key, full).is_ok(),
                    mask.count_ones() <= 1 && !(serial && (symbol || audible)),
                    "serial={serial} mask={mask}");
            }
        }
    }
    #[test]
    fn interactive_session_survives_old_deadline_and_capture_budget() {
        assert_eq!(
            session_budget_error(false, Duration::from_secs(300), 400000),
            None
        );
        assert_eq!(
            session_budget_error(false, Duration::from_secs(301), 0),
            Some("Chipsoft native session expired")
        );
        assert_eq!(
            session_budget_error(false, Duration::ZERO, 400001),
            Some("Chipsoft receive budget exceeded")
        );
        assert_eq!(
            session_budget_error(true, Duration::from_secs(3600), 400001),
            None
        );
    }
    use crate::candi_cpu::CanElectricalState;
    fn request() -> CanTransmission {
        CanTransmission {
            ticket: 1,
            frame: CanFrame {
                id: 0x241,
                extended: false,
                rtr: false,
                dlc: 8,
                data: vec![3, 0xae, 3, 2, 0, 0, 0, 0],
            },
            btr0: 0xdd,
            btr1: 0x36,
            electrical: CanElectricalState::SingleWireGpio {
                latch: 3,
                assignment: 0,
                direction: 3,
            },
        }
    }
    #[test]
    fn observed_vehicle_seed_request_requires_seeds_profile_and_never_allows_key() {
        let (input, commands) = mpsc::sync_channel(16);
        let (_, output) = mpsc::sync_channel(1);
        let (_, errors) = mpsc::sync_channel(1);
        let mut bridge = Bridge {
            input: Some(input), output, errors,
            stop: Arc::new(AtomicBool::new(false)), worker: None, confirmations: 0,
            gate: CommandGate::default(), full_native: false,
            key_status: KeyStatusGate::default(), profile: Profile::Read, policy: None,
        };
        let mut tx = request();
        tx.frame.id = 0x257;
        tx.frame.data = vec![2, 0x27, 1, 0, 0, 0, 0, 0];
        assert!(bridge.transmit(2, tx.clone()).unwrap_err().contains("profile=Read"));
        assert!(commands.try_recv().is_err());
        bridge.profile = Profile::Seeds;
        bridge.transmit(2, tx.clone()).unwrap();
        let sent = commands.try_recv().unwrap();
        assert_eq!(sent.0, 2);
        assert_eq!(sent.1.frame.id, 0x257);
        assert_eq!(sent.1.frame.data, tx.frame.data);
        for data in [vec![4, 0x27, 2, 0x12, 0x34, 0, 0, 0],
                     vec![1, 4, 0, 0, 0, 0, 0, 0],
                     vec![4, 0x3b, 1, 0x99, 0, 0, 0, 0]] {
            tx.frame.data = data;
            assert!(bridge.transmit(2, tx.clone()).is_err());
            assert!(commands.try_recv().is_err());
        }
    }
    #[test]
    fn full_native_forwards_guest_commands_but_preserves_transport_validation() {
        let (input, commands) = mpsc::sync_channel(16);
        let (_, output) = mpsc::sync_channel(1);
        let (_, errors) = mpsc::sync_channel(1);
        let mut bridge = Bridge {
            input: Some(input),
            output,
            errors,
            stop: Arc::new(AtomicBool::new(false)),
            worker: None,
            confirmations: 0,
            gate: CommandGate::default(),
            full_native: false,
            key_status: KeyStatusGate::default(),
            profile: Profile::Read,
            policy: None,
        };
        let mut tx = request();
        tx.frame.data = vec![4, 0x27, 2, 0x12, 0x34, 0, 0, 0];
        assert!(bridge
            .transmit(2, tx.clone())
            .unwrap_err()
            .contains("before USB"));
        assert!(commands.try_recv().is_err());
        bridge.full_native = true;
        for data in [
            // Exact request from the Intel ignition-status failure capture.
            vec![3, 0xae, 3, 2, 0, 0, 0, 0],
            vec![2, 0x10, 3, 0, 0, 0, 0, 0],
            tx.frame.data.clone(),
            vec![4, 0x3b, 1, 0x99, 0, 0, 0, 0],
        ] {
            tx.frame.data = data.clone();
            bridge.transmit(2, tx.clone()).unwrap();
            assert_eq!(commands.try_recv().unwrap().1.frame.data, data);
        }
        tx.frame.extended = true;
        assert!(bridge.transmit(2, tx.clone()).is_err());
        tx.frame.extended = false;
        assert!(bridge.transmit(0, tx.clone()).is_err());
        tx.frame.data.push(0);
        assert!(bridge.transmit(2, tx).is_err());
        assert!(commands.try_recv().is_err());
    }

    #[test]
    fn key_status_requires_explicit_mode_exact_request_and_bounded_retries() {
        let tx = request();
        assert!(!crate::adapters::common::policy::diagnostic_allowed(2, &tx));
        assert!(!KeyStatusGate::default().allowed(2, &tx));
        let mut gate = KeyStatusGate {
            enabled: true,
            sent: 0,
        };
        assert!(!gate.allowed(0, &tx));
        for index in 0..8 {
            let mut changed = request();
            changed.frame.data[index] ^= 1;
            assert!(!gate.allowed(2, &changed));
        }
        for id in [0x101, 0x242, 0x7e0] {
            let mut changed = request();
            changed.frame.id = id;
            assert!(!gate.allowed(2, &changed));
        }
        let mut changed = request();
        changed.frame.extended = true;
        assert!(!gate.allowed(2, &changed));
        assert_eq!(gate.sent, 0);
        for _ in 0..3 {
            assert!(gate.allowed(2, &tx));
        }
        assert!(!gate.allowed(2, &tx));
    }
}
