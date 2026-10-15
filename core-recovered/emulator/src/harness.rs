// SPDX-License-Identifier: MPL-2.0
//! Per-run guest screen assertions and manual screen capture. No global stage flags.
use crate::options::HarnessTarget;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    Splash,
    Main,
    Year,
    YearScroll,
    Platform,
    PlatformSport,
    Diagnostics,
    Engine,
    SecurityAll,
    AwaitSecurity,
    AwaitDtc,
    Checking,
    AwaitLink,
    Recovered,
    BackToPlatform,
    Complete,
}

#[derive(Debug)]
pub struct Event {
    pub capture: &'static str,
    pub key: Option<u8>,
    pub exit: bool,
    pub complete: bool,
}

pub struct Harness {
    stage: Stage,
    target: HarnessTarget,
    entered: u64,
    stable_since: Option<u64>,
    pending_milestone: Option<&'static str>,
    last_security_prompt: Option<&'static str>,
    pub last_verified: &'static str,
    year_row: usize,
    dtc_candidate: String,
    dtc_captured: String,
}

fn platform(text: &str) -> bool {
    text.contains("Vehicle Type") && text.contains("1 / 1")
}
fn diagnostics(text: &str) -> bool {
    text.contains("Diagnostics") && text.contains("Engine") && !text.contains("Checking")
}

impl Harness {
    pub fn new(target: HarnessTarget, insns: u64) -> Self {
        Self {
            stage: Stage::Splash,
            target,
            entered: insns,
            stable_since: None,
            pending_milestone: None,
            last_security_prompt: None,
            last_verified: "none",
            year_row: 1,
            dtc_candidate: String::new(),
            dtc_captured: String::new(),
        }
    }

    pub fn waiting_for(&self) -> String {
        format!("{:?}", self.stage)
    }

    pub fn observe(&mut self, text: &str, insns: u64) -> Result<Option<Event>, String> {
        if self.stage == Stage::Complete {
            return Ok(None);
        }
        let timeout = match self.stage {
            Stage::Splash => 80_000_000,
            // The guest communication wait ($1DBC74) permits $682 timer
            // ticks. At the current 10k-instruction PIT cadence, the previous
            // 2M deadline expired well before even one 16.66M wait completed.
            Stage::Recovered | Stage::AwaitLink => 40_000_000,
            // Keep the same live guest session through physical ON/LOCK prompts.
            // The remote seed-only bridge still imposes a five-minute wall limit.
            Stage::AwaitSecurity | Stage::AwaitDtc => 750_000_000,
            Stage::BackToPlatform => 2_000_000,
            _ => 10_000_000,
        };
        // After reaching Main Menu, NativeManual hands navigation to the
        // operator. Equipment questions can wait indefinitely for an answer;
        // this capture-only stage must not terminate a module-add session.
        // Bootstrap and scripted targets keep their existing deadlines. The
        // host's explicit run budget and operator stop remain independent.
        let manual_capture =
            self.target == HarnessTarget::NativeManual && self.stage == Stage::AwaitDtc;
        if !manual_capture && insns.saturating_sub(self.entered) >= timeout {
            return Err("harness stage deadline exceeded".into());
        }
        if self.stage == Stage::AwaitDtc {
            if self.dtc_candidate != text {
                self.dtc_candidate = text.into();
                self.stable_since = Some(insns);
            }
            if self.dtc_captured != text
                && insns.saturating_sub(self.stable_since.unwrap_or(insns)) >= 200_000
            {
                self.dtc_captured = text.into();
                return Ok(Some(Event {
                    capture: "native_dtc_screen",
                    key: None,
                    exit: false,
                    complete: false,
                }));
            }
            return Ok(None);
        }
        if self.stage == Stage::AwaitSecurity {
            let prompt = if text.contains("Turn Ignition key to ON position") {
                Some("security_ignition_on_prompt")
            } else if text.contains("Turn Ignition key to LOCK position") {
                Some("security_ignition_lock_prompt")
            } else if text.contains("Remove Ignition key") {
                Some("security_remove_key_prompt")
            } else if text.contains("You need Security Access from TIS2000")
                && text.contains("Disconnect Tech 2 from Vehicle")
            {
                Some("security_transfer_to_tis_prompt")
            } else {
                None
            };
            if let Some(capture) = prompt {
                if self.pending_milestone != prompt {
                    self.pending_milestone = prompt;
                    self.stable_since = None;
                }
                let since = *self.stable_since.get_or_insert(insns);
                if self.last_security_prompt != prompt && insns.saturating_sub(since) >= 200_000 {
                    self.last_security_prompt = prompt;
                    return Ok(Some(Event {
                        capture,
                        key: None,
                        exit: false,
                        complete: false,
                    }));
                }
                return Ok(None);
            }
            let ok = |label: &str| {
                text.lines().any(|line| {
                    line.trim()
                        .strip_prefix(label)
                        .is_some_and(|rest| rest.trim() == "OK")
                })
            };
            let milestone = if ok("Reading all vehicle VINs") && ok("Reading all vehicle Seed") {
                Some("stage8_native_seed_sweep_ok")
            } else if ok("Reading all vehicle VINs")
                && self.last_verified != "stage8_native_seed_sweep_ok"
            {
                Some("stage7_native_vin_sweep_ok")
            } else {
                None
            };
            if self.pending_milestone != milestone {
                self.pending_milestone = milestone;
                self.stable_since = None;
            }
            if let Some(capture) = milestone.filter(|capture| *capture != self.last_verified) {
                let since = *self.stable_since.get_or_insert(insns);
                if insns.saturating_sub(since) >= 200_000 {
                    self.last_verified = capture;
                    self.stable_since = None;
                    return Ok(Some(Event {
                        capture,
                        key: None,
                        exit: false,
                        complete: false,
                    }));
                }
                return Ok(None);
            }
        }
        if self.stage == Stage::AwaitLink {
            let status_ok = |label: &str| {
                text.lines().any(|line| {
                    line.trim()
                        .strip_prefix(label)
                        .is_some_and(|rest| rest.trim() == "OK")
                })
            };
            let milestone =
                if status_ok("Checking Key Position") && status_ok("Checking Vehicle Selection") {
                    Some("stage9_vehicle_selection_ok")
                } else if status_ok("Checking Key Position")
                    && self.last_verified != "stage9_vehicle_selection_ok"
                {
                    Some("stage8_key_position_ok")
                } else {
                    None
                };
            if self.pending_milestone != milestone {
                self.pending_milestone = milestone;
                self.stable_since = None;
            }
            if let Some(capture) = milestone.filter(|capture| *capture != self.last_verified) {
                let since = *self.stable_since.get_or_insert(insns);
                if insns.saturating_sub(since) >= 200_000 {
                    self.last_verified = capture;
                    self.stable_since = None;
                    return Ok(Some(Event {
                        capture,
                        key: None,
                        exit: false,
                        complete: false,
                    }));
                }
                return Ok(None);
            }
        }
        let matches = match self.stage {
            Stage::Splash => {
                (text.contains("Press [ENTER]") || text.contains("Press ENTER"))
                    && text.contains("Software Version")
                    && text.contains("North American Operations")
            }
            Stage::Main => text.contains("Main Menu") && text.contains("F0:"),
            Stage::Year => text.contains("Model Year") && text.contains("1 / 15"),
            Stage::YearScroll => {
                text.contains("Model Year") && text.contains(&format!("{} / 15", self.year_row))
            }
            Stage::Platform
                if matches!(
                    self.target,
                    HarnessTarget::Ecm
                        | HarnessTarget::EcmLink
                        | HarnessTarget::EcmLink1367
                        | HarnessTarget::SecurityLink1367
                        | HarnessTarget::DtcLink1367
                        | HarnessTarget::EngineData1367
                ) =>
            {
                text.contains("Vehicle Type")
                    && text.contains("1 / 2")
                    && text.contains("Saab 9-3 Sport")
                    && text.trim().ends_with("SAAB 9-5")
            }
            Stage::PlatformSport => {
                text.contains("Vehicle Type")
                    && text.contains("2 / 2")
                    && text.trim().ends_with("Saab 9-3 Sport (9440)")
            }
            Stage::Platform | Stage::BackToPlatform => platform(text),
            Stage::Diagnostics | Stage::Recovered => diagnostics(text),
            Stage::SecurityAll => {
                text.contains("Get Security Access") && text.contains("ECU Information")
            }
            Stage::Engine => text.contains("Customer Functions") && text.contains("Engine Control"),
            Stage::Checking => text.contains("Checking Key") && text.contains("Working"),
            Stage::Complete | Stage::AwaitLink | Stage::AwaitSecurity | Stage::AwaitDtc => false,
        };
        if !matches {
            self.stable_since = None;
            return Ok(None);
        }
        let stable = *self.stable_since.get_or_insert(insns);
        let settle = if self.stage == Stage::Checking {
            500_000
        } else {
            200_000
        };
        if insns.saturating_sub(stable) < settle {
            return Ok(None);
        }
        let (capture, key, exit, next) = match self.stage {
            Stage::Splash => (
                "stage1_splash",
                Some(crate::bus::KEY_ENTER_DEFAULT),
                false,
                Stage::Main,
            ),
            Stage::Main if self.target == HarnessTarget::NativeManual => {
                ("stage2_manual_main_menu", None, false, Stage::AwaitDtc)
            }
            Stage::Main => ("stage2_main_menu", Some(0x18), false, Stage::Year),
            Stage::Year
                if matches!(
                    self.target,
                    HarnessTarget::Ecm
                        | HarnessTarget::EcmLink
                        | HarnessTarget::EcmLink1367
                        | HarnessTarget::SecurityLink1367
                        | HarnessTarget::DtcLink1367
                        | HarnessTarget::EngineData1367
                ) =>
            {
                self.year_row = 2;
                (
                    "stage3_model_year",
                    Some(crate::bus::KEY_DOWN),
                    false,
                    Stage::YearScroll,
                )
            }
            Stage::Year => ("stage3_model_year", Some(0x10), false, Stage::Platform),
            Stage::YearScroll
                if self.year_row
                    < if matches!(
                        self.target,
                        HarnessTarget::EcmLink1367
                            | HarnessTarget::SecurityLink1367
                            | HarnessTarget::DtcLink1367
                            | HarnessTarget::EngineData1367
                    ) {
                        5
                    } else {
                        9
                    } =>
            {
                self.year_row += 1;
                (
                    "stage3_year_scrolling",
                    Some(crate::bus::KEY_DOWN),
                    false,
                    Stage::YearScroll,
                )
            }
            Stage::YearScroll => {
                let expected = if matches!(
                    self.target,
                    HarnessTarget::EcmLink1367
                        | HarnessTarget::SecurityLink1367
                        | HarnessTarget::DtcLink1367
                        | HarnessTarget::EngineData1367
                ) {
                    "(8) 2008"
                } else {
                    "(4) 2004"
                };
                if !text
                    .lines()
                    .rfind(|line| !line.trim().is_empty())
                    .is_some_and(|line| line.trim() == expected)
                {
                    return Err(format!("Year selection is not {expected}"));
                }
                (
                    if matches!(
                        self.target,
                        HarnessTarget::EcmLink1367
                            | HarnessTarget::SecurityLink1367
                            | HarnessTarget::DtcLink1367
                            | HarnessTarget::EngineData1367
                    ) {
                        "stage3_year_2008"
                    } else {
                        "stage3_year_2004"
                    },
                    Some(0x10),
                    false,
                    Stage::Platform,
                )
            }
            Stage::Platform
                if matches!(
                    self.target,
                    HarnessTarget::Ecm
                        | HarnessTarget::EcmLink
                        | HarnessTarget::EcmLink1367
                        | HarnessTarget::SecurityLink1367
                        | HarnessTarget::DtcLink1367
                        | HarnessTarget::EngineData1367
                ) =>
            {
                (
                    "stage4_vehicle_platform",
                    Some(crate::bus::KEY_DOWN),
                    false,
                    Stage::PlatformSport,
                )
            }
            Stage::Platform | Stage::PlatformSport => (
                "stage4_vehicle_platform",
                Some(0x10),
                false,
                Stage::Diagnostics,
            ),
            Stage::Diagnostics
                if matches!(self.target, HarnessTarget::Menus | HarnessTarget::Ecm) =>
            {
                ("stage5_diagnostics", None, false, Stage::Complete)
            }
            Stage::Diagnostics
                if matches!(
                    self.target,
                    HarnessTarget::SecurityLink1367 | HarnessTarget::DtcLink1367
                ) =>
            {
                ("stage5_diagnostics", Some(0x03), false, Stage::SecurityAll)
            }
            Stage::SecurityAll if self.target == HarnessTarget::DtcLink1367 => (
                "stage6_native_all_dtc_menu",
                Some(0x18),
                false,
                Stage::AwaitDtc,
            ),
            Stage::SecurityAll => (
                "stage6_security_access_menu",
                Some(0x16),
                false,
                Stage::AwaitSecurity,
            ),
            Stage::Diagnostics => ("stage5_diagnostics", Some(0x18), false, Stage::Engine),
            Stage::Engine if self.target == HarnessTarget::EngineData1367 => {
                ("stage6_engine_data", Some(0x10), false, Stage::AwaitDtc)
            }
            Stage::Engine => ("stage6_engine", Some(0x10), false, Stage::Checking),
            Stage::Checking => (
                "stage7_checking_key",
                None,
                !matches!(
                    self.target,
                    HarnessTarget::EcmLink | HarnessTarget::EcmLink1367
                ),
                if matches!(
                    self.target,
                    HarnessTarget::EcmLink | HarnessTarget::EcmLink1367
                ) {
                    Stage::AwaitLink
                } else {
                    Stage::Recovered
                },
            ),
            Stage::Recovered => ("stage7_recovered_menu", None, true, Stage::BackToPlatform),
            Stage::BackToPlatform => ("stage8_back_to_platform", None, false, Stage::Complete),
            Stage::Complete | Stage::AwaitLink | Stage::AwaitSecurity | Stage::AwaitDtc => {
                unreachable!()
            }
        };
        self.last_verified = capture;
        self.stage = next;
        self.entered = insns;
        self.stable_since = None;
        Ok(Some(Event {
            capture,
            key,
            exit,
            complete: next == Stage::Complete,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn engine_data_hands_off_to_operator_without_automatic_service_selection() {
        let mut h = Harness::new(HarnessTarget::EngineData1367, 0);
        h.stage = Stage::Diagnostics;
        h.observe("Diagnostics Engine", 1).unwrap();
        assert_eq!(
            h.observe("Diagnostics Engine", 200001)
                .unwrap()
                .unwrap()
                .key,
            Some(0x18)
        );
        h.observe("Customer Functions Engine Control", 200002)
            .unwrap();
        let e = h
            .observe("Customer Functions Engine Control", 400003)
            .unwrap()
            .unwrap();
        assert_eq!(e.key, Some(0x10));
        assert!(!e.exit && !e.complete);
        assert_eq!(h.waiting_for(), "AwaitDtc");
        h.observe("Engine Data Display", 500000).unwrap();
        let e = h.observe("Engine Data Display", 700001).unwrap().unwrap();
        assert_eq!(e.key, None);
        assert!(!e.exit && !e.complete);
    }

    #[test]
    fn ecm_link_probe_does_not_cancel_or_claim_success_at_checking() {
        let mut h = Harness::new(HarnessTarget::EcmLink, 0);
        h.stage = Stage::Checking;
        h.observe("Checking Key Position Working", 1).unwrap();
        let event = h
            .observe("Checking Key Position Working", 500001)
            .unwrap()
            .unwrap();
        assert!(!event.exit);
        assert!(!event.complete);
        assert_eq!(event.key, None);
        assert_eq!(h.waiting_for(), "AwaitLink");
        assert!(h.observe("Diagnostics Engine", 500002).unwrap().is_none());
    }
    #[test]
    fn key_position_ok_is_a_stable_partial_milestone() {
        let mut h = Harness::new(HarnessTarget::EcmLink1367, 0);
        h.stage = Stage::AwaitLink;
        assert!(h
            .observe(
                "Checking Key Position Working\nChecking Vehicle Selection OK",
                1
            )
            .unwrap()
            .is_none());
        let text = "Diagnostics\nChecking Key Position     OK\nChecking Vehicle Selection Working";
        assert!(h.observe(text, 100).unwrap().is_none());
        let event = h.observe(text, 200100).unwrap().unwrap();
        assert_eq!(event.capture, "stage8_key_position_ok");
        assert!(!event.complete && !event.exit && event.key.is_none());
        assert!(h.observe(text, 200101).unwrap().is_none());
    }
    #[test]
    fn native_dtc_target_selects_read_menu_and_only_captures_subsequent_screens() {
        let mut h = Harness::new(HarnessTarget::DtcLink1367, 0);
        h.stage = Stage::Diagnostics;
        h.observe("Diagnostics Engine", 1).unwrap();
        assert_eq!(
            h.observe("Diagnostics Engine", 200001)
                .unwrap()
                .unwrap()
                .key,
            Some(0x03)
        );
        let menu =
            "All\nF0: Diagnostic Trouble Codes (DTC)\nF1: ECU Information\nF6: Get Security Access";
        h.observe(menu, 300001).unwrap();
        let event = h.observe(menu, 500001).unwrap().unwrap();
        assert_eq!(event.key, Some(0x18));
        assert!(!event.complete);
        let screen = "Diagnostic Trouble Codes\nF0: Read DTC\nF1: Clear DTC";
        h.observe(screen, 600001).unwrap();
        let event = h.observe(screen, 800001).unwrap().unwrap();
        assert_eq!(event.capture, "native_dtc_screen");
        assert!(event.key.is_none() && !event.complete && !event.exit);
        assert!(h.observe(screen, 900001).unwrap().is_none());
    }

    #[test]
    fn security_navigation_uses_all_then_get_access_without_claiming_success() {
        let mut h = Harness::new(HarnessTarget::SecurityLink1367, 0);
        h.stage = Stage::Diagnostics;
        h.observe("Diagnostics Engine", 1).unwrap();
        assert_eq!(
            h.observe("Diagnostics Engine", 200001)
                .unwrap()
                .unwrap()
                .key,
            Some(0x03)
        );
        let menu = "All\nF1: ECU Information\nF6: Get Security Access";
        h.observe(menu, 300001).unwrap();
        let event = h.observe(menu, 500001).unwrap().unwrap();
        assert_eq!(event.key, Some(0x16));
        assert!(!event.complete && !event.exit);
        assert_eq!(h.waiting_for(), "AwaitSecurity");
        let sweep = "Reading all vehicle VINs OK\nReading all vehicle Seed Working";
        h.observe(sweep, 600001).unwrap();
        let event = h.observe(sweep, 800001).unwrap().unwrap();
        assert_eq!(event.capture, "stage7_native_vin_sweep_ok");
        assert!(!event.complete && !event.exit && event.key.is_none());
    }
    #[test]
    fn security_prompts_capture_evidence_without_acknowledging_vehicle_state() {
        let mut h = Harness::new(HarnessTarget::SecurityLink1367, 0);
        h.stage = Stage::AwaitSecurity;
        h.last_verified = "stage8_native_seed_sweep_ok";
        for (i, (text, capture)) in [
            (
                "Turn Ignition key to ON position",
                "security_ignition_on_prompt",
            ),
            (
                "Turn Ignition key to LOCK position",
                "security_ignition_lock_prompt",
            ),
            (
                "You need Security Access from TIS2000\n1. Disconnect Tech 2 from Vehicle.",
                "security_transfer_to_tis_prompt",
            ),
        ]
        .iter()
        .enumerate()
        {
            let at = 1 + i as u64 * 500_000;
            assert!(h.observe(text, at).unwrap().is_none());
            let event = h.observe(text, at + 200_000).unwrap().unwrap();
            assert_eq!(event.capture, *capture);
            assert!(!event.complete && !event.exit && event.key.is_none());
            assert!(h.observe(text, at + 250_000).unwrap().is_none());
            assert_eq!(h.last_verified, "stage8_native_seed_sweep_ok");
        }
    }
    #[test]
    fn vehicle_selection_requires_both_stable_statuses_and_never_regresses() {
        let mut h = Harness::new(HarnessTarget::EcmLink1367, 0);
        h.stage = Stage::AwaitLink;
        let key = "Checking Key Position OK\nChecking Vehicle Selection Working";
        let both = "Checking Key Position OK\nChecking Vehicle Selection OK";
        h.observe(key, 1).unwrap();
        assert!(h.observe(both, 200001).unwrap().is_none());
        h.observe(key, 300001).unwrap(); // A transient OK must not pass.
        assert!(h.observe(both, 400001).unwrap().is_none());
        let event = h.observe(both, 600001).unwrap().unwrap();
        assert_eq!(event.capture, "stage9_vehicle_selection_ok");
        assert!(!event.complete && !event.exit && event.key.is_none());
        assert!(h.observe(key, 900001).unwrap().is_none());
        assert_eq!(h.last_verified, "stage9_vehicle_selection_ok");
    }
    #[test]
    fn manual_vehicle_entry_stops_before_any_year_or_model_selection() {
        let mut h = Harness::new(HarnessTarget::NativeManual, 0);
        h.stage = Stage::Main;
        h.observe("Main Menu\nF0: Diagnostics", 1).unwrap();
        let e = h
            .observe("Main Menu\nF0: Diagnostics", 200001)
            .unwrap()
            .unwrap();
        assert_eq!(e.capture, "stage2_manual_main_menu");
        assert!(e.key.is_none());
        assert!(!e.complete);
        assert_eq!(h.stage, Stage::AwaitDtc);
    }
    #[test]
    fn manual_equipment_prompt_survives_scripted_deadline_and_keeps_capturing() {
        let mut h = Harness::new(HarnessTarget::NativeManual, 0);
        h.stage = Stage::Main;
        h.observe("Main Menu\nF0: Diagnostics", 1).unwrap();
        h.observe("Main Menu\nF0: Diagnostics", 200001).unwrap();
        let prompt = "PSS (Passenger Sensing System)\nWithout\nWith";
        let at = 900_000_000;
        assert!(h.observe(prompt, at).unwrap().is_none());
        let event = h.observe(prompt, at + 200_000).unwrap().unwrap();
        assert_eq!(event.capture, "native_dtc_screen");
        assert!(event.key.is_none() && !event.complete && !event.exit);
        assert!(h.observe(prompt, at * 2).unwrap().is_none());
        // An operator's next screen is still recorded without injecting keys.
        let next = "Select Option\nNext equipment question";
        assert!(h.observe(next, at * 2 + 1).unwrap().is_none());
        let event = h.observe(next, at * 2 + 200_001).unwrap().unwrap();
        assert!(event.key.is_none() && !event.complete && !event.exit);
    }
    #[test]
    fn manual_bootstrap_and_scripted_dtc_still_have_deadlines() {
        let mut manual = Harness::new(HarnessTarget::NativeManual, 0);
        assert!(manual.observe("", 80_000_000).is_err());
        let mut dtc = Harness::new(HarnessTarget::DtcLink1367, 0);
        dtc.stage = Stage::AwaitDtc;
        assert!(dtc.observe("Working", 750_000_000).is_err());
    }
    #[test]
    fn vehicle_1367_navigation_requires_the_actual_2008_row() {
        let mut h = Harness::new(HarnessTarget::EcmLink1367, 0);
        h.stage = Stage::YearScroll;
        h.year_row = 5;
        h.observe("Model Year 5 / 15\n(8) 2008", 1).unwrap();
        assert_eq!(
            h.observe("Model Year 5 / 15\n(8) 2008", 200001)
                .unwrap()
                .unwrap()
                .capture,
            "stage3_year_2008"
        );
    }
    #[test]
    fn ecm_navigation_requires_the_actual_year_and_platform_selection() {
        let mut h = Harness::new(HarnessTarget::Ecm, 0);
        h.stage = Stage::YearScroll;
        h.year_row = 9;
        h.observe("Model Year 9 / 15\n(5) 2005", 1).unwrap();
        assert!(h.observe("Model Year 9 / 15\n(5) 2005", 200001).is_err());
        h.stage = Stage::PlatformSport;
        h.stable_since = None;
        assert!(h
            .observe(
                "Vehicle Type 1 / 2\nSaab 9-3 Sport (9440)\nSAAB 9-5",
                300000
            )
            .unwrap()
            .is_none());
        let selected = "Vehicle Type 2 / 2\nSAAB 9-5\nSaab 9-3 Sport (9440)";
        h.observe(selected, 400000).unwrap();
        assert_eq!(
            h.observe(selected, 600000).unwrap().unwrap().key,
            Some(0x10)
        );
    }
    #[test]
    fn elapsed_time_and_unrelated_screens_do_not_pass() {
        let mut h = Harness::new(HarnessTarget::Recovery, 0);
        h.stage = Stage::BackToPlatform;
        assert!(h.observe("Diagnostics Engine", 600_000).unwrap().is_none());
        assert!(h.observe("Diagnostics Engine", 2_000_000).is_err());
    }
    #[test]
    fn unstable_screen_resets_settle_period() {
        let mut h = Harness::new(HarnessTarget::Menus, 0);
        h.stage = Stage::Diagnostics;
        assert!(h.observe("Diagnostics Engine", 50_000).unwrap().is_none());
        assert!(h.observe("", 200_000).unwrap().is_none());
        assert!(h.observe("Diagnostics Engine", 250_000).unwrap().is_none());
        assert!(
            h.observe("Diagnostics Engine", 450_000)
                .unwrap()
                .unwrap()
                .complete
        );
    }
    #[test]
    fn independent_runs_start_fresh() {
        for _ in 0..2 {
            let mut h = Harness::new(HarnessTarget::Menus, 0);
            let text = "Press [ENTER] Software Version North American Operations";
            h.observe(text, 50_000).unwrap();
            assert_eq!(
                h.observe(text, 250_000).unwrap().unwrap().capture,
                "stage1_splash"
            );
        }
    }
}
