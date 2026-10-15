// SPDX-License-Identifier: MPL-2.0
//! Passive ignition observation for the capture-validated Saab 9440 profile (captures from 1367 and 2017).
//! Never originates CAN requests, changes guest memory, or treats bus silence as OFF.
use crate::candi_cpu::CanFrame;
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

pub const FRESH_MS: u64 = 2000;

#[derive(Default)]
pub struct State {
    subscribed: bool,
    last: Option<(u64, &'static str, Vec<u8>)>,
}
impl State {
    pub fn transmitted(&mut self, controller: usize, frame: &CanFrame) {
        if controller != 2 || frame.id != 0x241 || frame.extended || frame.rtr {
            return;
        }
        if frame.data == [3, 0xaa, 1, 1, 0, 0, 0, 0] {
            self.subscribed = true;
        }
        if frame.data == [1, 0x20, 0, 0, 0, 0, 0, 0] {
            self.subscribed = false;
        }
    }
    pub fn received(&mut self, controller: usize, frame: &CanFrame, now_ms: u64) {
        if !self.subscribed
            || controller != 2
            || frame.id != 0x541
            || frame.extended
            || frame.rtr
            || frame.dlc != 8
            || frame.data.len() != 8
            || frame.data[0] != 1
        {
            return;
        }
        // Match the complete observed three-byte state tuple. Do not guess ACC,
        // START or key removal from a single bit or another module's DPID.
        let state = match &frame.data[2..5] {
            [6, 2, 2] => "on",
            [1, 1, 0] => "lock",
            _ => "unknown",
        };
        self.last = Some((now_ms, state, frame.data.clone()));
    }
    pub fn snapshot(&self, now_ms: u64, connected: bool) -> Value {
        let age = self
            .last
            .as_ref()
            .map(|(t, _, _)| now_ms.saturating_sub(*t));
        let freshness = if !connected {
            "disconnected"
        } else if age.is_none() {
            "unavailable"
        } else if age.unwrap() > FRESH_MS {
            "stale"
        } else {
            "fresh"
        };
        let observed = self.last.as_ref().map(|(_, s, _)| *s).unwrap_or("unknown");
        json!({"schema":1,"profile":"saab-9440-cim-capture-validated",
            "source":"CIM DPID01 / I-bus 0x541", "freshness":freshness,
            "state":if freshness == "fresh" { observed } else { "unknown" },
            "last_observed_state":observed, "age_ms":age, "fresh_for_ms":FRESH_MS,
            "raw":self.last.as_ref().map(|(_,_,b)| b.iter().map(|x|format!("{x:02X}")).collect::<Vec<_>>().join(" ")),
            "connected":connected, "engine_running":Value::Null,
            "requests_origin":"original-firmware", "monitor_vehicle_transmissions":0})
    }
}

pub struct Monitor {
    pub state: State,
    directory: PathBuf,
    started: Instant,
    published: Option<Instant>,
    last_label: String,
    connected: bool,
}
impl Monitor {
    pub fn new(directory: &Path) -> Self {
        let mut m = Self {
            state: State::default(),
            directory: directory.into(),
            started: Instant::now(),
            published: None,
            last_label: String::new(),
            connected: true,
        };
        m.publish();
        m
    }
    pub fn received(&mut self, controller: usize, frame: &CanFrame) {
        self.state
            .received(controller, frame, self.started.elapsed().as_millis() as u64);
    }
    pub fn tick(&mut self) {
        if self
            .published
            .is_none_or(|t| t.elapsed().as_millis() >= 500)
        {
            self.publish();
        }
    }
    pub fn close(&mut self) {
        if self.connected {
            self.connected = false;
            self.publish();
        }
    }
    fn publish(&mut self) {
        let mut value = self
            .state
            .snapshot(self.started.elapsed().as_millis() as u64, self.connected);
        value["generated_unix_ms"] = json!(SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64);
        let label = format!(
            "{} {}",
            value["state"].as_str().unwrap(),
            value["freshness"].as_str().unwrap()
        );
        if label != self.last_label {
            println!("IGNITION_STATUS {value}");
            self.last_label = label;
        }
        let tmp = self.directory.join("ignition-status.json.tmp");
        if let Err(e) = std::fs::write(&tmp, value.to_string())
            .and_then(|_| std::fs::rename(&tmp, self.directory.join("ignition-status.json")))
        {
            eprintln!("IGNITION_STATUS telemetry write failed: {e}");
        }
        self.published = Some(Instant::now());
    }
}
impl Drop for Monitor {
    fn drop(&mut self) {
        self.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn frame(id: u32, data: &[u8]) -> CanFrame {
        CanFrame {
            id,
            extended: false,
            rtr: false,
            dlc: data.len() as u8,
            data: data.into(),
        }
    }
    fn subscribe(s: &mut State) {
        s.transmitted(2, &frame(0x241, &[3, 0xaa, 1, 1, 0, 0, 0, 0]));
    }
    #[test]
    fn captured_on_lock_and_unmapped_transition_with_freshness() {
        let mut s = State::default();
        subscribe(&mut s);
        s.received(2, &frame(0x541, &[1, 0x81, 6, 2, 2, 0xfd, 4, 0xfc]), 100);
        assert_eq!(s.snapshot(100, true)["state"], "on");
        assert_eq!(s.snapshot(2101, true)["state"], "unknown");
        assert_eq!(s.snapshot(2101, true)["freshness"], "stale");
        s.received(2, &frame(0x541, &[1, 0x81, 6, 1, 2, 0xfd, 4, 0xfc]), 2200);
        assert_eq!(s.snapshot(2200, true)["state"], "unknown");
        s.received(
            2,
            &frame(0x541, &[1, 0x82, 1, 1, 0, 0xfd, 0x5b, 0xfc]),
            2300,
        );
        assert_eq!(s.snapshot(2300, true)["state"], "lock");
        assert_eq!(s.snapshot(2300, false)["state"], "unknown");
        assert_eq!(s.snapshot(2300, false)["freshness"], "disconnected");
        assert!(s.snapshot(2300, true)["engine_running"].is_null());
    }
    #[test]
    fn only_correlated_standard_cim_dpid_can_update_state() {
        let mut s = State::default();
        let on = frame(0x541, &[1, 0x81, 6, 2, 2, 0xfd, 4, 0xfc]);
        s.received(2, &on, 1);
        assert_eq!(s.snapshot(1, true)["freshness"], "unavailable");
        subscribe(&mut s);
        for c in [0, 1] {
            s.received(c, &on, 2);
        }
        let mut bad = on.clone();
        bad.extended = true;
        s.received(2, &bad, 2);
        bad = on.clone();
        bad.rtr = true;
        s.received(2, &bad, 2);
        bad = on.clone();
        bad.id = 0x542;
        s.received(2, &bad, 2);
        bad = on.clone();
        bad.data[0] = 0x81;
        s.received(2, &bad, 2); // DTC response, same ID
        for n in 0..8 {
            s.received(2, &frame(0x541, &on.data[..n]), 2);
        }
        assert_eq!(s.snapshot(2, true)["freshness"], "unavailable");
        s.received(2, &on, 3);
        assert_eq!(s.snapshot(3, true)["state"], "on");
        s.transmitted(2, &frame(0x241, &[1, 0x20, 0, 0, 0, 0, 0, 0]));
        s.received(2, &on, 9999);
        assert_eq!(s.snapshot(9999, true)["freshness"], "stale");
    }
}
