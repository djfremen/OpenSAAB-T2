// SPDX-License-Identifier: MPL-2.0
use opensaab_vlinker_workflow::vlinker::{Session, VIN_REQUEST, DTC_READ, INFO_READS};
use serde_json::{json, Value};
const VIN: &str = "7E8 10 13 5A 90 59 53 33 46\r7E8 21 44 34 39 59 58 34 31\r7E8 22 30 30 30 30 30 31 00";
fn response(command: &str) -> String {
    match command {
        "ATZ" | "ATI" => "ELM327 v1.4b".into(),
        "STDI" => "OBDLink MX+ r3.1.3".into(),
        "STI" => "STN2255 v5.12.4".into(),
        "ATRV" => "13.0V".into(),
        "STPR" => "6".into(), "STPBRR" => "500000".into(),
        VIN_REQUEST => VIN.into(),
        "0210020000000000" => "7E8 01 50 00 00 00 00 00 00".into(),
        "0120000000000000" => "7E8 01 60 00 00 00 00 00 00".into(),
        DTC_READ => "7E8 03 7F A9 78 00 00 00 00\r5E8 81 01 07 00 6F 00 00 00\r5E8 81 00 00 00 FF 00 00 00".into(),
        s if INFO_READS.iter().any(|r|r.0==s) => format!("7E8 03 5A {} 41 00 00 00 00", &s[4..6]),
        _ => "OK".into(),
    }
}
fn run(mode: &str, changed: Option<(&str,&str)>) -> (Value,Vec<String>) {
    let mut s = if mode=="ecm_dtc" {Session::probe(true)} else if mode=="ecm_info" {Session::probe(false)} else {Session::new(mode=="vin")};
    assert_eq!(s.event(&json!({"op":"select_profile","model":"obdlink_mxp"}))["state"],"idle");
    let mut effect=s.event(&json!({"op":"start"}));let mut commands=vec![];
    while effect["state"]=="send" || effect.get("resync_quiet_ms").is_some() {
        if effect.get("resync_quiet_ms").is_some() {
            std::thread::sleep(std::time::Duration::from_millis(310));
            effect=s.event(&json!({"op":"tick"}));continue;
        }
        let command=effect["command"].as_str().unwrap().to_owned();commands.push(command.clone());
        let reply=changed.filter(|(c,_)|*c==command).map(|(_,r)|r.to_string()).unwrap_or_else(||response(&command));
        let bytes=format!("{reply}\r>").into_bytes();
        for part in bytes.chunks(2) {effect=s.event(&json!({"op":"rx","bytes":part}));}
    }
    (effect,commands)
}
#[test] fn mxp_identity_vin_information_and_codes_use_stn_only() {
    for mode in ["identity","vin","ecm_info","ecm_dtc"] {
        let (v,commands)=run(mode,None);assert_eq!(v["state"],"complete","{v}");
        assert_eq!(v["result"]["command_family"],"STN");
        assert_eq!(v["result"]["adapter_model"],"obdlink_mxp");
        assert!(!commands.iter().any(|c|c.starts_with("VT")));
        assert_eq!(commands.iter().filter(|c|c.as_str()==VIN_REQUEST).count(),usize::from(mode!="identity"));
        if mode!="identity" {assert_eq!(commands.last().unwrap(),"STPC");assert_eq!(v["result"]["vin"],"YS3FD49YX41000001");}
        if mode=="ecm_info" {assert_eq!(v["result"]["ecm_identifiers"].as_object().unwrap().len(),7);}
        if mode=="ecm_dtc" {assert_eq!(v["result"]["dtc_report"]["complete"],true);assert_eq!(v["result"]["diagnostic_session_stopped"],true);}
    }
}
#[test] fn rejected_identity_firmware_or_readback_sends_no_vehicle_request() {
    for bad in [("AT","?"),("ATI","garbage"),("ATD","?"),("STDI","OBDLink LX"),("STI","STN2256 v5.13.1"),("STPR","31"),("STPBRR","125000")] {
        let (v,commands)=run("vin",Some(bad));assert_eq!(v["state"],"failed");
        assert!(!commands.iter().any(|c|c.as_str()==VIN_REQUEST));assert!(v["result"].is_null());
    }
}
#[test] fn wake_repeats_only_a_confirmed_benign_probe_without_reboot_or_vehicle_replay() {
    let (v,c)=run("vin",None);
    assert_eq!(v["state"],"complete");
    assert_eq!(&c[..5], &["AT", "", "", "ATI", "ATD"]);
    assert!(!c.iter().any(|c| matches!(c.as_str(),"ATZ"|"ATWS")));
    assert!(!opensaab_vlinker_workflow::vlinker::allowed(""));
    assert!(!opensaab_vlinker_workflow::vlinker::allowed(opensaab_vlinker_workflow::adapters::obdlink_mxp::REPEAT_READY_AT));
    let mut s=Session::startup();
    s.event(&json!({"op":"select_profile","model":"obdlink_mxp"}));
    assert_eq!(s.event(&json!({"op":"start"}))["command"],"AT");
    let repeat=s.event(&json!({"op":"rx","bytes":b"AT\rOK\r>"}));
    assert_eq!(repeat["transport_op"],"repeat_ready_at");
    let failed=s.event(&json!({"op":"rx","bytes":b"AT\r?\r>"}));
    assert_eq!(failed["state"],"failed");
    assert_eq!(s.event(&json!({"op":"tick"})),failed);
}
#[test] fn cancel_or_ambiguous_wake_never_advances_to_repeat_or_vehicle_commands() {
    for raw in [b"AT\rOK\r>garbage".as_slice(),b"AT\rELM327 v1.4b\r>".as_slice(),b"AT\rOK\rOK\r>".as_slice()] {
        let mut s=Session::startup();s.event(&json!({"op":"select_profile","model":"obdlink_mxp"}));s.event(&json!({"op":"start"}));
        assert_eq!(s.event(&json!({"op":"rx","bytes":raw}))["state"],"failed");
    }
    let mut s=Session::startup();s.event(&json!({"op":"select_profile","model":"obdlink_mxp"}));s.event(&json!({"op":"start"}));
    let stopped=s.event(&json!({"op":"cancel"}));
    assert_eq!(stopped["state"],"failed");
    assert_eq!(s.event(&json!({"op":"rx","bytes":b"AT\rOK\r>"})),stopped);
}
#[test] fn incomplete_codes_are_not_published_and_session_is_stopped_once() {
    let (v,c)=run("ecm_dtc",Some((DTC_READ,"5E8 81 01 07 00 6F 00 00 00")));
    assert_eq!(v["state"],"failed");assert!(v["result"].is_null());
    assert_eq!(c.iter().filter(|v|v.as_str()=="0120000000000000").count(),1);
    assert_eq!(c.last().unwrap(),"STPC");
}
