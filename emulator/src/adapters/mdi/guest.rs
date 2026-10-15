// SPDX-License-Identifier: MPL-2.0
//! Driver-free D-PDU raw bridge. Original firmware owns every diagnostic byte.
use crate::{
    can_adapter::{electrical_route, Backend, CompletionSource, Event},
    candi_cpu::{CanFrame, CanTransmission},
};
use openmdi::client::{RawEvent, RawPair};
use std::{
    collections::BTreeMap,
    fs::OpenOptions,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender},
        Arc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

pub struct Bridge {
    input: Option<SyncSender<(usize, CanTransmission)>>,
    events: Receiver<Event>,
    errors: Receiver<String>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    mode: String,
    confirmations: u64,
    gate: crate::adapters::common::policy::CommandGate,
}
fn validate(c: usize, frame: &CanFrame) -> Result<(), String> {
    use opensaab_adapter_translation::{Bus, Packet, RawFrame};
    if frame.extended || frame.rtr || frame.id > 0x7ff {
        return Err("MDI raw profile requires a standard data frame".into());
    }
    let bus = match c {
        0 => Bus::HsCan,
        2 => Bus::SwCan,
        _ => return Err("MDI unmapped controller".into()),
    };
    Packet::Raw(RawFrame {
        bus,
        id: frame.id,
        extended: frame.extended,
        rtr: frame.rtr,
        dlc: frame.dlc,
        data: frame.data.clone(),
    })
    .guest_frame()?;
    Ok(())
}
impl Bridge {
    pub fn start(directory: &Path, mode: &str) -> Result<Self, String> {
        if !matches!(mode, "read" | "seeds" | "full") {
            return Err("Invalid explicit MDI mode".into());
        }
        let log = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(directory.join("mdi-raw-private.jsonl"))
            .map_err(|e| e.to_string())?;
        let (input, commands) = mpsc::sync_channel(16);
        let (sender, events) = mpsc::sync_channel(4096);
        let (error_sender, errors) = mpsc::sync_channel(1);
        let (ready_sender, ready) = mpsc::sync_channel(1);
        let stop = Arc::new(AtomicBool::new(false));
        let cancelled = stop.clone();
        let directory = directory.to_path_buf();
        let worker = thread::spawn(move || {
            let mut log = opensaab_session_evidence::bounded_capture::BoundedCapture::new(log, 128 * 1024 * 1024);
            let mut pair: Option<RawPair> = None;
            let mut pending = BTreeMap::<(usize, u32), (usize, u64)>::new();
            let result = (|| -> Result<(), String> {
                if cancelled.load(Ordering::Acquire) {
                    return Err("MDI cancelled during init".into());
                }
                pair = Some(
                    RawPair::open_live("127.0.0.1:10123".parse().unwrap()).map_err(|e| {
                        let _ = std::fs::write(
                            directory.join("mdi-open-failure-private.json"),
                            e.report.to_string(),
                        );
                        e.error
                    })?,
                );
                log.record(&pair.as_mut().unwrap().take_activity());
                ready_sender
                    .try_send(())
                    .map_err(|_| "MDI startup caller left")?;
                println!(
                    "MDI_RAW_READY buses=HS500k,SW33k source=generated-DPDU vendor_driver=false"
                );
                while !cancelled.load(Ordering::Acquire) {
                    for _ in 0..16 {
                        match commands.try_recv() {
                            Ok((controller, tx)) => {
                                let route = electrical_route(controller, &tx)?;
                                let ch = route.channel as usize;
                                let token = pair.as_mut().unwrap().transmit(
                                    ch,
                                    tx.frame.id,
                                    &tx.frame.data,
                                    route.mode == crate::can_adapter::ElectricalMode::SingleWireWake,
                                )?;
                                if pending
                                    .insert((ch, token), (controller, tx.ticket))
                                    .is_some()
                                {
                                    return Err("MDI reused TX token".into());
                                }
                                println!("MDI_RAW_TX controller={controller} ticket={} id={:03X} bytes={} origin=original-candi",tx.ticket,tx.frame.id,tx.frame.data.len());
                            }
                            Err(mpsc::TryRecvError::Empty) => break,
                            Err(mpsc::TryRecvError::Disconnected) => return Ok(()),
                        }
                    }
                    let session = pair.as_mut().unwrap();
                    for (ch, event) in session.poll()? {
                        let event = match event {
                            RawEvent::Received { id, data, .. }
                            | RawEvent::ObservedMdiData { id, data, .. } => {
                                let c = if ch == 0 { 0 } else { 2 };
                                let frame = CanFrame {
                                    id,
                                    extended: false,
                                    rtr: false,
                                    dlc: data.len() as u8,
                                    data,
                                };
                                validate(c, &frame)?;
                                Some(Event::Received(c, frame))
                            }
                            RawEvent::Transmitted { token, .. } => {
                                let (controller, ticket) = pending
                                    .remove(&(ch, token))
                                    .ok_or("MDI unmatched original TX ticket")?;
                                Some(Event::Completed {
                                    controller,
                                    ticket,
                                    source: CompletionSource::MdiDeviceTxIndication,
                                })
                            }
                            RawEvent::Metadata(_) => None,
                        };
                        if let Some(e) = event {
                            sender
                                .try_send(e)
                                .map_err(|_| "MDI guest event queue full/closed")?;
                        }
                    }
                    log.record(&session.take_activity());
                    thread::sleep(Duration::from_millis(1));
                }
                Ok(())
            })();
            let cleanup = pair
                .as_mut()
                .map(|pair| {
                    let result = pair.close();
                    log.record(&pair.take_activity());
                    result
                })
                .unwrap_or(serde_json::Value::Null);
            let clean = cleanup["cleanup_ok"] == true;
            let report = serde_json::json!({"result_error":result.as_ref().err(),"cleanup_ok":clean,"raw_pair":cleanup,"unconfirmed_guest_tickets":pending.len(),"capture":log.report()});
            let _ = std::fs::write(
                directory.join("mdi-cleanup-private.json"),
                report.to_string(),
            );
            println!("MDI_RAW_CLEANUP success={clean}");
            if let Err(e) = result {
                let _ = error_sender.try_send(e);
            } else if !clean {
                let _ = error_sender.try_send("MDI cleanup incomplete".into());
            }
        });
        let mut bridge = Self {
            input: Some(input),
            events,
            errors,
            stop,
            worker: Some(worker),
            mode: mode.into(),
            confirmations: 0,
            gate: Default::default(),
        };
        if ready.recv_timeout(Duration::from_secs(35)).is_err() {
            bridge.close();
            return Err(bridge
                .errors
                .try_recv()
                .unwrap_or_else(|_| "MDI startup deadline/worker failure".into()));
        }
        Ok(bridge)
    }
}
impl Backend for Bridge {
    fn poll(&mut self) -> Result<Vec<Event>, String> {
        if let Ok(e) = self.errors.try_recv() {
            return Err(e);
        }
        let mut events = Vec::new();
        for _ in 0..256 {
            match self.events.try_recv() {
                Ok(e) => {
                    if matches!(e, Event::Completed { .. }) {
                        self.confirmations += 1;
                    }
                    events.push(e);
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(_) => return Err("MDI raw worker closed".into()),
            }
        }
        Ok(events)
    }
    fn transmit(&mut self, c: usize, tx: CanTransmission) -> Result<(), String> {
        validate(c, &tx.frame)?;
        electrical_route(c, &tx)?;
        let profile = if self.mode == "seeds" {
            crate::nano_native::Profile::Seeds
        } else {
            crate::nano_native::Profile::Read
        };
        if self.mode != "full" && !self.gate.allowed(c, &tx, profile, Instant::now()) {
            let offset = usize::from(tx.frame.data.first() == Some(&0xfe));
            let service = tx.frame.data.get(offset + 1).copied();
            return Err(format!(
                "MDI command guard blocked before transport: controller={c} CAN_ID={:03X} service={} profile={profile:?}; no ECU rejection",
                tx.frame.id,
                service.map(|s| format!("{s:02X}")).unwrap_or_else(|| "unknown".into()),
            ));
        }
        self.input
            .as_ref()
            .ok_or("MDI raw bridge closed")?
            .try_send((c, tx))
            .map_err(|_| "MDI TX queue full/closed".into())
    }
    fn confirmations(&self) -> u64 {
        self.confirmations
    }
    fn close(&mut self) {
        self.stop.store(true, Ordering::Release);
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
    #[test]
    fn read_key_test_is_denied_before_transport_and_reported_as_policy() {
        let (input, commands) = mpsc::sync_channel(1);
        let (_, events) = mpsc::sync_channel(1);
        let (_, errors) = mpsc::sync_channel(1);
        let mut bridge = Bridge {
            input: Some(input),
            events,
            errors,
            stop: Arc::new(AtomicBool::new(false)),
            worker: None,
            mode: "read".into(),
            confirmations: 0,
            gate: Default::default(),
        };
        let tx = CanTransmission {
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
            electrical: crate::candi_cpu::CanElectricalState::SingleWireGpio {
                latch: 3,
                assignment: 0,
                direction: 3,
            },
        };
        let error = bridge.transmit(2, tx.clone()).unwrap_err();
        assert!(matches!(commands.try_recv(), Err(mpsc::TryRecvError::Empty)));
        assert!(error.contains("controller=2 CAN_ID=241 service=AE profile=Read; no ECU rejection"));
        let failure = crate::adapter_failure::classify(
            &error,
            Some(&crate::adapter_failure::TxContext::new(2, &tx)),
        );
        assert_eq!(failure["kind"], "command_blocked");
        assert!(failure["message"].as_str().unwrap().contains("Full native control"));
        assert_eq!(failure["ecu_rejected"], false);
        bridge.mode = "full".into();
        bridge.transmit(2, tx.clone()).unwrap();
        let (controller, forwarded) = commands.try_recv().unwrap();
        assert_eq!(controller, 2);
        assert_eq!(forwarded.frame, tx.frame);
    }
    #[test]
    fn original_frame_boundaries_are_checked_before_transport() {
        let mut frame = CanFrame {
            id: 0x25b,
            extended: false,
            rtr: false,
            dlc: 8,
            data: vec![2, 0x1a, 0x9a, 0, 0, 0, 0, 0],
        };
        assert!(validate(2, &frame).is_ok());
        assert!(validate(1, &frame).is_err());
        frame.dlc = 7;
        assert!(validate(2, &frame).is_err());
        frame.dlc = 8;
        frame.extended = true;
        assert!(validate(2, &frame).is_err());
        frame.extended = false;
        frame.rtr = true;
        assert!(validate(2, &frame).is_err());
        frame.rtr = false;
        frame.id = 0x800;
        assert!(validate(2, &frame).is_err());
    }
}
