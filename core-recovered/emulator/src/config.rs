// SPDX-License-Identifier: MPL-2.0
//! Lossless OEM configuration document and validated runtime options.
//! Contracts: recovered config.cpp and MSI parser 0x4011c0. External tools are
//! retained as text; parsing never executes them. Backend selection is metadata,
//! not a substitute for selecting and verifying an adapter transport.
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
};

pub type Environment = BTreeMap<String, String>;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigBackend {
    Pdu,
    J2534,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeypadSide {
    Off,
    Left,
    Right,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    Utf8,
    Utf8Bom,
    Utf16Le,
    Utf16Be,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigPath {
    pub raw: String,
    pub expanded: String,
    pub resolved: PathBuf,
    pub unresolved_environment: bool,
}
#[derive(Debug, Clone)]
pub struct ConfigLine {
    pub source: PathBuf,
    pub line: usize,
    pub raw: String,
    pub flag: String,
    pub value: String,
    pub supported: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenSize {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub keypad_width: u32,
}
impl ScreenSize {
    pub fn resolved_for_desktop(&self, width: u32, height: u32) -> Self {
        let mut size = self.clone();
        if size.width == 0 || size.height == 0 {
            size.width = width.max(1);
            size.height = height.max(1);
            size.keypad_width = size.width / 3;
        }
        size
    }
}
#[derive(Debug, Clone)]
pub struct Tech2WinConfig {
    pub bank0_kb: u32,
    pub card_path: PathBuf,
    pub card_path_explicit: bool,
    pub pcmcia: bool,
    pub flash_write_pc: u64,
    pub ata: bool,
    pub bg_color: u32,
    pub fg_color: u32,
    pub kp_bkc: u32,
    pub kp_key: Option<String>,
    /// Compatibility field; OEM -S actually enables sound.
    pub silent: bool,
    pub sound: bool,
    pub watch_mode: Option<String>,
    pub backend: ConfigBackend,
    pub keypad_side: KeypadSide,
    pub sizes: Vec<ScreenSize>,
    pub screen_handling: u32,
    pub version: String,
    pub paths: BTreeMap<String, ConfigPath>,
    pub options: BTreeMap<String, String>,
    pub lines: Vec<ConfigLine>,
    pub warnings: Vec<String>,
    pub source_path: PathBuf,
    pub source_text: String,
    pub encoding: Encoding,
    environment: Environment,
    loaded_bytes: Option<Vec<u8>>,
    loaded_path: Option<PathBuf>,
}
impl Default for Tech2WinConfig {
    fn default() -> Self {
        Self {
            bank0_kb: 128,
            card_path: "NAO.bin".into(),
            card_path_explicit: false,
            pcmcia: true,
            flash_write_pc: 253558,
            ata: true,
            bg_color: 0xff,
            fg_color: 0xffffff,
            kp_bkc: 0xffffff,
            kp_key: Some("R".into()),
            silent: false,
            sound: false,
            watch_mode: Some("pfrc".into()),
            backend: ConfigBackend::Pdu,
            keypad_side: KeypadSide::Off,
            sizes: vec![ScreenSize {
                name: "640 x 480".into(),
                width: 640,
                height: 480,
                keypad_width: 213,
            }],
            screen_handling: 0,
            version: "1.1".into(),
            paths: BTreeMap::new(),
            options: BTreeMap::new(),
            lines: vec![],
            warnings: vec![],
            source_path: PathBuf::new(),
            source_text: String::new(),
            encoding: Encoding::Utf8,
            environment: Environment::new(),
            loaded_bytes: None,
            loaded_path: None,
        }
    }
}
#[derive(Debug)]
pub struct ConfigSaveResult {
    pub written: bool,
    pub backup_path: Option<PathBuf>,
}
fn unquote(value: &str) -> Result<&str, String> {
    let value = value.trim();
    if value.starts_with('"') {
        value
            .strip_prefix('"')
            .and_then(|v| v.strip_suffix('"'))
            .ok_or_else(|| "Unterminated quoted value".into())
    } else {
        Ok(value)
    }
}
fn absolute_reference(s: &str) -> bool {
    let p = s.replace('\\', "/");
    Path::new(&p).is_absolute()
        || (p.len() >= 3 && p.as_bytes()[0].is_ascii_alphabetic() && &p.as_bytes()[1..3] == b":/")
}
pub fn resolve_path(
    value: &str,
    source: &Path,
    environment: &Environment,
) -> Result<ConfigPath, String> {
    let plain = unquote(value)?;
    if plain.is_empty() {
        return Err("Path requires a value".into());
    }
    let mut expanded = String::new();
    let mut rest = plain;
    let mut unresolved = false;
    while let Some(at) = rest.find('%') {
        expanded.push_str(&rest[..at]);
        rest = &rest[at..];
        let Some(end) = rest[1..].find('%').map(|i| i + 1) else {
            expanded.push_str(rest);
            unresolved = true;
            rest = "";
            break;
        };
        let name = &rest[1..end];
        if let Some((_, value)) = environment
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
        {
            expanded.push_str(value);
        } else {
            expanded.push_str(&rest[..=end]);
            unresolved = true;
        }
        rest = &rest[end + 1..];
    }
    expanded.push_str(rest);
    let portable = expanded.replace('\\', "/");
    let resolved = if absolute_reference(&portable) {
        PathBuf::from(&portable)
    } else {
        source.parent().unwrap_or(Path::new(".")).join(&portable)
    };
    Ok(ConfigPath {
        raw: value.into(),
        expanded,
        resolved,
        unresolved_environment: unresolved,
    })
}
fn number(v: &str, max: u32) -> Result<u32, String> {
    v.parse::<u32>()
        .ok()
        .filter(|n| *n <= max)
        .ok_or_else(|| format!("Expected decimal number in 0..{max}"))
}
fn color(v: &str) -> Result<u32, String> {
    if v.len() != 6 || !v.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err("Expected six RGB hex digits".into());
    }
    u32::from_str_radix(v, 16).map_err(|e| e.to_string())
}
fn screen_size(v: &str) -> Result<ScreenSize, String> {
    let v = v.strip_prefix('"').ok_or("-SS requires a quoted name")?;
    let end = v.find('"').ok_or("-SS name has no closing quote")?;
    let tail = v[end + 1..]
        .trim()
        .strip_prefix(',')
        .ok_or("-SS requires comma-separated dimensions")?;
    let fields: Vec<_> = tail.split(',').map(str::trim).collect();
    if !(2..=3).contains(&fields.len()) {
        return Err("-SS requires width,height[,keypad_width]".into());
    }
    let width = number(fields[0], 6400)?;
    let height = number(fields[1], 4800)?;
    if (width != 0 && width < 320) || (height != 0 && height < 240) {
        return Err("-SS dimensions are smaller than the firmware display".into());
    }
    let keypad_width = if fields.len() == 3 {
        number(fields[2], 4800)?
    } else {
        width / 3
    };
    if fields.len() == 3 && (keypad_width < 240 || keypad_width >= width) {
        return Err("-SS keypad width requires 240..4800, smaller than total width".into());
    }
    Ok(ScreenSize {
        name: v[..end].into(),
        width,
        height,
        keypad_width,
    })
}
fn split_text(text: &str) -> Vec<(&str, &str)> {
    let mut rows = vec![];
    let mut rest = text;
    while !rest.is_empty() {
        if let Some(end) = rest.find(['\r', '\n']) {
            let len = if rest[end..].starts_with("\r\n") {
                2
            } else {
                1
            };
            rows.push((&rest[..end], &rest[end..end + len]));
            rest = &rest[end + len..];
        } else {
            rows.push((rest, ""));
            break;
        }
    }
    rows
}
fn normalized(path: &Path) -> PathBuf {
    fs::canonicalize(path)
        .unwrap_or_else(|_| std::path::absolute(path).unwrap_or_else(|_| path.into()))
}
impl Tech2WinConfig {
    pub fn load(path: &Path) -> Result<Self, String> {
        Self::load_with_environment(path, &std::env::vars().collect())
    }
    pub fn load_with_environment(path: &Path, env: &Environment) -> Result<Self, String> {
        let bytes = fs::read(path).map_err(|e| format!("configuration {}: {e}", path.display()))?;
        let (text, encoding) = decode(&bytes)?;
        let mut cfg = Self::parse_with_environment(path, &text, env)?;
        cfg.encoding = encoding;
        cfg.loaded_bytes = Some(bytes);
        cfg.loaded_path = Some(normalized(path));
        Ok(cfg)
    }
    pub fn parse(path: &Path, text: &str) -> Result<Self, String> {
        Self::parse_with_environment(path, text, &std::env::vars().collect())
    }
    pub fn parse_with_environment(
        path: &Path,
        text: &str,
        env: &Environment,
    ) -> Result<Self, String> {
        let mut cfg = Self {
            source_path: path.into(),
            source_text: text.into(),
            environment: env.clone(),
            ..Self::default()
        };
        cfg.parse_into(
            path,
            text,
            &mut BTreeSet::new(),
            &mut vec![normalized(path)],
        )?;
        Ok(cfg)
    }
    fn parse_into(
        &mut self,
        source: &Path,
        text: &str,
        seen: &mut BTreeSet<String>,
        chain: &mut Vec<PathBuf>,
    ) -> Result<(), String> {
        for (index, (raw, _)) in split_text(text).iter().enumerate() {
            let line = raw.trim();
            let mut parts = line.splitn(2, char::is_whitespace);
            let flag = parts.next().unwrap_or("").to_ascii_uppercase();
            let value = parts.next().unwrap_or("").trim();
            let mut record = ConfigLine {
                source: source.into(),
                line: index + 1,
                raw: (*raw).into(),
                flag: flag.clone(),
                value: value.into(),
                supported: true,
            };
            let parsed = (|| -> Result<(), String> {
                if line.is_empty() || line.starts_with(';') {
                    record.flag.clear();
                    return Ok(());
                }
                if line.starts_with('<') {
                    record.supported = false;
                    return Ok(());
                }
                if flag == "@" {
                    if !matches!(value, "1.0" | "1.1") {
                        return Err("Expected configuration version 1.0 or 1.1".into());
                    }
                    self.version = value.into();
                    return Ok(());
                }
                if flag == "-OF" {
                    let path = resolve_path(value, source, &self.environment)?;
                    if path.unresolved_environment {
                        return Err("-OF has an unresolved environment variable".into());
                    }
                    let key = normalized(&path.resolved);
                    if chain.len() >= 16 || chain.contains(&key) {
                        return Err("Recursive -OF reference".into());
                    }
                    let bytes = fs::read(&path.resolved)
                        .map_err(|e| format!("include {}: {e}", path.resolved.display()))?;
                    let (text, _) = decode(&bytes)?;
                    chain.push(key);
                    self.lines.push(record.clone());
                    self.parse_into(&path.resolved, &text, seen, chain)?;
                    chain.pop();
                    return Ok(());
                }
                if !seen.insert(flag.clone()) {
                    return Err(format!("Option {flag} appears more than once"));
                }
                self.options.insert(flag.clone(), value.into());
                match flag.as_str() {
                    "-P0" | "-P1" | "-A0" | "-A1" | "-KF" | "-UD" | "-AD" => {
                        let path = resolve_path(value, source, &self.environment)?;
                        if path.unresolved_environment {
                            self.warnings.push(format!(
                                "{}:{}: unresolved environment retained in {flag}",
                                source.display(),
                                index + 1
                            ));
                        }
                        if flag == "-P0" {
                            self.card_path = path.resolved.clone();
                            self.card_path_explicit = true;
                        }
                        self.paths.insert(flag.clone(), path);
                    }
                    "-B0" | "-B1" => {
                        let n = number(value, 256)?;
                        if !matches!(n, 128 | 256) {
                            return Err("Bank size must be 128 or 256 KB".into());
                        }
                        if flag == "-B0" {
                            self.bank0_kb = n;
                        }
                    }
                    "-PP" | "-PJ" => {
                        if seen.contains(if flag == "-PP" { "-PJ" } else { "-PP" }) {
                            return Err("-PP and -PJ select different backends".into());
                        }
                        if !value.is_empty() {
                            return Err(format!("{flag} takes no value"));
                        }
                        self.backend = if flag == "-PJ" {
                            ConfigBackend::J2534
                        } else {
                            ConfigBackend::Pdu
                        };
                    }
                    "-KP" => {
                        let side = unquote(value)?;
                        if side.is_empty() {
                            return Err("-KP requires a value".into());
                        }
                        self.kp_key = Some(side.into());
                        self.keypad_side = match side.to_ascii_uppercase().as_str() {
                            "L" => KeypadSide::Left,
                            "R" => KeypadSide::Right,
                            _ => KeypadSide::Off,
                        };
                    }
                    "-SS" => {
                        let size = screen_size(value)?;
                        if !size.name.is_empty() {
                            self.sizes.push(size);
                        }
                    }
                    "-SH" => self.screen_handling = number(value, 15)?,
                    "-WR" => self.flash_write_pc = number(value, 0xffffff)? as u64,
                    "-CB" => self.bg_color = color(value)?,
                    "-CT" => self.fg_color = color(value)?,
                    "-KPBKC" => self.kp_bkc = color(value)?,
                    "-PC" => self.pcmcia = true,
                    "-ATA" => self.ata = true,
                    "-S" => {
                        self.sound = true;
                        self.silent = false;
                    }
                    "-W" => self.watch_mode = Some(value.into()),
                    "-C" => {
                        if number(value, 9)? == 0 {
                            return Err("COM port must be 1..9".into());
                        }
                    }
                    "-TP" | "-TD" => {
                        number(value, 10_000_000)?;
                    }
                    "-MS" | "-L" | "-H" => {
                        unquote(value)?;
                    }
                    "-W0" | "-W1" | "-US" | "-WA" => {}
                    _ => record.supported = false,
                }
                Ok(())
            })();
            parsed.map_err(|e| format!("{}:{}: {e}", source.display(), index + 1))?;
            if flag == "-OF" {
                continue;
            }
            if !record.supported {
                self.warnings.push(format!(
                    "{}:{}: unsupported reference/option retained: {line}",
                    source.display(),
                    index + 1
                ));
            }
            self.lines.push(record);
        }
        Ok(())
    }
    pub fn edited(&self, changes: &BTreeMap<String, String>) -> Result<Self, String> {
        let allowed = [
            "-P0", "-P1", "-KP", "-CB", "-CT", "-KPBKC", "-SS", "-PP", "-PJ",
        ];
        let mut requested = BTreeMap::new();
        for (flag, value) in changes {
            let flag = flag.to_ascii_uppercase();
            if !allowed.contains(&flag.as_str()) {
                return Err(format!("Option {flag} is not editable"));
            }
            if value.contains(['\r', '\n', '\0']) {
                return Err("Edit must contain one text value".into());
            }
            if requested
                .insert(flag.clone(), value.trim().to_owned())
                .is_some()
            {
                return Err(format!("Repeated edit {flag}"));
            }
            if matches!(flag.as_str(), "-PP" | "-PJ") && !value.trim().is_empty() {
                return Err("Backend takes no value".into());
            }
            if flag == "-KP"
                && !value.is_empty()
                && !matches!(
                    unquote(value)?.to_ascii_uppercase().as_str(),
                    "L" | "R" | "OFF"
                )
            {
                return Err("-KP edit expects L, R or OFF".into());
            }
        }
        if requested.contains_key("-PP") && requested.contains_key("-PJ") {
            return Err("Select one backend".into());
        }
        let base =
            Self::parse_with_environment(&self.source_path, &self.source_text, &self.environment)?;
        let mut rows: Vec<(String, String)> = split_text(&self.source_text)
            .into_iter()
            .map(|(b, e)| (b.into(), e.into()))
            .collect();
        let ending = rows
            .iter()
            .find(|(_, e)| !e.is_empty())
            .map(|(_, e)| e.clone())
            .unwrap_or_else(|| "\n".into());
        let final_ending = self.source_text.ends_with(['\r', '\n']);
        let mut removed = BTreeSet::new();
        let mut additions = vec![];
        for (flag, value) in requested {
            let backend = matches!(flag.as_str(), "-PP" | "-PJ");
            let owner = base.lines.iter().find(|l| {
                l.flag == flag || (backend && l.flag == if flag == "-PP" { "-PJ" } else { "-PP" })
            });
            if let Some(owner) = owner {
                if normalized(&owner.source) != normalized(&self.source_path) {
                    return Err(format!(
                        "Cannot edit {flag}: owned by included file {}:{}",
                        owner.source.display(),
                        owner.line
                    ));
                }
                let index = owner.line - 1;
                if !backend && value.is_empty() {
                    removed.insert(index);
                    continue;
                }
                if value == owner.value && flag == owner.flag {
                    continue;
                }
                if flag == "-KP" {
                    let side = |v: &str| -> Result<KeypadSide, String> {
                        Ok(match unquote(v)?.to_ascii_uppercase().as_str() {
                            "L" => KeypadSide::Left,
                            "R" => KeypadSide::Right,
                            _ => KeypadSide::Off,
                        })
                    };
                    if side(&value)? == side(&owner.value)? {
                        continue;
                    }
                }
                if !backend
                    && matches!(flag.as_str(), "-P0" | "-P1" | "-KP")
                    && unquote(&value)? == unquote(&owner.value)?
                {
                    continue;
                }
                if matches!(flag.as_str(), "-CT" | "-CB" | "-KPBKC")
                    && color(&value)? == color(&owner.value)?
                {
                    continue;
                }
                if flag == "-SS" && screen_size(&value)? == screen_size(&owner.value)? {
                    continue;
                }
                let body = &rows[index].0;
                let start = body.len() - body.trim_start().len();
                let token_end = body[start..]
                    .find(char::is_whitespace)
                    .map(|n| n + start)
                    .unwrap_or(body.len());
                if backend {
                    let styled: String = flag
                        .chars()
                        .zip(body[start..token_end].chars())
                        .map(|(c, old)| {
                            if old.is_ascii_lowercase() {
                                c.to_ascii_lowercase()
                            } else {
                                c
                            }
                        })
                        .collect();
                    rows[index].0 = format!("{}{}{}", &body[..start], styled, &body[token_end..]);
                } else {
                    let tail = &body[token_end..];
                    let value_start = token_end + tail.len() - tail.trim_start().len();
                    let trailing = &body[body.trim_end().len()..];
                    let value = if owner.value.starts_with('"')
                        && !value.starts_with('"')
                        && matches!(flag.as_str(), "-P0" | "-P1" | "-KP")
                    {
                        format!("\"{value}\"")
                    } else {
                        value
                    };
                    rows[index].0 = format!(
                        "{}{}{value}{trailing}",
                        &body[..token_end],
                        if value_start == token_end {
                            " "
                        } else {
                            &body[token_end..value_start]
                        }
                    );
                }
            } else if backend || !value.is_empty() {
                additions.push(if backend {
                    flag
                } else {
                    format!("{flag} {value}")
                });
            }
        }
        let mut text = String::new();
        for (n, (b, e)) in rows.iter().enumerate() {
            if !removed.contains(&n) {
                text.push_str(b);
                text.push_str(e);
            }
        }
        if !additions.is_empty() && !text.is_empty() && !text.ends_with(['\r', '\n']) {
            text.push_str(&ending);
        }
        for (n, row) in additions.iter().enumerate() {
            text.push_str(row);
            if final_ending || n + 1 < additions.len() {
                text.push_str(&ending);
            }
        }
        let mut result = Self::parse_with_environment(&self.source_path, &text, &self.environment)?;
        result.encoding = self.encoding;
        result.loaded_bytes = self.loaded_bytes.clone();
        result.loaded_path = self.loaded_path.clone();
        Ok(result)
    }
    pub fn save_checked(&self, path: &Path) -> Result<ConfigSaveResult, String> {
        let valid =
            Self::parse_with_environment(&self.source_path, &self.source_text, &self.environment)?;
        if normalized(self.source_path.parent().unwrap_or(Path::new(".")))
            != normalized(path.parent().unwrap_or(Path::new(".")))
        {
            for line in &valid.lines {
                if normalized(&line.source) == normalized(&self.source_path)
                    && ["-P0", "-P1", "-A0", "-A1", "-KF", "-UD", "-AD", "-OF"]
                        .contains(&line.flag.as_str())
                {
                    let p = resolve_path(&line.value, &line.source, &self.environment)?;
                    if p.unresolved_environment || !absolute_reference(&p.expanded) {
                        return Err(format!(
                            "Cannot move relative/unresolved {} reference",
                            line.flag
                        ));
                    }
                }
            }
        }
        let meta = match fs::symlink_metadata(path) {
            Ok(m) => Some(m),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(e.to_string()),
        };
        if meta.as_ref().is_some_and(|m| !m.file_type().is_file()) {
            return Err("Destination must be a regular file, not a link".into());
        }
        let previous = meta
            .as_ref()
            .map(|_| fs::read(path))
            .transpose()
            .map_err(|e| e.to_string())?;
        if self.loaded_path.as_ref() == Some(&normalized(path))
            && previous.as_ref() != self.loaded_bytes.as_ref()
        {
            return Err("Configuration source changed since loading".into());
        }
        let bytes = encode(&self.source_text, self.encoding);
        if previous.as_ref() == Some(&bytes) {
            return Ok(ConfigSaveResult {
                written: false,
                backup_path: None,
            });
        }
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let suffix = format!("{}-{stamp}-{id}", std::process::id());
        let temp = path.with_file_name(format!(
            "{}.stage-{suffix}",
            path.file_name()
                .ok_or("Missing filename")?
                .to_string_lossy()
        ));
        let backup = path.with_file_name(format!(
            "{}.backup-{suffix}",
            path.file_name().unwrap().to_string_lossy()
        ));
        fn write_new(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
            let mut f = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)?;
            f.write_all(bytes)?;
            f.sync_all()
        }
        write_new(&temp, &bytes).map_err(|e| e.to_string())?;
        if fs::read(&temp).map_err(|e| e.to_string())? != bytes {
            return Err("Configuration staging verification failed".into());
        }
        let result = (|| -> Result<ConfigSaveResult, String> {
            if let Some(old) = &previous {
                write_new(&backup, old).map_err(|e| e.to_string())?;
                if fs::read(&backup).map_err(|e| e.to_string())? != *old {
                    return Err("Configuration backup verification failed".into());
                }
            }
            if fs::symlink_metadata(path).is_ok_and(|m| !m.file_type().is_file()) {
                return Err("Destination changed to a link/non-file during save".into());
            }
            let current = fs::read(path).ok();
            if current != previous {
                return Err("Destination changed while staging configuration".into());
            }
            if let Some(m) = meta {
                fs::set_permissions(&temp, m.permissions()).map_err(|e| e.to_string())?;
            }
            if previous.is_none() {
                fs::hard_link(&temp, path).map_err(|e| e.to_string())?;
                fs::remove_file(&temp).map_err(|e| e.to_string())?;
            } else {
                fs::rename(&temp, path).map_err(|e| e.to_string())?;
            }
            if fs::read(path).map_err(|e| e.to_string())? != bytes {
                return Err("Saved configuration verification failed".into());
            }
            #[cfg(unix)]
            fs::File::open(
                path.parent()
                    .filter(|p| !p.as_os_str().is_empty())
                    .unwrap_or(Path::new(".")),
            )
            .and_then(|f| f.sync_all())
            .map_err(|e| e.to_string())?;
            Ok(ConfigSaveResult {
                written: true,
                backup_path: previous.map(|_| backup),
            })
        })();
        if result.is_err() {
            let _ = fs::remove_file(temp);
        }
        result
    }
    pub fn bg_u32(&self) -> u32 {
        self.bg_color
    }
    pub fn fg_u32(&self) -> u32 {
        self.fg_color
    }
    pub fn print_banner(&self) {
        println!(
            "conf: -B0 {} -P0 {} -WR {} backend={:?} keypad={:?}",
            self.bank0_kb,
            self.card_path.display(),
            self.flash_write_pc,
            self.backend,
            self.keypad_side
        );
        for warning in &self.warnings {
            eprintln!("conf: {warning}");
        }
    }
}
fn decode(bytes: &[u8]) -> Result<(String, Encoding), String> {
    let encoding = if bytes.starts_with(&[0xff, 0xfe]) {
        Encoding::Utf16Le
    } else if bytes.starts_with(&[0xfe, 0xff]) {
        Encoding::Utf16Be
    } else if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        Encoding::Utf8Bom
    } else {
        Encoding::Utf8
    };
    let text = match encoding {
        Encoding::Utf16Le | Encoding::Utf16Be => {
            let (pairs, tail) = bytes[2..].as_chunks::<2>();
            if !tail.is_empty() {
                return Err("Truncated UTF-16 configuration".into());
            }
            let words: Vec<_> = pairs
                .iter()
                .map(|w| {
                    if encoding == Encoding::Utf16Le {
                        u16::from_le_bytes(*w)
                    } else {
                        u16::from_be_bytes(*w)
                    }
                })
                .collect();
            String::from_utf16(&words).map_err(|e| e.to_string())?
        }
        _ => String::from_utf8(bytes[if encoding == Encoding::Utf8Bom { 3 } else { 0 }..].to_vec())
            .map_err(|e| e.to_string())?,
    };
    Ok((text, encoding))
}
fn encode(text: &str, encoding: Encoding) -> Vec<u8> {
    match encoding {
        Encoding::Utf8 => text.as_bytes().to_vec(),
        Encoding::Utf8Bom => [&[0xef, 0xbb, 0xbf][..], text.as_bytes()].concat(),
        Encoding::Utf16Le | Encoding::Utf16Be => {
            let mut out = if encoding == Encoding::Utf16Le {
                vec![0xff, 0xfe]
            } else {
                vec![0xfe, 0xff]
            };
            for word in text.encode_utf16() {
                out.extend(if encoding == Encoding::Utf16Le {
                    word.to_le_bytes()
                } else {
                    word.to_be_bytes()
                });
            }
            out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn temp() -> PathBuf {
        static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let p = std::env::temp_dir().join(format!(
            "opensaab-config-{}-{}",
            std::process::id(),
            N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        p
    }
    #[test]
    fn environment_and_windows_paths_keep_directories() {
        let env = Environment::from([(
            "AppData".into(),
            "C:\\Users\\Owner\\AppData\\Roaming".into(),
        )]);
        let cfg = Tech2WinConfig::parse_with_environment(
            Path::new("/configs/card.conf"),
            "-P0 \"%APPDATA%\\pcmcia\\Saab NAO.bin\"",
            &env,
        )
        .unwrap();
        assert_eq!(
            cfg.card_path,
            PathBuf::from("C:/Users/Owner/AppData/Roaming/pcmcia/Saab NAO.bin")
        );
        assert!(!cfg.paths["-P0"].unresolved_environment);
        let missing = resolve_path(
            "%MISSING%\\pcmcia\\card.bin",
            Path::new("/configs/card.conf"),
            &Environment::new(),
        )
        .unwrap();
        assert!(missing.unresolved_environment);
        assert_eq!(
            missing.resolved,
            PathBuf::from("/configs/%MISSING%/pcmcia/card.bin")
        );
        assert_eq!(
            resolve_path(
                "cards\\nested\\card.bin",
                Path::new("/configs/card.conf"),
                &env
            )
            .unwrap()
            .resolved,
            PathBuf::from("/configs/cards/nested/card.bin")
        );
    }
    #[test]
    fn all_four_encodings_roundtrip_unicode_and_untouched_text() {
        let text="@ 1.1\r\n; keep 日本語 🚗\r\n  -P0 \"/images/Saab NAO.bin\"  \r\n<TOOL  command --quoted \"arg\"\r\n-UNKNOWN keep\r\n-PP\r\n";
        let dir = temp();
        let p = dir.join("card.conf");
        for encoding in [
            Encoding::Utf8,
            Encoding::Utf8Bom,
            Encoding::Utf16Le,
            Encoding::Utf16Be,
        ] {
            let bytes = encode(text, encoding);
            fs::write(&p, &bytes).unwrap();
            let cfg = Tech2WinConfig::load_with_environment(&p, &Environment::new()).unwrap();
            assert_eq!(cfg.source_text, text);
            assert_eq!(cfg.encoding, encoding);
            assert!(!cfg.save_checked(&p).unwrap().written);
            assert_eq!(fs::read(&p).unwrap(), bytes);
        }
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn backend_and_layout_oem_semantics() {
        let cfg = Tech2WinConfig::parse(
            Path::new("c.conf"),
            "-Pp\n-KP L\n-SS \"Work area\",0,0\n-SH 9\n-S",
        )
        .unwrap();
        assert_eq!(cfg.backend, ConfigBackend::Pdu);
        assert_eq!(cfg.keypad_side, KeypadSide::Left);
        assert_eq!(
            cfg.sizes[1].resolved_for_desktop(1200, 900).keypad_width,
            400
        );
        assert!(cfg.sound);
        assert!(!cfg.silent);
        assert_eq!(cfg.screen_handling, 9);
        assert!(Tech2WinConfig::parse(Path::new("c"), "-PP\n-PJ").is_err());
    }
    #[test]
    fn editing_preserves_quotes_unknown_lines_and_exact_backup() {
        let dir = temp();
        let p = dir.join("c.conf");
        let text = "; retained\r\n  -p0 \"/old/card.bin\"  \r\n-X opaque\r\n-PJ\r\n";
        let bytes = encode(text, Encoding::Utf16Be);
        fs::write(&p, &bytes).unwrap();
        let cfg = Tech2WinConfig::load(&p).unwrap();
        let edited = cfg
            .edited(&BTreeMap::from([
                ("-P0".into(), "/new/card.bin".into()),
                ("-PP".into(), "".into()),
            ]))
            .unwrap();
        assert_eq!(
            edited.source_text,
            "; retained\r\n  -p0 \"/new/card.bin\"  \r\n-X opaque\r\n-PP\r\n"
        );
        let saved = edited.save_checked(&p).unwrap();
        assert!(saved.written);
        assert_eq!(fs::read(saved.backup_path.unwrap()).unwrap(), bytes);
        assert_eq!(decode(&fs::read(&p).unwrap()).unwrap().1, Encoding::Utf16Be);
        assert!(cfg
            .edited(&BTreeMap::from([("-CB".into(), "ffffff".into())]))
            .unwrap()
            .save_checked(&p)
            .is_err());
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn includes_have_provenance_cycles_and_duplicate_guards() {
        let dir = temp();
        let inc = dir.join("options.conf");
        fs::write(&inc, "-KP L\n-P0 cards/card.bin\n").unwrap();
        let p = dir.join("root.conf");
        fs::write(&p, "-OF options.conf\n-CB 0000FF\n").unwrap();
        let cfg = Tech2WinConfig::load(&p).unwrap();
        assert_eq!(cfg.card_path, dir.join("cards/card.bin"));
        assert_eq!(
            cfg.lines.iter().find(|l| l.flag == "-KP").unwrap().source,
            inc
        );
        assert!(cfg
            .edited(&BTreeMap::from([("-KP".into(), "R".into())]))
            .is_err());
        assert!(cfg.save_checked(&temp().join("moved.conf")).is_err());
        fs::write(&inc, "-OF root.conf").unwrap();
        assert!(Tech2WinConfig::load(&p).unwrap_err().contains("Recursive"));
        fs::write(&inc, "-CB FFFFFF").unwrap();
        assert!(Tech2WinConfig::load(&p)
            .unwrap_err()
            .contains("more than once"));
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn malformed_values_and_injected_edits_fail() {
        for text in [
            "-B0 wrong",
            "-CB BAD",
            "-WR -1",
            "-P0",
            "-SS x,640,480",
            "-SS \"x\",319,480",
            "@ 2.0",
            "-C 0",
            "-PJ value",
        ] {
            assert!(
                Tech2WinConfig::parse(Path::new("c"), text).is_err(),
                "{text}"
            );
        }
        assert!(decode(&[0xff, 0xfe, 0x41]).is_err());
        let c = Tech2WinConfig::parse(Path::new("c"), "-KP R").unwrap();
        assert!(c
            .edited(&BTreeMap::from([("-KP".into(), "R\n-PJ".into())]))
            .is_err());
        assert!(c
            .edited(&BTreeMap::from([("-WR".into(), "1".into())]))
            .is_err());
    }
    #[cfg(unix)]
    #[test]
    fn saving_does_not_follow_destination_symlink() {
        let dir = temp();
        let p = dir.join("c");
        let target = dir.join("target");
        fs::write(&target, b"keep").unwrap();
        std::os::unix::fs::symlink(&target, &p).unwrap();
        let c = Tech2WinConfig::parse(&p, "-CB 0000FF").unwrap();
        assert!(c.save_checked(&p).is_err());
        assert_eq!(fs::read(target).unwrap(), b"keep");
        fs::remove_dir_all(dir).unwrap();
    }
}
