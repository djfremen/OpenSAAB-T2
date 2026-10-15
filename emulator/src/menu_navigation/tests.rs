//! Port of android/tests/FirmwareMenuNavigatorTest.java (same screens, keys and timing).
use super::*;

fn vin(year: u16) -> String {
    let c = match year {
        2003..=2009 => (b'0' + (year - 2000) as u8) as char,
        2010 => 'A',
        2011 => 'B',
        2012 => 'C',
        _ => '1',
    };
    format!("YS3FD49YX{c}1000001")
}
struct T {
    now: u64,
}
impl T {
    fn key(&mut self, n: &mut Navigator, screen: &str, expected: u8) {
        self.now += 2000;
        assert_eq!(
            n.step(screen, self.now),
            None,
            "first sight must only start stability: {screen}"
        );
        assert_eq!(
            n.step(screen, self.now + 100),
            Some(expected),
            "on {screen}"
        );
        assert_eq!(
            n.step(screen, self.now + 200),
            None,
            "repeated key on {screen}"
        );
    }
    fn none(&mut self, n: &mut Navigator, screen: &str) {
        self.now += 2000;
        n.step(screen, self.now);
        assert_eq!(
            n.step(screen, self.now + 100),
            None,
            "unexpected key on {screen}"
        );
    }
    fn vehicle(&mut self, n: &mut Navigator, year: u16) {
        self.key(n, "Main Menu\nF2: Diagnostics", 0x13);
        self.key(n, &format!("Model Year\n{year}"), 0x10);
        self.key(n, "Vehicle Type\nSaab 9-3 Sport (9440)", 0x10);
    }
}
fn nav(target: Target, year: u16) -> Navigator {
    Navigator::new(target, &vin(year), false)
}

#[test]
fn android_diagnostic_shortcuts() {
    let mut t = T { now: 0 };
    for year in [2004, 2008] {
        for target in [Target::ReadDtc, Target::ClearDtc] {
            let mut ecu = nav(Target::EcuInfo, 2004);
            t.vehicle(&mut ecu, 2004);
            t.key(&mut ecu, "Diagnostics\nF0: Engine\nF4: All", 0x18);
            t.key(&mut ecu, "Customer Functions\nEngine Control", 0x10);
            t.none(
                &mut ecu,
                "Checking Key Position\nEngine Control\nF1: ECU Information",
            );
            t.none(
                &mut ecu,
                "Engine Control\nF0: Diagnostic Trouble Codes\nF2: Data Display",
            );
            t.key(
                &mut ecu,
                "Engine Control\nF0: Diagnostic Trouble Codes\nF1: ECU Information",
                0x04,
            );
            t.none(&mut ecu, "ECU Information\nPress ENTER to continue");
            assert!(
                !ecu.active(),
                "ECU shortcut should leave resulting prompts to user"
            );
            let mut n = nav(target, year);
            t.vehicle(&mut n, year);
            t.none(&mut n, "Diagnostics\nChecking Key Position\nF4: All");
            t.key(&mut n, "Diagnostics\nF0: Engine\nF4: All", 0x03);
            t.key(
                &mut n,
                "All\nF0: Diagnostic Trouble Codes (DTC)\nF1: ECU Information",
                0x18,
            );
            t.key(
                &mut n,
                "Diagnostic Trouble Codes\nF2: Read DTC\nF5: Clear DTC",
                if target == Target::ReadDtc {
                    0x13
                } else {
                    0x12
                },
            );
            assert!(!n.active(), "shortcut did not end");
            t.none(&mut n, "Clear all codes?\nF0: Yes\nF1: No");
            t.none(&mut n, "Turn key to ON\nPress ENTER");
        }
    }
    let mut engine = nav(Target::EngineData, 2008);
    t.vehicle(&mut engine, 2008);
    t.key(&mut engine, "Diagnostics\nF0: Engine\nF4: All", 0x18);
    t.none(
        &mut engine,
        "Customer Functions\nEngine Control\nTransmission",
    );
    t.key(&mut engine, "Customer Functions\nEngine Control", 0x10);
    t.none(&mut engine, "Checking Key Position\nWorking");
    t.key(
        &mut engine,
        "Engine Control\nF0: Diagnostic Trouble Codes\nF2: Data Display",
        0x13,
    );
    t.none(&mut engine, "Engine Data Display");
    let mut engine = nav(Target::EngineData, 2004);
    t.vehicle(&mut engine, 2004);
    t.key(&mut engine, "Diagnostics\nF0: Engine", 0x18);
    t.key(&mut engine, "Customer Functions\nEngine Control", 0x10);
    t.none(&mut engine, "Engine Data Display");
    assert!(!engine.active(), "data display must finish shortcut");
    let mut n = nav(Target::ClearDtc, 2004);
    t.vehicle(&mut n, 2004);
    n.cancel();
    t.none(&mut n, "Diagnostics\nF4: All");
    let mut n = nav(Target::ReadDtc, 2004);
    n.step("Unexpected menu", 0);
    n.step("Unexpected menu", 90_001);
    t.none(&mut n, "Main Menu\nF0: Diagnostics");
    t.none(
        &mut Navigator::new(Target::ClearDtc, "", false),
        "Main Menu\nF0: Diagnostics",
    );
    t.none(
        &mut Navigator::new(Target::ClearDtc, "YS3EB49E123456789", false),
        "Main Menu\nF0: Diagnostics",
    );
    let mut n = Navigator::new(Target::ReadDtc, &vin(2004), true);
    t.key(
        &mut n,
        "All\nF0: Diagnostic Trouble Codes (DTC)\nF1: ECU Information\nF6: Get Security Access",
        0x18,
    );
    t.key(
        &mut n,
        "Diagnostic Trouble Codes\nF0: Read DTC\nF1: Clear DTC",
        0x18,
    );
    let mut n = Navigator::new(Target::Security, &vin(2004), true);
    t.key(
        &mut n,
        "All\nF1: ECU Information\nF6: Get Security Access",
        0x16,
    );
    let mut n = Navigator::new(Target::ClearDtc, &vin(2004), true);
    t.none(&mut n, "Programming in progress\nDo not disconnect");
    assert!(!n.active(), "unknown active task must be left alone");
    for target in [
        Target::Security,
        Target::ReadDtc,
        Target::ClearDtc,
        Target::EngineData,
        Target::EcuInfo,
    ] {
        let mut n = Navigator::new(target, &vin(2004), true);
        t.none(&mut n, "");
        t.key(
            &mut n,
            "32 MB\nPress [ENTER] to continue.\nSoftware Version: 9.250\nNorth American Operations",
            0x10,
        );
        t.none(&mut n, "");
        t.key(&mut n, "Main Menu\nF0: Diagnostics", 0x18);
        t.key(&mut n, "Model Year\n2004", 0x10);
        n.cancel();
        t.none(
            &mut n,
            "Press [ENTER] to continue.\nSoftware Version: 9.250",
        );
    }
    let mut n = nav(Target::ReadDtc, 2004);
    t.key(
        &mut n,
        "Press ENTER to continue\nSoftware Version: 9.250",
        0x10,
    );
    t.key(&mut n, "Main Menu\nF0: Diagnostics", 0x18);
}

#[test]
fn year_scroll_is_bounded_and_state_round_trips() {
    let mut t = T { now: 0 };
    let mut n = nav(Target::ReadDtc, 2004);
    t.key(&mut n, "Main Menu\nF0: Diagnostics", 0x18);
    t.key(&mut n, "Model Year\n(A) 2006", 0x0c);
    t.key(&mut n, "Model Year\n2003", 0x09);
    // Serialized state behaves identically across worker calls.
    let mut restored = Navigator::from_json(&n.to_json()).unwrap();
    t.key(&mut restored, "Model Year\n2004", 0x10);
    let mut n = nav(Target::ReadDtc, 2004);
    t.key(&mut n, "Main Menu\nF0: Diagnostics", 0x18);
    for i in 0..30 {
        t.key(&mut n, &format!("Model Year\n{}", 2010 - (i % 2)), 0x0c);
    }
    t.none(&mut n, "Model Year\n2010");
    assert!(!n.active(), "move budget must hand control back");
}

#[test]
fn worker_rejects_writes_in_read_only_and_supports_takeover() {
    let r = dispatch(&json!({"target":"clear_dtc","vin":vin(2004),"full_control":false})).unwrap();
    assert_eq!(r["rejected"], true);
    let r = dispatch(&json!({"target":"security","vin":vin(2004)})).unwrap();
    assert_eq!(r["rejected"], true);
    let r = dispatch(&json!({"target":"read_dtc","vin":vin(2004),"full_control":false,"screen":"Main Menu\nF0: Diagnostics","now_ms":1000})).unwrap();
    assert_eq!(r["active"], true);
    let r = dispatch(
        &json!({"navigation":r["navigation"],"screen":"Main Menu\nF0: Diagnostics","now_ms":1100}),
    )
    .unwrap();
    assert_eq!(r["key"], 0x18);
    let r = dispatch(&json!({"navigation":r["navigation"],"cancel":true})).unwrap();
    assert_eq!(
        (r["active"].as_bool(), r["key"].is_null()),
        (Some(false), true)
    );
    let r = dispatch(&json!({"target":"engine_data","vin":""})).unwrap();
    assert_eq!(r["active"], false);
    assert!(r["hint"].as_str().unwrap().contains("9-3 Sport"));
    assert!(dispatch(&json!({"target":"format_ecu"})).is_err());
}
