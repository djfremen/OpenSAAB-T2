// SPDX-License-Identifier: MPL-2.0
//! Tech2Win Configuration File Parser.
//!
//! Parses `.conf` files in the original Tech2Win format:
//!   - Lines starting with `;` are comments
//!   - Lines starting with `<` are external tool references (ignored)
//!   - `-FLAG [VALUE]` sets options
//!
//! Supported flags (subset used by this emulator):
//!   -B0 <n>          PCMCIA bank 0 erase-block size in KB
//!   -P0 <path>       PCMCIA card image path (env vars stripped)
//!   -PC              Enable PCMCIA mode
//!   -WR <n>          Guest PC allowed to write flash/card
//!   -ATA             Enable ATA/IDE mode
//!   -W <mode>        Watch mode
//!   -Pp              Partial-power mode
//!   -CB <RRGGBB>     Background colour (hex RGB, no #)
//!   -CT <RRGGBB>     Text/character colour (hex RGB, no #)
//!   -S               Silent mode
//!   -KP <key>        Keypad start key
//!   -KPBKC <RRGGBB>  Keypad background-key colour

use std::fs;
use std::path::{Path, PathBuf};

/// Parsed Tech2Win configuration.
#[derive(Debug, Clone)]
pub struct Tech2WinConfig {
    /// PCMCIA bank 0 erase-block size in KB (-B0)
    pub bank0_kb: u32,
    /// Resolved PCMCIA card image path (-P0, env vars substituted with local fallback)
    pub card_path: PathBuf,
    pub card_path_explicit: bool,
    /// PCMCIA mode (-PC)
    pub pcmcia: bool,
    /// Guest PC allowed to write flash/card (-WR)
    pub flash_write_pc: u64,
    /// ATA/IDE mode (-ATA)
    pub ata: bool,
    /// Background colour as 0x00RRGGBB (-CB)
    pub bg_color: u32,
    /// Text/foreground colour as 0x00RRGGBB (-CT)
    pub fg_color: u32,
    /// Keypad background-key colour as 0x00RRGGBB (-KPBKC)
    pub kp_bkc: u32,
    /// Keypad start key (-KP)
    pub kp_key: Option<String>,
    /// Silent mode (-S)
    pub silent: bool,
    /// Watch mode string (-W)
    pub watch_mode: Option<String>,
}

impl Default for Tech2WinConfig {
    fn default() -> Self {
        Self {
            bank0_kb: 128,
            card_path: PathBuf::from("NAO.bin"),
            card_path_explicit: false,
            pcmcia: true,
            flash_write_pc: 253558,
            ata: true,
            bg_color: 0x0000_00FF, // blue  (#0000FF)
            fg_color: 0x00FF_FFFF, // white (#FFFFFF)
            kp_bkc: 0x00FF_FFFF,
            kp_key: Some("R".to_string()),
            silent: true,
            watch_mode: Some("pfrc".to_string()),
        }
    }
}

impl Tech2WinConfig {
    /// Load UTF-8 or the UTF-16 format shipped with Tech2Win. Explicit errors
    /// are returned rather than quietly loading defaults.
    pub fn load(path: &Path) -> Result<Self, String> {
        let bytes = fs::read(path).map_err(|e| format!("configuration {}: {e}", path.display()))?;
        let text =
            decode_text(&bytes).map_err(|e| format!("configuration {}: {e}", path.display()))?;
        Self::parse(path, &text)
    }

    fn parse(path: &Path, text: &str) -> Result<Self, String> {
        let mut cfg = Self::default();

        for raw_line in text.lines() {
            let line = raw_line.trim();
            // Skip comments and empty lines
            if line.is_empty() || line.starts_with(';') || line.starts_with('<') {
                continue;
            }

            let mut parts = line.splitn(2, char::is_whitespace);
            let flag = parts.next().unwrap_or("").to_uppercase();
            let value = parts.next().map(|v| v.trim().to_string());

            match flag.as_str() {
                "-B0" => {
                    cfg.bank0_kb = value
                        .as_deref()
                        .and_then(|s| s.parse::<u32>().ok())
                        .filter(|v| matches!(v, 128 | 256))
                        .ok_or_else(|| format!("{}: -B0 expects 128 or 256 KB", path.display()))?;
                }
                "-P0" => {
                    if let Some(raw) = value {
                        cfg.card_path_explicit = true;
                        let raw = raw.trim_matches('"');
                        let cleaned = strip_env_vars(raw).replace('\\', "/");
                        let windows_path =
                            raw.contains('%') || raw.as_bytes().get(1) == Some(&b':');
                        let resolved = PathBuf::from(&cleaned);
                        let resolved = if windows_path {
                            resolved
                                .file_name()
                                .map(PathBuf::from)
                                .ok_or_else(|| format!("{}: -P0 has no filename", path.display()))?
                        } else {
                            resolved
                        };
                        cfg.card_path = if resolved.is_absolute() {
                            resolved
                        } else {
                            path.parent().unwrap_or(Path::new(".")).join(resolved)
                        };
                    }
                }
                "-PC" => cfg.pcmcia = true,
                "-WR" => {
                    cfg.flash_write_pc = value
                        .as_deref()
                        .and_then(|s| s.parse::<u64>().ok())
                        .filter(|v| *v <= 0x00ff_ffff)
                        .ok_or_else(|| {
                            format!("{}: -WR expects a 24-bit guest PC", path.display())
                        })?;
                }
                "-ATA" => cfg.ata = true,
                "-W" => cfg.watch_mode = value,
                "-PP" => {} // partial-power, acknowledged
                "-CB" => {
                    cfg.bg_color = value
                        .as_deref()
                        .and_then(parse_hex_rgb)
                        .ok_or_else(|| format!("{}: -CB expects six hex digits", path.display()))?;
                }
                "-CT" => {
                    cfg.fg_color = value
                        .as_deref()
                        .and_then(parse_hex_rgb)
                        .ok_or_else(|| format!("{}: -CT expects six hex digits", path.display()))?;
                }
                "-KPBKC" => {
                    cfg.kp_bkc = value.as_deref().and_then(parse_hex_rgb).ok_or_else(|| {
                        format!("{}: -KPBKC expects six hex digits", path.display())
                    })?;
                }
                "-KP" => cfg.kp_key = value,
                "-S" => cfg.silent = true,
                _ => {} // unknown flags silently ignored
            }
        }

        Ok(cfg)
    }

    /// Return the background colour in `minifb` 0x00RRGGBB format.
    pub fn bg_u32(&self) -> u32 {
        self.bg_color
    }

    /// Return the foreground colour in `minifb` 0x00RRGGBB format.
    pub fn fg_u32(&self) -> u32 {
        self.fg_color
    }

    /// Print the parsed configuration in the Tech2Win log style.
    pub fn print_banner(&self) {
        let cb = format!("{:06X}", self.bg_color & 0xFFFFFF);
        let ct = format!("{:06X}", self.fg_color & 0xFFFFFF);
        let kp = self.kp_key.as_deref().unwrap_or("?");
        let wm = self.watch_mode.as_deref().unwrap_or("none");
        println!(
            "conf: -B0 {}  -P0 {}  {}  -WR {}",
            self.bank0_kb,
            self.card_path.display(),
            if self.pcmcia { "-PC -ATA" } else { "" },
            self.flash_write_pc,
        );
        println!(
            "      -W {wm} -Pp -CB {cb} -CT {ct} -{} -KP {kp}  (osk ignored)",
            if self.silent { "S" } else { "" }
        );
    }
}

/// Parse a 6-digit hex RGB string like `"0000FF"` → `0x0000_00FFu32`.
fn parse_hex_rgb(s: &str) -> Option<u32> {
    let s = s.trim();
    if s.len() != 6 {
        return None;
    }
    let n = u32::from_str_radix(s, 16).ok()?;
    // Input is RRGGBB → output is 0x00RRGGBB
    let r = (n >> 16) & 0xFF;
    let g = (n >> 8) & 0xFF;
    let b = n & 0xFF;
    Some((r << 16) | (g << 8) | b)
}

/// Remove Windows-style `%VARIABLE%` tokens from a path string.
fn strip_env_vars(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_var = false;
    for ch in s.chars() {
        if ch == '%' {
            in_var = !in_var;
        } else if !in_var {
            out.push(ch);
        }
    }
    out
}

fn decode_text(bytes: &[u8]) -> Result<String, String> {
    if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
        let little = bytes[0] == 0xff;
        let (pairs, tail) = bytes[2..].as_chunks::<2>();
        if !tail.is_empty() {
            return Err("truncated UTF-16 configuration".into());
        }
        let words: Vec<u16> = pairs
            .iter()
            .map(|w| {
                if little {
                    u16::from_le_bytes(*w)
                } else {
                    u16::from_be_bytes(*w)
                }
            })
            .collect();
        String::from_utf16(&words).map_err(|e| e.to_string())
    } else {
        String::from_utf8(
            bytes
                .strip_prefix(&[0xef, 0xbb, 0xbf])
                .unwrap_or(bytes)
                .to_vec(),
        )
        .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shipped_utf16_format_and_paths() {
        let text = "@ 1.1\r\n-B0 128\r\n-P0 %APPDATA%\\pcmcia\\Saab NAO.bin\r\n";
        let mut bytes = vec![0xff, 0xfe];
        for w in text.encode_utf16() {
            bytes.extend_from_slice(&w.to_le_bytes());
        }
        let decoded = decode_text(&bytes).unwrap();
        let cfg = Tech2WinConfig::parse(Path::new("/configs/test.conf"), &decoded).unwrap();
        assert_eq!(cfg.card_path, PathBuf::from("/configs/Saab NAO.bin"));
        let cfg =
            Tech2WinConfig::parse(Path::new("/configs/test.conf"), "-P0 /data/card.bin").unwrap();
        assert_eq!(cfg.card_path, PathBuf::from("/data/card.bin"));
    }
    #[test]
    fn malformed_config_is_not_silently_accepted() {
        assert!(decode_text(&[0xff, 0xfe, 0x41]).is_err());
        for text in ["-B0 wrong", "-CB BAD", "-WR -1"] {
            assert!(Tech2WinConfig::parse(Path::new("test.conf"), text).is_err());
        }
    }
}
