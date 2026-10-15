// SPDX-License-Identifier: MPL-2.0
//! Recovered TRAP11 file ABI. OEM status oddities are intentional; unsafe guest
//! buffers fail before mutation. Logical files never write capture FAT/source.
use crate::{
    capture::{digest, fresh_export_directory, safe_name, CaptureStorage},
    recovered_memory::GuestMemory,
};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
const CAPACITY: usize = 32 * 1024 * 1024;
const ERROR: u32 = 0x2002;
#[derive(Clone, Debug)]
pub struct HostFile {
    pub original: Vec<u8>,
    pub bytes: Vec<u8>,
    pub created: bool,
}
#[derive(Clone, Debug)]
struct Handle {
    name: String,
    offset: usize,
}
#[derive(Clone, Debug, Default)]
pub struct PsosFiles {
    pub files: BTreeMap<String, HostFile>,
    handles: BTreeMap<u32, Handle>,
    pub source: PathBuf,
    pub last_error: String,
    pub calls: u64,
    pub enabled: bool,
    pub initialized: bool,
}
impl PsosFiles {
    pub fn staged(storage: &CaptureStorage) -> Self {
        Self {
            source: storage.source_directory.clone(),
            enabled: true,
            ..Self::default()
        }
    }

    pub fn seed(storage: &CaptureStorage) -> Result<Self, String> {
        let files = storage.files()?;
        let mut session = Self {
            source: storage.source_directory.clone(),
            enabled: true,
            initialized: true,
            ..Self::default()
        };
        let mut total = 0;
        for f in files {
            let name = f.name.to_ascii_uppercase();
            total += f.bytes.len();
            if !safe_name(&name)
                || total > CAPACITY
                || session.files.len() >= 256
                || session.files.contains_key(&name)
            {
                return Err("Unsupported pSOS flat namespace/capacity".into());
            }
            session.files.insert(
                name,
                HostFile {
                    original: f.bytes.clone(),
                    bytes: f.bytes,
                    created: false,
                },
            );
        }
        Ok(session)
    }
    pub fn reset_descriptors(&mut self) {
        self.handles.clear();
    }
    pub fn open_count(&self) -> usize {
        self.handles.len()
    }
    pub fn guest_filename(memory: &impl GuestMemory, p: u32) -> Option<String> {
        let mut bytes = vec![];
        for n in 0..260 {
            let c = memory.u8(p.checked_add(n)?)?;
            if c == 0 {
                break;
            }
            bytes.push(c);
        }
        if bytes.len() == 260 {
            bytes.pop();
        }
        if bytes.len() > 5 && bytes[1] == b'.' && bytes[5] == b'/' {
            bytes.drain(..6);
        }
        let name = String::from_utf8(bytes).ok()?.to_ascii_uppercase();
        safe_name(&name).then_some(name)
    }
    /// Arguments are stack longs at A7+4, even though the opcode is in D0.
    /// Returns None for services outside the recovered file contract.
    pub fn dispatch(&mut self, op: u32, sp: u32, memory: &mut impl GuestMemory) -> Option<u32> {
        let count = match op {
            0x204 | 0x20c | 0x214 => 1,
            0x21c => 2,
            0x208 | 0x20d | 0x210 => 3,
            0x20e | 0x20f => 4,
            _ => return None,
        };
        if !self.enabled {
            return None;
        }
        self.calls += 1;
        self.last_error.clear();
        let mut args = [0; 4];
        for (i, arg) in args.iter_mut().enumerate().take(count) {
            let Some(p) = sp.checked_add(4 + i as u32 * 4) else {
                return Some(ERROR);
            };
            let Some(v) = memory.u32(p) else {
                self.last_error = "pSOS arguments exceed mapped memory".into();
                return Some(ERROR);
            };
            *arg = v;
        }
        let result = match op {
            0x204 | 0x21c => 0,
            0x20c => {
                for n in 0..260 {
                    if memory.u8(args[0].wrapping_add(n)) == Some(0) {
                        break;
                    }
                }
                0
            }
            0x208 => match Self::guest_filename(memory, args[0]) {
                Some(name) if !self.files.contains_key(&name) && self.files.len() < 256 => {
                    self.files.insert(
                        name,
                        HostFile {
                            original: vec![],
                            bytes: vec![],
                            created: true,
                        },
                    );
                    0
                }
                _ => 0x2011,
            },
            0x20d => {
                // OEM resolves the name before writing a possibly aliased handle.
                let name = Self::guest_filename(memory, args[1]);
                let Some(out) = memory.ram_mut(args[0], 4) else {
                    return Some(ERROR);
                };
                out.fill(0);
                let slot = (1..5).find(|s| !self.handles.contains_key(s));
                if let Some(slot) = slot {
                    match name {
                        Some(name)
                            if self.files.contains_key(&name)
                                && !self.handles.values().any(|h| h.name == name) =>
                        {
                            self.handles.insert(slot, Handle { name, offset: 0 });
                            memory
                                .ram_mut(args[0], 4)
                                .unwrap()
                                .copy_from_slice(&slot.to_be_bytes());
                            0
                        }
                        _ => ERROR,
                    }
                } else {
                    0
                } // Native full-slot status is success with handle zero.
            }
            0x20f => {
                if args[2] as usize > CAPACITY || memory.ram_mut(args[3], 4).is_none() {
                    return Some(ERROR);
                }
                let Some(h) = self.handles.get_mut(&args[0]) else {
                    return Some(ERROR);
                };
                let bytes = &self.files[&h.name].bytes;
                let amount = (args[2] as usize).min(bytes.len().saturating_sub(h.offset));
                if amount > 0 && memory.ram_mut(args[1], amount).is_none() {
                    return Some(ERROR);
                }
                // Recovered order: count words first, then the binary byte copy.
                memory
                    .ram_mut(args[3], 4)
                    .unwrap()
                    .copy_from_slice(&(amount as u32).to_be_bytes());
                if amount > 0 {
                    memory
                        .ram_mut(args[1], amount)
                        .unwrap()
                        .copy_from_slice(&bytes[h.offset..h.offset + amount]);
                }
                h.offset += amount;
                0
            }
            0x20e => {
                let Some(h) = self.handles.get_mut(&args[0]) else {
                    return Some(ERROR);
                };
                let base = match args[1] {
                    0 => 0,
                    1 => h.offset,
                    2 => self.files[&h.name].bytes.len(),
                    _ => return Some(ERROR),
                };
                let position = base as i64 + i64::from(args[2] as i32);
                if !(0..=CAPACITY as i64).contains(&position) {
                    ERROR
                } else {
                    h.offset = position as usize;
                    if position == 0 {
                        0
                    } else {
                        self.last_error =
                            "OEM nonzero seek position returns 2002 after moving".into();
                        ERROR
                    }
                }
            }
            0x210 => {
                let Some(h) = self.handles.get(&args[0]) else {
                    return Some(ERROR);
                };
                let count = args[2] as usize;
                if count > CAPACITY {
                    return Some(ERROR);
                }
                if count == 0 {
                    return Some(0);
                }
                let Some(input) = memory.bytes(args[1], count) else {
                    return Some(ERROR);
                };
                // Requiring writable RAM also rejects firmware/MMIO as write source.
                let input = input.to_vec();
                if memory.ram_mut(args[1], count).is_none() {
                    return Some(ERROR);
                }
                let end = h.offset + count;
                let total: usize = self.files.values().map(|f| f.bytes.len()).sum();
                let growth = end.saturating_sub(self.files[&h.name].bytes.len());
                if end > CAPACITY || growth > CAPACITY - total {
                    ERROR
                } else {
                    let h = self.handles.get_mut(&args[0]).unwrap();
                    let f = self.files.get_mut(&h.name).unwrap();
                    if f.bytes
                        .try_reserve(end.saturating_sub(f.bytes.len()))
                        .is_err()
                    {
                        return Some(ERROR);
                    }
                    f.bytes.resize(end.max(f.bytes.len()), 0);
                    f.bytes[h.offset..end].copy_from_slice(&input);
                    h.offset = end;
                    0
                }
            }
            0x214 => {
                if self.handles.remove(&args[0]).is_some() {
                    0
                } else {
                    ERROR
                }
            }
            _ => unreachable!(),
        };
        if result != 0 && self.last_error.is_empty() {
            self.last_error = format!("pSOS file service {op:03x} failed ({result:04x})");
        }
        Some(result)
    }
    pub fn export_session(&self, directory: &Path) -> Result<(), String> {
        fresh_export_directory(directory, &self.source)?;
        fs::create_dir(directory.join("current")).map_err(|e| e.to_string())?;
        fs::create_dir(directory.join("originals")).map_err(|e| e.to_string())?;
        let mut entries = vec![];
        for (name, f) in &self.files {
            if !safe_name(name) {
                return Err("Unsafe pSOS export filename".into());
            }
            fs::write(directory.join("current").join(name), &f.bytes).map_err(|e| e.to_string())?;
            if !f.created {
                fs::write(directory.join("originals").join(name), &f.original)
                    .map_err(|e| e.to_string())?;
            }
            entries.push(serde_json::json!({"name":name,"created":f.created,"changed":f.created||f.bytes!=f.original,"sha256":digest(&f.bytes),"original_sha256":(!f.created).then(||digest(&f.original))}));
        }
        let json = serde_json::json!({"kind":"pSOS logical files, separate from ATA FAT image","source":self.source,"initialized":self.initialized,"files":entries,"open_handles":self.open_count(),"calls":self.calls,"last_error":self.last_error});
        fs::write(
            directory.join("file-session.json"),
            serde_json::to_vec_pretty(&json).unwrap(),
        )
        .map_err(|e| e.to_string())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::recovered_memory::TestMemory;
    fn call(s: &mut PsosFiles, m: &mut TestMemory, op: u32, args: &[u32]) -> u32 {
        for (n, a) in args.iter().enumerate() {
            m.0[0x104 + n * 4..0x108 + n * 4].copy_from_slice(&a.to_be_bytes());
        }
        s.dispatch(op, 0x100, m).unwrap()
    }
    fn session() -> (PsosFiles, TestMemory) {
        let s = PsosFiles::seed(&crate::capture::tests::fixture()).unwrap();
        let mut m = TestMemory(vec![0; 4096]);
        m.0[0x200..0x20d].copy_from_slice(b"CAPTURE0.BIN\0");
        (s, m)
    }
    #[test]
    fn exact_abi_open_partial_read_eof_and_close() {
        let (mut s, mut m) = session();
        assert_eq!(call(&mut s, &mut m, 0x20d, &[0x300, 0x200, 0]), 0);
        assert_eq!(m.u32(0x300), Some(1));
        assert_eq!(call(&mut s, &mut m, 0x20f, &[1, 0x400, 10, 0x350]), 0);
        assert_eq!(m.u32(0x350), Some(3));
        assert_eq!(&m.0[0x400..0x403], b"a\0z");
        assert_eq!(call(&mut s, &mut m, 0x20f, &[1, 0xffff_fff0, 3, 0x350]), 0);
        assert_eq!(m.u32(0x350), Some(0));
        assert_eq!(call(&mut s, &mut m, 0x214, &[1]), 0);
        assert_eq!(call(&mut s, &mut m, 0x214, &[1]), ERROR);
    }
    #[test]
    fn sharing_and_full_slots_have_oem_status() {
        let (mut s, mut m) = session();
        assert_eq!(call(&mut s, &mut m, 0x20d, &[0x300, 0x200, 0]), 0);
        assert_eq!(call(&mut s, &mut m, 0x20d, &[0x300, 0x200, 0]), ERROR);
        assert_eq!(m.u32(0x300), Some(0));
        for n in 1..4 {
            let name = format!("CAPTURE{n}.BIN");
            s.files.insert(
                name.clone(),
                HostFile {
                    original: vec![],
                    bytes: vec![],
                    created: true,
                },
            );
            m.0[0x200..0x200 + name.len()].copy_from_slice(name.as_bytes());
            assert_eq!(call(&mut s, &mut m, 0x20d, &[0x300, 0x200, 0]), 0);
        }
        assert_eq!(s.open_count(), 4);
        assert_eq!(call(&mut s, &mut m, 0x20d, &[0x300, 0x200, 0]), 0);
        assert_eq!(m.u32(0x300), Some(0));
    }
    #[test]
    fn sparse_binary_write_seek_oddity_and_reset_retention() {
        let (mut s, mut m) = session();
        assert_eq!(call(&mut s, &mut m, 0x20d, &[0x300, 0x200, 0]), 0);
        assert_eq!(call(&mut s, &mut m, 0x20e, &[1, 0, 5, 0]), ERROR);
        assert_eq!(call(&mut s, &mut m, 0x210, &[1, u32::MAX, 0]), 0);
        assert_eq!(s.files["CAPTURE0.BIN"].bytes.len(), 3);
        m.0[0x400..0x403].copy_from_slice(b"x\0y");
        assert_eq!(call(&mut s, &mut m, 0x210, &[1, 0x400, 3]), 0);
        assert_eq!(s.files["CAPTURE0.BIN"].bytes, b"a\0z\0\0x\0y");
        assert_eq!(call(&mut s, &mut m, 0x20e, &[1, 2, (-8i32) as u32, 0]), 0);
        s.reset_descriptors();
        assert_eq!(s.open_count(), 0);
        assert_eq!(s.files["CAPTURE0.BIN"].bytes, b"a\0z\0\0x\0y");
        assert_eq!(s.files["CAPTURE0.BIN"].original, b"a\0z");
    }
    #[test]
    fn unsafe_buffers_and_paths_do_not_modify_objects() {
        let (mut s, mut m) = session();
        call(&mut s, &mut m, 0x20d, &[0x300, 0x200, 0]);
        let before = s.files["CAPTURE0.BIN"].bytes.clone();
        assert_eq!(call(&mut s, &mut m, 0x210, &[1, 4095, 3]), ERROR);
        assert_eq!(call(&mut s, &mut m, 0x20f, &[1, 4095, 3, 0x350]), ERROR);
        assert_eq!(s.files["CAPTURE0.BIN"].bytes, before);
        m.0[0x200..0x208].copy_from_slice(b"../evil\0");
        assert_eq!(call(&mut s, &mut m, 0x208, &[0x200, 0, 0]), 0x2011);
        assert_eq!(s.files.len(), 1);
        assert!(s.dispatch(0x1, 0x100, &mut m).is_none());
    }
    #[test]
    fn aliased_name_and_read_count_follow_recovered_write_order() {
        let (mut s, mut m) = session();
        assert_eq!(call(&mut s, &mut m, 0x20d, &[0x200, 0x200, 0]), 0);
        assert_eq!(m.u32(0x200), Some(1));
        assert_eq!(call(&mut s, &mut m, 0x20f, &[1, 0x400, 3, 0x400]), 0);
        assert_eq!(&m.0[0x400..0x404], b"a\0z\x03");
    }
    #[test]
    fn virtual_prefix_create_exclusive_and_noop_contracts() {
        let (mut s, mut m) = session();
        m.0[0x200..0x214].copy_from_slice(b"a.bcd/NEWFILE.BIN\0\0\0");
        assert_eq!(call(&mut s, &mut m, 0x208, &[0x200, 0, 0]), 0);
        assert!(s.files.contains_key("NEWFILE.BIN"));
        assert_eq!(call(&mut s, &mut m, 0x208, &[0x200, 0, 0]), 0x2011);
        for (op, args) in [
            (0x204, vec![u32::MAX]),
            (0x20c, vec![u32::MAX]),
            (0x21c, vec![1, 2]),
        ] {
            assert_eq!(call(&mut s, &mut m, op, &args), 0);
        }
    }
}
