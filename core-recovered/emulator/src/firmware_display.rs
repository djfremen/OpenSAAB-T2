// SPDX-License-Identifier: MPL-2.0
//! Recovered SF abs.w host display signals and genuine card metadata.
//! Original SF executes normally; only the OEM handshake updates D0. No PID
//! names, values or ECU success are invented. The >5 gate is OEM DTC-only.
use crate::recovered_memory::GuestMemory;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
#[derive(Clone, Debug)]
struct Table {
    offset: usize,
    rows: usize,
}
#[derive(Clone, Debug)]
pub struct DataRow {
    pub category: u8,
    pub epid: u32,
    pub kind: u8,
    pub precision: u8,
    pub enum_group: u16,
    pub conversion: u16,
    pub raw_bits: u32,
    pub value_known: bool,
    pub last_observed: bool,
    pub format_supported: bool,
    pub name: String,
    pub unit: String,
    pub value: String,
}
impl DataRow {
    fn invalidate(&mut self) {
        self.last_observed |= self.value_known;
        self.value_known = false;
        self.value = "*".into();
    }
}
#[derive(Clone, Debug)]
pub struct FirmwareDisplay {
    card: Arc<[u8]>,
    definitions: Vec<Table>,
    enums: Vec<Table>,
    strings: Vec<Table>,
    string_base: usize,
    common: usize,
    extended: usize,
    new_extracts: bool,
    parameter_hooks: bool,
    pub tables_valid: bool,
    pub signal_version: u32,
    pub data_enabled: bool,
    pub data_active: bool,
    pub data_application: bool,
    pub dtc_application: bool,
    pub units: u32,
    pub list_anchor: u32,
    pub value_table: u32,
    pub rows: Vec<DataRow>,
    pub notice: String,
    pub generation: u64,
    pub pointer_rejections: u64,
    pub signal_counts: BTreeMap<u16, u64>,
}
impl FirmwareDisplay {
    pub fn new(card: Arc<[u8]>) -> Self {
        let mut display = Self {
            card,
            definitions: vec![],
            enums: vec![],
            strings: vec![],
            string_base: 0,
            common: 0,
            extended: 0,
            new_extracts: false,
            parameter_hooks: false,
            tables_valid: false,
            signal_version: 0,
            data_enabled: false,
            data_active: false,
            data_application: false,
            dtc_application: false,
            units: 0,
            list_anchor: 0,
            value_table: 0,
            rows: vec![],
            notice: String::new(),
            generation: 0,
            pointer_rejections: 0,
            signal_counts: BTreeMap::new(),
        };
        display.tables_valid = display.load_tables();
        if !display.tables_valid {
            display.notice = "Required firmware display extracts are unavailable or invalid".into();
        }
        display
    }
    pub fn available(&self) -> bool {
        self.tables_valid && self.signal_version > 2
    }
    /// Does not affect Rust DTC queries or saved reports.
    pub fn oem_dtc_available(&self) -> bool {
        self.available() && self.signal_version > 5
    }
    pub fn set_data_enabled(&mut self, enabled: bool) -> bool {
        if enabled && !self.data_enabled && (self.data_application || self.dtc_application) {
            self.notice = "Enable Data Display before entering the data or DTC application".into();
            return false;
        }
        self.data_enabled = enabled;
        if !enabled {
            self.rows.clear();
            self.data_active = false;
            self.parameter_hooks = false;
        }
        self.generation += 1;
        true
    }
    fn c16(&self, p: usize) -> Option<u16> {
        self.card
            .get(p..p.checked_add(2)?)
            .map(|b| u16::from_be_bytes(b.try_into().unwrap()))
    }
    fn c32(&self, p: usize) -> Option<u32> {
        self.card
            .get(p..p.checked_add(4)?)
            .map(|b| u32::from_be_bytes(b.try_into().unwrap()))
    }
    fn tables(&self, file: (usize, usize), widths: &[usize]) -> Option<Vec<Table>> {
        let (base, size) = file;
        let end = base.checked_add(size)?;
        if end > self.card.len()
            || self.c32(base)? as usize != widths.len()
            || base + 4 + 10 * widths.len() > end
        {
            return None;
        }
        widths
            .iter()
            .enumerate()
            .map(|(n, &width)| {
                let p = base + 4 + n * 10;
                let offset = base.checked_add(self.c32(p)? as usize)?;
                let rows = self.c32(p + 6)? as usize;
                if self.c16(p + 4)? as usize != width
                    || offset > end
                    || rows.checked_mul(width)? > end - offset
                {
                    return None;
                }
                Some(Table { offset, rows })
            })
            .collect()
    }
    fn load_tables(&mut self) -> bool {
        let mut files = BTreeMap::new();
        let mut font = false;
        for n in 0..=1000 {
            let p = 24 + n * 38;
            if p + 38 > self.card.len() || p >= 0x20000 || self.c16(p) == Some(0xffff) {
                break;
            }
            let name = String::from_utf8_lossy(&self.card[p..p + 14])
                .split(['\0', ' '])
                .next()
                .unwrap_or("")
                .to_ascii_lowercase();
            let Some(bank) = self.c16(p + 14) else {
                return false;
            };
            let Some(within) = self.c32(p + 16) else {
                return false;
            };
            // OEM uses 32-bit wrapping bank arithmetic, including 0xFFE bias.
            let offset = (u32::from(bank)
                .wrapping_add(0xffe)
                .wrapping_mul(0x100000)
                .wrapping_add(within)) as usize;
            let size = self.c32(p + 20).unwrap_or(u32::MAX) as usize;
            if offset
                .checked_add(size)
                .is_none_or(|end| end > self.card.len())
            {
                continue;
            }
            for prefix in ["datadisp", "dispconv.ext", "sev.ext", "strhdr", "strngs"] {
                if name.starts_with(prefix) {
                    files.insert(prefix, (offset, size));
                }
            }
            font |= matches!(name.as_str(), "1252font.dwn" | "1252thin.dwn");
        }
        let parsed = (|| -> Option<_> {
            self.tables(
                *files.get("datadisp")?,
                &[
                    4, 8, 4, 2, 10, 2, 6, 2, 10, 4, 4, 10, 6, 2, 2, 1, 6, 6, 6, 4,
                ],
            )?;
            let definitions = self.tables(*files.get("dispconv.ext")?, &[16, 4, 4])?;
            let enums = self.tables(*files.get("sev.ext")?, &[10, 2, 4])?;
            let strings = self.tables(*files.get("strhdr")?, &[4, 2, 4, 2, 1])?;
            let (base, size) = *files.get("strngs")?;
            if !font || size < if self.new_extracts { 12 } else { 8 } || self.card[base] != 0 {
                return None;
            }
            let common = if self.new_extracts {
                self.c32(base + 4)? as usize
            } else {
                self.c16(base + 4)? as usize
            };
            let extended = if self.new_extracts {
                self.c32(base + 8)? as usize
            } else {
                self.c16(base + 6)? as usize
            };
            if common == 0 {
                return None;
            }
            Some((definitions, enums, strings, base, common, extended))
        })();
        if let Some((d, e, s, b, c, x)) = parsed {
            self.definitions = d;
            self.enums = e;
            self.strings = s;
            self.string_base = b;
            self.common = c;
            self.extended = x;
            true
        } else {
            false
        }
    }
    fn character(byte: u8) -> char {
        const CP1252: [u32; 32] = [
            0x20ac, 0x81, 0x201a, 0x192, 0x201e, 0x2026, 0x2020, 0x2021, 0x2c6, 0x2030, 0x160,
            0x2039, 0x152, 0x8d, 0x17d, 0x8f, 0x90, 0x2018, 0x2019, 0x201c, 0x201d, 0x2022, 0x2013,
            0x2014, 0x2dc, 0x2122, 0x161, 0x203a, 0x153, 0x9d, 0x17e, 0x178,
        ];
        char::from_u32(if (0x80..0xa0).contains(&byte) {
            CP1252[(byte - 0x80) as usize]
        } else {
            u32::from(byte)
        })
        .unwrap_or('\u{fffd}')
    }
    fn compressed(&self, mut p: usize, depth: usize) -> Option<String> {
        if depth > 24 {
            return None;
        }
        let mut out = String::new();
        for _ in 0..256 {
            let byte = *self.card.get(p)?;
            p += 1;
            if byte == 0 || byte == 0xff {
                return Some(out);
            }
            if byte == 3 {
                let index = self.c16(p)? as usize;
                p += 2;
                let table = &self.strings[2];
                let offsets = &self.strings[3];
                let data = &self.strings[4];
                if index >= offsets.rows || index >> 8 >= table.rows {
                    return None;
                }
                let target = data
                    .offset
                    .checked_add(self.c32(table.offset + (index >> 8) * 4)? as usize)?
                    .checked_add(self.c16(offsets.offset + index * 2)? as usize)?;
                if target >= data.offset + data.rows {
                    return None;
                }
                out.push_str(&self.compressed(target, depth + 1)?);
            } else {
                out.push(Self::character(byte));
            }
            if out.len() > 2048 {
                return None;
            }
        }
        None
    }
    pub fn firmware_string(&self, reference: u32) -> Option<String> {
        if !self.tables_valid || self.strings.len() != 5 {
            return None;
        }
        let reference = if reference < 0x40000000 {
            if reference == 0 || reference as usize > self.common {
                return None;
            }
            reference as usize
        } else {
            let x = (reference & 0x3fffffff) as usize;
            if x >= self.extended {
                return None;
            }
            x.checked_add(self.common)?
        };
        let index = reference.checked_sub(1)?;
        if index >= self.strings[1].rows || index / 250 >= self.strings[0].rows {
            return None;
        }
        let offset = self.c16(self.strings[1].offset + index * 2)?;
        if offset == 0xffff {
            return None;
        }
        let p = self
            .string_base
            .checked_add(self.c32(self.strings[0].offset + (index / 250) * 4)? as usize)?
            .checked_add((offset & 0x7fff) as usize)?;
        self.compressed(p, 0)
    }
    fn enum_text(&self, group: u16, value: u32) -> Option<String> {
        if self.enums.len() != 3 {
            return None;
        }
        let group = usize::from(group);
        if group + 1 >= self.enums[0].rows {
            return None;
        }
        let p = self.enums[0].offset + group * 10;
        let first = self.c32(p)? as usize;
        let last = self.c32(p + 10)? as usize;
        let fallback = self.c32(p + 4)?;
        if last < first || last > self.enums[2].rows {
            return self.firmware_string(fallback);
        }
        if self.c16(p + 8)? != 0 {
            if (value as usize) < last - first {
                return self.firmware_string(
                    self.c32(self.enums[2].offset + (first + value as usize) * 4)?,
                );
            }
        } else {
            for n in first..last.min(self.enums[1].rows) {
                if u32::from(self.c16(self.enums[1].offset + n * 2)?) == value {
                    return self.firmware_string(self.c32(self.enums[2].offset + n * 4)?);
                }
            }
        }
        self.firmware_string(fallback)
    }
    pub fn describe(&self, category: u8, epid: u32) -> Option<DataRow> {
        if !self.tables_valid || self.definitions.len() != 3 {
            return None;
        }
        for n in 0..self.definitions[0].rows {
            let p = self.definitions[0].offset + n * 16;
            let flags = self.card[p + 4];
            if self.c32(p)? != epid || (flags & 0x20) != (if category == 2 { 0x20 } else { 0 }) {
                continue;
            }
            let kind = flags & 31;
            let enum_group = self.c16(p + 8)?;
            let mut unit = self.c16(p + 6)? as usize;
            if self.units != 0 && flags & 0x40 != 0 {
                unit += 1;
            }
            let unit = if unit < self.definitions[1].rows {
                self.firmware_string(self.c32(self.definitions[1].offset + unit * 4)?)
                    .unwrap_or_default()
            } else {
                String::new()
            };
            return Some(DataRow {
                category,
                epid,
                kind,
                precision: self.card[p + 5] >> 4,
                enum_group,
                conversion: if kind == 5 {
                    enum_group
                } else {
                    u16::from(self.card[p + 9])
                },
                raw_bits: 0,
                value_known: false,
                last_observed: false,
                format_supported: true,
                name: self.firmware_string(self.c32(p + 10)?).unwrap_or_default(),
                unit,
                value: "*".into(),
            });
        }
        None
    }
    fn update(&mut self, category: u8, epid: u32, raw: u32) {
        let Some(index) = self
            .rows
            .iter()
            .position(|r| r.category == category && r.epid == epid)
        else {
            return;
        };
        let row = &self.rows[index];
        let mut supported = true;
        let value = match row.kind {
            1 => {
                let p = row.precision.min(4);
                let v = i64::from(raw as i32);
                let magnitude = v.unsigned_abs();
                let scale = 10u64.pow(u32::from(4 - p));
                let rounded = (magnitude + scale / 2) / scale;
                let decimals = 10u64.pow(u32::from(p));
                let sign = if v < 0 { "-" } else { "" };
                if p == 0 {
                    format!("{sign}{rounded}")
                } else {
                    format!(
                        "{sign}{}.{:0width$}",
                        rounded / decimals,
                        rounded % decimals,
                        width = p as usize
                    )
                }
            }
            2 => format!(
                "{:.*}",
                usize::from(row.precision.min(9)),
                f32::from_bits(raw)
            ),
            3 => (raw as i32).to_string(),
            5 => self.enum_text(row.enum_group, raw).unwrap_or_else(|| {
                supported = false;
                "*".into()
            }),
            6 | 13 => format!("{raw:X}"),
            7 => format!(
                "{:0width$b}",
                raw & 0xffff,
                width = if raw > 255 { 16 } else { 8 }
            ),
            10 => format!("{raw:06X}"),
            11 => format!("{raw:04X}"),
            12 => format!("{raw:02X}"),
            _ => {
                supported = false;
                format!("0x{raw:08X}")
            }
        };
        let row = &mut self.rows[index];
        row.raw_bits = raw;
        row.value_known = true;
        row.last_observed = false;
        row.format_supported = supported;
        if self.units != 0 && row.conversion != 0 {
            row.format_supported = false;
            row.value = format!("0x{raw:08X}");
            self.notice = "Raw value retained: alternate-unit conversion is not implemented".into();
        } else {
            row.value = value;
        }
    }
    fn rejected(&mut self) {
        self.pointer_rejections += 1;
        self.notice = "Firmware display pointer outside mapped memory".into();
    }
    fn update_data(&mut self, m: &impl GuestMemory) {
        if !self.data_enabled || !self.data_active || self.dtc_application {
            return;
        }
        for row in &mut self.rows {
            row.invalidate();
        }
        if self.list_anchor == 0 || self.value_table == 0 {
            return;
        }
        let Some(mut p) = m.u32(self.list_anchor) else {
            self.rejected();
            return;
        };
        let mut seen = BTreeSet::new();
        for _ in 0..300 {
            if p == 0 {
                break;
            }
            if !seen.insert(p) {
                self.notice = "Firmware parameter list contains a cycle".into();
                break;
            }
            if m.bytes(p, 0x1c).is_none() {
                self.rejected();
                break;
            }
            if m.u16(p + 0x16) == Some(1) {
                let index = m.u32(p + 0x18).unwrap();
                if index != u32::MAX && index < 0x10000 {
                    if let Some(v) = index
                        .checked_mul(6)
                        .and_then(|v| self.value_table.checked_add(v))
                        .filter(|v| *v <= 0xffffff)
                    {
                        if let Some(flags) = m.u16(v + 4) {
                            if flags & 0xff00 != 0 {
                                if let Some(raw) = m.u32(v) {
                                    self.update(m.u8(p).unwrap(), m.u32(p + 2).unwrap(), raw);
                                }
                            }
                        } else {
                            self.rejected();
                        }
                    } else {
                        self.rejected();
                    }
                }
            }
            p = m.u32(p + 0x12).unwrap();
        }
    }
    /// Called before SF; the CPU still performs its original four-byte store.
    pub fn before_instruction(&mut self, pc: u32, registers: &mut [u32; 16], m: &impl GuestMemory) {
        let pc = pc & 0xffffff;
        if pc >= 0x400000 && !(0x7c0000..0x7ff000).contains(&pc) {
            return;
        }
        if m.u16(pc) != Some(0x51f8) {
            return;
        }
        let Some(signal) = m.u16(pc + 2) else {
            return;
        };
        if !matches!(
            signal,
            0x1003
                | 0x1011
                | 0x101a
                | 0x1100
                | 0x1103
                | 0x1106
                | 0x1108
                | 0x1113
                | 0x1400
                | 0x1403
                | 0x1410
                | 0x1411
                | 0x1501
                | 0x1504
                | 0x1505
                | 0x1506
                | 0x5001
                | 0x5002
        ) {
            return;
        }
        if signal == 0x1003 && self.list_anchor == 0 {
            if pc >= 6 && m.u16(pc - 6).is_some_and(|op| op & 0xf1ff == 0x2079) {
                if let Some(anchor) = m.u32(pc - 4) {
                    if m.bytes(anchor, 4).is_some() {
                        self.list_anchor = anchor;
                    } else {
                        self.rejected();
                    }
                }
            } else {
                self.notice = "Data list effective-address form not recovered".into();
            }
        }
        self.signal(signal, registers, m);
    }
    pub fn signal(&mut self, signal: u16, r: &mut [u32; 16], m: &impl GuestMemory) {
        *self.signal_counts.entry(signal).or_default() += 1;
        self.generation += 1;
        match signal {
            0x101a => {
                self.signal_version = r[0];
                r[0] = 0x800059ec;
            }
            0x1113 => {
                self.new_extracts = true;
                self.tables_valid = self.load_tables();
            }
            0x1501 => {
                if self.available() {
                    self.data_application = true;
                }
            }
            0x1108 => {
                if r[6] == 0x9200004b && self.available() {
                    self.parameter_hooks = true;
                    self.rows.clear();
                    self.data_active = false;
                }
            }
            0x1103 => {
                if self.parameter_hooks && self.available() && !self.dtc_application {
                    self.data_application = true;
                }
                if self.data_enabled
                    && self.parameter_hooks
                    && self.available()
                    && !self.dtc_application
                    && !self
                        .rows
                        .iter()
                        .any(|row| row.category == r[6] as u8 && row.epid == r[5])
                {
                    if self.rows.len() >= 300 {
                        self.notice = "OEM parameter limit of 300 reached".into();
                    } else if let Some(row) = self.describe(r[6] as u8, r[5]) {
                        self.rows.push(row);
                        self.data_active = true;
                        self.notice.clear();
                    } else {
                        self.notice = format!("Firmware EPID metadata unavailable: {}", r[5]);
                    }
                }
            }
            0x1100 => {
                self.rows.clear();
                self.data_active = false;
            }
            0x1011 => {
                self.parameter_hooks = false;
                self.data_application = false;
                self.dtc_application = false;
                self.data_active = false;
                self.rows.clear();
                self.list_anchor = 0;
                self.value_table = 0;
            }
            0x1403 => {
                if let Some(p) = r[14].checked_sub(0x14).and_then(|p| m.u32(p)) {
                    self.value_table = p;
                } else {
                    self.rejected();
                }
            }
            0x1400 => {
                if r[14].checked_sub(0x18).and_then(|p| m.u8(p)) == Some(2) {
                    self.update_data(m);
                }
            }
            0x1410 => {
                self.units = r[0];
                for index in 0..self.rows.len() {
                    if let Some(meta) =
                        self.describe(self.rows[index].category, self.rows[index].epid)
                    {
                        self.rows[index].unit = meta.unit;
                    }
                }
                self.update_data(m);
            }
            0x1411 | 0x5002 => {
                self.dtc_application = r[0] != 0;
                if self.dtc_application {
                    for row in &mut self.rows {
                        row.invalidate();
                    }
                    self.data_active = false;
                } else {
                    self.data_active = self.data_enabled && !self.rows.is_empty();
                }
            }
            0x1106 => {
                if self.data_enabled
                    && self.data_active
                    && !self.dtc_application
                    && (self.value_table == 0 || self.list_anchor == 0)
                {
                    let parsed = (|| {
                        let sp = r[15];
                        let a5 = r[13];
                        Some((
                            m.u8(sp.checked_add(0x17)?)?,
                            m.u32(sp.checked_add(0x18)?)?,
                            m.u16(a5.checked_add(0x16)?)?,
                            m.u32(sp.checked_add(0x1c)?)?,
                        ))
                    })();
                    if let Some((category, epid, valid, p)) = parsed {
                        if valid != 0 {
                            if let Some(raw) = m.u32(p) {
                                self.update(category, epid, raw);
                                return;
                            }
                            self.rejected();
                        }
                        for row in &mut self.rows {
                            if row.category == category && row.epid == epid {
                                row.invalidate();
                            }
                        }
                    } else {
                        for row in &mut self.rows {
                            row.invalidate();
                        }
                        self.rejected();
                    }
                }
            }
            _ => {}
        }
    }
    pub fn invalidate_live(&mut self) {
        for row in &mut self.rows {
            row.invalidate();
        }
        self.data_active = false;
        self.generation += 1;
    }
    pub fn snapshot(&self) -> serde_json::Value {
        let rows:Vec<_>=self.rows.iter().map(|r|serde_json::json!({"category":r.category,"epid":r.epid,"name":r.name,"unit":r.unit,"value":r.value,"value_known":r.value_known,"last_observed":r.last_observed,"raw_bits":r.raw_bits,"format_supported":r.format_supported})).collect();
        serde_json::json!({"tables_valid":self.tables_valid,"available":self.available(),"oem_dtc_available":self.oem_dtc_available(),"signal_version":self.signal_version,"data_enabled":self.data_enabled,"data_active":self.data_active,"data_application":self.data_application,"dtc_application":self.dtc_application,"notice":self.notice,"generation":self.generation,"pointer_rejections":self.pointer_rejections,"signal_counts":self.signal_counts,"data":rows})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recovered_memory::TestMemory;
    fn put16(b: &mut [u8], p: usize, v: u16) {
        b[p..p + 2].copy_from_slice(&v.to_be_bytes());
    }
    fn put32(b: &mut [u8], p: usize, v: u32) {
        b[p..p + 4].copy_from_slice(&v.to_be_bytes());
    }
    fn table_file(widths: &[usize], data: Vec<Vec<u8>>) -> Vec<u8> {
        let mut out = vec![0; 4 + 10 * widths.len()];
        put32(&mut out, 0, widths.len() as u32);
        for (n, (&w, bytes)) in widths.iter().zip(data).enumerate() {
            let p = 4 + n * 10;
            let offset = out.len();
            put32(&mut out, p, offset as u32);
            put16(&mut out, p + 4, w as u16);
            put32(&mut out, p + 6, (bytes.len() / w) as u32);
            out.extend(bytes);
        }
        out
    }
    fn fixture() -> FirmwareDisplay {
        let names = ["Engine speed", "rpm", "°C", "On", "Off"];
        let mut string_data = vec![0; 8];
        put16(&mut string_data, 4, 5);
        let mut offsets = vec![];
        for name in names {
            offsets.extend(((string_data.len() - 8) as u16).to_be_bytes());
            for c in name.chars() {
                string_data.push(c as u8);
            }
            string_data.push(0);
        }
        let widths = [
            4, 8, 4, 2, 10, 2, 6, 2, 10, 4, 4, 10, 6, 2, 2, 1, 6, 6, 6, 4,
        ];
        let data = table_file(&widths, widths.iter().map(|w| vec![0; *w]).collect());
        let mut definition = vec![0; 16];
        put32(&mut definition, 0, 42);
        definition[4] = 1 | 0x40;
        definition[5] = 3 << 4;
        definition[9] = 1;
        put32(&mut definition, 10, 1);
        let conv = table_file(
            &[16, 4, 4],
            vec![
                definition,
                [2u32.to_be_bytes(), 3u32.to_be_bytes()].concat(),
                vec![],
            ],
        );
        let mut enum_group = vec![0; 20];
        put32(&mut enum_group, 10, 2);
        put16(&mut enum_group, 8, 1);
        let enums = table_file(
            &[10, 2, 4],
            vec![
                enum_group,
                vec![0, 0, 0, 1],
                [4u32.to_be_bytes(), 5u32.to_be_bytes()].concat(),
            ],
        );
        let strings = table_file(
            &[4, 2, 4, 2, 1],
            vec![8u32.to_be_bytes().to_vec(), offsets, vec![], vec![], vec![]],
        );
        let mut card = vec![0; 16384];
        let mut offset = 4096;
        for (n, (name, bytes)) in [
            ("datadisp", data),
            ("dispconv.ext", conv),
            ("sev.ext", enums),
            ("strhdr", strings),
            ("strngs", string_data),
            ("1252font.dwn", vec![0]),
        ]
        .into_iter()
        .enumerate()
        {
            let p = 24 + n * 38;
            card[p..p + name.len()].copy_from_slice(name.as_bytes());
            put16(&mut card, p + 14, 2);
            put32(&mut card, p + 16, offset as u32);
            put32(&mut card, p + 20, bytes.len() as u32);
            card[offset..offset + bytes.len()].copy_from_slice(&bytes);
            offset += bytes.len();
        }
        put16(&mut card, 24 + 6 * 38, 0xffff);
        FirmwareDisplay::new(card.into())
    }
    fn active() -> (FirmwareDisplay, [u32; 16], TestMemory) {
        let mut d = fixture();
        assert!(d.tables_valid);
        assert!(d.set_data_enabled(true));
        let mut r = [0; 16];
        let m = TestMemory(vec![0; 4096]);
        r[0] = 3;
        d.signal(0x101a, &mut r, &m);
        r[6] = 0x9200004b;
        d.signal(0x1108, &mut r, &m);
        r[6] = 1;
        r[5] = 42;
        d.signal(0x1103, &mut r, &m);
        assert_eq!(d.rows.len(), 1);
        (d, r, m)
    }
    fn list(m: &mut TestMemory) {
        put32(&mut m.0, 0x100, 0x200);
        m.0[0x200] = 1;
        put32(&mut m.0, 0x202, 42);
        put16(&mut m.0, 0x216, 1);
        put32(&mut m.0, 0x218, 0);
        put32(&mut m.0, 0x300, 123450);
        put16(&mut m.0, 0x304, 0x100);
    }
    #[test]
    fn genuine_metadata_layout_and_oem_handshake_without_values() {
        let (d, r, _) = active();
        assert_eq!(r[0], 0x800059ec);
        assert_eq!(d.rows[0].name, "Engine speed");
        assert_eq!(d.rows[0].unit, "rpm");
        assert!(!d.rows[0].value_known);
        assert_eq!(d.rows[0].value, "*");
        assert!(d.available());
        assert!(!d.oem_dtc_available());
        assert_eq!(d.firmware_string(3).unwrap(), "°C");
    }
    #[test]
    fn valid_to_stale_to_valid_then_removed_list() {
        let (mut d, mut r, mut m) = active();
        list(&mut m);
        d.list_anchor = 0x100;
        d.value_table = 0x300;
        r[0] = 0;
        d.signal(0x1410, &mut r, &m);
        assert_eq!(d.rows[0].value, "12.345");
        assert!(d.rows[0].value_known);
        put16(&mut m.0, 0x304, 0);
        d.signal(0x1410, &mut r, &m);
        assert_eq!(d.rows[0].value, "*");
        assert!(!d.rows[0].value_known);
        assert!(d.rows[0].last_observed);
        assert_eq!(d.rows[0].raw_bits, 123450);
        put16(&mut m.0, 0x304, 0x100);
        d.signal(0x1410, &mut r, &m);
        assert!(d.rows[0].value_known);
        assert!(!d.rows[0].last_observed);
        put32(&mut m.0, 0x100, 0);
        d.signal(0x1410, &mut r, &m);
        assert!(!d.rows[0].value_known);
    }
    #[test]
    fn inactive_sentinel_out_of_range_cycles_and_bad_pointers() {
        let (mut d, mut r, mut m) = active();
        list(&mut m);
        d.list_anchor = 0x100;
        d.value_table = 0x300;
        d.signal(0x1410, &mut r, &m);
        for index in [u32::MAX, 0x10000] {
            put32(&mut m.0, 0x218, index);
            d.signal(0x1410, &mut r, &m);
            assert!(!d.rows[0].value_known);
        }
        put32(&mut m.0, 0x218, 0);
        put16(&mut m.0, 0x216, 0);
        d.signal(0x1410, &mut r, &m);
        assert!(!d.rows[0].value_known);
        put32(&mut m.0, 0x212, 0x200);
        d.signal(0x1410, &mut r, &m);
        assert!(d.notice.contains("cycle"));
        put32(&mut m.0, 0x100, 0xfffffff0);
        d.signal(0x1410, &mut r, &m);
        assert!(d.pointer_rejections > 0);
        assert!(!d.rows[0].value_known);
    }
    #[test]
    fn effective_address_anchor_is_pointer_variable_not_a5_value() {
        let (mut d, mut r, mut m) = active();
        put16(&mut m.0, 0x500, 0x2a79);
        put32(&mut m.0, 0x502, 0x100);
        put16(&mut m.0, 0x506, 0x51f8);
        put16(&mut m.0, 0x508, 0x1003);
        put32(&mut m.0, 0x100, 0x200);
        r[13] = 0x200;
        d.before_instruction(0x506, &mut r, &m);
        assert_eq!(d.list_anchor, 0x100);
        assert_eq!(m.u16(0x506), Some(0x51f8));
    }
    #[test]
    fn fallback_pointer_rejection_clears_current_value() {
        let (mut d, mut r, mut m) = active();
        r[15] = 0x600;
        r[13] = 0x500;
        put16(&mut m.0, 0x516, 1);
        m.0[0x617] = 1;
        put32(&mut m.0, 0x618, 42);
        put32(&mut m.0, 0x61c, 0x300);
        put32(&mut m.0, 0x300, (-123450i32) as u32);
        d.signal(0x1106, &mut r, &m);
        assert_eq!(d.rows[0].value, "-12.345");
        put32(&mut m.0, 0x61c, u32::MAX);
        d.signal(0x1106, &mut r, &m);
        assert!(!d.rows[0].value_known);
        assert!(d.rows[0].last_observed);
        assert!(d.pointer_rejections > 0);
    }
    #[test]
    fn alternate_units_preserve_raw_and_dtc_entry_invalidates_data_only() {
        let (mut d, mut r, mut m) = active();
        list(&mut m);
        d.list_anchor = 0x100;
        d.value_table = 0x300;
        r[0] = 1;
        d.signal(0x1410, &mut r, &m);
        assert_eq!(d.rows[0].unit, "°C");
        assert_eq!(d.rows[0].value, "0x0001E23A");
        assert!(!d.rows[0].format_supported);
        d.signal(0x1411, &mut r, &m);
        assert!(!d.rows[0].value_known);
        assert!(!d.data_active);
        d.signal(0x1011, &mut r, &m);
        assert!(d.rows.is_empty());
        assert_eq!(d.list_anchor, 0);
    }
    #[test]
    fn float_integer_enum_and_unrecognized_formats() {
        let (mut d, _, _) = active();
        d.rows[0].conversion = 0;
        d.rows[0].enum_group = 0;
        for (kind, raw, expected) in [
            (2, 1.25f32.to_bits(), "1.250"),
            (3, (-7i32) as u32, "-7"),
            (5, 0, "On"),
            (5, 1, "Off"),
            (11, 0xab, "00AB"),
            (7, 1, "00000001"),
            (7, 0x1ffff, "1111111111111111"),
            (31, 1, "0x00000001"),
        ] {
            d.rows[0].kind = kind;
            d.update(1, 42, raw);
            assert_eq!(d.rows[0].value, expected);
        }
        assert!(!d.rows[0].format_supported);
    }
    #[test]
    fn invalid_tables_and_late_enable_are_explicit() {
        let mut d = FirmwareDisplay::new(vec![0; 100].into());
        assert!(!d.tables_valid);
        let (mut d2, mut r, m) = active();
        d2.set_data_enabled(false);
        assert!(!d2.set_data_enabled(true));
        r[0] = 6;
        d2.signal(0x101a, &mut r, &m);
        assert!(d2.oem_dtc_available());
        d.signal_version = 6;
        assert!(!d.oem_dtc_available());
    }
    #[test]
    #[ignore = "requires an owner-supplied genuine card; set OPENSAAB_TEST_CARD"]
    fn genuine_card_catalogue() {
        let card = std::fs::read(std::env::var("OPENSAAB_TEST_CARD").expect("OPENSAAB_TEST_CARD"))
            .unwrap();
        let mut d = FirmwareDisplay::new(card.into());
        assert!(d.tables_valid);
        d.signal_version = 3;
        assert!(d.available());
        assert!(!d.oem_dtc_available());
        let mut described = 0;
        let mut named = 0;
        for n in 0..d.definitions[0].rows {
            let p = d.definitions[0].offset + n * 16;
            let category = if d.card[p + 4] & 0x20 != 0 { 2 } else { 1 };
            if let Some(r) = d.describe(category, d.c32(p).unwrap()) {
                described += 1;
                named += usize::from(!r.name.is_empty());
            }
        }
        assert_eq!(described, 4419);
        assert!(named > 2000);
        assert!(d.rows.is_empty());
    }
}
