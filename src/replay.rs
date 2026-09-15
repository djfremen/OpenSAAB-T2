// SPDX-License-Identifier: MPL-2.0
//! Deterministic headless keypad replay: instruction count and raw encoder code.
use std::path::Path;
#[derive(Clone, Copy, Debug)]
pub struct ReplayKey {
    pub insns: u64,
    pub code: u8,
}
pub fn load(path: &Path) -> Result<Vec<ReplayKey>, String> {
    let text =
        std::fs::read_to_string(path).map_err(|e| format!("replay {}: {e}", path.display()))?;
    parse(&text).map_err(|e| format!("replay {}: {e}", path.display()))
}
fn parse(text: &str) -> Result<Vec<ReplayKey>, String> {
    let mut keys: Vec<ReplayKey> = Vec::new();
    for (line, row) in text.lines().enumerate() {
        let row = row.split('#').next().unwrap().trim();
        if row.is_empty() {
            continue;
        }
        let fields: Vec<_> = row.split_whitespace().collect();
        let invalid = || {
            format!(
                "line {}: expected nondecreasing instruction count and encoder code 0x00..0x1f",
                line + 1
            )
        };
        if fields.len() != 2 {
            return Err(invalid());
        }
        let insns: u64 = fields[0].parse().map_err(|_| invalid())?;
        let code = if let Some(hex) = fields[1].strip_prefix("0x") {
            u8::from_str_radix(hex, 16)
        } else {
            fields[1].parse()
        }
        .map_err(|_| invalid())?;
        if code > 31 || keys.last().is_some_and(|k| k.insns > insns) {
            return Err(invalid());
        }
        keys.push(ReplayKey { insns, code });
    }
    if keys.is_empty() {
        return Err("no key events".into());
    }
    Ok(keys)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replay_validates_order_codes_and_preserves_same_step_events() {
        assert_eq!(parse("# keys\n10 0x0e\n10 1\n20 0x10").unwrap().len(), 3);
        for text in ["", "10 32", "10 0x40", "20 1\n10 2", "x 2", "10 1 extra"] {
            assert!(parse(text).is_err());
        }
    }
}
