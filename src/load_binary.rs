// SPDX-License-Identifier: MPL-2.0
//! Binary file loaders and ROM module restorers.

use std::fs;
use std::path::{Path, PathBuf};

pub fn be_word(b: &[u8], off: usize) -> u16 {
    if off <= b.len().saturating_sub(2) && b.len() >= 2 {
        u16::from_be_bytes(b[off..off + 2].try_into().unwrap())
    } else {
        0
    }
}

pub fn be_long(b: &[u8], off: usize) -> u32 {
    if off <= b.len().saturating_sub(4) && b.len() >= 4 {
        u32::from_be_bytes(b[off..off + 4].try_into().unwrap())
    } else {
        0
    }
}

/// Read a bounded image and report the actual offending path. Never substitute
/// another file for an explicitly selected image.
fn read_image(path: &Path, min: usize, max: usize) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let file = fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut bytes = Vec::new();
    file.take(max as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    if !(min..=max).contains(&bytes.len()) {
        return Err(format!(
            "{}: expected {min}..={max} bytes, got {}",
            path.display(),
            bytes.len()
        ));
    }
    Ok(bytes)
}

fn discover(explicit: Option<&Path>, candidates: &[&str]) -> Result<PathBuf, String> {
    if let Some(path) = explicit {
        return Ok(path.to_path_buf());
    }
    candidates
        .iter()
        .map(PathBuf::from)
        .find(|p| p.exists())
        .ok_or_else(|| {
            format!(
                "image not found; tried {}. Supply an explicit image path.",
                candidates.join(", ")
            )
        })
}

pub fn load_boot(explicit: Option<&Path>) -> Result<(PathBuf, Vec<u8>), String> {
    let path = discover(explicit, &["eprom.bin", "extracted/eprom.bin"])?;
    let b = read_image(&path, 0x40000, 0x40000)?;
    let pc = be_long(&b, 4);
    let sp = be_long(&b, 0);
    if pc & 1 != 0 || pc >= 0x40000 || sp & 1 != 0 || sp == 0 || sp == 0xffff_ffff {
        return Err(format!(
            "{}: invalid reset vectors PC={pc:#010x} SP={sp:#010x}",
            path.display()
        ));
    }
    Ok((path, b))
}

pub fn load_nao(explicit: Option<&Path>) -> Result<(PathBuf, Vec<u8>), String> {
    let path = discover(
        explicit,
        &["Saab NAO.bin", "NAO.bin", "extracted/Saab NAO.bin"],
    )?;
    let b = read_image(&path, 0x100000, 128 * 1024 * 1024)?;
    println!(
        "PCMCIA CARD{} loaded from {} ({} bytes)",
        if b.starts_with(b"T2  ") { "" } else { " (raw)" },
        path.display(),
        b.len()
    );
    Ok((path, b))
}

pub fn load_opsys(explicit: Option<&Path>) -> Result<(PathBuf, Vec<u8>), String> {
    let path = discover(explicit, &["extracted/opsys.dwn", "opsys.dwn"])?;
    let b = read_image(&path, 0x37fe8, 0x37fe8)?;
    if be_word(&b, 0x12a4a) != 0x42a7 {
        return Err(format!(
            "{}: unsupported opsys image (signature at $12A4A does not match)",
            path.display()
        ));
    }
    Ok((path, b))
}

/// Sum of big-endian 16-bit body words, verified against all three module
/// headers in the supplied download image. Headers are excluded.
fn module_checksum(body: &[u8]) -> u32 {
    body.as_chunks::<2>().0.iter().fold(0u32, |sum, w| {
        sum.wrapping_add(u16::from_be_bytes(*w) as u32)
    })
}

/// Research-only module restoration. The shipped EPROM intentionally contains
/// blank modules (tech2win-ref/TECH2WIN_REFERENCE.md); they are not bad input.
/// This validates experimental replacement bodies, not native loading behavior.
pub fn restore_rom_modules(flash: &mut [u8], opsys: &[u8]) -> Vec<(usize, usize, usize)> {
    let mut mods = Vec::new();
    let mut i = 0x8018;
    while i + 0x20 <= flash.len() {
        if flash[i..i + 4] == [0x00, 0x10, 0x00, 0x44]
            && flash[i + 12..i + 20] == [0x4E, 0x4B, 0x4E, 0x75, 0x4E, 0x4C, 0x4E, 0x75]
        {
            let cksum = be_long(flash, i + 4);
            let len = be_long(flash, i + 8) as usize;
            if len >= 0x20 && i + len <= flash.len() {
                let blank_body = flash[i + 0x20..i + len].iter().all(|b| *b == 0);
                if blank_body {
                    mods.push((i, cksum, len));
                }
            }
        }
        i += 2;
    }
    let mut src_mods = Vec::new();
    let mut j = 0;
    while j + 0x20 <= opsys.len() {
        if opsys[j..j + 4] == [0x00, 0x10, 0x00, 0x44]
            && opsys[j + 12..j + 20] == [0x4E, 0x4B, 0x4E, 0x75, 0x4E, 0x4C, 0x4E, 0x75]
        {
            let cksum = be_long(opsys, j + 4);
            let len = be_long(opsys, j + 8) as usize;
            if len > 0x20
                && len.is_multiple_of(2)
                && j + len <= opsys.len()
                && module_checksum(&opsys[j + 0x20..j + len]) == cksum
            {
                src_mods.push((j, cksum, len));
            }
        }
        j += 2;
    }
    let mut done = Vec::new();
    for (dst, cksum, len) in mods {
        let Some(&(src, _, n)) = src_mods.iter().find(|(_, c, l)| *c == cksum && *l == len) else {
            continue;
        };
        if src + n > opsys.len() || flash[dst..dst + 0x20] != opsys[src..src + 0x20] {
            continue;
        }
        flash[dst..dst + n].copy_from_slice(&opsys[src..src + n]);
        done.push((dst, src, n));
    }
    done
}

#[allow(dead_code)]
pub fn slice_v148(path: &Path) -> Option<Vec<u8>> {
    let b = fs::read(path).ok()?;
    if b.len() < 0x200_000 {
        return None;
    }
    let off = 0x0010_0000;
    let len = 0x37FE8;
    if off + len > b.len() {
        return None;
    }
    Some(b[off..off + len].to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn images() -> (Vec<u8>, Vec<u8>) {
        let mut source = vec![0; 36];
        source[..4].copy_from_slice(&[0, 0x10, 0, 0x44]);
        source[8..12].copy_from_slice(&36u32.to_be_bytes());
        source[12..20].copy_from_slice(&[0x4e, 0x4b, 0x4e, 0x75, 0x4e, 0x4c, 0x4e, 0x75]);
        source[32..].copy_from_slice(&[0x4e, 0x71, 0x4e, 0x75]);
        let checksum = module_checksum(&source[32..]);
        source[4..8].copy_from_slice(&checksum.to_be_bytes());
        let mut flash = vec![0; 0x8100];
        flash[0x8018..0x8038].copy_from_slice(&source[..32]);
        (flash, source)
    }
    #[test]
    fn restoration_checks_body_and_preserves_nonblank_modules() {
        let (mut flash, source) = images();
        let mut corrupt = source.clone();
        corrupt[32] ^= 1;
        assert!(restore_rom_modules(&mut flash, &corrupt).is_empty());
        assert_eq!(restore_rom_modules(&mut flash, &source).len(), 1);
        assert_eq!(&flash[0x8038..0x803c], &source[32..]);
        assert!(restore_rom_modules(&mut flash, &source).is_empty());
    }
    #[test]
    fn malformed_lengths_and_extreme_offsets_do_not_panic() {
        assert_eq!(be_word(&[1, 2], usize::MAX), 0);
        assert_eq!(be_long(&[1, 2, 3, 4], usize::MAX), 0);
        let (mut flash, mut source) = images();
        source[8..12].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(restore_rom_modules(&mut flash, &source).is_empty());
    }
}
