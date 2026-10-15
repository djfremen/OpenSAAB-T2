// SPDX-License-Identifier: MPL-2.0
//! Port of Android FirmwareMenuNavigator: one explicit shortcut selects recognized original
//! firmware menus, then leaves task prompts (ignition, confirmations, results) to the operator.
//! English menus, identified MY2003–2012 Saab 9-3 Sport (9440) only. Stable screen for 50 ms,
//! never repeats a key on the same screen, 90 s per stage, 30-move budget, manual takeover.
//! The caller keeps the returned `navigation` state and asks only when its key queue is empty;
//! committing the state together with enqueueing the key is Android's `sent()`.
use serde_json::{json, Value};

const KEYS: [u8; 10] = [0x18, 0x04, 0x13, 0x17, 0x03, 0x12, 0x16, 0x02, 0x11, 0x15];
const ENTER: u8 = 0x10;
/// Tech 2 arrow codes as the Mac keypad maps them (↓ = 0x0c, ↑ = 0x09); same values as Android.
const DOWN: u8 = 0x0c;
const UP: u8 = 0x09;
const STAGE_MS: u64 = 90_000;
const STABLE_MS: u64 = 50;
const MOVES: u64 = 30;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Target {
    Security,
    ReadDtc,
    ClearDtc,
    EngineData,
    EcuInfo,
}
impl Target {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "security" => Self::Security,
            "read_dtc" => Self::ReadDtc,
            "clear_dtc" => Self::ClearDtc,
            "engine_data" => Self::EngineData,
            "ecu_info" => Self::EcuInfo,
            _ => return None,
        })
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Security => "Get Security Access",
            Self::ReadDtc => "Read DTC",
            Self::ClearDtc => "Clear DTC",
            Self::EngineData => "Engine Data",
            Self::EcuInfo => "ECU information",
        }
    }
    fn engine(self) -> bool {
        matches!(self, Self::EngineData | Self::EcuInfo)
    }
    /// Operations that write to the vehicle are rejected in read-only sessions (Android restricted mode).
    pub fn needs_full_control(self) -> bool {
        matches!(self, Self::Security | Self::ClearDtc)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Stage {
    Main,
    Year,
    Platform,
    Diagnostics,
    All,
    Dtc,
    Engine,
    EngineMenu,
    Done,
}
impl Stage {
    fn name(self) -> &'static str {
        match self {
            Self::Main => "main",
            Self::Year => "year",
            Self::Platform => "platform",
            Self::Diagnostics => "diagnostics",
            Self::All => "all",
            Self::Dtc => "dtc",
            Self::Engine => "engine",
            Self::EngineMenu => "engine_menu",
            Self::Done => "done",
        }
    }
    fn parse(s: &str) -> Self {
        match s {
            "main" => Self::Main,
            "year" => Self::Year,
            "platform" => Self::Platform,
            "diagnostics" => Self::Diagnostics,
            "all" => Self::All,
            "dtc" => Self::Dtc,
            "engine" => Self::Engine,
            "engine_menu" => Self::EngineMenu,
            _ => Self::Done,
        }
    }
}

/// Model year from VIN position 10 for supported 9-3 Sport (9440) VINs; 0 when unsupported.
pub fn supported_year(vin: &str) -> u16 {
    let b = vin.as_bytes();
    if b.len() != 17 || !vin.starts_with("YS3F") || !b.iter().all(|c| c.is_ascii_alphanumeric()) {
        return 0;
    }
    let year = match b[9] {
        b'3'..=b'9' => 2000 + (b[9] - b'0') as u16,
        b'A' => 2010,
        b'B' => 2011,
        b'C' => 2012,
        _ => 0,
    };
    if (2003..=2012).contains(&year) {
        year
    } else {
        0
    }
}

pub fn function(s: &str, label: &str) -> Option<u8> {
    s.lines().find_map(|line| {
        let l = line.trim();
        let b = l.as_bytes();
        (b.len() > 3
            && b[0] == b'F'
            && b[1].is_ascii_digit()
            && b[2] == b':'
            && l[3..].trim() == label)
            .then(|| KEYS[(b[1] - b'0') as usize])
    })
}
fn last(s: &str) -> &str {
    s.lines().last().unwrap_or("").trim()
}
/// Android: `(?:\([A-Z0-9]\)\s*)?((?:19|20)\d{2})` must match the whole selected line.
fn shown_year(line: &str) -> Option<u16> {
    let mut rest = line;
    if rest.len() >= 3 && rest.as_bytes()[0] == b'(' && rest.as_bytes()[2] == b')' {
        let c = rest.as_bytes()[1];
        if !(c.is_ascii_uppercase() || c.is_ascii_digit()) {
            return None;
        }
        rest = rest[3..].trim_start();
    }
    (rest.len() == 4
        && (rest.starts_with("19") || rest.starts_with("20"))
        && rest.bytes().all(|b| b.is_ascii_digit()))
    .then(|| rest.parse().ok())
    .flatten()
}

pub struct Navigator {
    target: Target,
    year: u16,
    stage: Stage,
    join: bool,
    manual: bool,
    candidate: String,
    sent: String,
    stable_at: u64,
    stage_at: Option<u64>,
    moves: u64,
}

impl Navigator {
    pub fn new(target: Target, vin: &str, join_current_menu: bool) -> Self {
        let year = supported_year(vin);
        let mut n = Navigator {
            target,
            year,
            stage: Stage::Main,
            join: join_current_menu,
            manual: false,
            candidate: String::new(),
            sent: String::new(),
            stable_at: 0,
            stage_at: None,
            moves: 0,
        };
        if year == 0 {
            n.cancel();
        }
        n
    }
    pub fn active(&self) -> bool {
        self.stage != Stage::Done
    }
    pub fn cancel(&mut self) {
        self.manual = true;
        self.stage = Stage::Done;
    }
    pub fn hint(&self) -> String {
        let label = self.target.label();
        if self.manual {
            if self.year == 0 {
                return format!("{label} shortcut needs an identified MY2003–2012 Saab 9-3 Sport (9440) with English menus. Navigate the original menus manually.");
            }
            format!("Return to a firmware menu and choose {label} again, or navigate manually.")
        } else if self.stage == Stage::Done {
            format!("Opened {label} · follow the firmware prompts.")
        } else {
            format!("Selecting {} · Saab 9-3 Sport (9440) → {label}… Press any firmware key to take over.", self.year)
        }
    }
    /// Returns the key to send for this screen, without committing (see `sent`).
    fn propose(&mut self, screen: &str, now: u64) -> Option<(u8, Stage)> {
        if self.stage == Stage::Done {
            return None;
        }
        let text = screen.trim();
        if self.stage_at.is_none() {
            if text.is_empty() {
                return None;
            }
            self.stage_at = Some(now);
        }
        if now.saturating_sub(self.stage_at.unwrap()) > STAGE_MS {
            self.cancel();
            return None;
        }
        if self.candidate != text {
            self.candidate = text.to_string();
            self.stable_at = now;
            return None;
        }
        if text.is_empty() || text == self.sent || now.saturating_sub(self.stable_at) < STABLE_MS {
            return None;
        }
        // The operator can return to the logo after a task or restart: advance only the ENTER splash.
        if self.stage == Stage::Main
            && text.contains("Software Version")
            && (text.contains("Press [ENTER]") || text.contains("Press ENTER"))
        {
            self.join = false;
            return Some((ENTER, Stage::Main));
        }
        let t = self.target;
        if self.join {
            self.join = false;
            self.stage = if text.contains("Main Menu") && function(text, "Diagnostics").is_some() {
                Stage::Main
            } else if text.contains("Model Year") {
                Stage::Year
            } else if text.contains("Vehicle Type") {
                Stage::Platform
            } else if text.contains("Diagnostics")
                && !text.contains("Checking")
                && function(text, "All").is_some()
            {
                Stage::Diagnostics
            } else if !t.engine()
                && text.contains("ECU Information")
                && function(text, "Get Security Access").is_some()
            {
                Stage::All
            } else if matches!(t, Target::ReadDtc | Target::ClearDtc)
                && function(text, "Read DTC").is_some()
                && function(text, "Clear DTC").is_some()
            {
                Stage::Dtc
            } else if t.engine()
                && text.contains("Customer Functions")
                && last(text) == "Engine Control"
            {
                Stage::Engine
            } else {
                self.cancel();
                return None;
            };
        }
        match self.stage {
            Stage::Main if text.contains("Main Menu") => {
                function(text, "Diagnostics").map(|k| (k, Stage::Year))
            }
            Stage::Year if text.contains("Model Year") => {
                let shown = shown_year(last(text))?;
                if shown == self.year {
                    Some((ENTER, Stage::Platform))
                } else if self.moves >= MOVES {
                    self.cancel();
                    None
                } else {
                    Some((if shown > self.year { DOWN } else { UP }, Stage::Year))
                }
            }
            Stage::Platform if text.contains("Vehicle Type") => {
                let selected = last(text).to_ascii_lowercase();
                if selected == "saab 9-3 sport (9440)" {
                    Some((ENTER, Stage::Diagnostics))
                } else if text.contains("Saab 9-3 Sport (9440)")
                    && selected == "saab 9-5"
                    && self.moves < MOVES
                {
                    Some((DOWN, Stage::Platform))
                } else {
                    None
                }
            }
            Stage::Diagnostics if text.contains("Diagnostics") && !text.contains("Checking") => {
                if t.engine() {
                    function(text, "Engine").map(|k| (k, Stage::Engine))
                } else {
                    function(text, "All").map(|k| (k, Stage::All))
                }
            }
            Stage::All if text.contains("ECU Information") => {
                if t == Target::Security {
                    function(text, "Get Security Access").map(|k| (k, Stage::Done))
                } else {
                    function(text, "Diagnostic Trouble Codes (DTC)").map(|k| (k, Stage::Dtc))
                }
            }
            Stage::Dtc if text.contains("Diagnostic Trouble Codes") => {
                function(text, t.label()).map(|k| (k, Stage::Done))
            }
            Stage::Engine
                if text.contains("Customer Functions") && last(text) == "Engine Control" =>
            {
                Some((ENTER, Stage::EngineMenu))
            }
            Stage::EngineMenu => {
                if text.contains("Checking")
                    || text.contains("Working")
                    || text.contains("Turn key")
                {
                    return None;
                }
                if t == Target::EcuInfo {
                    return text
                        .contains("Engine")
                        .then(|| function(text, "ECU Information"))
                        .flatten()
                        .map(|k| (k, Stage::Done));
                }
                if text.contains("Engine Data Display") {
                    self.stage = Stage::Done;
                    return None;
                }
                if text.contains("Engine") {
                    function(text, "Data Display")
                        .or_else(|| function(text, "Engine Data"))
                        .map(|k| (k, Stage::Done))
                } else {
                    None
                }
            }
            _ => None,
        }
    }
    /// Android `next()` + `sent()`: the caller enqueues the key and keeps the returned state.
    pub fn step(&mut self, screen: &str, now: u64) -> Option<u8> {
        let (key, proposed) = self.propose(screen, now)?;
        self.sent = self.candidate.clone();
        self.moves += 1;
        if proposed != self.stage {
            self.stage = proposed;
            self.stage_at = Some(now);
            self.moves = 0;
        }
        Some(key)
    }
    pub fn to_json(&self) -> Value {
        json!({"target":target_name(self.target),"year":self.year,"stage":self.stage.name(),"join":self.join,
            "manual":self.manual,"candidate":self.candidate,"sent":self.sent,"stable_at":self.stable_at,
            "stage_at":self.stage_at,"moves":self.moves})
    }
    pub fn from_json(v: &Value) -> Option<Self> {
        Some(Navigator {
            target: Target::parse(v["target"].as_str()?)?,
            year: v["year"].as_u64()? as u16,
            stage: Stage::parse(v["stage"].as_str()?),
            join: v["join"].as_bool()?,
            manual: v["manual"].as_bool()?,
            candidate: v["candidate"].as_str()?.chars().take(8192).collect(),
            sent: v["sent"].as_str()?.chars().take(8192).collect(),
            stable_at: v["stable_at"].as_u64()?,
            stage_at: v["stage_at"].as_u64(),
            moves: v["moves"].as_u64()?,
        })
    }
}
fn target_name(t: Target) -> &'static str {
    match t {
        Target::Security => "security",
        Target::ReadDtc => "read_dtc",
        Target::ClearDtc => "clear_dtc",
        Target::EngineData => "engine_data",
        Target::EcuInfo => "ecu_info",
    }
}

/// Worker op. Start: {"target","vin","full_control"}; continue: {"navigation","screen","now_ms"};
/// manual takeover: {"navigation","cancel":true}.
pub fn dispatch(v: &Value) -> Result<Value, String> {
    let mut nav = if let Some(state) = v.get("navigation").filter(|s| s.is_object()) {
        Navigator::from_json(state).ok_or("Invalid navigation state")?
    } else {
        let target =
            Target::parse(v["target"].as_str().unwrap_or("")).ok_or("Unsupported shortcut")?;
        if target.needs_full_control() && v["full_control"] != true {
            return Ok(json!({"rejected":true,"active":false,
                "hint":"This connection is read-only. Start a Full native control session for this task."}));
        }
        Navigator::new(target, v["vin"].as_str().unwrap_or(""), true)
    };
    if v["cancel"] == true {
        nav.cancel();
    }
    let key = match v["screen"].as_str() {
        Some(screen) if v["cancel"] != true => nav.step(screen, v["now_ms"].as_u64().unwrap_or(0)),
        _ => None,
    };
    Ok(json!({"navigation":nav.to_json(),"key":key,"active":nav.active(),"hint":nav.hint()}))
}

#[cfg(test)]
mod tests;
