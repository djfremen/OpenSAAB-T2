// SPDX-License-Identifier: MPL-2.0
//! Separate slot-1 ATA/FAT16 storage from the recovered OEM 42Cxxx contract.
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs, io,
    path::{Path, PathBuf},
};
pub const IMAGE_SIZE: usize = 0x1e8c000;
const SECTOR: usize = 512;
const CLUSTER: usize = 0x800;
const ROOT: usize = 0xf600;
const BIAS: usize = 0x12600;
const OEM_TEMPLATE: &[u8; 516] = include_bytes!("capture-template.bin");
/// Resolve parent aliases before creating an export; keep the input tree intact.
pub fn fresh_export_directory(directory: &Path, source: &Path) -> Result<(), String> {
    let parent = fs::canonicalize(
        directory
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new(".")),
    )
    .map_err(|e| e.to_string())?;
    if !source.as_os_str().is_empty() {
        let source = fs::canonicalize(source).map_err(|e| e.to_string())?;
        if parent.starts_with(&source) {
            return Err("Export must be outside the capture source directory".into());
        }
    }
    fs::create_dir(directory).map_err(|e| format!("Export requires a fresh directory: {e}"))
}
pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub fn safe_name(name: &str) -> bool {
    if name.is_empty()
        || name.len() > 255
        || name.ends_with(['.', ' '])
        || name
            .bytes()
            .any(|c| !(32..127).contains(&c) || b"<>:\"/\\|?*".contains(&c))
    {
        return false;
    }
    let stem = name.split('.').next().unwrap().to_ascii_uppercase();
    !matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        && !(stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && (b'1'..=b'9').contains(&stem.as_bytes()[3]))
}
fn le16(b: &[u8], p: usize) -> u16 {
    u16::from_le_bytes(b[p..p + 2].try_into().unwrap())
}
fn le32(b: &[u8], p: usize) -> u32 {
    u32::from_le_bytes(b[p..p + 4].try_into().unwrap())
}
#[derive(Clone, Debug)]
pub struct CaptureFile {
    pub name: String,
    pub bytes: Vec<u8>,
    pub changed: bool,
}
#[derive(Clone, Debug, Default)]
pub struct CaptureStorage {
    pub image: Vec<u8>,
    pub source_directory: PathBuf,
    pub originals: BTreeMap<String, Vec<u8>>,
    pub inserted: bool,
    pub changed_bytes: u64,
    pub unsupported: u64,
    pub out_of_bounds: u64,
    sector_count: u8,
    sector: u8,
    cylinder_low: u8,
    cylinder_high: u8,
    head: u8,
    control: u8,
    cor: u8,
    read_pending: bool,
    read_cursor: u32,
    write_cursor: u32,
}
impl CaptureStorage {
    pub fn load_directory(directory: &Path) -> Result<Self, String> {
        Self::load_directory_with_empty(directory, false)
    }
    /// Explicit new-session storage; no invented captures or changes to input files.
    pub fn load_directory_with_empty(directory: &Path, allow_empty: bool) -> Result<Self, String> {
        let source_directory = fs::canonicalize(directory).map_err(|e| e.to_string())?;
        if !source_directory.is_dir() {
            return Err("Capture input directory unavailable".into());
        }
        let mut storage = Self {
            source_directory,
            image: vec![0; IMAGE_SIZE],
            ..Self::default()
        };
        storage.image[..516].copy_from_slice(OEM_TEMPLATE);
        let mut cluster = 0x26usize;
        let mut entry = 0;
        let names = (0..10)
            .map(|n| format!("CAPTURE{n}.BIN"))
            .chain(std::iter::once("$$CONF$$.DTA".into()));
        for name in names {
            let path = storage.source_directory.join(&name);
            let meta = match fs::symlink_metadata(&path) {
                Ok(m) => m,
                Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e.to_string()),
            };
            if !meta.file_type().is_file() {
                return Err(format!("Capture input must be a regular file: {name}"));
            }
            if meta.len() > IMAGE_SIZE as u64 {
                return Err("Capture input exceeds bounded storage".into());
            }
            let bytes = fs::read(&path).map_err(|e| e.to_string())?;
            if bytes.len() as u64 != meta.len() {
                return Err("Capture input changed during import".into());
            }
            let clusters = bytes.len().div_ceil(CLUSTER).max(1);
            if cluster + clusters > 0x3cf0 || BIAS + (cluster + clusters) * CLUSTER > IMAGE_SIZE {
                return Err("Capture files exceed FAT16 allocation".into());
            }
            let root = ROOT + entry * 32;
            storage.image[root..root + 8].copy_from_slice(&name.as_bytes()[..8]);
            storage.image[root + 8..root + 11].copy_from_slice(&name.as_bytes()[9..12]);
            storage.image[root + 26..root + 28].copy_from_slice(&(cluster as u16).to_le_bytes());
            storage.image[root + 28..root + 32]
                .copy_from_slice(&(bytes.len() as u32).to_le_bytes());
            let start = BIAS + cluster * CLUSTER;
            storage.image[start..start + bytes.len()].copy_from_slice(&bytes);
            for i in 0..clusters {
                let p = SECTOR + (cluster + i) * 2;
                let next = if i + 1 == clusters {
                    0xffff
                } else {
                    (cluster + i + 1) as u16
                };
                storage.image[p..p + 2].copy_from_slice(&next.to_le_bytes());
            }
            storage.originals.insert(name, bytes);
            cluster += clusters;
            entry += 1;
        }
        if storage.originals.is_empty() && !allow_empty {
            return Err("No installed capture/configuration files supplied".into());
        }
        Ok(storage)
    }
    pub fn files(&self) -> Result<Vec<CaptureFile>, String> {
        let b = &self.image;
        if b.len() != IMAGE_SIZE
            || le16(b, 510) != 0xaa55
            || le16(b, 11) != 512
            || b[13] != 4
            || le16(b, 14) != 1
            || b[16] != 2
            || le16(b, 17) != 512
            || le16(b, 22) != 61
        {
            return Err("Guest capture FAT16 geometry is invalid".into());
        }
        let mut result = vec![];
        let mut names = BTreeSet::new();
        for n in 0..512 {
            let r = ROOT + n * 32;
            if b[r] == 0 {
                break;
            }
            if b[r + 11] & 15 == 15 {
                return Err("Capture long filenames unsupported".into());
            }
            if b[r] == 0xe5 || b[r + 11] & 0x18 != 0 {
                continue;
            }
            let base = std::str::from_utf8(&b[r..r + 8])
                .map_err(|_| "Invalid capture filename")?
                .trim_end_matches(' ');
            let ext = std::str::from_utf8(&b[r + 8..r + 11])
                .map_err(|_| "Invalid capture extension")?
                .trim_end_matches(' ');
            let name = if ext.is_empty() {
                base.into()
            } else {
                format!("{base}.{ext}")
            };
            let key = name.to_ascii_uppercase();
            if !safe_name(&name)
                || !names.insert(key.clone())
                || ["CAPTURE-CARD.IMG", "CAPTURE-EXPORT.JSON"].contains(&key.as_str())
            {
                return Err("Unsafe or duplicate capture filename".into());
            }
            let size = le32(b, r + 28) as usize;
            if size > IMAGE_SIZE {
                return Err("Capture file exceeds card".into());
            }
            let mut remaining = size;
            let mut cluster = le16(b, r + 26) as usize;
            let mut seen = BTreeSet::new();
            let mut bytes = Vec::with_capacity(size);
            while remaining > 0 {
                if !(2..0x3cf1).contains(&cluster) || !seen.insert(cluster) {
                    return Err("Invalid or cyclic capture FAT chain".into());
                }
                let p = BIAS + cluster * CLUSTER;
                let take = remaining.min(CLUSTER);
                bytes.extend_from_slice(b.get(p..p + take).ok_or("Capture FAT data exceeds card")?);
                remaining -= take;
                if remaining > 0 {
                    cluster = le16(b, SECTOR + cluster * 2) as usize;
                }
            }
            let changed = self.originals.get(&name) != Some(&bytes);
            result.push(CaptureFile {
                name,
                bytes,
                changed,
            });
        }
        Ok(result)
    }
    /// Even invalid FAT exports the exact raw image and recovery error.
    pub fn export_session(&self, directory: &Path) -> Result<(), String> {
        fresh_export_directory(directory, &self.source_directory)?;
        fs::write(directory.join("capture-card.img"), &self.image).map_err(|e| e.to_string())?;
        let mut entries = vec![];
        let (recovered, error) = match self.files() {
            Ok(f) => (f, None),
            Err(e) => (vec![], Some(e)),
        };
        for f in recovered {
            fs::write(directory.join(&f.name), &f.bytes).map_err(|e| e.to_string())?;
            entries.push(serde_json::json!({"name":f.name,"bytes":f.bytes.len(),"sha256":digest(&f.bytes),"changed":f.changed}));
        }
        let json = serde_json::json!({"representation":"separate guest FAT16 capture card; source unchanged","image_bytes":self.image.len(),"image_sha256":digest(&self.image),"recovery_error":error,"files":entries});
        fs::write(
            directory.join("capture-export.json"),
            serde_json::to_vec_pretty(&json).unwrap(),
        )
        .map_err(|e| e.to_string())
    }
    pub fn reset_device(&mut self) {
        self.sector_count = 0;
        self.sector = 0;
        self.cylinder_low = 0;
        self.cylinder_high = 0;
        self.head = 0;
        self.control = 0;
        self.cor = 0;
        self.read_pending = false;
        self.read_cursor = 0;
        self.write_cursor = 0;
    }
    fn offset(&mut self, cursor: u32) -> Option<usize> {
        let sector = (i64::from(self.cylinder_low) * 4 + i64::from(self.head & 15)) * 32
            + i64::from(self.sector)
            - 1
            - 32;
        let p = sector * 512 + i64::from(cursor);
        if p < 0 || p as usize >= self.image.len() {
            self.out_of_bounds += 1;
            None
        } else {
            Some(p as usize)
        }
    }
    pub fn read8(&mut self, off: u32, bank: u8, scratch: u8) -> u8 {
        if !self.inserted {
            return 0xff;
        }
        if scratch & 1 != 0 {
            return if bank & 1 != 0 {
                self.unsupported += 1;
                0xff
            } else {
                match off {
                    0 => 0x22,
                    2 => 2,
                    6 => 1,
                    1 | 3 | 4 | 5 | 7 => 0,
                    _ => 0xff,
                }
            };
        }
        match off & 15 {
            0 => {
                let p = self.offset(self.read_cursor);
                self.read_cursor = (self.read_cursor + 1) & 511;
                p.map(|p| self.image[p]).unwrap_or(0xff)
            }
            7 => {
                let v = u8::from(self.read_pending);
                self.read_pending = false;
                v
            }
            14 => 8,
            _ => {
                self.unsupported += 1;
                0xff
            }
        }
    }
    pub fn read16(&mut self, off: u32, bank: u8, scratch: u8) -> u16 {
        if !self.inserted {
            0xffff
        } else if scratch & 1 != 0 || bank & 1 != 0 {
            0
        } else {
            match off {
                0 => 0x22,
                2 => 2,
                4 => 0,
                6 => 1,
                _ => 0xffff,
            }
        }
    }
    pub fn write8(&mut self, off: u32, value: u8) {
        if !self.inserted {
            return;
        }
        match off {
            0 => {
                if let Some(p) = self.offset(self.write_cursor) {
                    if self.image[p] != value {
                        self.image[p] = value;
                        self.changed_bytes += 1;
                    }
                }
                if self.write_cursor < IMAGE_SIZE as u32 {
                    self.write_cursor += 1;
                }
            }
            2 => self.sector_count = value,
            3 => self.sector = value,
            4 => self.cylinder_low = value,
            5 => self.cylinder_high = value,
            6 => self.head = value,
            7 => match value {
                0x20 => self.read_pending = true,
                0x30 => self.write_cursor = 0,
                _ => self.unsupported += 1,
            },
            14 => self.control = value,
            0x200 => self.cor = value,
            _ => self.unsupported += 1,
        }
    }
    pub fn write16(&mut self, _off: u32, _value: u16) {
        self.unsupported += 1;
    }
}
#[cfg(test)]
pub mod tests {
    use super::*;
    pub fn fixture() -> CaptureStorage {
        let mut c = CaptureStorage {
            image: vec![0; IMAGE_SIZE],
            ..Default::default()
        };
        c.image[..516].copy_from_slice(OEM_TEMPLATE);
        let name = b"CAPTURE0BIN";
        c.image[ROOT..ROOT + 11].copy_from_slice(name);
        c.image[ROOT + 26..ROOT + 28].copy_from_slice(&38u16.to_le_bytes());
        c.image[ROOT + 28..ROOT + 32].copy_from_slice(&3u32.to_le_bytes());
        c.image[BIAS + 38 * CLUSTER..BIAS + 38 * CLUSTER + 3].copy_from_slice(b"a\0z");
        c.originals.insert("CAPTURE0.BIN".into(), b"a\0z".to_vec());
        c
    }
    fn temp() -> PathBuf {
        static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let p = std::env::temp_dir().join(format!(
            "opensaab-capture-{}-{}",
            std::process::id(),
            N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        p
    }
    #[test]
    fn empty_capture_storage_requires_explicit_permission() {
        let directory=std::env::temp_dir().join(format!("opensaab-empty-capture-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir(&directory).unwrap();
        assert!(CaptureStorage::load_directory(&directory).is_err());
        let storage=CaptureStorage::load_directory_with_empty(&directory,true).unwrap();
        assert_eq!(storage.image.len(),IMAGE_SIZE);assert!(storage.files().unwrap().is_empty());assert!(storage.originals.is_empty());assert!(!storage.inserted);
        assert_eq!(std::fs::read_dir(&directory).unwrap().count(),0);
        std::fs::remove_dir(directory).unwrap();
    }
    #[test]
    fn imported_bytes_are_exact_and_inputs_unchanged() {
        let dir = temp();
        fs::write(dir.join("CAPTURE0.BIN"), b"a\0z").unwrap();
        let c = CaptureStorage::load_directory(&dir).unwrap();
        assert_eq!(c.files().unwrap()[0].bytes, b"a\0z");
        assert!(!c.inserted);
        let out_root = temp();
        let out = out_root.join("export");
        c.export_session(&out).unwrap();
        assert!(c.export_session(&out).is_err());
        assert_eq!(fs::read(dir.join("CAPTURE0.BIN")).unwrap(), b"a\0z");
        assert_eq!(fs::read(out.join("capture-card.img")).unwrap(), c.image);
        fs::remove_dir_all(dir).unwrap();
        fs::remove_dir_all(out_root).unwrap();
    }
    #[test]
    fn damaged_geometry_and_cycles_keep_raw_evidence() {
        let dir = temp();
        let mut c = fixture();
        c.image[13] = 0;
        assert!(c.files().is_err());
        c.export_session(&dir.join("damaged")).unwrap();
        assert_eq!(
            fs::read(dir.join("damaged/capture-card.img")).unwrap(),
            c.image
        );
        let m: serde_json::Value =
            serde_json::from_slice(&fs::read(dir.join("damaged/capture-export.json")).unwrap())
                .unwrap();
        assert!(m["recovery_error"].is_string());
        assert_eq!(m["files"].as_array().unwrap().len(), 0);
        c.image[13] = 4;
        c.image[ROOT + 28..ROOT + 32].copy_from_slice(&4096u32.to_le_bytes());
        c.image[SECTOR + 38 * 2..SECTOR + 38 * 2 + 2].copy_from_slice(&38u16.to_le_bytes());
        assert!(c.files().unwrap_err().contains("cyclic"));
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn native_byte_and_word_io_and_reset_semantics() {
        let mut c = fixture();
        c.inserted = true;
        c.write8(4, 0);
        c.write8(6, 1);
        c.write8(3, 1);
        c.write8(7, 0x20);
        assert_eq!(c.read8(7, 0, 0), 1);
        assert_eq!(c.read8(7, 0, 0), 0);
        assert_eq!(c.read8(0, 0, 0), 0xeb);
        assert_eq!(c.read16(0, 0, 0), 0x22);
        c.write8(7, 0x30);
        c.write8(0, 0xaa);
        assert_eq!(c.image[0], 0xaa);
        let image = c.image.clone();
        c.write16(0, 0xffff);
        assert_eq!(c.image, image);
        assert_eq!(c.unsupported, 1);
        c.reset_device();
        assert_eq!(c.image, image);
        assert!(c.inserted);
    }
    #[test]
    fn unsafe_names_are_rejected() {
        for name in [
            "../CAPTURE.BIN",
            "CON.BIN",
            "lpt1.bin",
            "bad ",
            "a/b",
            "é.bin",
            ".",
            "..",
        ] {
            assert!(!safe_name(name), "{name}");
        }
        assert!(safe_name("$$CONF$$.DTA"));
    }
}
