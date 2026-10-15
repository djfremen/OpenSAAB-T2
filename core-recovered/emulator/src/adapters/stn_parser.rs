// SPDX-License-Identifier: MPL-2.0
use crate::candi_cpu::CanFrame;
const MAX_LINE: usize = 256;
#[derive(Default)]
pub(super) struct Parser {
    line: Vec<u8>,
}
pub(super) enum Item {
    Line(String),
    Prompt,
}
impl Parser {
    pub(super) fn feed(&mut self, bytes: &[u8]) -> Result<Vec<Item>, String> {
        let mut items = Vec::new();
        for &b in bytes {
            match b {
                b'>' => {
                    if !self.line.is_empty() {
                        items.push(Item::Line(self.take()?));
                    }
                    items.push(Item::Prompt);
                }
                b'\r' | b'\n' => {
                    if !self.line.is_empty() {
                        items.push(Item::Line(self.take()?));
                    }
                }
                0x20..=0x7e => {
                    if self.line.len() >= MAX_LINE {
                        return Err("STN raw receive line exceeded limit".into());
                    }
                    self.line.push(b);
                }
                _ => return Err("STN raw receive contained non-ASCII data".into()),
            }
        }
        Ok(items)
    }
    fn take(&mut self) -> Result<String, String> {
        String::from_utf8(std::mem::take(&mut self.line)).map_err(|_| "Invalid STN ASCII".into())
    }
}
pub(super) fn frame(line: &str) -> Result<Option<CanFrame>, String> {
    let fields: Vec<_> = line.split_whitespace().collect();
    if fields
        .first()
        .is_none_or(|s| s.len() != 3 || !s.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        return Ok(None);
    }
    if fields.len() < 2 {
        return Err("STN raw frame missing DLC".into());
    }
    let id = u32::from_str_radix(fields[0], 16).map_err(|_| "Invalid STN ID")?;
    let dlc = u8::from_str_radix(fields[1], 16).map_err(|_| "Invalid STN DLC")?;
    if fields[1].len() != 1 || dlc > 8 || fields.len() != 2 + usize::from(dlc) {
        return Err("STN raw DLC/byte count mismatch".into());
    }
    let data = fields[2..]
        .iter()
        .map(|s| {
            if s.len() != 2 {
                Err("Invalid STN byte".into())
            } else {
                u8::from_str_radix(s, 16).map_err(|_| "Invalid STN byte".into())
            }
        })
        .collect::<Result<Vec<_>, String>>()?;
    let f = CanFrame {
        id,
        extended: false,
        rtr: false,
        dlc,
        data,
    };
    f.validate()?;
    Ok(Some(f))
}
