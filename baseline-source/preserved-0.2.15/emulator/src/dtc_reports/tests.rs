use super::*;
fn screen(i: usize, n: usize) -> String {
    format!(" DTC Information \nACC B0260 06 Vent Door Motor Left\nICM2 B1000 08 Control Module. Inte\nICM2 B1000 08 Control Module. Inte\n{i} / {n}\n")
}
fn fixture() -> (PathBuf, PathBuf) {
    let root = std::env::temp_dir().join(format!("dtc-test-{}", unique()));
    let run = root.join("runs/test");
    fs::create_dir_all(&run).unwrap();
    (root, run)
}
fn capture(root: &Path, run: &Path, s: &str) -> Value {
    fs::write(run.join("screen.txt"), s).unwrap();
    observe(root, run).unwrap()
}
#[test]
fn rejects_non_results() {
    for s in [
        "DTC Information\nACC".into(),
        "Main Menu\nF0 Read DTC Information\n1 / 2".into(),
        screen(0, 3),
        screen(4, 3),
        screen(1, 201),
        screen(1, 3) + "2 / 3\n",
        "DTC Information\nNo DTCs\n1 / 1".into(),
        "x".repeat(8193),
    ] {
        assert!(parse(&s).is_none(), "{s}");
    }
}
#[test]
fn android_grouping_coverage_details_and_durable_reopen() {
    let (root, run) = fixture();
    let first = capture(&root, &run, &screen(2, 3));
    let id = string(&first, "id");
    assert_eq!(first["codes"], 2);
    assert_eq!(first["partial"], true);
    assert_eq!(capture(&root, &run, &screen(2, 3))["changed"], false);
    capture(&root, &run, &screen(1, 3));
    capture(&root, &run, &screen(3, 3));
    let p = report_path(&root.join("dtc-reports"), id, "json").unwrap();
    let mut r = load(&p).unwrap();
    assert_eq!(r["all_list_positions_captured"], true);
    assert!(r["vin"].is_null());
    assert_eq!(
        r["snapshots"][0]["visible_rows"].as_array().unwrap().len(),
        3
    );
    assert!(!detail(
        &mut r,
        "DTC Information\nUEC\nB2699 04\nLow Beam Right Circuit, Open\n"
    ));
    let d = "DTC Information\nICM2\nB1000 08\nControl Module.\nInternal fault\n";
    capture(&root, &run, d);
    let r = load(&p).unwrap();
    let text = render(&r);
    assert_eq!(text.matches("B1000-08").count(), 1);
    assert!(text.contains("Control Module. Internal fault [repeated"));
    assert!(text.contains("not a count of vehicle modules scanned"));
    assert!(text.contains("Vent Door Motor Left…"));
    let listed = dispatch(&json!({"schema":1,"op":"list","root":root})).unwrap();
    assert_eq!(listed["reports"].as_array().unwrap().len(), 1);
    assert_eq!(
        dispatch(&json!({"schema":1,"op":"read","root":root,"id":id})).unwrap()["text"],
        text
    );
    // A menu ends the list; revisiting the same count creates a separate report.
    capture(&root, &run, "Main Menu");
    let next = capture(&root, &run, &screen(1, 3));
    assert_ne!(next["id"], id);
    assert!(p.exists());
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn different_symptoms_and_totals_do_not_merge() {
    let (root, run) = fixture();
    let a=capture(&root,&run,"DTC Information\nUEC B1000 08 Internal\nUEC B1000 04 Other\nACC B1000 08 Different module\n1 / 3\n");
    assert_eq!(a["codes"], 3);
    let b = capture(&root, &run, &screen(1, 4));
    assert_ne!(a["id"], b["id"]);
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn session_identity_and_paths_fail_closed() {
    let (root, run) = fixture();
    fs::write(
        root.join("vehicle-status.json"),
        r#"{"vin":"YS3FD49YX41000001"}"#,
    )
    .unwrap();
    let a = capture(&root, &run, &screen(1, 1));
    let p = report_path(&root.join("dtc-reports"), string(&a, "id"), "json").unwrap();
    assert!(load(&p).unwrap()["vin"].is_null());
    capture(&root, &run, "Main Menu");
    fs::write(run.join("dtc-context.json"),r#"{"vin":"YS3FD49YX41000001","adapter":"synthetic","observed_utc":"2026-09-24T00:00:00Z"}"#).unwrap();
    let a = capture(&root, &run, &screen(1, 1));
    assert_eq!(
        load(&report_path(&root.join("dtc-reports"), string(&a, "id"), "json").unwrap()).unwrap()
            ["vin"],
        "YS3FD49YX41000001"
    );
    assert!(report_path(&root, "../../outside", "txt").is_err());
    assert!(observe(&root, &root).is_err());
    fs::remove_dir_all(root).unwrap();
}
