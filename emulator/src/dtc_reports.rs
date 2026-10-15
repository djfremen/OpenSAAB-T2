// SPDX-License-Identifier: MPL-2.0
//! Passive original-firmware report capture, ported from Android DtcReport/View.
//! No adapter, networking, or firmware-key dependencies.
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
static SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
fn unique() -> String {
    format!(
        "{}_{}_{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )
}
const MAX_SCREEN: usize = 8192;
const MAX_REPORT: u64 = 2_000_000;
fn now() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap()
}
fn string<'a>(v: &'a Value, key: &str) -> &'a str {
    v[key].as_str().unwrap_or("")
}
fn counter(line: &str) -> Option<(usize, usize)> {
    let (a, b) = line.trim().split_once('/')?;
    let number = |s: &str| {
        let s = s.trim();
        if s.is_empty() || s.len() > 3 || !s.bytes().all(|b| b.is_ascii_digit()) {
            None
        } else {
            s.parse::<usize>().ok()
        }
    };
    Some((number(a)?, number(b)?))
}
fn module(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 12
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
fn code(s: &str) -> bool {
    s.len() == 5
        && matches!(s.as_bytes()[0], b'B' | b'C' | b'P' | b'U')
        && s.as_bytes()[1..]
            .iter()
            .all(|b| b.is_ascii_digit() || (b'A'..=b'F').contains(b))
}
fn suffix(s: &str) -> bool {
    s.len() == 2
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'A'..=b'F').contains(&b))
}
fn key(r: &Value) -> String {
    format!(
        "{}/{}/{}",
        string(r, "module"),
        string(r, "dtc"),
        string(r, "suffix")
    )
}
pub fn parse(text: &str) -> Option<Value> {
    if text.len() > MAX_SCREEN || text.lines().next()?.trim() != "DTC Information" {
        return None;
    }
    let counters: Vec<_> = text.lines().filter_map(counter).collect();
    if counters.len() != 1 {
        return None;
    }
    let (index, total) = counters[0];
    if index < 1 || index > total || total > 200 {
        return None;
    }
    let mut rows = vec![];
    for line in text.lines() {
        let mut fields = line.split_whitespace();
        if let (Some(m), Some(c), Some(s)) = (fields.next(), fields.next(), fields.next()) {
            let desc = fields.collect::<Vec<_>>().join(" ");
            if module(m) && code(c) && suffix(s) && !desc.is_empty() {
                rows.push(json!({"module":m,"dtc":c,"suffix":s,"description_as_displayed":desc}));
            }
        }
    }
    if rows.is_empty() {
        return None;
    }
    Some(
        json!({"selected_item":index,"total":total,"observed_utc":now(),"screen":text,"visible_rows":rows}),
    )
}
fn entries(report: &Value) -> BTreeMap<String, Value> {
    let mut out: BTreeMap<String, Value> = BTreeMap::new();
    for page in report["snapshots"].as_array().into_iter().flatten() {
        for row in page["visible_rows"].as_array().into_iter().flatten() {
            let k = key(row);
            if out.get(&k).is_none_or(|old| {
                string(row, "description_as_displayed").len()
                    > string(old, "description_as_displayed").len()
            }) {
                out.insert(k, row.clone());
            }
        }
    }
    out
}
fn detail(report: &mut Value, text: &str) -> bool {
    if text.len() > MAX_SCREEN || text.lines().any(|l| counter(l).is_some()) {
        return false;
    }
    let lines: Vec<_> = text
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    if lines.len() < 4 || lines[0] != "DTC Information" || !module(lines[1]) {
        return false;
    }
    let c: Vec<_> = lines[2].split_whitespace().collect();
    if c.len() != 2 || !code(c[0]) || !suffix(c[1]) {
        return false;
    }
    let k = format!("{}/{}/{}", lines[1], c[0], c[1]);
    if !entries(report).contains_key(&k) {
        return false;
    }
    report["full_descriptions"][k] = json!(lines[3..].join(" "));
    true
}
fn update(report: &mut Value) {
    report["captured_positions"] = json!(report["snapshots"].as_array().unwrap().len());
    report["all_list_positions_captured"] =
        json!(report["captured_positions"] == report["total_positions"]);
    report["unique_codes"] = json!(entries(report).len());
}
fn summary(report: &Value) -> String {
    format!("{} distinct fault codes captured.\n{}\nThis is coverage of the displayed list, not a count of vehicle modules scanned.",report["unique_codes"],if report["all_list_positions_captured"]==true {format!("All {} entries from the displayed list were captured.",report["total_positions"])} else {"Partial list — scroll through the remaining codes to finish capturing results.".into()})
}
pub fn render(report: &Value) -> String {
    let mut out = format!(
        "OpenSAAB DTC report\n{}\n{} · {} · Original Tech2 firmware\nDate (UTC): {}\nVIN: {}\n",
        summary(report),
        string(report, "platform"),
        string(report, "adapter"),
        string(report, "created_utc"),
        report["vin"].as_str().unwrap_or("not captured")
    );
    if let Some(v) = report["vehicle_description"].as_str() {
        out += &format!(
            "Vehicle: {v}\nVIN read at (UTC): {}\n",
            string(report, "vin_observed_utc")
        );
    }
    let mut last = String::new();
    for (k, r) in entries(report) {
        let m = string(&r, "module");
        if m != last {
            out += &format!("\n{m}\n");
            last = m.into();
        }
        let repeated = report["snapshots"].as_array().unwrap().iter().any(|p| {
            p["visible_rows"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|r| key(r) == k)
                .count()
                > 1
        });
        let desc = report["full_descriptions"][&k]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| format!("{}…", string(&r, "description_as_displayed")));
        out += &format!(
            "  {}-{} — {}{}\n",
            string(&r, "dtc"),
            string(&r, "suffix"),
            desc,
            if repeated {
                " [repeated in firmware list]"
            } else {
                ""
            }
        );
    }
    out+"\nCodes are grouped by module. A repeated code is listed once; different symptom suffixes stay separate.\n… means only list text was captured; open that code in the firmware for its full description.\nCurrent versus historical fault status is not provided by these screens.\nReport collection does not clear codes. Raw screen observations remain in the saved JSON log.\n\n---\nSupport OpenSAAB\nIf OpenSAAB helped you, please consider supporting the project. Your donation helps fund development and adapter testing.\nhttps://ko-fi.com/djfremen\nDonations are always optional. Thank you for your support!\n"
}
fn read(path: &Path, max: u64) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let f = fs::File::open(path).map_err(|_| "Report unavailable")?;
    let mut b = vec![];
    f.take(max + 1)
        .read_to_end(&mut b)
        .map_err(|_| "Cannot read report")?;
    if b.len() as u64 > max {
        return Err("Report too large".into());
    }
    Ok(b)
}
fn load(path: &Path) -> Result<Value, String> {
    serde_json::from_slice(&read(path, MAX_REPORT)?).map_err(|_| "Invalid report JSON".into())
}
fn atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    let tmp = path.with_extension("tmp");
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut f = options.open(&tmp).map_err(|_| "Cannot save report")?;
    f.write_all(bytes)
        .and_then(|_| f.sync_all())
        .map_err(|_| "Cannot save report")?;
    fs::rename(tmp, path).map_err(|_| "Cannot finish saving report".into())
}
fn report_path(dir: &Path, id: &str, ext: &str) -> Result<PathBuf, String> {
    if !(id.starts_with("macos_dtc_") || opensaab_vlinker_workflow::reports::valid_id(id))
        || id.len() > 100
        || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return Err("Invalid report selection".into());
    }
    Ok(dir.join(format!("{id}.{ext}")))
}
fn observe(root: &Path, run: &Path) -> Result<Value, String> {
    let runs = root.join("runs").canonicalize().map_err(|_| "No session")?;
    let run = run.canonicalize().map_err(|_| "No session")?;
    if run.parent() != Some(runs.as_path()) {
        return Err("Invalid session location".into());
    }
    let text = String::from_utf8(read(&run.join("screen.txt"), MAX_SCREEN as u64)?)
        .map_err(|_| "Invalid screen text")?;
    let cursor = run.join("dtc-capture-state.json");
    let mut state = if cursor.exists() {
        load(&cursor)?
    } else {
        json!({})
    };
    let dir = root.join("dtc-reports");
    fs::create_dir_all(&dir).map_err(|_| "Cannot create report folder")?;
    let mut report = if let Some(id) = state["id"].as_str() {
        load(&report_path(&dir, id, "json")?)?
    } else {
        Value::Null
    };
    if let Some(mut page) = parse(&text) {
        let total = page["total"].take();
        page.as_object_mut().unwrap().remove("total");
        if report.is_null() || report["total_positions"] != total {
            let id = format!("macos_dtc_{}", unique());
            state = json!({"id":id});
            let identity = load(&run.join("dtc-context.json")).unwrap_or(json!({}));
            // Identity is a session-local snapshot, never the last global VIN.
            let vin = identity["vin"].as_str().filter(|v| {
                v.len() == 17
                    && v.bytes().all(|b| {
                        b.is_ascii_digit()
                            || (b'A'..=b'Z').contains(&b) && !matches!(b, b'I' | b'O' | b'Q')
                    })
            });
            report = json!({"schema":1,"source":"original-firmware-lcd","platform":"macOS","adapter":identity["adapter"].as_str().unwrap_or("unknown"),"session":run.file_name().unwrap().to_string_lossy(),"created_utc":now(),"vin":vin,"vehicle_description":if vin.is_some(){identity["vehicle_description"].clone()}else{Value::Null},"vin_observed_utc":if vin.is_some(){identity["observed_utc"].clone()}else{Value::Null},"total_positions":total,"full_descriptions":{},"snapshots":[]});
        }
        let pages = report["snapshots"].as_array_mut().unwrap();
        if let Some(old) = pages
            .iter_mut()
            .find(|p| p["selected_item"] == page["selected_item"])
        {
            if old["screen"] == page["screen"] {
                return Ok(json!({"changed":false}));
            }
            *old = page;
        } else {
            pages.push(page)
        }
        pages.sort_by_key(|p| p["selected_item"].as_u64().unwrap_or(0));
    } else if report.is_null() || !detail(&mut report, &text) {
        atomic(&cursor, b"{}")?;
        return Ok(json!({"changed":false}));
    }
    update(&mut report);
    let id = string(&state, "id");
    atomic(
        &report_path(&dir, id, "json")?,
        &serde_json::to_vec(&report).unwrap(),
    )?;
    atomic(&report_path(&dir, id, "txt")?, render(&report).as_bytes())?;
    atomic(&cursor, &serde_json::to_vec(&state).unwrap())?;
    Ok(
        json!({"changed":true,"id":id,"codes":report["unique_codes"],"partial":report["all_list_positions_captured"]!=true,"summary":summary(&report)}),
    )
}
pub fn dispatch(v: &Value) -> Result<Value, String> {
    if v["schema"] != 1 {
        return Err("Unsupported report schema".into());
    }
    let root = Path::new(v["root"].as_str().ok_or("Missing report root")?);
    let dir = root.join("dtc-reports");
    match string(v, "op") {
        "observe" => observe(root, Path::new(v["run"].as_str().ok_or("Missing session")?)),
        "list" => {
            if !dir.exists() {
                return Ok(json!({"reports":[]}));
            }
            let mut files: Vec<_> = fs::read_dir(&dir)
                .map_err(|_| "Cannot list reports")?
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == "txt"))
                .collect();
            files.sort_by_key(|p| {
                std::cmp::Reverse(fs::metadata(p).and_then(|m| m.modified()).ok())
            });
            let rows: Vec<_> = files
                .into_iter()
                .take(100)
                .filter_map(|p| {
                    let id = p.file_stem()?.to_str()?;
                    report_path(&dir, id, "txt").ok()?;
                    if opensaab_vlinker_workflow::reports::valid_id(id) {
                        let meta = opensaab_vlinker_workflow::reports::dispatch(&json!({"schema":1,"op":"read","root":root,"id":id})).ok()?;
                        Some(json!({"id":id,"title":meta["title"]}))
                    } else { Some(json!({"id":id})) }
                })
                .collect();
            Ok(json!({"reports":rows}))
        }
        "read" => {
            if opensaab_vlinker_workflow::reports::valid_id(string(v,"id")) { return opensaab_vlinker_workflow::reports::dispatch(v); }
            let p = report_path(&dir, string(v, "id"), "txt")?;
            let text =
                String::from_utf8(read(&p, MAX_REPORT)?).map_err(|_| "Invalid report text")?;
            Ok(json!({"text":text,"id":v["id"]}))
        }
        _ => Err("Unknown report operation".into()),
    }
}
#[cfg(test)]
mod tests;
