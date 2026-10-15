// SPDX-License-Identifier: MPL-2.0
//! Bounded live VIN transport through a separate 32-bit Windows J2534 helper.
//! This is a host diagnostic operation, not a decoded Tech2 guest request.
use std::fs::{self, OpenOptions};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// Explicit, capture-backed identification profiles. The ME9.6 helper is VIN guarded.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EcuProfile {
    #[default]
    Trionic8,
    Me96Vehicle1367,
    IbusBcm1367,
}
impl EcuProfile {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "t8" => Ok(Self::Trionic8),
            "me96-1367" => Ok(Self::Me96Vehicle1367),
            "ibus-bcm-1367" => Ok(Self::IbusBcm1367),
            _ => Err("--vcx-ecu must be t8, me96-1367 or ibus-bcm-1367".into()),
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Trionic8 => "Trionic 8",
            Self::Me96Vehicle1367 => "Bosch ME9.6",
            Self::IbusBcm1367 => "BCM / I-bus",
        }
    }
    fn pids(self) -> &'static [u8] {
        match self {
            Self::Trionic8 => &[0x90, 0x71, 0x08, 0x74, 0xb4, 0x95, 0x73],
            Self::Me96Vehicle1367 => &[0x90, 0x97, 0x9a, 0xc1, 0xc2, 0xc3, 0xcb],
            Self::IbusBcm1367 => &[0x90, 0x97, 0x9a, 0xc1],
        }
    }
    pub fn protocol(self) -> u32 {
        if self == Self::IbusBcm1367 {
            0x8007
        } else {
            6
        }
    }
    pub fn baud(self) -> u32 {
        if self == Self::IbusBcm1367 {
            33333
        } else {
            500000
        }
    }
    pub fn request_id(self) -> u32 {
        if self == Self::IbusBcm1367 {
            0x242
        } else {
            0x7e0
        }
    }
    pub fn response_id(self) -> u32 {
        if self == Self::IbusBcm1367 {
            0x642
        } else {
            0x7e8
        }
    }
    fn response_prefix(self) -> [u8; 5] {
        let id = self.response_id().to_be_bytes();
        [id[0], id[1], id[2], id[3], 0x5a]
    }
    fn connect_evidence(self) -> &'static str {
        if self == Self::IbusBcm1367 {
            "PassThruConnect SW_ISO15765_PS flags=0 baud=33333 pin1 rc=0x00000000"
        } else {
            "PassThruConnect ISO15765 flags=0 baud=500000 default-HSCAN rc=0x00000000"
        }
    }
    fn verify(self, result: &VinResult) -> Result<(), String> {
        if !self
            .pids()
            .iter()
            .all(|pid| result.identities.iter().any(|v| v.pid == *pid))
        {
            return Err(format!(
                "Incomplete {} ECM identification; see helper log",
                self.name()
            ));
        }
        if self == Self::Me96Vehicle1367
            && (result.vin != "YS3FH46U681000002"
                || !result
                    .identities
                    .iter()
                    .any(|v| v.pid == 0x97 && v.bytes == b"BOSCH_ME96"))
        {
            return Err("ECU identity does not match the ME9.6 / 1367 profile".into());
        }
        if self == Self::IbusBcm1367
            && (result.vin != "YS3FH46U681000002"
                || !result
                    .identities
                    .iter()
                    .any(|v| v.pid == 0x97 && v.value == "BCM"))
        {
            return Err("Module identity does not match the BCM / 1367 profile".into());
        }
        Ok(())
    }
}

/// Fixed installed-driver profiles; never accept a shell command or arbitrary DLL path.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum J2534Adapter {
    #[default]
    Nano,
    ChipsoftPro,
}
impl J2534Adapter {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "nano" => Ok(Self::Nano),
            "chipsoft-pro" => Ok(Self::ChipsoftPro),
            _ => Err("J2534 adapter must be nano or chipsoft-pro".into()),
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Nano => "nano",
            Self::ChipsoftPro => "chipsoft-pro",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Connection {
    pub target: String,
    pub control_path: Option<PathBuf>,
    pub ecu_profile: EcuProfile,
}

impl Connection {
    pub fn validate(&self) -> Result<(), String> {
        if self.target.is_empty()
            || self.target.starts_with('-')
            || !self
                .target
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"@._-:".contains(&b))
        {
            return Err("VCX SSH target must be a hostname or user@hostname".into());
        }
        Ok(())
    }

    fn command(&self, identity: bool, seed: bool) -> Result<Command, String> {
        if seed && self.ecu_profile != EcuProfile::IbusBcm1367 {
            return Err("Seed job is restricted to the BCM / 1367 profile".into());
        }
        // Fixed command: no target/path/user input is interpolated into PowerShell.
        let script = "$ProgressPreference='SilentlyContinue'; & ([scriptblock]::Create((Get-Content \"$env:USERPROFILE\\invoke_vcx_t8_vin.ps1\" -Raw)))";
        let probe = if seed {
            " -ProbePath \"$env:USERPROFILE\\vcx_bcm_1367_seed.ps1\""
        } else {
            match self.ecu_profile {
                EcuProfile::Trionic8 => "",
                EcuProfile::IbusBcm1367 => {
                    " -ProbePath \"$env:USERPROFILE\\vcx_ibus_1367_identity.ps1\""
                }
                EcuProfile::Me96Vehicle1367 => {
                    " -ProbePath \"$env:USERPROFILE\\vcx_me96_1367_identity.ps1\""
                }
            }
        };
        let script = format!(
            "{script}{probe} -Profile {}",
            if identity { "Identity" } else { "Vin" }
        );
        self.script_command(&script)
    }

    pub(crate) fn script_command(&self, script: &str) -> Result<Command, String> {
        self.powershell_command(script, false)
    }
    pub(crate) fn powershell_command(&self, script: &str, x86: bool) -> Result<Command, String> {
        let mut command = self.ssh_command()?;
        let bytes: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
        command.args([
            if x86 {
                r"C:\Windows\SysWOW64\WindowsPowerShell\v1.0\powershell.exe"
            } else {
                "powershell.exe"
            },
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-EncodedCommand",
            &base64(&bytes),
        ]);
        Ok(command)
    }
    pub(crate) fn native_bridge_command(
        &self,
        port: u16,
        seed_reads: bool,
        adapter: J2534Adapter,
        seatbelt_audible: bool,
    ) -> Result<Command, String> {
        if seatbelt_audible && (seed_reads || adapter != J2534Adapter::ChipsoftPro) {
            return Err("Seatbelt mode cannot combine seed mode or another adapter".into());
        }
        let base = self.ssh_command()?;
        let args: Vec<_> = base.get_args().collect();
        let mut command = Command::new("ssh");
        command.args(&args[..args.len() - 1]);
        command
            .arg("-L")
            .arg(format!("127.0.0.1:{port}:127.0.0.1:35672"));
        command.args(["-o", "ExitOnForwardFailure=yes", &self.target]);
        command.arg(match adapter {
            J2534Adapter::Nano => r#""%USERPROFILE%\vcx-native-bridge.exe""#,
            J2534Adapter::ChipsoftPro => r#""%USERPROFILE%\chipsoft-native-bridge.exe""#,
        });
        if seatbelt_audible {
            command.arg("--seatbelt-audible");
        } else if seed_reads {
            command.arg("--seed-read-only");
        }
        Ok(command)
    }
    pub(crate) fn native_forward_cleanup(&self, port: u16) -> Option<Command> {
        let path = self.control_path.as_ref()?;
        let mut command = Command::new("ssh");
        command
            .arg("-S")
            .arg(path)
            .args(["-O", "cancel", "-L"])
            .arg(format!("127.0.0.1:{port}:127.0.0.1:35672"))
            .arg(&self.target);
        Some(command)
    }
    fn ssh_command(&self) -> Result<Command, String> {
        self.validate()?;
        let mut command = Command::new("ssh");
        if let Some(path) = &self.control_path {
            command.arg("-S").arg(path);
        }
        command.args([
            "-T",
            "-o",
            "BatchMode=yes",
            "-o",
            "ConnectTimeout=8",
            "-o",
            "StrictHostKeyChecking=yes",
            "-o",
            "ServerAliveInterval=5",
            "-o",
            "ServerAliveCountMax=2",
            &self.target,
        ]);
        Ok(command)
    }
}

fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();
    for chunk in bytes.chunks(3) {
        let n = ((chunk[0] as u32) << 16)
            | ((chunk.get(1).copied().unwrap_or(0) as u32) << 8)
            | chunk.get(2).copied().unwrap_or(0) as u32;
        result.push(TABLE[((n >> 18) & 63) as usize] as char);
        result.push(TABLE[((n >> 12) & 63) as usize] as char);
        result.push(if chunk.len() > 1 {
            TABLE[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        result.push(if chunk.len() > 2 {
            TABLE[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    result
}

#[derive(Debug, Clone)]
pub struct IdentityValue {
    pub pid: u8,
    pub bytes: Vec<u8>,
    pub value: String,
}
#[derive(Debug, Clone)]
pub struct AdapterVersion {
    pub firmware: String,
    pub dll: String,
    pub api: String,
}
#[derive(Debug, Clone)]
pub struct VinResult {
    pub vin: String,
    pub response: Vec<u8>,
    pub log_path: PathBuf,
    pub identities: Vec<IdentityValue>,
    pub adapter: Option<AdapterVersion>,
}

/// Verify a full ECU payload and successful cleanup, not just a printed VIN.
pub fn parse_response(log: &str) -> Result<(String, Vec<u8>), String> {
    parse_response_for(log, EcuProfile::Trionic8)
}

fn parse_response_for(log: &str, profile: EcuProfile) -> Result<(String, Vec<u8>), String> {
    let request = format!("REQUEST CAN={:03X} payload=1A 90;", profile.request_id());
    for required in [
        profile.connect_evidence(),
        &request,
        "DRIVER_ACCEPTED_MESSAGES=1 (not evidence of ECU response)",
        "PassThruStopMsgFilter rc=0x00000000",
        "PassThruDisconnect rc=0x00000000",
        "PassThruClose rc=0x00000000",
        "PROBE_RESULT=0",
        "HELPER_EXIT=0",
    ] {
        if !log.lines().any(|line| line.contains(required)) {
            return Err(format!("Incomplete live transaction: missing {required}"));
        }
    }
    if profile == EcuProfile::IbusBcm1367 && !log.contains("SET_CONFIG J1962_PINS rc=0x00000000") {
        return Err("Missing successful I-bus pin configuration".into());
    }
    let mut received = None;
    for line in log.lines() {
        if !line.contains(&format!(
            "RX status=0x00000000 protocol={} ",
            profile.protocol()
        )) {
            continue;
        }
        let Some((_, hex)) = line.split_once(" bytes=") else {
            continue;
        };
        let bytes = hex
            .trim()
            .split('-')
            .map(|part| u8::from_str_radix(part, 16))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| "Malformed J2534 receive bytes")?;
        if bytes.len() < 6 || !bytes.starts_with(&profile.response_prefix()) || bytes[5] != 0x90 {
            continue;
        }
        if bytes.len() != 23
            || !bytes[6..]
                .iter()
                .all(|b| b.is_ascii_digit() || (b.is_ascii_uppercase() && !b"IOQ".contains(b)))
        {
            return Err("ECU VIN response has invalid length or characters".into());
        }
        let vin = String::from_utf8(bytes[6..].to_vec()).map_err(|_| "Invalid VIN encoding")?;
        if received
            .as_ref()
            .is_some_and(|(previous, _)| previous != &vin)
        {
            return Err("Conflicting VIN responses".into());
        }
        received = Some((vin, bytes));
    }
    received.ok_or_else(|| {
        format!(
            "No complete {:03X} / 5A 90 module response",
            profile.response_id()
        )
    })
}

#[cfg(test)]
fn identity_details(log: &str) -> Result<(Vec<IdentityValue>, Option<AdapterVersion>), String> {
    identity_details_for(log, EcuProfile::Trionic8)
}

fn identity_details_for(
    log: &str,
    profile: EcuProfile,
) -> Result<(Vec<IdentityValue>, Option<AdapterVersion>), String> {
    let mut identities: Vec<IdentityValue> = Vec::new();
    let mut adapter = None;
    for line in log.lines() {
        if line.contains(&format!(
            "RX status=0x00000000 protocol={} ",
            profile.protocol()
        )) {
            let Some((_, hex)) = line.split_once(" bytes=") else {
                continue;
            };
            let bytes = hex
                .trim()
                .split('-')
                .map(|p| u8::from_str_radix(p, 16))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| "Malformed identity response")?;
            if bytes.len() <= 6 || !bytes.starts_with(&profile.response_prefix()) {
                continue;
            }
            let pid = bytes[5];
            if !EcuProfile::Trionic8.pids().contains(&pid)
                && !EcuProfile::Me96Vehicle1367.pids().contains(&pid)
            {
                continue;
            }
            if !log.contains(&format!(
                "REQUEST CAN={:03X} payload=1A {pid:02X};",
                profile.request_id()
            )) {
                continue;
            }
            let data = bytes[6..].to_vec();
            if let Some(previous) = identities.iter().find(|v| v.pid == pid) {
                if previous.bytes != data {
                    return Err(format!("Conflicting identity replies for {pid:02X}"));
                }
                continue;
            }
            let value = if data.iter().all(|b| (0x20..=0x7e).contains(b)) {
                String::from_utf8_lossy(&data).trim().to_owned()
            } else {
                format!(
                    "{} (hex)",
                    data.iter()
                        .map(|b| format!("{b:02X}"))
                        .collect::<Vec<_>>()
                        .join(" ")
                )
            };
            identities.push(IdentityValue {
                pid,
                bytes: data,
                value,
            });
        }
        if let Some((_, versions)) = line.split_once("ADAPTER_VERSION firmware=") {
            if !log.contains("PassThruReadVersion rc=0x00000000") {
                continue;
            }
            if let Some((firmware, rest)) = versions.split_once(" dll=") {
                if let Some((dll, api)) = rest.split_once(" api=") {
                    adapter = Some(AdapterVersion {
                        firmware: firmware.into(),
                        dll: dll.into(),
                        api: api.trim().into(),
                    });
                }
            }
        }
    }
    Ok((identities, adapter))
}

fn run_command(
    command: Command,
    profile: EcuProfile,
    directory: &Path,
    cancel: &AtomicBool,
    timeout: Duration,
    progress: &mpsc::Sender<String>,
) -> Result<VinResult, String> {
    let stdout = run_logged_command(command, directory, cancel, timeout, progress)?;
    let log = fs::read_to_string(&stdout).map_err(|e| format!("Read VCX output: {e}"))?;
    let (vin, response) = parse_response_for(&log, profile)?;
    let (identities, adapter) = identity_details_for(&log, profile)?;
    Ok(VinResult {
        identities,
        adapter,
        vin,
        response,
        log_path: stdout,
    })
}

pub(crate) fn run_logged_command(
    mut command: Command,
    directory: &Path,
    cancel: &AtomicBool,
    timeout: Duration,
    progress: &mpsc::Sender<String>,
) -> Result<PathBuf, String> {
    fs::create_dir_all(directory).map_err(|e| format!("Create VCX log directory: {e}"))?;
    let stdout = directory.join("vcx-stdout.log");
    let stderr = directory.join("vcx-stderr.log");
    let out = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&stdout)
        .map_err(|e| format!("Create new VCX stdout log: {e}"))?;
    let err = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&stderr)
        .map_err(|e| format!("Create new VCX stderr log: {e}"))?;
    let mut tail = LogTail::new(fs::File::open(&stdout).map_err(|e| e.to_string())?);
    let mut child = command
        .stdin(Stdio::null())
        .stdout(out)
        .stderr(err)
        .spawn()
        .map_err(|e| format!("Start VCX SSH helper: {e}"))?;
    let start = Instant::now();
    let status = loop {
        if let Err(error) = tail.drain(progress, false) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
        let cause = if cancel.load(Ordering::Acquire) {
            Some("cancelled")
        } else if start.elapsed() >= timeout {
            Some("timed out")
        } else {
            None
        };
        if let Some(cause) = cause {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!(
                "VCX request {cause}; remote helper has its own 25-second watchdog. Logs: {}",
                directory.display()
            ));
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => thread::sleep(Duration::from_millis(25)),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("Wait for VCX helper: {error}"));
            }
        }
    };
    tail.drain(progress, true)?;
    if !status.success() {
        return Err(format!(
            "VCX helper exited {status}; see {} and {}",
            stdout.display(),
            stderr.display()
        ));
    }
    if fs::metadata(&stdout).map_err(|e| e.to_string())?.len() > 65536 {
        return Err("VCX helper output exceeds 64 KiB".into());
    }
    Ok(stdout)
}

/// Validate a recorded BCM seed transaction. Does not calculate or submit a key.
pub fn parse_bcm_seed(log: &str) -> Result<[u8; 2], String> {
    let profile = EcuProfile::IbusBcm1367;
    let (vin, response) = parse_response_for(log, profile)?;
    let (identities, adapter) = identity_details_for(log, profile)?;
    profile.verify(&VinResult {
        vin,
        response,
        identities,
        adapter,
        log_path: PathBuf::new(),
    })?;
    let requests: Vec<_> = log
        .lines()
        .filter_map(|l| l.split_once("REQUEST CAN=").map(|(_, r)| r))
        .collect();
    if requests
        .iter()
        .filter(|r| r.starts_with("242 payload=27 01;"))
        .count()
        != 1
    {
        return Err("Expected exactly one BCM level-1 seed request".into());
    }
    if requests.iter().any(|r| {
        ![
            "242 payload=1A 90;",
            "242 payload=1A 97;",
            "242 payload=1A 9A;",
            "242 payload=1A C1;",
            "242 payload=27 01;",
        ]
        .iter()
        .any(|prefix| r.starts_with(prefix))
    }) {
        return Err("Unexpected request in seed-only transcript".into());
    }
    let requested_at = log.find("REQUEST CAN=242 payload=27 01;").unwrap();
    let mut seed = None;
    for line in log[requested_at..].lines() {
        if !line.contains("RX status=0x00000000 protocol=32775 ") {
            continue;
        }
        let Some((_, hex)) = line.split_once(" bytes=") else {
            continue;
        };
        let bytes = hex
            .trim()
            .split('-')
            .map(|v| u8::from_str_radix(v, 16))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| "Malformed seed receive bytes")?;
        if bytes.starts_with(&[0, 0, 6, 0x42, 0x7f, 0x27]) {
            if bytes.len() != 7 || bytes[6] != 0x78 {
                return Err("BCM rejected the seed request; no retry performed".into());
            }
        }
        if !bytes.starts_with(&[0, 0, 6, 0x42, 0x67, 0x01]) {
            continue;
        }
        if bytes.len() != 8 {
            return Err("Expected a two-byte BCM seed".into());
        }
        let current = [bytes[6], bytes[7]];
        if seed.is_some_and(|previous| previous != current) {
            return Err("Conflicting seed replies".into());
        }
        seed = Some(current);
    }
    seed.ok_or_else(|| "No complete 642 / 67 01 seed response".into())
}

/// Incremental, bounded line framing: incomplete writes wait for the next poll.
struct LogTail<R> {
    reader: R,
    pending: Vec<u8>,
    total: usize,
}
impl<R: Read> LogTail<R> {
    fn new(reader: R) -> Self {
        Self {
            reader,
            pending: Vec::new(),
            total: 0,
        }
    }
    fn drain(&mut self, sender: &mpsc::Sender<String>, final_read: bool) -> Result<(), String> {
        let mut chunk = [0; 4096];
        loop {
            let n = self
                .reader
                .read(&mut chunk)
                .map_err(|e| format!("Read live VCX log: {e}"))?;
            if n == 0 {
                break;
            }
            self.total += n;
            if self.total > 65536 {
                return Err("VCX helper output exceeds 64 KiB".into());
            }
            self.pending.extend_from_slice(&chunk[..n]);
        }
        while let Some(end) = self.pending.iter().position(|b| *b == b'\n') {
            let line: Vec<_> = self.pending.drain(..=end).collect();
            let _ = sender.send(String::from_utf8_lossy(&line).trim_end().to_owned());
        }
        if final_read && !self.pending.is_empty() {
            let _ = sender.send(String::from_utf8_lossy(&self.pending).trim_end().to_owned());
            self.pending.clear();
        }
        Ok(())
    }
}

/// Poll from the UI without blocking it. Each job owns its subprocess and logs.
pub struct VinJob {
    receiver: mpsc::Receiver<Result<VinResult, String>>,
    progress: mpsc::Receiver<String>,
    cancel: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl VinJob {
    pub fn start(connection: Connection, directory: PathBuf) -> Result<Self, String> {
        Self::start_profile(connection, directory, false)
    }

    pub fn start_profile(
        connection: Connection,
        directory: PathBuf,
        identity: bool,
    ) -> Result<Self, String> {
        Self::start_operation(connection, directory, identity, false)
    }

    pub fn start_bcm_seed(connection: Connection, directory: PathBuf) -> Result<Self, String> {
        Self::start_operation(connection, directory, true, true)
    }

    fn start_operation(
        connection: Connection,
        directory: PathBuf,
        identity: bool,
        seed: bool,
    ) -> Result<Self, String> {
        let command = connection.command(identity, seed)?;
        let ecu_profile = connection.ecu_profile;
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = Arc::clone(&cancel);
        let (sender, receiver) = mpsc::channel();
        let (progress_tx, progress) = mpsc::channel();
        let thread = thread::Builder::new()
            .name("vcx-vin".into())
            .spawn(move || {
                let result = run_command(
                    command,
                    ecu_profile,
                    &directory,
                    &worker_cancel,
                    Duration::from_secs(35),
                    &progress_tx,
                );
                let result = result.and_then(|result| {
                    if identity {
                        ecu_profile.verify(&result)?;
                    }
                    Ok(result)
                });
                let _ = sender.send(result);
            })
            .map_err(|e| format!("Start VCX worker: {e}"))?;
        Ok(Self {
            receiver,
            progress,
            cancel,
            thread: Some(thread),
        })
    }

    pub fn take_logs(&mut self) -> Vec<String> {
        self.progress.try_iter().collect()
    }

    pub fn poll(&mut self) -> Option<Result<VinResult, String>> {
        match self.receiver.try_recv() {
            Ok(result) => Some(result),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                Some(Err("VCX worker stopped without a result".into()))
            }
        }
    }
}

impl Drop for VinJob {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn native_adapter_selection_is_fixed_and_seeds_are_explicit() {
        let conn = super::Connection {
            target: "user@host".into(),
            control_path: None,
            ecu_profile: super::EcuProfile::default(),
        };
        for (adapter, exe) in [
            (super::J2534Adapter::Nano, "vcx-native-bridge.exe"),
            (
                super::J2534Adapter::ChipsoftPro,
                "chipsoft-native-bridge.exe",
            ),
        ] {
            let command = conn
                .native_bridge_command(12345, false, adapter, false)
                .unwrap();
            let args: Vec<_> = command.get_args().map(|a| a.to_string_lossy()).collect();
            assert!(args.last().unwrap().contains(exe));
            assert!(!args.iter().any(|a| a.contains("--seed-read-only")));
        }
        for adapter in [super::J2534Adapter::Nano, super::J2534Adapter::ChipsoftPro] {
            let command = conn
                .native_bridge_command(12345, true, adapter, false)
                .unwrap();
            assert_eq!(command.get_args().last().unwrap(), "--seed-read-only");
        }
        assert!(super::J2534Adapter::parse("chipsoft-pro & anything").is_err());
    }
    #[test]
    fn seatbelt_bridge_argument_is_explicit_and_exclusive() {
        let conn = super::Connection {
            target: "user@host".into(),
            control_path: None,
            ecu_profile: super::EcuProfile::default(),
        };
        let cmd = conn
            .native_bridge_command(12345, false, super::J2534Adapter::ChipsoftPro, true)
            .unwrap();
        assert_eq!(cmd.get_args().last().unwrap(), "--seatbelt-audible");
        assert!(conn
            .native_bridge_command(12345, true, super::J2534Adapter::ChipsoftPro, true)
            .is_err());
        assert!(conn
            .native_bridge_command(12345, false, super::J2534Adapter::Nano, true)
            .is_err());
    }
    use super::*;
    fn fixture() -> String {
        // Synthetic VIN: this fixture is not live bench evidence.
        let mut s = String::from("PassThruConnect ISO15765 flags=0 baud=500000 default-HSCAN rc=0x00000000\nREQUEST CAN=7E0 payload=1A 90;\nDRIVER_ACCEPTED_MESSAGES=1 (not evidence of ECU response)\nPassThruStopMsgFilter rc=0x00000000\nPassThruDisconnect rc=0x00000000\nPassThruClose rc=0x00000000\nPROBE_RESULT=0\nHELPER_EXIT=0\n");
        s.push_str("RX status=0x00000000 protocol=6 timestamp_us=1 bytes=00-00-07-E8-5A-90");
        for b in b"YS3FD49Y041000000" {
            s.push_str(&format!("-{b:02X}"));
        }
        s
    }
    #[test]
    fn accepts_complete_ecu_reply_only() {
        let s = fixture();
        assert_eq!(parse_response(&s).unwrap().0, "YS3FD49Y041000000");
        for bad in [
            s.replace("status=0x00000000", "status=0x00000009"),
            s.replace("07-E8", "07-E0"),
            s.replace("PROBE_RESULT=0", "PROBE_RESULT=1"),
            s.replace("PassThruClose rc=0x00000000", "PassThruClose rc=0x00000001"),
            s.replace("-59-53", "-49-53"),
        ] {
            assert!(parse_response(&bad).is_err());
        }
        assert!(parse_response("ECU_VIN=YS3FD49Y041000000\nPROBE_RESULT=0").is_err());
    }
    #[test]
    fn identity_binary_version_is_not_misrepresented_as_text() {
        let log = "REQUEST CAN=7E0 payload=1A 95;\nRX status=0x00000000 protocol=6 timestamp_us=1 bytes=00-00-07-E8-5A-95-00-0B\nPassThruReadVersion rc=0x00000000\nADAPTER_VERSION firmware=1.9.4.2 VCX-NANO dll=04.04.240321 api=04.04\n";
        let (ids, adapter) = identity_details(log).unwrap();
        assert_eq!(ids[0].bytes, [0, 11]);
        assert_eq!(ids[0].value, "00 0B (hex)");
        assert_eq!(adapter.unwrap().firmware, "1.9.4.2 VCX-NANO");
        let conflict = format!(
            "{log}RX status=0x00000000 protocol=6 timestamp_us=2 bytes=00-00-07-E8-5A-95-00-0C\n"
        );
        assert!(identity_details(&conflict).is_err());
        let (ids, _) =
            identity_details(&log.replace("status=0x00000000", "status=0x00000009")).unwrap();
        assert!(ids.is_empty());
        let (_, adapter) = identity_details(&log.replace(
            "PassThruReadVersion rc=0x00000000",
            "PassThruReadVersion rc=0x00000001",
        ))
        .unwrap();
        assert!(adapter.is_none());
    }

    #[test]
    fn me96_profile_requires_matching_ecu_and_complete_positive_replies() {
        let mut log = String::new();
        for (pid, data) in [
            (0x90, b"YS3FH46U681000002".as_slice()),
            (0x97, b"BOSCH_ME96".as_slice()),
            (0x9a, &[3, 10]),
            (0xc1, b"55566297".as_slice()),
            (0xc2, b"55571124".as_slice()),
            (0xc3, b"55566296".as_slice()),
            (0xcb, &[3, 79, 200, 87]),
        ] {
            log.push_str(&format!("REQUEST CAN=7E0 payload=1A {pid:02X};\nRX status=0x00000000 protocol=6 timestamp_us=1 bytes=00-00-07-E8-5A-{pid:02X}"));
            for byte in data {
                log.push_str(&format!("-{byte:02X}"));
            }
            log.push('\n');
        }
        let (identities, _) = identity_details(&log).unwrap();
        let mut result = VinResult {
            vin: "YS3FH46U681000002".into(),
            response: vec![],
            log_path: PathBuf::new(),
            identities,
            adapter: None,
        };
        assert!(EcuProfile::Me96Vehicle1367.verify(&result).is_ok());
        assert!(EcuProfile::Trionic8.verify(&result).is_err());
        result.vin = "YS3FD49Y041000000".into();
        assert!(EcuProfile::Me96Vehicle1367.verify(&result).is_err());
        result.vin = "YS3FH46U681000002".into();
        result.identities[1].bytes = b"OTHER_ECU".to_vec();
        assert!(EcuProfile::Me96Vehicle1367.verify(&result).is_err());
        result.identities = identity_details(&log.replace("5A-C3", "5A-EF")).unwrap().0;
        assert!(EcuProfile::Me96Vehicle1367.verify(&result).is_err());
        assert!(
            identity_details(&log.replace("status=0x00000000", "status=0x00000009"))
                .unwrap()
                .0
                .is_empty()
        );
    }

    #[test]
    fn ibus_reply_requires_correct_channel_address_and_pin_setup() {
        let log = fixture()
            .replace(
                "PassThruConnect ISO15765 flags=0 baud=500000 default-HSCAN",
                "PassThruConnect SW_ISO15765_PS flags=0 baud=33333 pin1",
            )
            .replace("CAN=7E0", "CAN=242")
            .replace("protocol=6 ", "protocol=32775 ")
            .replace("07-E8", "06-42")
            + "\nSET_CONFIG J1962_PINS rc=0x00000000\n";
        assert!(parse_response_for(&log, EcuProfile::IbusBcm1367).is_ok());
        assert!(parse_response(&log).is_err());
        for bad in [
            log.replace("protocol=32775", "protocol=6"),
            log.replace("06-42", "07-E8"),
            log.replace("CAN=242", "CAN=7E0"),
            log.replace(
                "SET_CONFIG J1962_PINS rc=0x00000000",
                "SET_CONFIG J1962_PINS rc=0x00000001",
            ),
            log.replace("status=0x00000000", "status=0x00000009"),
        ] {
            assert!(parse_response_for(&bad, EcuProfile::IbusBcm1367).is_err());
        }
        let mut result = VinResult {
            vin: "YS3FH46U681000002".into(),
            response: vec![],
            log_path: PathBuf::new(),
            adapter: None,
            identities: EcuProfile::IbusBcm1367
                .pids()
                .iter()
                .map(|pid| IdentityValue {
                    pid: *pid,
                    bytes: vec![],
                    value: if *pid == 0x97 {
                        "BCM".into()
                    } else {
                        "raw".into()
                    },
                })
                .collect(),
        };
        assert!(EcuProfile::IbusBcm1367.verify(&result).is_ok());
        result.identities[1].value = "CIM".into();
        assert!(EcuProfile::IbusBcm1367.verify(&result).is_err());
    }

    #[test]
    fn seed_parser_rejects_incomplete_stale_wrong_level_and_key_traffic() {
        let mut log = fixture()
            .replace(
                "PassThruConnect ISO15765 flags=0 baud=500000 default-HSCAN",
                "PassThruConnect SW_ISO15765_PS flags=0 baud=33333 pin1",
            )
            .replace("CAN=7E0", "CAN=242")
            .replace("protocol=6 ", "protocol=32775 ")
            .replace("07-E8", "06-42");
        let vin_hex = |vin: &[u8]| vin.iter().map(|b| format!("-{b:02X}")).collect::<String>();
        log = log.replace(
            &vin_hex(b"YS3FD49Y041000000"),
            &vin_hex(b"YS3FH46U681000002"),
        );
        log.push_str("\nSET_CONFIG J1962_PINS rc=0x00000000\n");
        for (pid, data) in [
            (0x97, b"BCM".as_slice()),
            (0x9a, &[1, 14]),
            (0xc1, &[0, 195, 248, 156]),
        ] {
            log.push_str(&format!("REQUEST CAN=242 payload=1A {pid:02X};\nRX status=0x00000000 protocol=32775 timestamp_us=1 bytes=00-00-06-42-5A-{pid:02X}{}\n", vin_hex(data)));
        }
        let request = "REQUEST CAN=242 payload=27 01;";
        let reply =
            "RX status=0x00000000 protocol=32775 timestamp_us=1 bytes=00-00-06-42-67-01-12-34";
        log.push_str(&format!("{request}\n{reply}\n"));
        assert_eq!(parse_bcm_seed(&log).unwrap(), [0x12, 0x34]);
        for bad in [
            log.replace("67-01-12-34", "67-01-12"),
            log.replace("67-01", "67-0B"),
            log.replace(reply, &reply.replace("06-42", "06-41")),
            log.replace(
                reply,
                &reply.replace("status=0x00000000", "status=0x00000009"),
            ),
            log.replace(request, "REQUEST CAN=242 payload=27 02;"),
            log.replace("PassThruClose rc=0x00000000", "PassThruClose rc=0x00000001"),
            format!("{log}{request}\n"),
            format!("{log}REQUEST CAN=242 payload=27 02 12 34;\n"),
            format!("{log}{}\n", reply.replace("12-34", "56-78")),
            log.replace(
                &format!("{request}\n{reply}"),
                &format!("{reply}\n{request}"),
            ),
        ] {
            assert!(parse_bcm_seed(&bad).is_err());
        }
    }

    #[test]
    fn ssh_target_and_encoding() {
        for target in ["-oProxyCommand=evil", "user@host;echo", "user@host\n"] {
            assert!(Connection {
                target: target.into(),
                control_path: None,
                ecu_profile: EcuProfile::default(),
            }
            .validate()
            .is_err());
        }
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
    }
    #[test]
    fn live_lines_preserve_partial_writes_and_enforce_limit() {
        let (tx, rx) = mpsc::channel();
        let mut tail = LogTail::new(std::io::Cursor::new(b"CALL par".to_vec()));
        tail.drain(&tx, false).unwrap();
        assert!(rx.try_recv().is_err());
        tail.reader
            .get_mut()
            .extend_from_slice(b"tial\r\nRX complete\nlast");
        tail.drain(&tx, false).unwrap();
        assert_eq!(
            rx.try_iter().collect::<Vec<_>>(),
            ["CALL partial", "RX complete"]
        );
        tail.drain(&tx, true).unwrap();
        assert_eq!(rx.try_recv().unwrap(), "last");
        let mut too_big = LogTail::new(std::io::Cursor::new(vec![b'x'; 65537]));
        assert!(too_big.drain(&tx, false).unwrap_err().contains("64 KiB"));
    }

    #[cfg(unix)]
    #[test]
    fn progress_arrives_while_helper_is_running() {
        let path = std::env::temp_dir().join(format!("vcx-stream-{}", std::process::id()));
        let worker_path = path.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        let (tx, rx) = mpsc::channel();
        let worker = thread::spawn(move || {
            let mut command = Command::new("sh");
            command.args(["-c", "printf 'CALL before wait\n'; exec sleep 10"]);
            run_command(
                command,
                EcuProfile::Trionic8,
                &worker_path,
                &worker_cancel,
                Duration::from_secs(3),
                &tx,
            )
        });
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(2)).unwrap(),
            "CALL before wait"
        );
        assert!(!worker.is_finished());
        cancel.store(true, Ordering::Release);
        assert!(worker.join().unwrap().unwrap_err().contains("cancelled"));
        fs::remove_dir_all(path).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn timeout_reaps_a_blocked_local_helper() {
        let path = std::env::temp_dir().join(format!("vcx-timeout-{}", std::process::id()));
        let mut command = Command::new("sh");
        command.args(["-c", "exec sleep 10"]);
        let start = Instant::now();
        let error = run_command(
            command,
            EcuProfile::Trionic8,
            &path,
            &AtomicBool::new(false),
            Duration::from_millis(100),
            &mpsc::channel().0,
        )
        .unwrap_err();
        assert!(error.contains("timed out"));
        assert!(start.elapsed() < Duration::from_secs(3));
        fs::remove_dir_all(path).unwrap();
    }
}
