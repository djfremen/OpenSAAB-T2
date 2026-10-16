use super::*;
use std::path::PathBuf;
#[test]
fn password_gate_covers_processing_fallback_import_and_automation_before_io() {
    for operation in ["authorize", "prepare", "prepare_existing", "http_result", "import"] {
        for password in [json!(null), json!(""), json!("wrong"), json!("opensaab"), json!("OpenSaab"), json!("OpenSAAB "), json!(false), json!(["OpenSAAB"])] {
            let request=json!({"schema":1,"op":operation,"password":password});
            assert_eq!(dispatch(&request).unwrap_err(), "Security access requires the case-sensitive OpenSAAB password.");
        }
        assert!(dispatch(&json!({"schema":1,"op":operation})).unwrap_err().contains("password"));
        assert!(dispatch(&json!({"schema":1,"op":operation,"password":"OpenSAAB","cancelled":true})).unwrap_err().contains("cancelled"));
        assert!(dispatch(&json!({"schema":1,"op":operation,"password":"OpenSAAB","cancelled":"false"})).unwrap_err().contains("cancellation state"));
    }
    assert_eq!(dispatch(&json!({"schema":1,"op":"authorize","password":"OpenSAAB"})).unwrap()["authorized"],true);
    assert_eq!(dispatch(&json!({"schema":1,"op":"http_result","password":"OpenSAAB","status":503,"primary":true,"allow_fallback":true})).unwrap()["action"],"fallback");
    // Passive observation is independent of the processing credential.
    assert!(dispatch(&json!({"schema":1,"op":"observe","screen":"Main Menu"})).is_ok());
}
fn fixture() -> Vec<u8> {
    let mut b = vec![255; SIZE];
    b[0] = 0xb1;
    b[20..37].copy_from_slice(b"YS3FF49Y541000000");
    b[0x132..0x138].copy_from_slice(&[0, 1, 3, 0x61, 0x12, 0x34]);
    b
}
fn reply(b: &[u8]) -> Vec<u8> {
    let mut a = b.to_vec();
    a[1] = 0;
    a[0x26..0x2e].fill(b'A');
    a[0x138..0x13a].copy_from_slice(&[0x56, 0x78]);
    a
}
#[test]
fn android_ssa_oracle_and_every_unrelated_byte() {
    let b = fixture();
    let a = reply(&b);
    assert_eq!(validate_input(&b), Ok(1));
    assert!(validate_reply(&b, &a).is_ok());
    for i in 0x14..SIZE {
        if i == 0x25 || (0x26..0x2e).contains(&i) || (0x138..0x13a).contains(&i) {
            continue;
        }
        let mut bad = a.clone();
        bad[i] ^= 1;
        assert!(validate_reply(&b, &bad).is_err(), "offset {i:x}");
    }
    assert!(validate_input(&vec![255; SIZE]).is_err());
    assert!(validate_input(&b[..713]).is_err());
    let mut partial = a;
    partial[0x138..0x13a].fill(255);
    assert!(validate_reply(&b, &partial).is_err());
}
#[test]
fn android_provider_oracle() {
    for status in [500, 502, 503, 504] {
        assert_eq!(
            http_decision(status, "", true, true, false)["action"],
            "fallback"
        );
        assert_eq!(
            http_decision(status, "", true, false, false)["action"],
            "error"
        );
        assert_eq!(
            http_decision(status, "", false, true, false)["action"],
            "error"
        );
    }
    for status in [301, 302, 307, 308, 400, 403, 404, 408, 409, 422, 429, 501] {
        assert_eq!(
            http_decision(status, "", true, true, false)["action"],
            "error"
        );
    }
    assert_eq!(
        http_decision(401, "", true, true, false)["action"],
        "unauthorized"
    );
    for network in ["timeout", "connect", "dns", "route"] {
        assert_eq!(
            http_decision(0, network, true, true, false)["action"],
            "fallback"
        );
    }
    for network in ["tls", "invalid", "oversize", "cancelled"] {
        assert_eq!(
            http_decision(0, network, true, true, false)["action"],
            "error"
        );
    }
    assert_eq!(http_decision(200, "", true, true, true)["action"], "error");
    assert_eq!(
        http_decision(200, "", true, true, false)["action"],
        "validate"
    );
}
#[test]
fn only_real_transfer_prompt() {
    assert_eq!(observe("F6: Get Security Access")["transfer"], false);
    assert_eq!(
        observe("You need Security Access from TIS2000\nDisconnect Tech 2 from Vehicle")
            ["transfer"],
        true
    );
}
struct Workspace(PathBuf);
impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn workspace() -> Workspace {
    let p = std::env::temp_dir().join(format!("ssa-test-{}", unique()));
    fs::create_dir_all(p.join("runs/collected")).unwrap();
    fs::create_dir(p.join("firmware")).unwrap();
    fs::write(p.join("firmware/active.txt"), "test").unwrap();
    let r = p.join("runs/collected");
    let b = vec![255; SIZE];
    let a = fixture();
    let mut card = vec![0x42; CARD_SIZE];
    card[OFFSET..OFFSET + SIZE].copy_from_slice(&b);
    fs::write(r.join("card.bin"), card).unwrap();
    fs::write(r.join("ssa-card-before.bin"), b).unwrap();
    fs::write(r.join("ssa-card-after.bin"), a).unwrap();
    fs::write(
        r.join("screen.txt"),
        "You need Security Access from TIS2000\nDisconnect Tech 2 from Vehicle",
    )
    .unwrap();
    write_json(&r.join("report.json"), &json!({})).unwrap();
    write_json(&r.join("native-security-snapshot.json"),&json!({"origin":"original-guest-memory","card_offset":OFFSET,"bytes":SIZE,"ssa_memory_flash_enabled":true,"ssa_erases":1,"ssa_programmed_bytes":714})).unwrap();
    write_json(
        &p.join("latest-session.json"),
        &json!({"run":r,"firmware":"test"}),
    )
    .unwrap();
    write_json(
        &p.join("vehicle-status.json"),
        &json!({"vin":"YS3FF49Y541000000"}),
    )
    .unwrap();
    Workspace(p)
}
#[test]
#[cfg(any(unix, windows))]
fn transaction_is_ssa_only_and_preserves_source() {
    let w = workspace();
    let p = &w.0;
    let r = p.join("runs/collected");
    let before = read(&r.join("card.bin"), CARD_SIZE).unwrap();
    let prepared = prepare(p, &r, "test", false).unwrap();
    let e = Path::new(prepared["attempt"].as_str().unwrap());
    let response = json!({"SSA_DATA":STANDARD.encode(reply(&fixture()))});
    assert!(import(e, &response, FALLBACK).is_err());
    let result = import(e, &response, PRIMARY).unwrap();
    assert_eq!(result["vehicle_access_verified"], false);
    assert_eq!(before, read(&r.join("card.bin"), CARD_SIZE).unwrap());
    let next = Path::new(result["run"].as_str().unwrap());
    let patched = read(&next.join("card.bin"), CARD_SIZE).unwrap();
    assert_eq!(&before[..OFFSET], &patched[..OFFSET]);
    assert_eq!(&before[OFFSET + SIZE..], &patched[OFFSET + SIZE..]);
    assert_eq!(&patched[OFFSET..OFFSET + SIZE], reply(&fixture()));
    assert!(import(e, &response, PRIMARY).is_err());
}
#[test]
fn cancellation_and_changed_vehicle_never_activate() {
    for change in [false, true] {
        let w = workspace();
        let p = &w.0;
        let r = p.join("runs/collected");
        let original = read(&p.join("latest-session.json"), 8192).unwrap();
        let prepared = prepare(p, &r, "test", false).unwrap();
        let e = Path::new(prepared["attempt"].as_str().unwrap());
        if change {
            fs::write(p.join("vehicle-status.json"), b"{}").unwrap();
        } else {
            fs::write(e.join("cancel"), b"1").unwrap();
        }
        assert!(import(
            e,
            &json!({"SSA_DATA":STANDARD.encode(reply(&fixture()))}),
            PRIMARY
        )
        .is_err());
        assert_eq!(
            original,
            read(&p.join("latest-session.json"), 8192).unwrap()
        );
    }
}
#[test]
fn navigation_stability_and_no_ignition_automation() {
    let mut v = json!({"vin":"YS3FF49Y541000000","screen":"Main Menu\nF0: Diagnostics","now_ms":0});
    let a = navigation::next(&v);
    assert!(a["key"].is_null());
    v["navigation"] = a["navigation"].clone();
    v["now_ms"] = json!(100);
    let a = navigation::next(&v);
    assert_eq!(a["key"], 24);
    v["navigation"] = a["navigation"].clone();
    v["now_ms"] = json!(200);
    assert!(navigation::next(&v)["key"].is_null());
    v["screen"] = json!("Turn Ignition key to LOCK position");
    v["navigation"] = navigation::next(&v)["navigation"].clone();
    v["now_ms"] = json!(300);
    assert!(navigation::next(&v)["key"].is_null());
}
#[test]
fn prepare_rejects_stale_collection_and_active_process() {
    for kind in [
        "no-flash",
        "no-prompt",
        "unchanged",
        "running",
        "firmware",
        "vehicle",
    ] {
        let w = workspace();
        let p = &w.0;
        let r = p.join("runs/collected");
        match kind {
            "no-flash" => fs::write(r.join("native-security-snapshot.json"), b"{}").unwrap(),
            "no-prompt" => fs::write(r.join("screen.txt"), b"F6: Get Security Access").unwrap(),
            "unchanged" => fs::copy(r.join("ssa-card-before.bin"), r.join("ssa-card-after.bin"))
                .map(|_| ())
                .unwrap(),
            "running" => fs::write(
                p.join("latest-session.json"),
                serde_json::to_vec(&json!({"run":r,"firmware":"test","pid":std::process::id()}))
                    .unwrap(),
            )
            .unwrap(),
            "firmware" => fs::write(p.join("firmware/active.txt"), b"other").unwrap(),
            _ => fs::write(p.join("vehicle-status.json"), b"{}").unwrap(),
        }
        assert!(prepare(p, &r, "test", true).is_err(), "accepted {kind}");
    }
}
#[test]
fn malformed_reply_and_changed_card_cannot_activate() {
    for change_card in [false, true] {
        let w = workspace();
        let p = &w.0;
        let r = p.join("runs/collected");
        let latest = read(&p.join("latest-session.json"), 8192).unwrap();
        let prepared = prepare(p, &r, "test", true).unwrap();
        let e = Path::new(prepared["attempt"].as_str().unwrap());
        let mut after = reply(&fixture());
        if change_card {
            let mut card = read(&r.join("card.bin"), CARD_SIZE).unwrap();
            card[0] ^= 1;
            fs::write(r.join("card.bin"), card).unwrap();
        } else {
            after[20] = b'1';
        }
        assert!(import(e, &json!({"SSA_DATA":STANDARD.encode(after)}), FALLBACK).is_err());
        assert_eq!(read(&p.join("latest-session.json"), 8192).unwrap(), latest);
        assert!(!e.join("result.json").exists());
    }
}
#[test]
#[cfg(any(unix, windows))]
fn fallback_receipt_is_bound_and_not_vehicle_authorization() {
    let w = workspace();
    let p = &w.0;
    let r = p.join("runs/collected");
    let prepared = prepare(p, &r, "test", true).unwrap();
    let e = Path::new(prepared["attempt"].as_str().unwrap());
    let result = import(
        e,
        &json!({"SSA_DATA":STANDARD.encode(reply(&fixture())),"request_id":"bad\nstage=verified"}),
        FALLBACK,
    )
    .unwrap();
    let receipt = String::from_utf8(
        read(
            &Path::new(result["run"].as_str().unwrap())
                .join("security-processing-status.properties"),
            8192,
        )
        .unwrap(),
    )
    .unwrap();
    assert!(receipt.contains("provider=Bojer\n"));
    assert!(receipt.contains("stage=imported\n"));
    assert!(receipt.contains(&hash(&reply(&fixture()))));
    assert!(!receipt.contains("request_id="));
    assert_eq!(result["vehicle_access_verified"], false);
}

fn reopened(w: &Workspace) -> std::path::PathBuf {
    let p = &w.0;
    let old = p.join("runs/collected");
    let new = p.join("runs/reopened");
    fs::create_dir(&new).unwrap();
    let mut card = read(&old.join("card.bin"), CARD_SIZE).unwrap();
    card[OFFSET..OFFSET + SIZE].copy_from_slice(&fixture());
    fs::write(new.join("card.bin"), card).unwrap();
    fs::write(new.join("working-ssa.bin"), fixture()).unwrap();
    fs::write(new.join("screen.txt"), "Tech 2 — Press ENTER to continue").unwrap();
    write_json(&new.join("report.json"), &json!({})).unwrap();
    fs::write(
        p.join("latest-session.json"),
        serde_json::to_vec(&json!({"run":new,"firmware":"test","mode":"offline"})).unwrap(),
    )
    .unwrap();
    new
}
#[test]
#[cfg(any(unix, windows))]
fn existing_seeds_can_process_from_reopened_offline_splash() {
    let w = workspace();
    let p = &w.0;
    let r = reopened(&w);
    let selected = inspect_existing(p, &r, "test").unwrap();
    assert_eq!(selected["records"], 1);
    assert_eq!(selected["collection_pending_stop"], false);
    let prepared = prepare_existing(
        p,
        &r,
        "test",
        false,
        selected["input_hash"].as_str().unwrap(),
    )
    .unwrap();
    let original = read(&r.join("card.bin"), CARD_SIZE).unwrap();
    let result = import(
        Path::new(prepared["attempt"].as_str().unwrap()),
        &json!({"SSA_DATA":STANDARD.encode(reply(&fixture()))}),
        PRIMARY,
    )
    .unwrap();
    assert_eq!(result["vehicle_access_verified"], false);
    assert_eq!(read(&r.join("card.bin"), CARD_SIZE).unwrap(), original);
    assert_eq!(
        read(
            &Path::new(result["run"].as_str().unwrap()).join("working-ssa.bin"),
            SIZE
        )
        .unwrap(),
        reply(&fixture())
    );
}
#[test]
fn existing_selection_requires_exact_provenance_not_just_a_vin() {
    for kind in [
        "seed",
        "firmware",
        "vehicle",
        "damaged",
        "postauth",
        "incomplete",
    ] {
        let w = workspace();
        let p = &w.0;
        let r = reopened(&w);
        match kind {
            "seed" => {
                let mut b = fixture();
                b[0x137] ^= 1;
                fs::write(r.join("working-ssa.bin"), b).unwrap();
            }
            "firmware" => {
                let mut b = read(&r.join("card.bin"), CARD_SIZE).unwrap();
                b[0] ^= 1;
                fs::write(r.join("card.bin"), b).unwrap();
            }
            "vehicle" => {
                fs::write(p.join("vehicle-status.json"), b"{}").unwrap();
            }
            "damaged" => {
                fs::write(r.join("working-ssa.bin"), [0]).unwrap();
            }
            "postauth" => {
                fs::write(r.join("working-ssa.bin"), reply(&fixture())).unwrap();
            }
            _ => {
                fs::write(
                    p.join("runs/collected/screen.txt"),
                    b"Reading all vehicle Seed",
                )
                .unwrap();
            }
        }
        assert!(inspect_existing(p, &r, "test").is_err(), "accepted {kind}");
    }
}
#[test]
fn changed_existing_selection_is_rejected_before_prepare_and_import() {
    let w = workspace();
    let p = &w.0;
    let r = reopened(&w);
    let selected = inspect_existing(p, &r, "test").unwrap();
    assert!(prepare_existing(p, &r, "test", false, "wrong").is_err());
    let prepared = prepare_existing(
        p,
        &r,
        "test",
        false,
        selected["input_hash"].as_str().unwrap(),
    )
    .unwrap();
    let original = read(&p.join("latest-session.json"), 8192).unwrap();
    let mut changed = fixture();
    changed[0x137] ^= 1;
    fs::write(r.join("working-ssa.bin"), changed).unwrap();
    assert!(import(
        Path::new(prepared["attempt"].as_str().unwrap()),
        &json!({"SSA_DATA":STANDARD.encode(reply(&fixture()))}),
        PRIMARY
    )
    .is_err());
    assert_eq!(
        original,
        read(&p.join("latest-session.json"), 8192).unwrap()
    );
}

#[test]
#[cfg(not(any(unix, windows)))]
fn unqualified_platform_import_preserves_card_and_session() {
    let w = workspace();
    let p = &w.0;
    let r = p.join("runs/collected");
    let card = read(&r.join("card.bin"), CARD_SIZE).unwrap();
    let latest = read(&p.join("latest-session.json"), 8192).unwrap();
    let prepared = prepare(p, &r, "test", false).unwrap();
    let attempt = Path::new(prepared["attempt"].as_str().unwrap());
    let error = import(
        attempt,
        &json!({"SSA_DATA":STANDARD.encode(reply(&fixture()))}),
        PRIMARY,
    )
    .unwrap_err();
    assert!(error.contains("qualified platform process/locking binding"));
    assert_eq!(read(&r.join("card.bin"), CARD_SIZE).unwrap(), card);
    assert_eq!(read(&p.join("latest-session.json"), 8192).unwrap(), latest);
    assert!(!attempt.join("result.json").exists());
}

#[test]
fn restored_collection_preserves_lineage_and_rejects_another_vehicle() {
    let w=workspace();let p=&w.0;let source=p.join("runs/collected");
    let next=p.join("runs/new");fs::create_dir(&next).unwrap();fs::copy(source.join("card.bin"),next.join("card.bin")).unwrap();
    assert_eq!(restore(p,"other",&next.join("card.bin")).unwrap()["restored"],false);
    assert_eq!(restore(p,"test",&next.join("card.bin")).unwrap()["restored"],true);
    assert_eq!(read(&next.join("working-ssa.bin"),SIZE).unwrap(),fixture());
    assert!(restore(p,"test",&source.join("card.bin")).is_err());
    assert!(restore(p,"test",&next.join("card.bin")).is_err());
    let other=p.join("runs/other");fs::create_dir(&other).unwrap();fs::copy(source.join("card.bin"),other.join("card.bin")).unwrap();
    fs::write(p.join("vehicle-status.json"),b"{}").unwrap();
    assert_eq!(restore(p,"test",&other.join("card.bin")).unwrap()["restored"],false);
}
#[test]
#[cfg(windows)]
fn windows_import_lock_and_process_liveness() {
    assert!(platform::require_stopped(std::process::id()).is_err());
    let mut child=std::process::Command::new("cmd.exe").args(["/c","exit","0"]).spawn().unwrap();
    child.wait().unwrap();assert!(platform::require_stopped(child.id()).is_ok());
    let w=workspace();let p=&w.0;let r=p.join("runs/collected");
    let prepared=prepare(p,&r,"test",false).unwrap();let e=Path::new(prepared["attempt"].as_str().unwrap());
    let lock=OpenOptions::new().create(true).read(true).write(true).open(p.join("runs/.security-import.lock")).unwrap();lock.try_lock().unwrap();
    assert!(import(e,&json!({"SSA_DATA":STANDARD.encode(reply(&fixture()))}),PRIMARY).is_err());
    drop(lock);
    assert!(import(e,&json!({"SSA_DATA":STANDARD.encode(reply(&fixture()))}),PRIMARY).is_ok());
}

#[test]
fn malformed_process_identity_cannot_prepare() {
    let w=workspace();let p=&w.0;let r=p.join("runs/collected");
    for pid in [json!("not-a-pid"),json!(0),json!(-1)] {
        fs::write(p.join("latest-session.json"),serde_json::to_vec(&json!({"run":r,"firmware":"test","pid":pid})).unwrap()).unwrap();
        assert!(prepare(p,&r,"test",false).is_err());
    }
}
