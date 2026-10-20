// SPDX-License-Identifier: MPL-2.0
//! Local MCP stdio server. The native binding alone owns the live app and its key queue.
use crate::native_menu;
use serde_json::{json, Value};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::{
        fs::{MetadataExt, OpenOptionsExt},
        io::AsRawFd,
    },
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
fn owned(path: &Path, directory: bool) -> Result<(), String> {
    let meta = fs::symlink_metadata(path)
        .map_err(|_| "MCP endpoint is unavailable. Enable AI control in the Mac app.")?;
    if meta.uid() != unsafe { libc::geteuid() }
        || meta.file_type().is_symlink()
        || (directory && (!meta.is_dir() || meta.mode() & 0o077 != 0))
        || (!directory && !meta.is_file())
    {
        return Err("MCP endpoint must contain owned regular files in a private directory".into());
    }
    Ok(())
}
fn read_json(path: &Path) -> Result<Value, String> {
    owned(path, false)?;
    let mut bytes = Vec::new();
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .map_err(|_| "Cannot open MCP state")?
        .take(131073)
        .read_to_end(&mut bytes)
        .map_err(|_| "Cannot read MCP state")?;
    if bytes.len() > 131072 {
        return Err("MCP state exceeds its size limit".into());
    }
    serde_json::from_slice(&bytes).map_err(|_| "Invalid MCP state JSON".into())
}
pub struct Bridge {
    directory: PathBuf,
    cancelled: Arc<AtomicBool>,
}
impl Bridge {
    pub fn new(directory: PathBuf, cancelled: Arc<AtomicBool>) -> Self {
        Self {
            directory,
            cancelled,
        }
    }
    fn check_cancel(&self) -> Result<(), String> {
        if self.cancelled.load(Ordering::Relaxed) {
            Err("MCP request cancelled; no further keys sent".into())
        } else {
            Ok(())
        }
    }
    pub fn state(&self) -> Result<Value, String> {
        owned(&self.directory, true)?;
        let state = read_json(&self.directory.join("state.json"))?;
        let timestamp = state["timestamp_ms"]
            .as_u64()
            .ok_or("MCP state lacks a timestamp")?;
        if state["enabled"] != true || now().abs_diff(timestamp) > 2000 {
            return Err(
                "MCP app state is stale or disabled. Reopen/enable the app endpoint.".into(),
            );
        }
        Ok(state)
    }
    fn health(&self, details: bool) -> Result<Value, String> {
        owned(&self.directory, true)?;
        let state = read_json(&self.directory.join("state.json"))?;
        let alive = |v: &Value| {
            v.as_i64()
                .filter(|pid| *pid > 0 && *pid <= i32::MAX as i64)
                .map(|pid| json!(unsafe { libc::kill(pid as i32, 0) } == 0))
                .unwrap_or(Value::Null)
        };
        let mut stamp = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        let mono = if unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut stamp) } == 0 {
            Some(stamp.tv_sec as u64 * 1000 + stamp.tv_nsec as u64 / 1000000)
        } else {
            None
        };
        let age = mono.and_then(|n| {
            state["heartbeat_monotonic_ms"]
                .as_u64()
                .and_then(|t| n.checked_sub(t))
        });
        let mut facts = json!({"platform":"macos","endpoint_enabled":state["enabled"],"app_version":state["app_version"],"architecture":state["architecture"],
            "app_pid":state["app_pid"],"app_alive":alive(&state["app_pid"]),"ui_heartbeat_age_ms":age,"busy":state["busy"],
            "emulator_active":state["session_running"],"emulator_pid":state["emulator_pid"],"emulator_alive":alive(&state["emulator_pid"]),
            "session_age_ms":state["session_age_ms"],"session":state["session"],"emulator_heartbeat_age_ms":null});
        let mut saved = Value::Null;
        if state["enabled"] == true {
            if let Some(root) = state["state_directory"]
                .as_str()
                .filter(|p| Path::new(p).is_absolute())
            {
                let root = PathBuf::from(root);
                let meta =
                    fs::symlink_metadata(&root).map_err(|_| "App state directory unavailable")?;
                if !meta.is_dir()
                    || meta.uid() != unsafe { libc::geteuid() }
                    || meta.file_type().is_symlink()
                {
                    return Err("Invalid app state directory".into());
                }
                let active = state["session"].as_str().filter(|s| !s.is_empty());
                let latest = read_json(&root.join("latest-session.json")).unwrap_or(Value::Null);
                let previous = latest["run"]
                    .as_str()
                    .and_then(|s| Path::new(s).file_name())
                    .and_then(|s| s.to_str());
                if let Some(id) = active.or(previous).filter(|s| {
                    s.len() <= 128
                        && s.bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
                        && *s != "."
                        && *s != ".."
                }) {
                    let run = root.join("runs").join(id);
                    if fs::symlink_metadata(&run).is_ok_and(|m| {
                        m.is_dir()
                            && !m.file_type().is_symlink()
                            && m.uid() == unsafe { libc::geteuid() }
                    }) {
                        if active.is_some() {
                            facts["emulator_heartbeat_age_ms"] =
                                fs::metadata(run.join("emulator-heartbeat"))
                                    .ok()
                                    .and_then(|m| m.modified().ok())
                                    .and_then(|t| SystemTime::now().duration_since(t).ok())
                                    .map(|d| json!(d.as_millis()))
                                    .unwrap_or(Value::Null);
                        }
                        if details {
                            let report = read_json(&run.join("report.json")).unwrap_or(Value::Null);
                            let crash = read_json(&run.join("crash.json")).unwrap_or(Value::Null);
                            saved = json!({"session":id,"current_session":active.is_some(),"source":"saved_local_session_evidence",
                                "outcome":crate::app_health::safe_outcome(&report,&crash),"adapter_diagnostics":crate::adapter_diagnostics::snapshot(&run)});
                        }
                    }
                }
            }
        }
        let mut health = crate::app_health::classify(&facts);
        if details {
            health["session_details"] = saved;
            health["privacy"]=json!("No screenshots, raw LCD, registers/stack, raw logs, VIN, security data or credentials.");
        }
        Ok(health)
    }
    fn command(&self, state: &Value, op: &str, extra: Value) -> Result<Value, String> {
        self.check_cancel()?;
        let mut random = [0; 16];
        File::open("/dev/urandom")
            .and_then(|mut f| f.read_exact(&mut random))
            .map_err(|_| "Cannot create request identity")?;
        let id = random
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let mut command =
            json!({"id":id,"op":op,"deadline_ms":now()+3000,"instance":state["instance"]});
        if let Some(fields) = extra.as_object() {
            command.as_object_mut().unwrap().extend(fields.clone());
        }
        let pending = self.directory.join(format!("pending-{id}.json"));
        let target = self.directory.join("command.json");
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&pending)
            .map_err(|_| "Cannot stage MCP command")?;
        let result = (|| {
            file.write_all(command.to_string().as_bytes())
                .map_err(|_| "Cannot write MCP command")?;
            // Hard-link publication cannot replace an outstanding command from another operator.
            fs::hard_link(&pending, &target).map_err(|_| "Another MCP command is pending")?;
            let deadline = Instant::now() + Duration::from_secs(4);
            while Instant::now() < deadline {
                self.check_cancel()?;
                if let Ok(response) = read_json(&self.directory.join("response.json")) {
                    if response["id"] == id {
                        return if response["accepted"] == true {
                            Ok(response["state"].clone())
                        } else {
                            Err(response["message"]
                                .as_str()
                                .unwrap_or("App rejected MCP command")
                                .to_owned())
                        };
                    }
                }
                thread::sleep(Duration::from_millis(10));
            }
            Err(
                "App did not acknowledge MCP command; do not repeat it without reading state"
                    .into(),
            )
        })();
        // Remove only our unconsumed command (a timed-out or cancelled key must not fire later).
        if let Ok(value) = read_json(&target) {
            if value["id"] == id {
                let _ = fs::remove_file(&target);
            }
        }
        let _ = fs::remove_file(pending);
        result
    }
    fn lock(&self) -> Result<File, String> {
        owned(&self.directory, true)?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(self.directory.join("client.lock"))
            .map_err(|_| "Cannot lock MCP endpoint")?;
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            return Err("Another AI client is navigating this app".into());
        }
        Ok(file)
    }
    fn expected(state: &Value, args: &Value) -> Result<(), String> {
        if args["session"].as_str().is_none_or(str::is_empty)
            || args["revision"].as_str().is_none_or(str::is_empty)
            || args["session"] != state["session"]
            || args["revision"] != state["menu"]["revision"]
        {
            return Err("Expected session/menu changed. Read opensaab_get_state again.".into());
        }
        Ok(())
    }
    fn press(&self, state: &Value, code: u8) -> Result<Value, String> {
        self.check_cancel()?;
        if state["input_available"] != true
            || state["menu"]["input_idle"] != true
            || state["automation_active"] == true
        {
            return Err(
                "Firmware is busy, another navigator is active, or a native dialog needs attention"
                    .into(),
            );
        }
        self.command(state,"key",json!({"encoder":code,"session":state["session"],"revision":state["menu"]["revision"],"operator_generation":state["operator_generation"]}))?;
        let before = state["menu"]["input_sequence"]
            .as_u64()
            .ok_or("Input sequence unavailable")?;
        let deadline = Instant::now() + Duration::from_secs(8);
        let mut candidate = Value::Null;
        let mut stable = Instant::now();
        while Instant::now() < deadline {
            self.check_cancel()?;
            let next = self.state()?;
            if next["session"] != state["session"]
                || next["instance"] != state["instance"]
                || next["operator_generation"] != state["operator_generation"]
            {
                return Err(
                    "Session changed or the operator took over. Navigation stopped.".into(),
                );
            }
            if next["menu"]["revision"] != candidate {
                candidate = next["menu"]["revision"].clone();
                stable = Instant::now();
            }
            if next["menu"]["input_sequence"]
                .as_u64()
                .is_some_and(|v| v > before)
                && next["input_available"] == true
                && next["menu"]["input_idle"] == true
                && stable.elapsed() >= Duration::from_millis(150)
            {
                return Ok(next);
            }
            thread::sleep(Duration::from_millis(20));
        }
        Err("Firmware did not settle after the key. Inspect state before another action.".into())
    }
    fn choose(&self, mut state: Value, label: &str, deadline: Instant) -> Result<Value, String> {
        for _ in 0..30 {
            if Instant::now() >= deadline {
                return Err("Menu navigation deadline reached".into());
            }
            let (code, entered) = native_menu::select(&state["menu"], label)?;
            let mut next = self.press(&state, code)?;
            if entered {
                let original = native_menu::screen_identity(&state["menu"]);
                let transition_deadline = deadline.min(Instant::now() + Duration::from_secs(8));
                while native_menu::screen_identity(&next["menu"]) == original
                    || native_menu::screen_identity(&next["menu"]).is_empty()
                {
                    self.check_cancel()?;
                    if Instant::now() >= transition_deadline {
                        return Err(
                            "Key consumed but no meaningful menu transition observed; path stopped"
                                .into(),
                        );
                    }
                    thread::sleep(Duration::from_millis(20));
                    next = self.state()?;
                    if next["session"] != state["session"]
                        || next["instance"] != state["instance"]
                        || next["operator_generation"] != state["operator_generation"]
                    {
                        return Err(
                            "Session changed or operator took over while awaiting the menu".into(),
                        );
                    }
                }
                return Ok(next);
            }
            if next["menu"]["title"] != state["menu"]["title"]
                || next["menu"]["selection"] == state["menu"]["selection"]
            {
                return Err("List selection did not advance as expected; path stopped".into());
            }
            state = next;
        }
        Err("List navigation move limit reached".into())
    }
    pub fn call(&self, name: &str, args: &Value) -> Result<Value, String> {
        if !args.is_object() {
            return Err("Tool arguments must be an object".into());
        }
        let allowed: &[&str] = match name {
            "opensaab_get_state"
            | "opensaab_start_offline"
            | "opensaab_get_health"
            | "opensaab_get_crash_details" => &[],
            "opensaab_android_health" | "opensaab_android_crash_details" => &["serial"],
            "opensaab_stop" => &["session"],
            "opensaab_press_key" => &["key", "session", "revision"],
            "opensaab_select_entry" => &["label", "session", "revision"],
            "opensaab_navigate_menu" => &["path", "session", "revision"],
            _ => return Err("Unknown tool".into()),
        };
        if args
            .as_object()
            .unwrap()
            .keys()
            .any(|k| !allowed.contains(&k.as_str()))
        {
            return Err("Unexpected tool argument".into());
        }
        if name == "opensaab_get_health" || name == "opensaab_get_crash_details" {
            return self.health(name == "opensaab_get_crash_details");
        }
        if name == "opensaab_android_health" || name == "opensaab_android_crash_details" {
            return crate::android_remote::collect(
                args["serial"]
                    .as_str()
                    .ok_or("Provide the exact paired ADB serial")?,
                name == "opensaab_android_crash_details",
            );
        }
        let mut state = self.state()?;
        if name == "opensaab_get_state" {
            return Ok(state);
        }
        let _lock = self.lock()?;
        match name {
            "opensaab_start_offline" => {
                self.command(&state, "start_offline", json!({}))?;
                let deadline = Instant::now() + Duration::from_secs(15);
                while Instant::now() < deadline {
                    self.check_cancel()?;
                    state = self.state()?;
                    if state["session_running"] == true
                        && state["menu"]["input_idle"] == true
                        && !state["menu"]["text"]
                            .as_str()
                            .unwrap_or("")
                            .trim()
                            .is_empty()
                    {
                        return Ok(state);
                    }
                    thread::sleep(Duration::from_millis(30));
                }
                Err("Offline startup is pending; read state to observe the result".into())
            }
            "opensaab_stop" => {
                if args["session"].as_str().is_none_or(str::is_empty)
                    || args["session"] != state["session"]
                {
                    return Err("Expected session changed before Stop".into());
                }
                self.command(&state, "stop", json!({"session":state["session"]}))?;
                Ok(json!({"outcome":"stop_requested","state":self.state()?}))
            }
            "opensaab_press_key" => {
                Self::expected(&state, args)?;
                let code = native_menu::key_code(
                    args["key"]
                        .as_str()
                        .ok_or("key must be a canonical keypad name")?,
                )
                .ok_or("Unknown keypad key")?;
                let next = self.press(&state, code)?;
                Ok(
                    json!({"screen_changed":next["menu"]["revision"]!=state["menu"]["revision"],"state":next}),
                )
            }
            "opensaab_select_entry" => {
                Self::expected(&state, args)?;
                let next = self.choose(
                    state,
                    args["label"]
                        .as_str()
                        .filter(|s| !s.is_empty() && s.len() <= 160)
                        .ok_or("label must be a bounded exact visible label")?,
                    Instant::now() + Duration::from_secs(30),
                )?;
                Ok(json!({"outcome":"entered","state":next}))
            }
            "opensaab_navigate_menu" => {
                Self::expected(&state, args)?;
                let path = args["path"]
                    .as_array()
                    .filter(|v| !v.is_empty() && v.len() <= 12)
                    .ok_or("path must contain 1 to 12 exact visible labels")?;
                if path
                    .iter()
                    .any(|v| v.as_str().is_none_or(|s| s.is_empty() || s.len() > 160))
                {
                    return Err("Invalid path label".into());
                }
                let deadline = Instant::now() + Duration::from_secs(30);
                let mut completed = Vec::new();
                for label in path {
                    match self.choose(state.clone(), label.as_str().unwrap(), deadline) {
                        Ok(next) => {
                            state = next;
                            completed.push(label.clone());
                        }
                        Err(error) => {
                            return Ok(
                                json!({"outcome":"incomplete","completed":completed,"blocked_at":label,"reason":error,"state":self.state().unwrap_or(state)}),
                            )
                        }
                    }
                }
                Ok(json!({"outcome":"complete","completed":completed,"state":state}))
            }
            _ => unreachable!(),
        }
    }
}

pub fn tools() -> Value {
    let context = json!({"session":{"type":"string","description":"Current session from get_state"},"revision":{"type":"string","description":"Current menu revision from get_state"}});
    let definitions=[
        ("opensaab_get_state","Read live app/menu text, visible entries, cursor selection, soft-key labels, input readiness and native dialog state.","",json!({}),true),
        ("opensaab_navigate_menu","Glide through a path of exact visible menu labels in one call. Re-read native state after each key; stop on ambiguity, prompts, timeout or manual takeover. Only visible rows in recognized lists can be selected.","path",json!({"type":"array","items":{"type":"string","maxLength":160},"minItems":1,"maxItems":12}),false),
        ("opensaab_press_key","Press one real keypad/soft key and return the settled native menu. ENTER/YES can acknowledge firmware prompts; invoke only when requested by the operator.","key",json!({"type":"string","enum":native_menu::KEYS.iter().filter(|(key,_)|*key!="SELECT").map(|(key,_)|*key).collect::<Vec<_>>()}),false),
        ("opensaab_select_entry","Select an exact observed function/soft-key label or a visible item in a recognized arrow list. Never infer a hidden menu.","label",json!({"type":"string","minLength":1,"maxLength":160}),false),
        ("opensaab_start_offline","Start the selected original firmware without opening an adapter; return its native menu state.","",json!({}),false),
        ("opensaab_stop","Request Stop through the app lifecycle. Observe state for actual session completion and cleanup.","session",json!({"type":"string","minLength":1}),false)];
    let mut result=definitions.into_iter().map(|(name,description,arg,schema,read)|{
        let mut props=if arg.is_empty(){json!({})}else{context.clone()};
        let required=if arg.is_empty(){json!([])}else if arg=="session" {props=json!({"session":schema});json!(["session"])}else{props[arg]=schema;json!([arg,"session","revision"])};
        json!({"name":name,"description":description,"inputSchema":{"type":"object","properties":props,"required":required,"additionalProperties":false},
            "annotations":{"readOnlyHint":read,"destructiveHint":!read,"idempotentHint":read,"openWorldHint":false}})
    }).collect::<Vec<_>>();
    for (name,description,android) in [
        ("opensaab_get_health","Quick read-only Mac process/UI/emulator health. Uses heartbeat evidence; a static menu or stale historical crash never establishes a current crash.",false),
        ("opensaab_get_crash_details","Read bounded local session outcomes and existing sanitized adapter/failure evidence. Distinguish guest firmware failure, adapter failure and unknown host cause; do not restart or upload.",false),
        ("opensaab_android_health","Read the explicitly selected paired Android device's installed app identity, current app PID and OS exit history over ADB. No app install, launch, stop, key or adapter operation. UI/emulator responsiveness remains unknown until the phone binding is implemented.",true),
        ("opensaab_android_crash_details","Read scoped Android OS exit reasons (Java/native crash, ANR, low-memory, user stop, package update), preserving their historical timestamps and process IDs. No raw crash/log payload or root/debuggable assumption.",true)
    ] {
        result.push(json!({"name":name,"description":description,"inputSchema":{"type":"object","properties":if android{json!({"serial":{"type":"string","minLength":1,"maxLength":160}})}else{json!({})},"required":if android{json!(["serial"])}else{json!([])},"additionalProperties":false},
            "annotations":{"readOnlyHint":true,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false}}));
    }
    json!(result)
}

pub struct Server {
    pub bridge: Bridge,
    initialized: bool,
    ready: bool,
    version: String,
}
impl Server {
    pub fn new(bridge: Bridge, version: String) -> Self {
        Self {
            bridge,
            initialized: false,
            ready: false,
            version,
        }
    }
    pub fn dispatch(&mut self, request: Value) -> Option<Value> {
        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let error = |code: i32, message: &str| json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}});
        if request["jsonrpc"] != "2.0"
            || !request.is_object()
            || request["method"].as_str().is_none()
        {
            return Some(error(-32600, "Invalid Request"));
        }
        if request.get("id").is_none() {
            if request["method"] == "notifications/initialized" && self.initialized {
                self.ready = true;
            }
            return None;
        }
        if !id.is_string() && !id.is_number() {
            return Some(error(-32600, "Invalid request ID"));
        }
        let result = match request["method"].as_str().unwrap() {
            "initialize" => {
                if self.initialized {
                    return Some(error(-32600, "Already initialized"));
                }
                let params = &request["params"];
                if params["protocolVersion"].as_str().is_none()
                    || !params["capabilities"].is_object()
                    || !params["clientInfo"].is_object()
                {
                    return Some(error(-32602, "Invalid initialize parameters"));
                }
                self.initialized = true;
                let protocol = match params["protocolVersion"].as_str().unwrap() {
                    "2024-11-05" => "2024-11-05",
                    "2025-03-26" => "2025-03-26",
                    "2025-06-18" => "2025-06-18",
                    _ => "2025-11-25",
                };
                json!({"protocolVersion":protocol,"capabilities":{"tools":{}},"serverInfo":{"name":"OpenSAAB","version":self.version},
                    "instructions":"Read get_state first. Use exact visible labels and returned session/revision for native navigation. Prompts and uncertain screens require operator guidance; do not guess keys."})
            }
            "ping" => json!({}),
            _ if !self.ready => {
                return Some(error(-32000, "Complete the initialize handshake first"))
            }
            "tools/list" => json!({"tools":tools()}),
            "tools/call" => {
                let params = &request["params"];
                let Some(name) = params["name"].as_str() else {
                    return Some(error(-32602, "Missing tool name"));
                };
                if !tools()
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|t| t["name"] == name)
                {
                    return Some(error(-32602, "Unknown tool"));
                }
                let args = params.get("arguments").cloned().unwrap_or(json!({}));
                match self.bridge.call(name, &args) {
                    Ok(value) => {
                        json!({"content":[{"type":"text","text":value.to_string()}],"structuredContent":value,"isError":false})
                    }
                    Err(message) => {
                        json!({"content":[{"type":"text","text":message}],"isError":true})
                    }
                }
            }
            _ => return Some(error(-32601, "Method not found")),
        };
        Some(json!({"jsonrpc":"2.0","id":id,"result":result}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn handshake_notifications_and_tool_discovery() {
        let mut server = Server::new(
            Bridge::new(
                PathBuf::from("/unavailable"),
                Arc::new(AtomicBool::new(false)),
            ),
            "test".into(),
        );
        assert!(server
            .dispatch(json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}))
            .unwrap()["error"]
            .is_object());
        assert_eq!(server.dispatch(json!({"jsonrpc":"2.0","id":2,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"1"}}})).unwrap()["result"]["serverInfo"]["version"],"test");
        assert!(server
            .dispatch(json!({"jsonrpc":"2.0","method":"notifications/initialized"}))
            .is_none());
        assert_eq!(
            server
                .dispatch(json!({"jsonrpc":"2.0","id":3,"method":"tools/list"}))
                .unwrap()["result"]["tools"]
                .as_array()
                .unwrap()
                .len(),
            10
        );
        assert_eq!(
            server
                .dispatch(
                    json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"absent"}})
                )
                .unwrap()["error"]["code"],
            -32602
        );
    }
}
