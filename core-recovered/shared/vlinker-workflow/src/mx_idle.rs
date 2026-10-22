// SPDX-License-Identifier: MPL-2.0
//! Adapter-only parked-link liveness. This state machine can emit only ATI.
use serde_json::{json, Value};
use std::time::Instant;

pub const PROBE_INTERVAL_MS: u64 = 30_000;
pub const REPLY_TIMEOUT_MS: u64 = 4_000;
const MAX_REPLY_BYTES: usize = 256;

pub struct Session {
    origin: Instant,
    previous_ms: u64,
    started: bool,
    pending_since: Option<u64>,
    next_probe_ms: u64,
    probes: u64,
    confirmed: u64,
    bytes: Vec<u8>,
    received_bytes: usize,
    prompt: bool,
    terminal: Option<Value>,
    owned: bool,
    lease: Option<crate::ownership::Lease>,
}
impl Default for Session { fn default() -> Self { Self::new() } }
impl Session {
    pub fn new() -> Self {
        Self { origin: Instant::now(), previous_ms: 0, started: false, pending_since: None,
            next_probe_ms: 0, probes: 0, confirmed: 0, bytes: Vec::new(), received_bytes: 0, prompt: false,
            terminal: None, owned: false, lease: None }
    }
    pub fn owned(mut self) -> Self { self.owned = true; self }
    fn evidence(&self, mut value: Value) -> Value {
        value["probe_count"] = json!(self.probes);
        value["confirmed_probe_count"] = json!(self.confirmed);
        value["vehicle_requests_sent"] = json!(0);
        value["automatic_replay"] = json!(false);
        value["adapter_only"] = json!(true);
        value["ready_for_transfer"] = json!(value["state"] == "waiting" && self.confirmed > 0 && self.pending_since.is_none());
        value
    }
    fn fail(&mut self, reason: impl Into<String>) -> Value {
        let value = self.evidence(json!({"state":"failed", "error":reason.into()}));
        self.terminal = Some(value.clone()); value
    }
    fn send(&mut self, now: u64) -> Value {
        self.pending_since = Some(now); self.probes += 1; self.bytes.clear(); self.received_bytes = 0; self.prompt = false;
        self.evidence(json!({"state":"send", "command":"ATI", "probe_index":self.probes,
            "reply_timeout_ms":REPLY_TIMEOUT_MS, "probe_interval_ms":PROBE_INTERVAL_MS}))
    }
    fn waiting(&self, now: u64) -> Value {
        self.evidence(json!({"state":"waiting", "next_probe_in_ms":self.next_probe_ms.saturating_sub(now),
            "reply_timeout_ms":REPLY_TIMEOUT_MS, "probe_interval_ms":PROBE_INTERVAL_MS}))
    }
    pub fn event(&mut self, event: &Value) -> Value {
        self.event_at(event, self.origin.elapsed().as_millis().min(u64::MAX as u128) as u64)
    }
    fn event_at(&mut self, event: &Value, now: u64) -> Value {
        if let Some(value) = &self.terminal { return value.clone(); }
        if now < self.previous_ms { return self.fail("Parked adapter clock regressed"); }
        self.previous_ms = now;
        let op = event["op"].as_str().unwrap_or("");
        if ["cancel", "disconnected", "error"].contains(&op) {
            return self.fail(event["message"].as_str().unwrap_or("Parked adapter monitoring stopped"));
        }
        if self.pending_since.is_some_and(|sent| now.saturating_sub(sent) >= REPLY_TIMEOUT_MS) {
            return self.fail("Parked adapter ATI prompt timeout; no automatic retry");
        }
        match op {
            "start" if !self.started => {
                if self.owned {
                    let owner = event["owner"].as_str().map(str::to_owned).unwrap_or_else(crate::ownership::unique);
                    self.lease = crate::ownership::Lease::acquire(&owner);
                    if self.lease.is_none() { return self.fail("Finish the active workflow before parked monitoring"); }
                }
                self.started = true; self.send(now)
            }
            "tick" if self.started => {
                if self.pending_since.is_none() && now >= self.next_probe_ms { self.send(now) }
                else { self.waiting(now) }
            }
            "rx" if self.started && self.pending_since.is_some() => {
                let Some(input) = event["bytes"].as_array() else { return self.fail("Missing parked ATI response bytes"); };
                if input.is_empty() { return self.fail("Empty parked ATI response packet"); }
                for value in input {
                    self.received_bytes += 1;
                    if self.received_bytes > MAX_REPLY_BYTES { return self.fail("Parked ATI reply exceeded limit"); }
                    let Some(byte) = value.as_u64().and_then(|n|u8::try_from(n).ok()) else { return self.fail("Invalid parked ATI response byte"); };
                    if self.prompt {
                        if !b"\r\n ".contains(&byte) { return self.fail("Unexpected bytes after parked ATI prompt"); }
                    } else if byte == b'>' { self.prompt = true; }
                    else if (32..=126).contains(&byte) || [10,13].contains(&byte) {
                        if self.bytes.len() >= MAX_REPLY_BYTES { return self.fail("Parked ATI reply exceeded limit"); }
                        self.bytes.push(byte);
                    } else { return self.fail("Non-ASCII parked ATI reply"); }
                }
                if !self.prompt { return self.waiting(now); }
                let raw = String::from_utf8_lossy(&self.bytes);
                let mut lines = raw.split(['\r','\n']).map(str::trim).filter(|s|!s.is_empty());
                let first = lines.next().unwrap_or("");
                let identity = if first == "ATI" { lines.next().unwrap_or("") } else { first };
                if lines.next().is_some() || !valid_identity(identity) { return self.fail("Parked ATI identity was not strictly confirmed"); }
                self.confirmed += 1; self.pending_since = None; self.next_probe_ms = now.saturating_add(PROBE_INTERVAL_MS);
                let mut value = self.waiting(now); value["identity"] = json!(identity); value
            }
            _ => self.fail("Unexpected parked adapter event or unsolicited input"),
        }
    }
}
fn valid_identity(identity: &str) -> bool {
    let Some(version) = identity.strip_prefix("ELM327 v") else { return false; };
    if version.len() > 16 { return false; }
    let numeric = if version.as_bytes().last().is_some_and(u8::is_ascii_alphabetic) { &version[..version.len()-1] } else { version };
    let parts = numeric.split('.').collect::<Vec<_>>();
    parts.len() >= 2 && parts.iter().all(|part|!part.is_empty() && part.len() <= 3 && part.bytes().all(|b|b.is_ascii_digit()))
}

#[cfg(test)] mod tests {
    use super::*;
    fn rx(session:&mut Session, bytes:&[u8], at:u64)->Value { session.event_at(&json!({"op":"rx","bytes":bytes}),at) }
    #[test] fn immediate_and_periodic_probe_are_only_ati() {
        let mut s=Session::new(); let sent=s.event_at(&json!({"op":"start"}),0);assert_eq!(sent["command"],"ATI");assert_eq!(sent["ready_for_transfer"],false);
        assert_eq!(rx(&mut s,b"ATI\rELM327",1)["ready_for_transfer"],false);
        let confirmed=rx(&mut s,b" v1.4b\r>",10);assert_eq!(confirmed["confirmed_probe_count"],1);assert_eq!(confirmed["ready_for_transfer"],true);
        assert_eq!(s.event_at(&json!({"op":"tick"}),30_009)["state"],"waiting");
        let sent=s.event_at(&json!({"op":"tick"}),30_010);assert_eq!(sent["command"],"ATI");assert_eq!(sent["ready_for_transfer"],false);
        let v=rx(&mut s,b"ELM327 v2.2\r>",30_020); assert_eq!(v["confirmed_probe_count"],2);assert_eq!(v["vehicle_requests_sent"],0);
    }
    #[test] fn fragmented_reply_is_bounded_and_single() {
        let mut s=Session::new();s.event_at(&json!({"op":"start"}),0);
        for part in b"ATI\rELM327 v1.4b\r>".chunks(2) { assert_ne!(rx(&mut s,part,1)["state"],"failed"); }
        assert_eq!(s.confirmed,1);
        assert_eq!(rx(&mut s,b"ELM327 v1.4b\r>",2)["state"],"failed");
    }
    #[test] fn noisy_duplicate_missing_or_invalid_identity_never_advances() {
        for raw in [b"ELM327 v1.4b\r>garbage".as_slice(),b"ELM327 v1.4b\r>>",b"ATI\rATI\rELM327 v1.4b\r>",b"ELM327 v1.4b\rOK\r>",b"ELM327 vfoo\r>",b"STOPPED\r>",b"?\r>",b"ELM327 v1.4b\0\r>"] {
            let mut s=Session::new();s.event_at(&json!({"op":"start"}),0);let v=rx(&mut s,raw,1);
            assert_eq!(v["state"],"failed","{raw:?}");assert_eq!(v["vehicle_requests_sent"],0);
            assert_eq!(s.event_at(&json!({"op":"tick"}),30_000),v);
        }
        let mut s=Session::new();s.event_at(&json!({"op":"start"}),0);assert_eq!(rx(&mut s,&vec![65;257],1)["state"],"failed");
    }
    #[test] fn timeout_cancel_and_clock_regression_are_terminal() {
        for op in ["cancel","disconnected","error"] {let mut s=Session::new();s.event_at(&json!({"op":"start"}),0);let v=s.event_at(&json!({"op":op}),1);assert_eq!(v["state"],"failed");assert_eq!(rx(&mut s,b"ELM327 v1.4b\r>",2),v);}
        let mut s=Session::new();s.event_at(&json!({"op":"start"}),0);rx(&mut s,b"ELM327 v1.4b\r",1);assert_eq!(s.event_at(&json!({"op":"tick"}),4_000)["state"],"failed");
        let mut s=Session::new();s.event_at(&json!({"op":"start"}),10);assert_eq!(s.event_at(&json!({"op":"tick"}),9)["state"],"failed");
    }
}
