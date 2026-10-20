// SPDX-License-Identifier: MPL-2.0
//! Android security workflow rules, independent of OS UI, HTTP and adapter ownership.
//! Reference: SsaData, SecurityApiClient, SsaCardImport and SecurityAccessStatus.
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::Path,
};
pub mod navigation;
mod platform;
mod prompt;
mod status;
pub const SIZE: usize = 714;
pub const OFFSET: usize = 0xfe0000;
pub const CARD_SIZE: usize = 32 * 1024 * 1024;
pub const PRIMARY: &str = "https://relevant-diann-djfremen2-c013cdc3.koyeb.app/api/process";
pub const FALLBACK: &str = "https://sas.mysaab.info/api/process";
pub type Result<T> = std::result::Result<T, String>;
fn need(ok: bool, msg: &str) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(msg.into())
    }
}
pub fn hash(b: &[u8]) -> String {
    format!("{:x}", Sha256::digest(b))
}
fn utc() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap()
}
pub fn valid_vin(v: &str) -> bool {
    v.len() == 17
        && v.bytes()
            .all(|c| b"ABCDEFGHJKLMNPRSTUVWXYZ0123456789".contains(&c))
}
pub fn vin(b: &[u8]) -> Result<&str> {
    need(b.len() == SIZE, "Expected 714-byte SSA data")?;
    let v = std::str::from_utf8(&b[20..37]).map_err(|_| "Invalid SSA VIN")?;
    need(valid_vin(v), "Invalid SSA VIN")?;
    Ok(v)
}
fn word(b: &[u8], p: usize) -> u16 {
    u16::from_be_bytes([b[p], b[p + 1]])
}
fn records(b: &[u8]) -> Result<Vec<usize>> {
    need(b.len() == SIZE, "Invalid SSA size")?;
    let mut out = vec![];
    for p in (0x132..=SIZE - 8).step_by(8) {
        if b[p..p + 8].iter().all(|x| *x == 255) {
            continue;
        }
        need(
            word(b, p) != 65535 && word(b, p + 2) >> 8 == 3 && word(b, p + 4) != 65535,
            "Invalid SSA seed record",
        )?;
        out.push(p);
    }
    Ok(out)
}
pub fn validate_input(b: &[u8]) -> Result<usize> {
    vin(b)?;
    need(b[0] == 0xb1, "Invalid SSA header")?;
    need(
        b[0x26..0x2e].iter().all(|x| *x == 255),
        "Collect fresh security data first",
    )?;
    let rows = records(b)?;
    need(!rows.is_empty(), "No collected seeds in SSA")?;
    for p in &rows {
        need(word(b, p + 6) == 65535, "SSA already contains keys")?;
    }
    Ok(rows.len())
}
pub fn validate_reply(before: &[u8], after: &[u8]) -> Result<()> {
    validate_input(before)?;
    need(
        after.len() == SIZE && after[0] == 0xb1 && after[1] == 0,
        "Invalid processed SSA header/size",
    )?;
    need(vin(before)? == vin(after)?, "API changed VIN")?;
    need(
        after[0x26..0x2e].iter().all(|c| (32..=126).contains(c)),
        "API security code incomplete",
    )?;
    let rows = records(before)?;
    need(rows == records(after)?, "API changed seed record positions")?;
    let mut allowed = [false; SIZE];
    allowed[..0x14].fill(true);
    allowed[0x25] = true;
    allowed[0x26..0x2e].fill(true);
    for p in rows {
        need(
            before[p..p + 6] == after[p..p + 6],
            "API changed a seed or algorithm",
        )?;
        need(word(after, p + 6) != 65535, "API returned an unfilled key")?;
        allowed[p + 6..p + 8].fill(true);
    }
    need(
        (0..SIZE).all(|i| before[i] == after[i] || allowed[i]),
        "API changed unrelated SSA data",
    )
}
pub fn observe(text: &str) -> Value {
    let s = text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    let transfer = s.contains("you need security access from tis2000")
        && s.contains("disconnect tech 2 from vehicle");
    json!({"collecting":s.contains("checking security access") && s.contains("reading all vehicle vins") && s.contains("reading all vehicle seed"),
        "transfer":transfer, "match":if transfer {json!("english")} else {Value::Null}})
}
// Localized cards show the transfer instructions from their own message
// database. The English wording remains the first check; otherwise the screen
// is matched against the prompt extracted from the session's card.
fn observe_prompts(screen: &[u8], prompts: impl FnOnce() -> Vec<prompt::TransferPrompt>) -> Value {
    let mut observed = observe(&String::from_utf8_lossy(screen));
    // Every card prompt title names TIS2000 in ASCII; other screens never read the card.
    if observed["transfer"] != true && prompt::mentions_anchor(screen) {
        let prompts = prompts();
        let card = prompt::matches_any(&prompts, screen);
        observed["transfer"] = json!(card);
        observed["match"] = if card { json!("card") } else { Value::Null };
        observed["card_prompts"] = json!(prompts.len());
    }
    observed
}
fn observe_card(screen: &[u8], card: &[u8]) -> Value {
    observe_prompts(screen, || prompt::extract(card))
}
fn observe_run(run: &Path, screen: &[u8], cache: prompt::Cache) -> Value {
    observe_prompts(screen, || prompt::for_run(run, cache))
}
// OS transport maps only documented network-outage categories. TLS and malformed
// successful replies never trigger a second disclosure to another provider.
fn http_decision(
    status: u64,
    network: &str,
    primary: bool,
    consent: bool,
    cancelled: bool,
) -> Value {
    if cancelled {
        return json!({"action":"error","message":"Security processing cancelled; no data imported."});
    }
    if status == 200 && network.is_empty() {
        return json!({"action":"validate"});
    }
    if primary && status == 401 {
        return json!({"action":"unauthorized","message":"The security service rejected this request. Update OpenSAAB or report the problem."});
    }
    let outage = if network.is_empty() {
        matches!(status, 500 | 502 | 503 | 504)
    } else {
        matches!(network, "timeout" | "connect" | "dns" | "route")
    };
    if primary && consent && outage {
        json!({"action":"fallback","endpoint":FALLBACK})
    } else {
        json!({"action":"error","message":if primary&&outage&&!consent {"OpenSAAB unavailable. No data sent to Bojer. Retry with fallback if desired.".to_owned()} else {format!("Security request failed (HTTP {status}, transport {}). No fallback or import.",if network.is_empty(){"none"}else{network})}})
    }
}
fn read(path: &Path, max: usize) -> Result<Vec<u8>> {
    let meta = fs::symlink_metadata(path).map_err(|_| "Required security file is missing")?;
    need(
        meta.file_type().is_file() && meta.len() <= max as u64,
        "Unexpected security file type/size",
    )?;
    let mut data = vec![];
    fs::File::open(path)
        .map_err(|_| "Cannot open security file")?
        .take(max as u64 + 1)
        .read_to_end(&mut data)
        .map_err(|_| "Cannot read security file")?;
    need(data.len() <= max, "Unexpected security file size")?;
    Ok(data)
}
fn read_json(p: &Path) -> Result<Value> {
    serde_json::from_slice(&read(p, 1024 * 1024)?).map_err(|_| "Invalid security metadata".into())
}
fn private_write(path: &Path, data: &[u8]) -> Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut f = options
        .open(path)
        .map_err(|_| "Cannot create private security evidence")?;
    f.write_all(data)
        .and_then(|_| f.sync_all())
        .map_err(|_| "Cannot save security evidence".into())
}
fn write_json(path: &Path, v: &Value) -> Result<()> {
    private_write(path, &serde_json::to_vec_pretty(v).unwrap())
}
fn private_dir(p: &Path) -> Result<()> {
    fs::create_dir(p).map_err(|_| "Cannot create private security workspace")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(p, fs::Permissions::from_mode(0o700))
            .map_err(|_| "Cannot protect workspace")?;
    }
    Ok(())
}
fn unique() -> String {
    static SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    format!(
        "{}-{}-{}",
        std::process::id(),
        time::OffsetDateTime::now_utc().unix_timestamp_nanos(),
        SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )
}

fn text<'a>(v: &'a Value, k: &str) -> Result<&'a str> {
    v[k].as_str().ok_or_else(|| format!("Missing field: {k}"))
}
fn context(state: &Path, run: &Path, firmware: &str) -> Result<Value> {
    let run = fs::canonicalize(run).map_err(|_| "Session is missing")?;
    let runs = fs::canonicalize(state.join("runs")).map_err(|_| "Sessions are missing")?;
    need(
        run.parent() == Some(runs.as_path()),
        "Invalid session location",
    )?;
    let latest = read_json(&state.join("latest-session.json"))?;
    need(
        latest["firmware"] == firmware
            && latest["run"]
                .as_str()
                .and_then(|p| fs::canonicalize(p).ok())
                .as_deref()
                == Some(run.as_path()),
        "Session or firmware changed; collect again",
    )?;
    let active = read(&state.join("firmware/active.txt"), 256)?;
    need(
        String::from_utf8_lossy(&active).trim() == firmware,
        "Selected firmware changed; collect again",
    )?;
    Ok(latest)
}
fn finished(run: &Path, latest: &Value) -> Result<()> {
    if !latest["pid"].is_null() {
        need(latest["pid"].as_u64().is_some_and(|p| p>0 && p<=i32::MAX as u64), "Invalid session process")?;
    }
    need(
        run.join("report.json").is_file(),
        "Stop firmware and release USB before processing",
    )?;
    #[cfg(windows)]
    if let Some(pid) = latest["pid"].as_u64() {
        need(pid > 0 && pid <= u32::MAX as u64, "Invalid session process")?;
        platform::require_stopped(pid as u32)?;
    }
    #[cfg(not(any(unix, windows)))]
    need(latest["pid"].is_null(), "Unsupported process binding")?;
    #[cfg(unix)]
    if let Some(pid) = latest["pid"].as_i64() {
        need(pid > 0 && pid <= i32::MAX as i64, "Invalid session process")?;
        // A process with this PID (including reuse) fails closed.
        need(
            unsafe { libc::kill(pid as i32, 0) } != 0
                && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH),
            "Firmware is still running",
        )?;
    }
    Ok(())
}
fn cancelled(e: &Path) -> Result<()> {
    need(
        !e.join("cancel").exists(),
        "Security processing cancelled; no data imported",
    )
}
// Reuse only the currently selected working data. History supplies provenance,
// never substitutes a different seed file merely because its VIN matches.
fn working_input(run: &Path) -> Result<Vec<u8>> {
    let working = run.join("working-ssa.bin");
    if working.exists() {
        return read(&working, SIZE);
    }
    let snapshot = run.join("native-security-snapshot.json");
    if snapshot.exists() {
        let meta = read_json(&snapshot)?;
        need(
            meta["origin"] == "original-guest-memory"
                && meta["bytes"] == SIZE
                && meta["card_offset"] == OFFSET,
            "Invalid guest security snapshot",
        )?;
        return read(&run.join("ssa-card-after.bin"), SIZE);
    }
    let card = read(&run.join("card.bin"), CARD_SIZE)?;
    need(card.len() == CARD_SIZE, "Invalid working card size")?;
    Ok(card[OFFSET..OFFSET + SIZE].to_vec())
}
fn collected_source(run: &Path, input: &[u8], card: &[u8]) -> Result<()> {
    let snapshot = read_json(&run.join("native-security-snapshot.json"))?;
    need(
        snapshot["origin"] == "original-guest-memory"
            && snapshot["card_offset"] == OFFSET
            && snapshot["bytes"] == SIZE
            && snapshot["ssa_memory_flash_enabled"] == true
            && snapshot["ssa_erases"].as_u64().unwrap_or(0) >= 1
            && snapshot["ssa_programmed_bytes"].as_u64().unwrap_or(0) >= SIZE as u64,
        "No completed collection evidence",
    )?;
    let before = read(&run.join("ssa-card-before.bin"), SIZE)?;
    need(
        before.len() == SIZE
            && before != input
            && read(&run.join("ssa-card-after.bin"), SIZE)? == input,
        "Collected data changed",
    )?;
    let screen = read(&run.join("screen.txt"), 8192)?;
    // English needs no card; a localized prompt is derived from the
    // candidate's own card, which must equal the selected firmware.
    let original = read(&run.join("card.bin"), CARD_SIZE)?;
    need(
        original.len() == CARD_SIZE
            && original[OFFSET..OFFSET + SIZE] == before
            && original[..OFFSET] == card[..OFFSET]
            && original[OFFSET + SIZE..] == card[OFFSET + SIZE..],
        "Collection belongs to different firmware",
    )?;
    need(
        observe_card(&screen, &original)["transfer"] == true,
        "Collection has not reached the transfer prompt",
    )
}
pub fn inspect_existing(state: &Path, run: &Path, firmware: &str) -> Result<Value> {
    context(state, run, firmware)?;
    let input = working_input(run)?;
    let count = validate_input(&input)?;
    let observation = read_json(&state.join("vehicle-status.json"))?;
    need(
        observation["vin"].as_str() == Some(vin(&input)?),
        "Existing security data belongs to another vehicle",
    )?;
    let card = read(&run.join("card.bin"), CARD_SIZE)?;
    need(card.len() == CARD_SIZE, "Invalid working card size")?;
    let runs = fs::canonicalize(state.join("runs")).map_err(|_| "Missing sessions")?;
    // Current live collection is finalized by stop() before prepare_existing.
    let current_prompt = read(&run.join("screen.txt"), 8192)
        .ok()
        .map(|b| observe_card(&b, &card)["transfer"] == true)
        .unwrap_or(false);
    if current_prompt && !run.join("report.json").exists() {
        return Ok(
            json!({"available":true,"input_hash":hash(&input),"records":count,"source_session":run,"collection_pending_stop":true}),
        );
    }
    let mut candidates = vec![run.to_path_buf()];
    // New sessions preserve a direct lineage. Legacy 0.2.13–15 sessions are
    // recovered by exact seed bytes AND unchanged firmware, not VIN alone.
    if let Ok(origin) = read_json(&run.join("security-collection-origin.json")) {
        if let Some(path) = origin["source_session"].as_str() {
            candidates.push(Path::new(path).to_path_buf());
        }
    }
    let mut history = fs::read_dir(&runs)
        .map_err(|_| "Cannot inspect collection history")?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .map(|e| (e.metadata().and_then(|m| m.modified()).ok(), e.path()))
        .collect::<Vec<_>>();
    history.sort_by(|a, b| b.0.cmp(&a.0));
    candidates.extend(history.into_iter().take(256).map(|(_, p)| p));
    for candidate in candidates {
        let Ok(source) = fs::canonicalize(&candidate) else {
            continue;
        };
        if source.parent() != Some(runs.as_path()) {
            continue;
        }
        if collected_source(&source, &input, &card).is_ok() {
            return Ok(
                json!({"available":true,"input_hash":hash(&input),"records":count,"source_session":source,"collection_pending_stop":false}),
            );
        }
    }
    Err("Existing data has no matching completed collection evidence. Collect fresh data.".into())
}
fn prepare_existing(
    state: &Path,
    run: &Path,
    firmware: &str,
    allow_fallback: bool,
    expected_hash: &str,
) -> Result<Value> {
    let latest = context(state, run, firmware)?;
    finished(run, &latest)?;
    let selected = inspect_existing(state, run, firmware)?;
    need(
        selected["collection_pending_stop"] == false && selected["input_hash"] == expected_hash,
        "Selected security data changed; review it again",
    )?;
    let input = working_input(run)?;
    let card = read(&run.join("card.bin"), CARD_SIZE)?;
    let observation = read_json(&state.join("vehicle-status.json"))?;
    create_attempt(
        state,
        run,
        firmware,
        allow_fallback,
        &latest,
        &input,
        &card,
        &observation,
        Some(&selected),
    )
}
fn create_attempt(
    state: &Path,
    run: &Path,
    firmware: &str,
    allow_fallback: bool,
    latest: &Value,
    input: &[u8],
    card: &[u8],
    observation: &Value,
    source: Option<&Value>,
) -> Result<Value> {
    let evidence = run.join(format!("security-api-{}", unique()));
    private_dir(&evidence)?;
    private_write(&evidence.join("pre-auth.bin"), input)?;
    let request = json!({"REQUEST_VERSION":1,"SSA_DATA":STANDARD.encode(input)});
    write_json(&evidence.join("request.json"), &request)?;
    let transaction = json!({"schema":1,"state":state,"run":run,"firmware":firmware,"latest":latest,"card_hash":hash(card),"input_hash":hash(input),"vehicle_hash":hash(vin(input)?.as_bytes()),"observation_hash":hash(&serde_json::to_vec(observation).unwrap()),"started_utc":utc(),"allow_fallback":allow_fallback,"existing_collection":source});
    write_json(&evidence.join("transaction.json"), &transaction)?;
    Ok(
        json!({"attempt":evidence,"payload":request,"endpoint":PRIMARY,"records":validate_input(input)?}),
    )
}
fn prepare(state: &Path, run: &Path, firmware: &str, allow_fallback: bool) -> Result<Value> {
    let latest = context(state, run, firmware)?;
    finished(run, &latest)?;
    let snapshot = read_json(&run.join("native-security-snapshot.json"))?;
    need(
        snapshot["origin"] == "original-guest-memory"
            && snapshot["card_offset"] == OFFSET
            && snapshot["bytes"] == SIZE
            && snapshot["ssa_memory_flash_enabled"] == true
            && snapshot["ssa_erases"].as_u64().unwrap_or(0) >= 1
            && snapshot["ssa_programmed_bytes"].as_u64().unwrap_or(0) >= SIZE as u64,
        "Firmware has not written fresh SSA; collect again and follow all key prompts",
    )?;
    let screen = read(&run.join("screen.txt"), 8192)?;
    let card = read(&run.join("card.bin"), CARD_SIZE)?;
    need(
        observe_card(&screen, &card)["transfer"] == true,
        "Finish collection and reach the TIS transfer prompt first",
    )?;
    let before = read(&run.join("ssa-card-before.bin"), SIZE)?;
    let input = read(&run.join("ssa-card-after.bin"), SIZE)?;
    need(
        before.len() == SIZE && before != input,
        "SSA unchanged; fresh collection required",
    )?;
    let count = validate_input(&input)?;
    let observation = read_json(&state.join("vehicle-status.json"))?;
    need(
        observation["vin"].as_str() == Some(vin(&input)?),
        "Collected data belongs to another vehicle",
    )?;
    need(
        card.len() == CARD_SIZE && card[OFFSET..OFFSET + SIZE] == before,
        "Working card changed; collect again",
    )?;
    let _ = count;
    create_attempt(
        state,
        run,
        firmware,
        allow_fallback,
        &latest,
        &input,
        &card,
        &observation,
        None,
    )
}
// Commit to a NEW immutable-evidence session and then atomically advance latest.
// Until the final pointer swap, cancellation/failure leaves the previous state intact.
fn import(e: &Path, response: &Value, provider: &str) -> Result<Value> {
    // The desktop shell owns its normal single-instance lease. This additional
    // lease prevents two API workers activating competing replies.
    let lock_path = e
        .parent()
        .and_then(Path::parent)
        .ok_or("Invalid attempt location")?
        .join(".security-import.lock");
    let mut opts = OpenOptions::new();
    opts.create(true).read(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    let lock = opts
        .open(lock_path)
        .map_err(|_| "Cannot lock security workspace")?;
    #[cfg(unix)]
    need(
        unsafe {
            libc::flock(
                std::os::fd::AsRawFd::as_raw_fd(&lock),
                libc::LOCK_EX | libc::LOCK_NB,
            )
        } == 0,
        "Another security import is running",
    )?;
    #[cfg(windows)]
    lock.try_lock().map_err(|_| "Another security import is running")?;
    #[cfg(not(any(unix, windows)))]
    return Err("Security import requires a qualified platform process/locking binding".into());

    let t = read_json(&e.join("transaction.json"))?;
    let state = Path::new(text(&t, "state")?);
    let run = Path::new(text(&t, "run")?);
    let firmware = text(&t, "firmware")?;
    let check = || -> Result<()> {
        cancelled(e)?;
        let latest = context(state, run, firmware)?;
        need(latest == t["latest"], "Session changed; import cancelled")?;
        finished(run, &latest)?;
        if !t["existing_collection"].is_null() {
            need(
                hash(&working_input(run)?) == text(&t, "input_hash")?,
                "Selected working security data changed; import cancelled",
            )?;
        }
        let observation = read_json(&state.join("vehicle-status.json"))?;
        need(
            hash(&serde_json::to_vec(&observation).unwrap()) == text(&t, "observation_hash")?,
            "Vehicle changed; import cancelled",
        )?;
        need(
            hash(&read(&run.join("card.bin"), CARD_SIZE)?) == text(&t, "card_hash")?,
            "Card changed; import cancelled",
        )
    };
    check()?;
    need(
        provider == PRIMARY || (provider == FALLBACK && t["allow_fallback"] == true),
        "Provider not authorized for this attempt",
    )?;
    let input = read(&e.join("pre-auth.bin"), SIZE)?;
    need(
        hash(&input) == text(&t, "input_hash")?,
        "Collection changed; import cancelled",
    )?;
    let after = STANDARD
        .decode(text(response, "SSA_DATA")?)
        .map_err(|_| "Invalid processed SSA encoding")?;
    validate_reply(&input, &after)?;
    write_json(&e.join("response.json"), response)?;
    private_write(&e.join("post-auth.bin"), &after)?;
    let card = read(&run.join("card.bin"), CARD_SIZE)?;
    private_write(&e.join("card-before-security.bin"), &card)?;
    let next = state
        .join("runs")
        .join(format!("security-import-{}", unique()));
    private_dir(&next)?;
    let mut patched = card.clone();
    patched[OFFSET..OFFSET + SIZE].copy_from_slice(&after);
    private_write(&next.join("card.bin"), &patched)?;
    let verified = read(&next.join("card.bin"), CARD_SIZE)?;
    need(
        verified == patched
            && verified[..OFFSET] == card[..OFFSET]
            && verified[OFFSET + SIZE..] == card[OFFSET + SIZE..],
        "Patched card verification failed",
    )?;
    private_write(&next.join("working-ssa.bin"), &after)?;
    let now = utc();
    let provider_label = if provider == PRIMARY {
        "OpenSAAB"
    } else {
        "Bojer"
    };
    let mut receipt=format!("schema=1\nvehicle_hash={}\nsource_session={}\nstarted_utc={}\npre_auth_utc={}\nprocessed_utc={}\nimported_utc={}\nprovider={}\nstage=imported\nssa_hash={}\n",text(&t,"vehicle_hash")?,run.file_name().unwrap().to_string_lossy(),text(&t,"started_utc")?,text(&t,"started_utc")?,now,now,provider_label,hash(&after));
    let rid = response["request_id"].as_str().unwrap_or("");
    if rid.len() == 38
        && rid.starts_with("OSSEC-")
        && rid[6..]
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    {
        receipt += &format!("request_id={rid}\n");
    }
    private_write(
        &next.join("security-processing-status.properties"),
        receipt.as_bytes(),
    )?;
    let result = json!({"status":"security_data_imported","run":next,"provider":provider_label,"imported_utc":now,"vehicle_access_verified":false,"original_card_sha256":hash(&card),"working_card_sha256":hash(&patched),"ssa_output_sha256":hash(&after)});
    write_json(
        &e.join("prepared-result.json"),
        &json!({"status":"prepared_not_active","result":result}),
    )?;
    let pointer =
        json!({"run":next,"firmware":firmware,"mode":"security-import","source_session":run});
    let temp = state.join(format!(".security-latest-{}", unique()));
    write_json(&temp, &pointer)?;
    if let Err(err) = check() {
        let _ = fs::remove_file(&temp);
        return Err(err);
    }
    fs::rename(&temp, state.join("latest-session.json"))
        .map_err(|_| "Could not activate imported card")?;
    // Activation is the commit point. Do not report failure after it succeeded.
    let _ = write_json(&e.join("result.json"), &result);
    Ok(result)
}
// Lifecycle restoration into a new disposable card, using the current matching data only.
pub fn restore(state: &Path, firmware: &str, destination: &Path) -> Result<Value> {
    if !state.join("latest-session.json").exists() { return Ok(json!({"restored":false})); }
    let latest = read_json(&state.join("latest-session.json"))?;
    if latest["firmware"] != firmware { return Ok(json!({"restored":false})); }
    let source = Path::new(text(&latest,"run")?);
    context(state, source, firmware)?;
    let input = working_input(source)?;
    let observation = read_json(&state.join("vehicle-status.json"))?;
    if vin(&input).ok() != observation["vin"].as_str() || vin(&input).is_err() { return Ok(json!({"restored":false})); }
    if validate_input(&input).is_err() && !(input.len()==SIZE && input[0]==0xb1 && input[1]==0 && input[0x26..0x2e].iter().all(|b| (32..=126).contains(b)) && records(&input).is_ok()) { return Ok(json!({"restored":false})); }
    if source.join("report.json").exists() { finished(source, &latest)?; }
    else { need(latest["mode"] == "security-import" && latest["pid"].is_null(), "Source session is not stopped")?; }
    let runs=fs::canonicalize(state.join("runs")).map_err(|_| "Missing sessions")?;
    let target=fs::canonicalize(destination).map_err(|_| "Missing new working card")?;
    let parent=target.parent().ok_or("Invalid new session")?;
    need(parent.parent()==Some(runs.as_path()) && target.file_name()==Some(std::ffi::OsStr::new("card.bin")) && parent!=fs::canonicalize(source).map_err(|_| "Missing source session")?, "Invalid new working card")?;
    need(!parent.join("report.json").exists() && !parent.join("transport.json").exists() && !parent.join("working-ssa.bin").exists(), "Destination is not a new session")?;
    let before=read(&target,CARD_SIZE)?;let old=read(&source.join("card.bin"),CARD_SIZE)?;
    need(before.len()==CARD_SIZE && old.len()==CARD_SIZE && before[..OFFSET]==old[..OFFSET] && before[OFFSET+SIZE..]==old[OFFSET+SIZE..], "Different diagnostic firmware; security data not restored")?;
    let mut patched=before;patched[OFFSET..OFFSET+SIZE].copy_from_slice(&input);
    let origin=source.join("security-collection-origin.json");
    if origin.exists() { private_write(&parent.join("security-collection-origin.json"),&read(&origin,8192)?)?; }
    else { write_json(&parent.join("security-collection-origin.json"), &json!({"source_session":source,"firmware":firmware}))?; }
    let receipt=source.join("security-processing-status.properties");
    if receipt.exists(){private_write(&parent.join("security-processing-status.properties"),&read(&receipt,8192)?)?;}
    fs::write(&target,&patched).map_err(|_| "Cannot restore working security data")?;
    need(read(&target,CARD_SIZE)?==patched,"Restored card verification failed")?;
    private_write(&parent.join("working-ssa.bin"),&input)?;
    Ok(json!({"restored":true}))
}
// Cancellation remains a common boundary for UI, automation and provider fallback.
// No operator password or feature entitlement is required in any release channel.
fn require_active_request(v: &Value) -> Result<()> {
    let cancelled = match v.get("cancelled") {
        None | Some(Value::Null) => false,
        Some(Value::Bool(value)) => *value,
        _ => return Err("Invalid security cancellation state".into()),
    };
    need(!cancelled, "Security processing cancelled; no data imported.")
}

pub fn dispatch(v: &Value) -> Result<Value> {
    need(v["schema"] == 1, "Unsupported security worker schema")?;
    let operation = text(v, "op")?;
    if matches!(operation, "authorize" | "prepare" | "prepare_existing" | "http_result" | "import") {
        require_active_request(v)?;
    }
    match operation {
        "authorize" => Ok(json!({"authorized":true,"policy":"security-access-password-1","mode":"no_operator_password"})),
        "status" => status::dispatch(v),
        "restore" => restore(Path::new(text(v,"state")?),text(v,"firmware")?,Path::new(text(v,"card")?)),
        "endpoints" => Ok(json!({"primary":PRIMARY,"fallback":FALLBACK})),
        "observe" => {
            let screen = text(v, "screen")?;
            Ok(match v.get("run").and_then(Value::as_str) {
                Some(run) => observe_run(Path::new(run), screen.as_bytes(), prompt::Cache::ReadWrite),
                None => observe(screen),
            })
        }
        "navigate" => Ok(navigation::next(v)),
        "menu_navigate" => crate::menu_navigation::dispatch(v),
        "validate_input" => Ok(
            json!({"records":validate_input(&STANDARD.decode(text(v,"ssa")?).map_err(|_|"Invalid encoding")?)?}),
        ),
        "validate_reply" => {
            validate_reply(
                &STANDARD
                    .decode(text(v, "before")?)
                    .map_err(|_| "Invalid encoding")?,
                &STANDARD
                    .decode(text(v, "after")?)
                    .map_err(|_| "Invalid encoding")?,
            )?;
            Ok(json!({"valid":true}))
        }
        "http_result" => Ok(http_decision(
            v["status"].as_u64().unwrap_or(0),
            v["network"].as_str().unwrap_or(""),
            v["primary"] == true,
            v["allow_fallback"] == true,
            v["cancelled"] == true,
        )),
        "inspect_existing" => inspect_existing(
            Path::new(text(v, "state")?),
            Path::new(text(v, "run")?),
            text(v, "firmware")?,
        ),
        "prepare_existing" => prepare_existing(
            Path::new(text(v, "state")?),
            Path::new(text(v, "run")?),
            text(v, "firmware")?,
            v["allow_fallback"] == true,
            text(v, "input_hash")?,
        ),
        "prepare" => prepare(
            Path::new(text(v, "state")?),
            Path::new(text(v, "run")?),
            text(v, "firmware")?,
            v["allow_fallback"] == true,
        ),
        "import" => import(
            Path::new(text(v, "attempt")?),
            &v["response"],
            text(v, "provider")?,
        ),
        _ => Err("Unknown security operation".into()),
    }
}
#[cfg(test)]
mod tests;
