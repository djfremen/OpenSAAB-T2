// SPDX-License-Identifier: MPL-2.0
//! Security slice of Android FirmwareMenuNavigator: recognized menus only,
//! stable screens, no repeated keys, bounded moves, manual ignition prompts.
use serde_json::{json, Value};
const KEYS: [u8; 10] = [0x18, 0x04, 0x13, 0x17, 0x03, 0x12, 0x16, 0x02, 0x11, 0x15];
fn function(s: &str, label: &str) -> Option<u8> {
    s.lines().find_map(|line| {
        let l = line.trim();
        let b = l.as_bytes();
        if b.len() > 3
            && b[0] == b'F'
            && b[1].is_ascii_digit()
            && b[2] == b':'
            && l[3..].trim() == label
        {
            Some(KEYS[(b[1] - b'0') as usize])
        } else {
            None
        }
    })
}
pub fn next(v: &Value) -> Value {
    let screen = v["screen"].as_str().unwrap_or("").trim();
    let now = v["now_ms"].as_u64().unwrap_or(0);
    let vin = v["vin"].as_str().unwrap_or("");
    let year = if super::valid_vin(vin) {
        match vin.as_bytes()[9] {
            b'3'..=b'9' => 2000 + (vin.as_bytes()[9] - b'0') as u16,
            b'A' => 2010,
            b'B' => 2011,
            b'C' => 2012,
            _ => 0,
        }
    } else {
        0
    };
    let supported = year >= 2003 && vin.starts_with("YS3F");
    let mut s = v["navigation"]
        .as_object()
        .cloned()
        .map(Value::Object)
        .unwrap_or_else(
            || json!({"stage":0,"moves":0,"started":now,"candidate":"","sent":"","stable":now}),
        );
    let mut stage = s["stage"].as_u64().unwrap_or(0);
    if !supported || stage >= 5 || now.saturating_sub(s["started"].as_u64().unwrap_or(now)) > 90000
    {
        s["stage"] = json!(5);
        return json!({"navigation":s,"key":null,"active":false,"hint":"Follow the original firmware menus and ignition-key prompts."});
    }
    if s["candidate"].as_str() != Some(screen) {
        s["candidate"] = json!(screen);
        s["stable"] = json!(now);
        return json!({"navigation":s,"key":null,"active":true});
    }
    if screen.is_empty()
        || s["sent"].as_str() == Some(screen)
        || now.saturating_sub(s["stable"].as_u64().unwrap_or(now)) < 50
    {
        return json!({"navigation":s,"key":null,"active":true});
    }
    let last = screen.lines().last().unwrap_or("").trim();
    let mut proposed = stage;
    let mut key = None;
    match stage {
        0 => {
            if screen.contains("Software Version")
                && (screen.contains("Press [ENTER]") || screen.contains("Press ENTER"))
            {
                key = Some(16);
            } else if screen.contains("Main Menu") {
                key = function(screen, "Diagnostics");
                if key.is_some() {
                    proposed = 1;
                }
            }
        }
        1 if screen.contains("Model Year") => {
            if let Some(shown) = last
                .split_whitespace()
                .last()
                .and_then(|y| y.parse::<u16>().ok())
            {
                if shown == year {
                    key = Some(16);
                    proposed = 2;
                } else if s["moves"].as_u64().unwrap_or(0) < 30 {
                    key = Some(if shown > year { 12 } else { 9 });
                } else {
                    stage = 5;
                }
            }
        }
        2 if screen.contains("Vehicle Type") => {
            if last.eq_ignore_ascii_case("saab 9-3 sport (9440)") {
                key = Some(16);
                proposed = 3;
            } else if last.eq_ignore_ascii_case("saab 9-5")
                && screen.contains("Saab 9-3 Sport (9440)")
                && s["moves"].as_u64().unwrap_or(0) < 30
            {
                key = Some(12);
            }
        }
        3 if screen.contains("Diagnostics") && !screen.contains("Checking") => {
            key = function(screen, "All");
            if key.is_some() {
                proposed = 4;
            }
        }
        4 if screen.contains("ECU Information") => {
            key = function(screen, "Get Security Access");
            if key.is_some() {
                proposed = 5;
            }
        }
        _ => {}
    }
    // Caller only requests navigation when its session key queue is empty and
    // commits returned state together with enqueueing the proposed key.
    if key.is_some() {
        s["sent"] = json!(screen);
        s["moves"] = json!(s["moves"].as_u64().unwrap_or(0) + 1);
        if proposed != stage {
            s["started"] = json!(now);
            s["moves"] = json!(0);
            stage = proposed;
        }
    }
    s["stage"] = json!(stage);
    json!({"navigation":s,"key":key,"active":stage<5})
}
