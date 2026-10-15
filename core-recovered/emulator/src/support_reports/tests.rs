use super::*;
fn fixture() -> PathBuf {
    let root = std::env::temp_dir().join(format!("support-test-{}", unique()));
    fs::create_dir_all(root.join("runs")).unwrap();
    root
}
fn mac() -> Value {
    json!({"platform":"macos","arch":"arm64","version":"0.2.18","build":"abc1234","os_version":"Version 15.1",
        "ram_total_bytes":17179869184u64,"storage_free_bytes":1u64<<35,"runtime_cpu_count":10,"extra":"dropped"})
}
const VIN: &str = "YS3FD49Y141012345";

#[test]
fn vin_redaction_is_bounded_to_vin_shaped_tokens() {
    assert_eq!(
        redact_vins(&format!("car {VIN} failed")),
        "car [VIN removed] failed"
    );
    assert_eq!(redact_vins(&format!("VIN={VIN}")), "VIN=[VIN removed]");
    for keep in [
        "ABCDEFGHJKLMNPRST",
        "12345678901234567",
        "YS3FD49Y1410123456",
        "ys3fd49y141012345",
        "0x00000100",
    ] {
        assert_eq!(redact_vins(keep), keep);
    }
}

#[test]
fn collects_bounded_session_summary_without_raw_content() {
    let root = fixture();
    let run = root.join("runs/2026-09-24T10-00-00Z-abcd1234");
    fs::create_dir_all(&run).unwrap();
    fs::write(
        run.join("console.log"),
        format!("VIN={VIN}\nUSB_TX 01 02\nCAN_RX data=[1A 90]\nTIMEOUT waiting\nsecret payload\n"),
    )
    .unwrap();
    fs::write(
        run.join("dtc-context.json"),
        json!({"adapter":"Chipsoft Pro","vin":VIN}).to_string(),
    )
    .unwrap();
    fs::write(run.join("report.json"), json!({"status":"incomplete","exit_code":3,"instructions":42,"pc":1,"reason":VIN,"card_image":"/Users/x/card.bin","adapter_failure":{"kind":"single_wire_wake_failed","adapter_error":"Chipsoft opcode=000F status=001D","message":"secret detail"}}).to_string()).unwrap();
    fs::write(
        run.join("session-end.json"),
        json!({"reason":"adapter_stopped","exit_code":3,"elapsed_ms":900,"console":"x"})
            .to_string(),
    )
    .unwrap();
    fs::write(run.join("crash.json"), "{}").unwrap();
    fs::write(
        run.join("host-performance.json"),
        json!({"performance_schema":1,"sample_count":2,"complete":true,
        "samples":[{"elapsed_ms":5000,"cpu_ms":10,"rss_kib":100,"argv":"x"}]})
        .to_string(),
    )
    .unwrap();
    fs::write(root.join("last-app-error.json"), json!({"utc":"2026-09-24T10:00:00Z","kind":"session_error","message":format!("No reply from {VIN}")}).to_string()).unwrap();
    let report = collect(&root, &format!("It froze on {VIN}"), &mac()).unwrap();
    let text = report.to_string();
    assert!(!text.contains(VIN), "{text}");
    assert!(!text.contains("secret detail") && !text.contains("001D"), "adapter text must not leak into reports");
    assert!(
        !text.contains("secret")
            && !text.contains("1A 90")
            && !text.contains("card.bin")
            && !text.contains("extra")
    );
    assert_eq!(report["app"], APP);
    assert_eq!(report["platform"], "macos");
    let s = &report["recent_sessions"][0];
    assert_eq!(s["adapter"], "chipsoft");
    assert_eq!(
        s["emulator_outcome"],
        json!({"status":"incomplete","exit_code":3,"instructions":42,"pc":1,"reason":"single_wire_wake_failed"})
    );
    assert_eq!(
        s["session_end"],
        json!({"reason":"adapter_stopped","exit_code":3,"elapsed_ms":900})
    );
    assert_eq!(s["guest_crash"], true);
    assert_eq!(
        s["native_performance"]["samples"][0],
        json!({"elapsed_ms":5000,"cpu_ms":10,"rss_kib":100})
    );
    let counts = &s["logs"][0]["event_counts"];
    assert_eq!(
        (
            counts["USB_TX"].as_u64(),
            counts["CAN_RX"].as_u64(),
            counts["TIMEOUT"].as_u64()
        ),
        (Some(1), Some(1), Some(1))
    );
    assert_eq!(
        report["last-app-error.json"]["message"],
        "No reply from [VIN removed]"
    );
}

#[test]
fn rejects_unknown_platform_and_bounds_sessions_and_attempts() {
    let root = fixture();
    assert!(collect(&root, "", &json!({"platform":"android","arch":"arm64"})).is_err());
    for i in 0..9 {
        fs::create_dir_all(root.join(format!("runs/r{i}"))).unwrap();
    }
    let dir = root.join("connection-attempts");
    fs::create_dir_all(&dir).unwrap();
    for i in 0..25 {
        let id = format!("00000000-0000-0000-0000-{i:012}");
        fs::write(dir.join(format!("{id}.json")), json!({"attempt_id":id,"adapter":"CHIPSOFT","stage":"VIN_REQUEST","outcome":"FAILED","reason":"VIN_UNAVAILABLE","elapsed_ms":7,"port":"/dev/cu.usbmodem1","stages":[{"stage":"DETECTION","elapsed_ms":0},{"stage":"BOGUS"}]}).to_string()).unwrap();
    }
    fs::write(
        dir.join("bad.json"),
        json!({"attempt_id":"x","stage":"SESSION","outcome":"FAILED"}).to_string(),
    )
    .unwrap();
    let r = collect(&root, &"x".repeat(5000), &mac()).unwrap();
    assert_eq!(r["recent_sessions"].as_array().unwrap().len(), 5);
    let a = r["connection_attempts"].as_array().unwrap();
    assert_eq!(a.len(), 20);
    assert!(!r.to_string().contains("usbmodem"));
    assert_eq!(a[0]["stages"].as_array().unwrap().len(), 1);
    assert_eq!(r["description"].as_str().unwrap().len(), 2000);
    assert!(serde_json::to_vec_pretty(&r).unwrap().len() < MAX_UPLOAD);
}

#[test]
fn save_is_frozen_and_rotates() {
    let root = fixture();
    let r = collect(&root, "hello", &mac()).unwrap();
    let mut last = Value::Null;
    for _ in 0..10 {
        last = save(&root, &r).unwrap();
    }
    let text = fs::read_to_string(last["path"].as_str().unwrap()).unwrap();
    assert_eq!(text, last["text"].as_str().unwrap());
    let n = fs::read_dir(root.join("support-reports")).unwrap().count();
    assert_eq!(n, 8);
}

#[test]
fn receipt_matches_android_upload_rules() {
    let ok = r#"{"report_id":"OS-0123456789abcdef01234567","stored":true}"#;
    assert_eq!(receipt(201, ok, "")["stored"], true);
    assert_eq!(receipt(200, ok, "")["stored"], false);
    assert_eq!(
        receipt(201, r#"{"report_id":"OS-XYZ","stored":true}"#, "")["stored"],
        false
    );
    assert_eq!(
        receipt(
            201,
            r#"{"report_id":"OS-0123456789abcdef01234567","stored":false}"#,
            ""
        )["stored"],
        false
    );
    assert_eq!(receipt(201, &"x".repeat(5000), "")["stored"], false);
    assert_eq!(receipt(429, "", "")["retry"], true);
    assert_eq!(receipt(400, "", "")["retry"], false);
    assert_eq!(receipt(0, "", "offline")["stored"], false);
    let root = fixture();
    let d =
        dispatch(&json!({"schema":1,"op":"support_receipt","root":root,"status":201,"body":ok}))
            .unwrap();
    assert_eq!(d["report_id"], "OS-0123456789abcdef01234567");
    assert_eq!(
        dispatch(&json!({"schema":1,"op":"support_last","root":root})).unwrap()["report_id"],
        "OS-0123456789abcdef01234567"
    );
}
