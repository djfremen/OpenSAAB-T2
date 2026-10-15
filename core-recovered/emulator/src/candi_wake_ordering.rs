// SPDX-License-Identifier: MPL-2.0
//! Shared captured CANdi wake serial ordering gate. This module is pure:
//! no clocks, hardware, firmware writes, fabricated ACKs, or byte replacement.
//! The integrating caller alone delivers admitted/released original frames.

use sha2::{Digest, Sha256};
use std::collections::VecDeque;
use crate::can_adapter::{electrical_route, CompletionSource, ElectricalMode};
use crate::candi_cpu::{CanElectricalState, CanTransmission};

const HZ: u64 = 16_777_216;
const TOTAL_CYCLES: u64 = 10 * HZ;
const RESTORE_CYCLES: u64 = HZ.div_ceil(10);
const MAX_BYTES: usize = 4096;
const MAX_FRAMES: usize = 512;
const MAX_DELIMITER_BYTES: usize = 32;
const MAX_EVENTS: usize = 2048;
const WAKE: [u8; 3] = [0x88, 0x0e, 0x6a];
const HASH_ENCODING: &str = "u64be_payload_length+payload+u64be_delimiter_length+delimiter_utf8";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SerialFrame {
    pub bytes: Vec<u8>,
    pub delimiter: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Admission {
    Send(SerialFrame),
    Held,
}

#[derive(Clone, Debug)]
enum Phase {
    Idle,
    AwaitIntent,
    AwaitCompletion {
        ticket: u64,
    },
    AwaitRestore {
        ticket: u64,
        completed_at: u64,
        deadline: u64,
    },
    Releasing,
    Failed(String),
}

#[derive(Clone)]
struct Queued {
    sequence: u64,
    arrived_at: u64,
    frame: SerialFrame,
}

#[derive(Clone)]
pub struct WakeSerialBarrier {
    phase: Phase,
    wake_source: CompletionSource,
    ordinary_source: CompletionSource,
    release_clock: Option<(u64,u64)>,
    deadline: Option<u64>,
    last_cycles: u64,
    queue: VecDeque<Queued>,
    queued_bytes: usize,
    next_sequence: u64,
    queued_count: u64,
    released_count: u64,
    queued_hash: Sha256,
    released_hash: Sha256,
    ordinary_pending: Option<u64>,
    events: Vec<String>,
}

fn hash_frame(hash: &mut Sha256, frame: &SerialFrame) {
    hash.update((frame.bytes.len() as u64).to_be_bytes());
    hash.update(&frame.bytes);
    hash.update((frame.delimiter.len() as u64).to_be_bytes());
    hash.update(frame.delimiter.as_bytes());
}

fn digest(hash: &Sha256) -> String {
    format!("{:x}", hash.clone().finalize())
}

fn frame_audit(frame: &SerialFrame) -> serde_json::Value {
    let mut hash = Sha256::new();
    hash_frame(&mut hash, frame);
    serde_json::json!({
        "bytes_hex": frame.bytes.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" "),
        "payload_bytes": frame.bytes.len(), "end": frame.delimiter,
        "modeled_delimiter_bytes": 1, "sha256": digest(&hash), "hash_encoding": HASH_ENCODING,
    })
}

impl Default for WakeSerialBarrier {
    fn default() -> Self {
        Self::new()
    }
}

impl WakeSerialBarrier {
    /// Read-only peek. A narrower caller may fail before consuming/auditing a
    /// retained frame whose original post-frame FCR epoch cannot be recovered.
    pub fn next_releasable_bytes(&self)->Option<&[u8]> {
        if matches!(self.phase,Phase::Releasing) {self.queue.front().map(|q|q.frame.bytes.as_slice())}
        else {None}
    }
    pub fn new() -> Self {
        Self {
            phase: Phase::Idle,
            wake_source: CompletionSource::MicTranslatedWakeWriteCompatibility,
            ordinary_source: CompletionSource::MicByteTransportWriteCompatibility,
            release_clock: None,
            deadline: None,
            last_cycles: 0,
            queue: VecDeque::new(),
            queued_bytes: 0,
            next_sequence: 1,
            queued_count: 0,
            released_count: 0,
            queued_hash: Sha256::new(),
            released_hash: Sha256::new(),
            ordinary_pending: None,
            events: Vec::new(),
        }
    }

    /// The same captured ordering policy uses each driver's explicit completion provenance.
    /// A compatibility indication is never evidence of an electrical CAN ACK.
    pub fn with_completion_sources(wake: CompletionSource, ordinary: CompletionSource) -> Self {
        let mut gate=Self::new();gate.wake_source=wake;gate.ordinary_source=ordinary;gate
    }

    pub fn active(&self) -> bool {
        !matches!(self.phase, Phase::Idle)
    }

    /// Read-only scheduling query. A released ordinary request is not a wake
    /// wait; a terminal failure must be handled by the stopped-link path.
    pub fn waiting_for_wake(&self) -> bool {
        matches!(self.phase, Phase::AwaitIntent | Phase::AwaitCompletion { .. } | Phase::AwaitRestore { .. })
    }

    fn fail(&mut self, reason: impl Into<String>, now: u64) -> String {
        let reason = reason.into();
        if !matches!(self.phase, Phase::Failed(_)) {
            self.phase = Phase::Failed(reason.clone());
            // Keep queued original data. A single terminal audit is permitted
            // beyond the normal event limit; repeated failures do not grow it.
            self.events.push(
                serde_json::json!({"wake_serial_barrier_failed": {
                "reason": reason, "native_cycles": now, "queued_frames": self.queue.len(),
                "queued_modeled_bytes": self.queued_bytes, "release_permitted": false,
                "queued_frames_cumulative": self.queued_count,
                "released_frames_cumulative": self.released_count,
                    "queued_stream_sha256": digest(&self.queued_hash),
                    "released_stream_sha256": digest(&self.released_hash),
                    "physical_can_ack_verified": false,
                }})
                .to_string(),
            );
        }
        match &self.phase {
            Phase::Failed(existing) => existing.clone(),
            _ => unreachable!(),
        }
    }

    fn event(&mut self, event: serde_json::Value, now: u64) -> Result<(), String> {
        if self.events.len() >= MAX_EVENTS {
            return Err(self.fail("Wake serial barrier audit queue full", now));
        }
        self.events.push(event.to_string());
        Ok(())
    }

    /// Terminal external backend/CPU/UART stop. Retain the original FIFO and
    /// record its queued/released digest receipt; no future call can release.
    pub fn abort(&mut self, reason: &str, now: u64) -> String {
        self.fail(reason, now)
    }

    pub fn tick(&mut self, now: u64) -> Result<(), String> {
        if let Phase::Failed(reason) = &self.phase {
            return Err(reason.clone());
        }
        if !self.active() {
            return Ok(());
        }
        if now < self.last_cycles {
            return Err(self.fail("Wake serial barrier native clock regressed", now));
        }
        self.last_cycles = now;
        if self.deadline.is_some_and(|deadline| now >= deadline) {
            return Err(self.fail("Wake serial barrier total native deadline exceeded", now));
        }
        if matches!(self.phase, Phase::AwaitRestore { deadline, .. } if now >= deadline) {
            return Err(self.fail(
                "Wake serial barrier native GPIO restore deadline exceeded",
                now,
            ));
        }
        Ok(())
    }

    pub fn admit(&mut self, frame: SerialFrame, now: u64) -> Result<Admission, String> {
        self.tick(now)?;
        if frame.delimiter.len() > MAX_DELIMITER_BYTES {
            return Err(self.fail("Wake serial barrier delimiter label exceeds bound", now));
        }
        if !self.active() {
            if frame.bytes != WAKE {
                return Ok(Admission::Send(frame));
            }
            let Some(deadline) = now.checked_add(TOTAL_CYCLES) else {
                return Err(self.fail("Wake serial barrier native deadline overflow", now));
            };
            self.phase = Phase::AwaitIntent;
            self.deadline = Some(deadline);
            self.last_cycles = now;
            self.event(
                serde_json::json!({"wake_serial_barrier_armed": {
                    "frame": frame_audit(&frame), "native_cycles": now,
                    "total_deadline_cycles": deadline, "restore_budget_cycles": RESTORE_CYCLES,
                    "maximum_queued_modeled_bytes": MAX_BYTES, "maximum_queued_frames": MAX_FRAMES,
                    "physical_can_ack_verified": false, "guest_bytes_modified": false,
                }}),
                now,
            )?;
            return Ok(Admission::Send(frame));
        }
        let Some(cost) = frame.bytes.len().checked_add(1) else {
            return Err(self.fail("Wake serial barrier serial frame size overflow", now));
        };
        if self.queue.len() >= MAX_FRAMES || cost > MAX_BYTES.saturating_sub(self.queued_bytes) {
            return Err(self.fail("Wake serial barrier serial queue full", now));
        }
        let Some(next_sequence) = self.next_sequence.checked_add(1) else {
            return Err(self.fail("Wake serial barrier sequence exhausted", now));
        };
        let sequence = self.next_sequence;
        let mut queued_hash = self.queued_hash.clone();
        hash_frame(&mut queued_hash, &frame);
        self.event(
            serde_json::json!({"wake_serial_barrier_queued": {
                "sequence": sequence, "frame": frame_audit(&frame), "native_cycles": now,
                "queued_stream_sha256": digest(&queued_hash), "hash_encoding": HASH_ENCODING,
                "physical_can_ack_verified": false, "guest_bytes_modified": false,
            }}),
            now,
        )?;
        self.next_sequence = next_sequence;
        self.queued_count += 1;
        self.queued_hash = queued_hash;
        self.queued_bytes += cost;
        self.queue.push_back(Queued { sequence, arrived_at:now, frame });
        Ok(Admission::Held)
    }

    pub fn associate_intent(
        &mut self,
        controller: usize,
        tx: &CanTransmission,
        now: u64,
    ) -> Result<(), String> {
        self.tick(now)?;
        if !self.active() || controller != 2 {
            return Ok(());
        }
        if matches!(self.phase, Phase::Releasing) {
            let route = electrical_route(controller, tx).map_err(|error| {
                self.fail(
                    format!("Wake serial barrier ordinary electrical route rejected: {error}"),
                    now,
                )
            })?;
            if tx.ticket == 0
                || self.ordinary_pending.is_some()
                || route.channel != 1
                || route.mode != ElectricalMode::Data
            {
                return Err(self.fail(
                    "Wake serial barrier unexpected high-voltage or overlapping ordinary intent",
                    now,
                ));
            }
            self.event(serde_json::json!({"wake_serial_barrier_ordinary_intent_observed": {
                "controller": controller, "ticket": tx.ticket, "native_cycles": now,
                "original_id": tx.frame.id, "original_dlc": tx.frame.dlc,
                "original_gpio": tx.electrical.to_string(), "electrical_route_guard_passed": true,
                "physical_can_ack_verified": false, "wake_phase_advanced": false,
            }}), now)?;
            self.ordinary_pending = Some(tx.ticket);
            return Ok(());
        }
        let exact = matches!(self.phase, Phase::AwaitIntent)
            && tx.ticket != 0
            && tx.btr0 == 0xdd
            && tx.btr1 == 0x36
            && tx.frame.id == 0x100
            && !tx.frame.extended
            && !tx.frame.rtr
            && tx.frame.dlc == 0
            && tx.frame.data.is_empty()
            && tx.electrical
                == (CanElectricalState::SingleWireGpio {
                    latch: 0x26,
                    assignment: 0,
                    direction: 0x7f,
                });
        if !exact {
            return Err(self.fail("Wake serial barrier unexpected controller2 intent", now));
        }
        let route = electrical_route(controller, tx).map_err(|error| {
            self.fail(
                format!("Wake serial barrier electrical route rejected: {error}"),
                now,
            )
        })?;
        if route.channel != 1 || route.mode != ElectricalMode::SingleWireWake {
            return Err(self.fail("Wake serial barrier electrical route mismatch", now));
        }
        self.event(
            serde_json::json!({"wake_serial_barrier_intent_associated": {
                "controller": controller, "ticket": tx.ticket, "native_cycles": now,
                "original_id": tx.frame.id, "original_dlc": tx.frame.dlc,
                "original_gpio": tx.electrical.to_string(), "electrical_route_guard_passed": true,
                "physical_can_ack_verified": false,
            }}),
            now,
        )?;
        self.phase = Phase::AwaitCompletion { ticket: tx.ticket };
        Ok(())
    }

    /// Preflight before the caller changes any SJA1000 completion state.
    /// Validation never advances the wake phase or consumes a pending ticket.
    pub fn validate_completion(
        &mut self,
        controller: usize,
        ticket: u64,
        source: CompletionSource,
        now: u64,
    ) -> Result<(), String> {
        self.tick(now)?;
        if !self.active() || controller != 2 {
            return Ok(());
        }
        let ordinary = self.ordinary_pending == Some(ticket)
            && source == self.ordinary_source;
        let wake = matches!(self.phase, Phase::AwaitCompletion { ticket: expected } if expected == ticket)
            && source == self.wake_source;
        if !ordinary && !wake {
            return Err(self.fail(
                "Wake serial barrier completion ticket/source mismatch before application",
                now,
            ));
        }
        Ok(())
    }

    pub fn completed(
        &mut self,
        controller: usize,
        ticket: u64,
        source: CompletionSource,
        applied: bool,
        now: u64,
    ) -> Result<(), String> {
        self.validate_completion(controller, ticket, source, now)?;
        if !self.active() || controller != 2 {
            if self.ordinary_pending == Some(ticket)
                && controller == 2
                && source == self.ordinary_source
                && applied
            {
                self.ordinary_pending = None;
            }
            return Ok(());
        }
        if !applied {
            return Err(self.fail(
                "Wake serial barrier completion was not applied by native controller",
                now,
            ));
        }
        if self.ordinary_pending == Some(ticket)
            && source == self.ordinary_source
        {
            self.event(
                serde_json::json!({"wake_serial_barrier_ordinary_completion_observed": {
                    "controller": controller, "ticket": ticket, "source": source.label(),
                    "emulated_completion_applied": applied, "native_cycles": now,
                    "physical_can_ack_verified": false, "wake_phase_advanced": false,
                }}),
                now,
            )?;
            self.ordinary_pending = None;
            return Ok(());
        }
        let Some(deadline) = now.checked_add(RESTORE_CYCLES) else {
            return Err(self.fail("Wake serial barrier restore deadline overflow", now));
        };
        self.event(
            serde_json::json!({"wake_serial_barrier_completion_observed": {
                "controller": controller, "ticket": ticket, "source": source.label(),
                "emulated_completion_applied": applied, "native_cycles": now,
                "restore_deadline_cycles": deadline, "physical_can_ack_verified": false,
            }}),
            now,
        )?;
        self.phase = Phase::AwaitRestore {
            ticket,
            completed_at: now,
            deadline,
        };
        Ok(())
    }

    pub fn observe_gpio(&mut self, state: CanElectricalState, now: u64) -> Result<(), String> {
        self.tick(now)?;
        let Phase::AwaitRestore {
            ticket,
            completed_at,
            ..
        } = self.phase
        else {
            return Ok(());
        };
        if now <= completed_at
            || state
                != (CanElectricalState::SingleWireGpio {
                    latch: 0x27,
                    assignment: 0,
                    direction: 0x7f,
                })
        {
            return Ok(());
        }
        self.event(
            serde_json::json!({"wake_serial_barrier_native_gpio_restore_observed": {
                "controller": 2, "ticket": ticket, "native_cycles": now,
                "gpio": state.to_string(), "fresh_after_completion": true,
                "queued_frames": self.queue.len(), "guest_gpio_modified": false,
                "physical_can_ack_verified": false,
            }}),
            now,
        )?;
        self.phase = Phase::Releasing;
        self.release_clock=None;
        Ok(())
    }

    pub fn pop_releasable(&mut self, now: u64) -> Result<Option<SerialFrame>, String> {
        self.tick(now)?;
        if !matches!(self.phase, Phase::Releasing) {
            return Ok(None);
        }
        let Some(front) = self.queue.front() else {
            self.close(now)?;
            return Ok(None);
        };
        // Retain the original native inter-frame spacing. Empty SCI input alone
        // does not establish that the previous firmware command has finished.
        let (released_at,arrived_at)=*self.release_clock.get_or_insert((now,front.arrived_at));
        let due=released_at.checked_add(front.arrived_at.saturating_sub(arrived_at))
            .ok_or_else(||"Wake serial barrier release clock overflow".to_string())?;
        if now<due {return Ok(None);}
        let mut released_hash = self.released_hash.clone();
        hash_frame(&mut released_hash, &front.frame);
        let final_frame = self.queue.len() == 1;
        let queued_wake = front.frame.bytes == WAKE;
        let front_sequence = front.sequence;
        let front_audit = frame_audit(&front.frame);
        let required_events = if final_frame || queued_wake { 2 } else { 1 };
        if self.events.len() > MAX_EVENTS - required_events {
            return Err(self.fail("Wake serial barrier audit queue full before release", now));
        }
        if final_frame
            && (self.queued_count != self.released_count + 1
                || digest(&self.queued_hash) != digest(&released_hash))
        {
            return Err(self.fail(
                "Wake serial barrier final queued/released stream mismatch",
                now,
            ));
        }
        if queued_wake {
            // Re-arm before returning the untouched wake frame to the caller.
            // Preserve the original total gate deadline and cumulative stream;
            // subsequent FIFO entries remain held for this new wake's evidence.
            self.event(
                serde_json::json!({"wake_serial_barrier_queued_wake_rearmed": {
                    "sequence": front_sequence, "frame": front_audit,
                    "native_cycles": now, "total_deadline_cycles": self.deadline,
                    "retained_later_frames": self.queue.len() - 1,
                    "physical_can_ack_verified": false, "guest_bytes_modified": false,
                }}),
                now,
            )?;
            self.phase = Phase::AwaitIntent;
        }
        self.event(
            serde_json::json!({"wake_serial_barrier_released": {
                "sequence": front_sequence, "frame": front_audit, "native_cycles": now,
                "released_stream_sha256": digest(&released_hash), "hash_encoding": HASH_ENCODING,
                "physical_can_ack_verified": false, "guest_bytes_modified": false,
            }}),
            now,
        )?;
        let front = self.queue.pop_front().unwrap();
        self.queued_bytes -= front.frame.bytes.len() + 1;
        self.released_hash = released_hash;
        self.released_count += 1;
        if self.queue.is_empty() && !queued_wake {
            self.close(now)?;
        }
        Ok(Some(front.frame))
    }

    fn close(&mut self, now: u64) -> Result<(), String> {
        if self.queued_count != self.released_count
            || digest(&self.queued_hash) != digest(&self.released_hash)
        {
            return Err(self.fail("Wake serial barrier queued/released stream mismatch", now));
        }
        self.event(serde_json::json!({"wake_serial_barrier_drained": {
            "native_cycles": now, "queued_frames_cumulative": self.queued_count,
            "released_frames_cumulative": self.released_count,
            "queued_stream_sha256": digest(&self.queued_hash),
            "released_stream_sha256": digest(&self.released_hash), "hash_encoding": HASH_ENCODING,
            "physical_can_ack_verified": false,
        }}), now)?;
        self.phase = Phase::Idle;
        self.deadline = None;
        self.release_clock=None;
        Ok(())
    }

    pub fn take_events(&mut self) -> Vec<String> {
        std::mem::take(&mut self.events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::candi_cpu::CanFrame;

    fn frame(bytes: &[u8], delimiter: &str) -> SerialFrame {
        SerialFrame {
            bytes: bytes.to_vec(),
            delimiter: delimiter.into(),
        }
    }
    fn gpio(latch: u8) -> CanElectricalState {
        CanElectricalState::SingleWireGpio {
            latch,
            assignment: 0,
            direction: 0x7f,
        }
    }
    fn tx() -> CanTransmission {
        CanTransmission {
            ticket: 7,
            btr0: 0xdd,
            btr1: 0x36,
            electrical: gpio(0x26),
            frame: CanFrame {
                id: 0x100,
                extended: false,
                rtr: false,
                dlc: 0,
                data: Vec::new(),
            },
        }
    }
    fn armed() -> WakeSerialBarrier {
        let mut gate = WakeSerialBarrier::new();
        let wake = frame(&WAKE, "break");
        assert_eq!(gate.admit(wake.clone(), 10).unwrap(), Admission::Send(wake));
        assert!(gate.active());
        gate
    }
    fn pending() -> WakeSerialBarrier {
        let mut gate = armed();
        gate.associate_intent(2, &tx(), 20).unwrap();
        gate
    }
    fn ready(gate: &mut WakeSerialBarrier, at: u64) {
        gate.completed(
            2,
            7,
            CompletionSource::MicTranslatedWakeWriteCompatibility,
            true,
            at,
        )
        .unwrap();
        gate.observe_gpio(gpio(0x27), at + 1).unwrap();
    }
    fn assert_failed_no_release(gate: &mut WakeSerialBarrier, now: u64, retained: usize) {
        assert!(gate.active());
        assert_eq!(gate.queue.len(), retained);
        assert!(gate.pop_releasable(now).is_err());
        assert!(gate.tick(now.saturating_add(1)).is_err());
        assert_eq!(gate.queue.len(), retained);
        assert_eq!(gate.released_count, 0);
        assert!(gate
            .take_events()
            .iter()
            .all(|e| !e.contains("wake_serial_barrier_released")));
    }

    #[test]
    fn stn_profile_requires_matching_typed_completion_and_original_gpio_restore() {
        let source=CompletionSource::StnRfcommWriteCompatibility;
        let mut gate=WakeSerialBarrier::with_completion_sources(source,source);
        gate.admit(frame(&WAKE,"break"),10).unwrap();gate.associate_intent(2,&tx(),20).unwrap();
        gate.admit(frame(&[0x2e,0xd2],"break"),21).unwrap();
        gate.completed(2,7,source,true,30).unwrap();gate.observe_gpio(gpio(0x22),31).unwrap();
        assert!(gate.waiting_for_wake());assert_eq!(gate.pop_releasable(32).unwrap(),None);
        gate.observe_gpio(gpio(0x27),33).unwrap();assert!(gate.pop_releasable(34).unwrap().is_some());
        let mut bad=WakeSerialBarrier::with_completion_sources(source,source);
        bad.admit(frame(&WAKE,"break"),10).unwrap();bad.associate_intent(2,&tx(),20).unwrap();
        assert!(bad.validate_completion(2,7,CompletionSource::MicTranslatedWakeWriteCompatibility,30).is_err());
    }

    #[test]
    fn guest_wait_query_requires_matching_completion_then_fresh_original_restore() {
        let mut gate=WakeSerialBarrier::new();
        assert!(!gate.waiting_for_wake());
        gate=armed();assert!(gate.waiting_for_wake());
        gate.associate_intent(2,&tx(),20).unwrap();assert!(gate.waiting_for_wake());
        gate.observe_gpio(gpio(0x27),21).unwrap();assert!(gate.waiting_for_wake());
        gate.completed(2,7,CompletionSource::MicTranslatedWakeWriteCompatibility,true,30).unwrap();
        gate.observe_gpio(gpio(0x27),30).unwrap();assert!(gate.waiting_for_wake());
        gate.observe_gpio(gpio(0x27),31).unwrap();assert!(!gate.waiting_for_wake());
        gate.pop_releasable(32).unwrap();assert!(!gate.active());
        gate=armed();gate.abort("simulated backend rejection",40);assert!(!gate.waiting_for_wake());
        assert!(gate.pop_releasable(41).is_err());
    }

    #[test]
    fn ordinary_frames_pass_unchanged_without_arming() {
        let mut gate = WakeSerialBarrier::default();
        for f in [
            frame(&[0x2e, 0xd2], "break"),
            frame(&[0x88, 0x0e, 0x6b], "space-parity-zero"),
        ] {
            assert_eq!(gate.admit(f.clone(), 100).unwrap(), Admission::Send(f));
        }
        gate.associate_intent(2, &tx(), 101).unwrap();
        gate.completed(
            2,
            7,
            CompletionSource::MicTranslatedWakeWriteCompatibility,
            true,
            102,
        )
        .unwrap();
        gate.observe_gpio(gpio(0x27), 103).unwrap();
        gate.tick(u64::MAX).unwrap();
        assert_eq!(gate.pop_releasable(u64::MAX).unwrap(), None);
        assert!(!gate.active());
        assert!(gate.take_events().is_empty());
    }

    #[test]
    fn matching_completion_and_fresh_native_restore_release_fifo_and_equal_digest() {
        let mut gate = pending();
        let first = frame(&[0x22, 0x01, 0xdd], "break");
        let second = frame(&[0x2e, 0xd2], "space-parity-zero");
        assert_eq!(gate.admit(first.clone(), 21).unwrap(), Admission::Held);
        assert_eq!(gate.admit(second.clone(), 22).unwrap(), Admission::Held);
        assert_eq!(gate.pop_releasable(23).unwrap(), None);
        gate.observe_gpio(gpio(0x27), 24).unwrap();
        assert_eq!(gate.pop_releasable(25).unwrap(), None);
        gate.completed(
            2,
            7,
            CompletionSource::MicTranslatedWakeWriteCompatibility,
            true,
            30,
        )
        .unwrap();
        gate.observe_gpio(gpio(0x27), 30).unwrap();
        assert_eq!(
            gate.pop_releasable(30).unwrap(),
            None,
            "same-cycle observation is not fresh"
        );
        gate.observe_gpio(gpio(0x26), 31).unwrap();
        assert_eq!(gate.pop_releasable(31).unwrap(), None);
        gate.observe_gpio(gpio(0x27), 32).unwrap();
        assert_eq!(gate.pop_releasable(33).unwrap(), Some(first));
        assert!(gate.active(), "gate closed before FIFO drained");
        let third = frame(&[0xab, 0, 0x55], "break");
        assert_eq!(gate.admit(third.clone(), 34).unwrap(), Admission::Held);
        assert_eq!(gate.pop_releasable(35).unwrap(), Some(second));
        assert_eq!(gate.pop_releasable(36).unwrap(), None, "retained requests keep their original spacing");
        assert_eq!(gate.pop_releasable(45).unwrap(), None);
        assert_eq!(gate.pop_releasable(46).unwrap(), Some(third));
        assert!(!gate.active());
        assert_eq!(digest(&gate.queued_hash), digest(&gate.released_hash));
        let events = gate.take_events();
        let queued = events
            .iter()
            .filter_map(|e| {
                serde_json::from_str::<serde_json::Value>(e)
                    .unwrap()
                    .get("wake_serial_barrier_queued")
                    .cloned()
            })
            .collect::<Vec<_>>();
        let released = events
            .iter()
            .filter_map(|e| {
                serde_json::from_str::<serde_json::Value>(e)
                    .unwrap()
                    .get("wake_serial_barrier_released")
                    .cloned()
            })
            .collect::<Vec<_>>();
        assert_eq!(queued.len(), 3);
        assert_eq!(released.len(), 3);
        for (q, r) in queued.iter().zip(&released) {
            assert_eq!(q["sequence"], r["sequence"]);
            assert_eq!(q["frame"], r["frame"]);
        }
        assert_eq!(
            queued.last().unwrap()["queued_stream_sha256"],
            released.last().unwrap()["released_stream_sha256"]
        );
    }

    #[test]
    fn unrelated_other_controller_events_do_not_advance_the_gate() {
        let mut gate = pending();
        gate.admit(frame(&[1], "break"), 21).unwrap();
        for controller in [0, 1] {
            gate.associate_intent(controller, &tx(), 22).unwrap();
            gate.completed(
                controller,
                7,
                CompletionSource::J2534SoftwareLoopback,
                false,
                22,
            )
            .unwrap();
        }
        assert_eq!(gate.pop_releasable(23).unwrap(), None);
        ready(&mut gate, 24);
        assert_eq!(gate.pop_releasable(26).unwrap().unwrap().bytes, [1]);
    }

    #[test]
    fn mismatched_intent_or_electrical_snapshot_fails_without_release() {
        for variant in 0..10 {
            let mut gate = armed();
            gate.admit(frame(&[1], "break"), 11).unwrap();
            let mut bad = tx();
            match variant {
                0 => bad.frame.id = 0x101,
                1 => {
                    bad.frame.dlc = 1;
                    bad.frame.data = vec![0];
                }
                2 => bad.frame.extended = true,
                3 => bad.frame.rtr = true,
                4 => bad.btr0 = 0xc1,
                5 => bad.btr1 = 0x35,
                6 => bad.electrical = gpio(0x27),
                7 => {
                    bad.electrical = CanElectricalState::SingleWireGpio {
                        latch: 0x26,
                        assignment: 1,
                        direction: 0x7f,
                    }
                }
                8 => {
                    bad.electrical = CanElectricalState::SingleWireGpio {
                        latch: 0x26,
                        assignment: 0,
                        direction: 3,
                    }
                }
                9 => bad.ticket = 0,
                _ => unreachable!(),
            }
            assert!(
                gate.associate_intent(2, &bad, 12).is_err(),
                "variant={variant}"
            );
            assert_failed_no_release(&mut gate, 13, 1);
        }
        let mut duplicate = pending();
        duplicate.admit(frame(&[1], "break"), 21).unwrap();
        assert!(duplicate.associate_intent(2, &tx(), 22).is_err());
        assert_failed_no_release(&mut duplicate, 23, 1);
    }

    #[test]
    fn wrong_or_unapplied_completion_is_terminal() {
        for (ticket, source, applied) in [
            (
                8,
                CompletionSource::MicTranslatedWakeWriteCompatibility,
                true,
            ),
            (
                7,
                CompletionSource::MicByteTransportWriteCompatibility,
                true,
            ),
            (7, CompletionSource::J2534SoftwareLoopback, true),
            (
                7,
                CompletionSource::MicTranslatedWakeWriteCompatibility,
                false,
            ),
        ] {
            let mut gate = pending();
            gate.admit(frame(&[1], "break"), 21).unwrap();
            assert!(gate.completed(2, ticket, source, applied, 22).is_err());
            assert_failed_no_release(&mut gate, 23, 1);
        }
        let mut gate = armed();
        gate.admit(frame(&[1], "break"), 11).unwrap();
        assert!(gate
            .completed(
                2,
                7,
                CompletionSource::MicTranslatedWakeWriteCompatibility,
                true,
                12
            )
            .is_err());
        assert_failed_no_release(&mut gate, 13, 1);
    }

    #[test]
    fn timeout_and_missing_restore_keep_original_queue_without_release() {
        let mut total = pending();
        total.admit(frame(&[1, 2], "break"), 21).unwrap();
        total.tick(10 + TOTAL_CYCLES - 1).unwrap();
        assert!(total.tick(10 + TOTAL_CYCLES).is_err());
        assert_failed_no_release(&mut total, 10 + TOTAL_CYCLES + 1, 1);
        let mut restore = pending();
        restore.admit(frame(&[1, 2], "break"), 21).unwrap();
        restore
            .completed(
                2,
                7,
                CompletionSource::MicTranslatedWakeWriteCompatibility,
                true,
                30,
            )
            .unwrap();
        restore
            .observe_gpio(
                CanElectricalState::SingleWireGpio {
                    latch: 0x27,
                    assignment: 1,
                    direction: 0x7f,
                },
                31,
            )
            .unwrap();
        restore
            .observe_gpio(
                CanElectricalState::SingleWireGpio {
                    latch: 0x27,
                    assignment: 0,
                    direction: 3,
                },
                32,
            )
            .unwrap();
        assert_eq!(restore.pop_releasable(33).unwrap(), None);
        restore.tick(30 + RESTORE_CYCLES - 1).unwrap();
        assert!(restore
            .observe_gpio(gpio(0x27), 30 + RESTORE_CYCLES)
            .is_err());
        assert_failed_no_release(&mut restore, 31 + RESTORE_CYCLES, 1);
    }

    #[test]
    fn queue_limits_include_wire_delimiter_and_frame_count() {
        let mut bytes = pending();
        bytes
            .admit(frame(&vec![0x55; MAX_BYTES - 1], "break"), 21)
            .unwrap();
        assert_eq!(bytes.queued_bytes, MAX_BYTES);
        assert!(bytes.admit(frame(&[], "break"), 22).is_err());
        assert_failed_no_release(&mut bytes, 23, 1);
        assert_eq!(
            bytes.queue.front().unwrap().frame.bytes,
            vec![0x55; MAX_BYTES - 1]
        );
        let mut count = pending();
        for _ in 0..MAX_FRAMES {
            count.admit(frame(&[], "break"), 21).unwrap();
        }
        assert_eq!(count.queued_bytes, MAX_FRAMES);
        assert!(count.admit(frame(&[], "break"), 22).is_err());
        assert_failed_no_release(&mut count, 23, MAX_FRAMES);
        let mut label = pending();
        label.admit(frame(&[1], "break"), 21).unwrap();
        assert!(label
            .admit(frame(&[], &"x".repeat(MAX_DELIMITER_BYTES + 1)), 22)
            .is_err());
        assert_failed_no_release(&mut label, 23, 1);
    }

    #[test]
    fn clock_regression_and_deadline_overflow_are_terminal_without_release() {
        let mut regressed = pending();
        regressed.admit(frame(&[1], "break"), 21).unwrap();
        assert!(regressed.tick(20).is_err());
        assert_failed_no_release(&mut regressed, 23, 1);
        let mut overflow = WakeSerialBarrier::new();
        assert!(overflow.admit(frame(&WAKE, "break"), u64::MAX).is_err());
        assert_failed_no_release(&mut overflow, u64::MAX, 0);
    }

    #[test]
    fn queued_second_wake_rearms_before_delivery_and_interleaves_prior_ordinary_completion() {
        let mut gate = pending();
        let first = frame(&[0x22, 0x01, 0xdd], "break");
        let second_wake = frame(&WAKE, "space-parity-zero");
        let last = frame(&[0x2e, 0xd2], "break");
        for f in [&first, &second_wake, &last] {
            assert_eq!(gate.admit(f.clone(), 21).unwrap(), Admission::Held);
        }
        assert_eq!(gate.pop_releasable(22).unwrap(), None);
        ready(&mut gate, 30);
        assert_eq!(gate.pop_releasable(32).unwrap(), Some(first));
        let mut normal = tx();
        normal.ticket = 8;
        normal.electrical = gpio(0x27);
        normal.frame.id = 0x101;
        normal.frame.dlc = 8;
        normal.frame.data = vec![0xfe, 1, 0x3e, 0, 0, 0, 0, 0];
        gate.associate_intent(2, &normal, 33).unwrap();
        assert!(matches!(gate.phase, Phase::Releasing));
        assert_eq!(gate.pop_releasable(34).unwrap(), Some(second_wake));
        assert!(matches!(gate.phase, Phase::AwaitIntent));
        assert_eq!(gate.queue.len(), 1);
        assert_eq!(gate.pop_releasable(35).unwrap(), None);
        gate.validate_completion(
            2,
            8,
            CompletionSource::MicByteTransportWriteCompatibility,
            36,
        )
        .unwrap();
        assert_eq!(gate.ordinary_pending, Some(8), "preflight consumed ticket");
        gate.completed(
            2,
            8,
            CompletionSource::MicByteTransportWriteCompatibility,
            true,
            36,
        )
        .unwrap();
        assert!(
            matches!(gate.phase, Phase::AwaitIntent),
            "ordinary completion advanced new wake"
        );
        let mut next_wake = tx();
        next_wake.ticket = 9;
        gate.associate_intent(2, &next_wake, 37).unwrap();
        assert_eq!(gate.pop_releasable(38).unwrap(), None);
        gate.validate_completion(
            2,
            9,
            CompletionSource::MicTranslatedWakeWriteCompatibility,
            40,
        )
        .unwrap();
        assert!(matches!(gate.phase, Phase::AwaitCompletion { ticket: 9 }));
        gate.completed(
            2,
            9,
            CompletionSource::MicTranslatedWakeWriteCompatibility,
            true,
            40,
        )
        .unwrap();
        gate.observe_gpio(gpio(0x27), 40).unwrap();
        assert_eq!(gate.pop_releasable(40).unwrap(), None);
        gate.observe_gpio(gpio(0x26), 41).unwrap();
        assert_eq!(gate.pop_releasable(41).unwrap(), None);
        gate.observe_gpio(gpio(0x27), 42).unwrap();
        assert_eq!(gate.pop_releasable(43).unwrap(), Some(last));
        assert!(!gate.active());
        assert_eq!(gate.queued_count, 3);
        assert_eq!(gate.released_count, 3);
        assert_eq!(digest(&gate.queued_hash), digest(&gate.released_hash));
    }

    #[test]
    fn releasing_rejects_unexpected_hv_and_premature_next_wake_intent() {
        let mut hv = pending();
        hv.admit(frame(&[1], "break"), 21).unwrap();
        ready(&mut hv, 30);
        assert!(hv.associate_intent(2, &tx(), 32).is_err());
        assert_failed_no_release(&mut hv, 33, 1);
        let mut premature = pending();
        premature.admit(frame(&[1], "break"), 21).unwrap();
        premature.admit(frame(&WAKE, "break"), 22).unwrap();
        let mut next = tx();
        next.ticket = 8;
        assert!(premature.associate_intent(2, &next, 23).is_err());
        assert_failed_no_release(&mut premature, 24, 2);
    }

    #[test]
    fn completion_preflight_rejects_wrong_source_without_advancing_wake() {
        let mut gate = pending();
        gate.admit(frame(&[1], "break"), 21).unwrap();
        gate.validate_completion(
            2,
            7,
            CompletionSource::MicTranslatedWakeWriteCompatibility,
            22,
        )
        .unwrap();
        assert!(matches!(gate.phase, Phase::AwaitCompletion { ticket: 7 }));
        assert!(gate
            .validate_completion(
                2,
                7,
                CompletionSource::MicByteTransportWriteCompatibility,
                23
            )
            .is_err());
        assert_failed_no_release(&mut gate, 24, 1);
    }

    #[test]
    fn ordinary_sw_completion_is_validated_without_advancing_nonempty_release_fifo() {
        let mut gate = pending();
        gate.admit(frame(&[1], "break"), 21).unwrap();
        gate.admit(frame(&[2], "space-parity-zero"), 22).unwrap();
        ready(&mut gate, 30);
        let mut ordinary = tx();
        ordinary.ticket = 8;
        ordinary.electrical = gpio(0x27);
        ordinary.frame.id = 0x101;
        ordinary.frame.dlc = 8;
        ordinary.frame.data = vec![0xfe, 1, 0x3e, 0, 0, 0, 0, 0];
        gate.associate_intent(2, &ordinary, 32).unwrap();
        gate.validate_completion(
            2,
            8,
            CompletionSource::MicByteTransportWriteCompatibility,
            33,
        )
        .unwrap();
        assert_eq!(gate.ordinary_pending, Some(8));
        gate.completed(
            2,
            8,
            CompletionSource::MicByteTransportWriteCompatibility,
            true,
            33,
        )
        .unwrap();
        assert!(matches!(gate.phase, Phase::Releasing));
        assert_eq!(gate.queue.len(), 2);
        assert_eq!(gate.released_count, 0);
        assert_eq!(gate.pop_releasable(34).unwrap().unwrap().bytes, [1]);
        assert_eq!(gate.pop_releasable(35).unwrap().unwrap().bytes, [2]);
        assert!(!gate.active());
    }

    #[test]
    fn external_abort_retains_original_queue_and_cannot_resume_or_release() {
        let mut gate = pending();
        let first = frame(&[0x22, 0x01, 0xdd], "break");
        let second = frame(&[0x2e, 0xd2], "space-parity-zero");
        gate.admit(first.clone(), 21).unwrap();
        gate.admit(second.clone(), 22).unwrap();
        let expected_hash = digest(&gate.queued_hash);
        assert_eq!(
            gate.abort("fixture backend failure", 23),
            "fixture backend failure"
        );
        assert_eq!(gate.abort("later stop", 24), "fixture backend failure");
        assert!(gate.admit(frame(&[0x55], "break"), 25).is_err());
        assert!(gate
            .completed(
                2,
                7,
                CompletionSource::MicTranslatedWakeWriteCompatibility,
                true,
                26
            )
            .is_err());
        assert!(gate.observe_gpio(gpio(0x27), 27).is_err());
        assert_eq!(
            gate.queue
                .iter()
                .map(|q| q.frame.clone())
                .collect::<Vec<_>>(),
            [first, second]
        );
        let events = gate
            .events
            .iter()
            .map(|e| serde_json::from_str::<serde_json::Value>(e).unwrap())
            .collect::<Vec<_>>();
        let failures = events
            .iter()
            .filter_map(|e| e.get("wake_serial_barrier_failed"))
            .collect::<Vec<_>>();
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0]["queued_frames"], 2);
        assert_eq!(failures[0]["queued_frames_cumulative"], 2);
        assert_eq!(failures[0]["released_frames_cumulative"], 0);
        assert_eq!(failures[0]["queued_stream_sha256"], expected_hash);
        assert_eq!(failures[0]["release_permitted"], false);
        assert_failed_no_release(&mut gate, 28, 2);
    }

    #[test]
    fn empty_success_and_later_gate_preserve_cumulative_hashes() {
        let mut gate = pending();
        ready(&mut gate, 30);
        assert!(gate.active());
        assert_eq!(gate.pop_releasable(32).unwrap(), None);
        assert!(!gate.active());
        gate.admit(frame(&WAKE, "space-parity-zero"), 100).unwrap();
        gate.associate_intent(2, &tx(), 101).unwrap();
        let frame = frame(&[0x2e, 0xd2], "break");
        gate.admit(frame.clone(), 102).unwrap();
        ready(&mut gate, 103);
        assert_eq!(gate.pop_releasable(105).unwrap(), Some(frame));
        assert_eq!(gate.next_sequence, 2);
        assert_eq!(digest(&gate.queued_hash), digest(&gate.released_hash));
    }

    #[test]
    fn audit_bound_fails_before_frame_removal() {
        for event_count in [MAX_EVENTS - 1, MAX_EVENTS] {
            let mut gate = pending();
            gate.admit(frame(&[1], "break"), 21).unwrap();
            ready(&mut gate, 30);
            while gate.events.len() < event_count {
                gate.events.push("fixture-existing-audit".into());
            }
            assert!(gate.pop_releasable(32).is_err());
            assert_failed_no_release(&mut gate, 33, 1);
        }
    }

    #[test]
    fn stream_hash_distinguishes_frame_and_delimiter_boundaries() {
        let mut first = Sha256::new();
        hash_frame(&mut first, &frame(&[1], "break"));
        hash_frame(&mut first, &frame(&[2], "break"));
        let mut second = Sha256::new();
        hash_frame(&mut second, &frame(&[1, 2], "break"));
        assert_ne!(digest(&first), digest(&second));
        let mut third = Sha256::new();
        hash_frame(&mut third, &frame(&[1, 2], "space-parity-zero"));
        assert_ne!(digest(&second), digest(&third));
    }
}
