// SPDX-License-Identifier: MPL-2.0
//! Portable captures and machine-readable outcomes, with explicit I/O errors.
use crate::bus::Tech2Bus;
use std::{fs, io, path::Path};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Success,
    GuestFailure,
    Incomplete,
    OutputFailure,
}
impl Outcome {
    pub fn code(self) -> u8 {
        match self {
            Self::Success => 0,
            Self::GuestFailure => 1,
            Self::Incomplete => 3,
            Self::OutputFailure => 4,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::GuestFailure => "guest_failure",
            Self::Incomplete => "incomplete",
            Self::OutputFailure => "output_failure",
        }
    }
}

pub fn save_screen(bus: &Tech2Bus, path: &Path) -> io::Result<()> {
    let mut pixels = vec![0; 320 * 240];
    bus.render_lcd_pixels(&mut pixels);
    let mut ppm = b"P6\n320 240\n255\n".to_vec();
    for pixel in pixels {
        ppm.extend_from_slice(&[(pixel >> 16) as u8, (pixel >> 8) as u8, pixel as u8]);
    }
    fs::write(path, ppm)
}

/// Reuses capture buffers and publishes only changed pixels by atomic rename.
/// Comparing rendered pixels (not screen text) preserves cursor/highlight changes.
pub struct LiveFrame {
    pixels: Vec<u32>,
    previous: Vec<u32>,
    ppm: Vec<u8>,
}
impl Default for LiveFrame {
    fn default() -> Self {
        Self {
            pixels: vec![0; 320 * 240],
            previous: Vec::new(),
            ppm: Vec::with_capacity(15 + 320 * 240 * 3),
        }
    }
}
impl LiveFrame {
    pub fn publish(&mut self, bus: &Tech2Bus, directory: &Path) -> io::Result<bool> {
        bus.render_lcd_pixels(&mut self.pixels);
        self.publish_pixels(directory)
    }
    fn publish_pixels(&mut self, directory: &Path) -> io::Result<bool> {
        if self.pixels == self.previous {
            return Ok(false);
        }
        self.ppm.clear();
        self.ppm.extend_from_slice(b"P6\n320 240\n255\n");
        for &pixel in &self.pixels {
            self.ppm
                .extend_from_slice(&[(pixel >> 16) as u8, (pixel >> 8) as u8, pixel as u8]);
        }
        let pending = directory.join("live.ppm.tmp");
        fs::write(&pending, &self.ppm)?;
        fs::rename(pending, directory.join("live.ppm"))?;
        self.previous.clone_from(&self.pixels);
        Ok(true)
    }
}

/// Evidence only: preserve guest bytes for SSA inspection. A saved block or
/// VIN-shaped RAM candidate is not proof of fresh seeds or completed collection.
pub fn save_security_snapshot(bus: &Tech2Bus, before: &[u8], output: &Path) -> io::Result<()> {
    const OFFSET: usize = 0xfe0000;
    const SIZE: usize = 714;
    let after = bus
        .card
        .get(OFFSET..OFFSET + SIZE)
        .ok_or_else(|| io::Error::other("card lacks the known SSA region"))?;
    fs::write(output.join("ssa-card-before.bin"), before)?;
    fs::write(output.join("ssa-card-after.bin"), after)?;
    fs::write(output.join("security-guest-ram.bin"), &bus.ram)?;
    fs::write(output.join("security-guest-eram.bin"), &bus.eram)?;
    let ssa_flash_enabled = bus.ssa_flash.is_some();
    let (ssa_erases, ssa_programmed_bytes) = bus
        .ssa_flash
        .as_ref()
        .map_or((0, 0), |flash| (flash.erases, flash.programmed_bytes));
    fs::write(output.join("native-security-snapshot.json"), format!(
        "{{\n  \"origin\": \"original-guest-memory\",\n  \"card_offset\": {OFFSET},\n  \"bytes\": {SIZE},\n  \"ssa_memory_flash_enabled\": {ssa_flash_enabled},\n  \"ssa_erases\": {ssa_erases},\n  \"ssa_programmed_bytes\": {ssa_programmed_bytes},\n  \"card_unchanged\": {},\n  \"card_writes_enabled\": {},\n  \"card_writes_dropped\": {},\n  \"fresh_ssa_verified\": false,\n  \"api_submitted\": false\n}}\n",
        before == after, bus.card_writes, bus.card_wr_dropped))
}

#[cfg(feature = "gui")]
pub fn save_console_screen(bus: &Tech2Bus, path: &Path) -> io::Result<()> {
    use crate::gui_console::{scale_lcd, ConsoleView, HEIGHT, WIDTH};
    let mut lcd = vec![0; 320 * 240];
    bus.render_lcd_pixels(&mut lcd);
    let mut pixels = vec![0; WIDTH * HEIGHT];
    scale_lcd(&mut pixels, &lcd);
    let mut console = ConsoleView::default();
    console.native_link = true;
    console.link_only = true;
    console.draw(&mut pixels);
    let mut ppm = format!("P6\n{WIDTH} {HEIGHT}\n255\n").into_bytes();
    for pixel in pixels {
        ppm.extend_from_slice(&[(pixel >> 16) as u8, (pixel >> 8) as u8, pixel as u8]);
    }
    fs::write(path, ppm)
}

pub fn json_string(value: &str) -> String {
    let mut out = String::from("\"");
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if c < ' ' => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn live_frame_preserves_pixels_skips_duplicates_and_retries_failed_publish() {
        let dir = std::env::temp_dir().join(format!("tech2-live-frame-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let mut frame = LiveFrame::default();
        frame.pixels[0] = 0x0a20ff;
        assert!(frame.publish_pixels(&dir).unwrap());
        let original = fs::read(dir.join("live.ppm")).unwrap();
        assert_eq!(&original[..18], b"P6\n320 240\n255\n\x0a\x20\xff");
        assert_eq!(original.len(), 230415);
        // An unchanged frame must not even attempt to open the temporary file.
        fs::create_dir(dir.join("live.ppm.tmp")).unwrap();
        assert!(!frame.publish_pixels(&dir).unwrap());
        // Pixel-only cursor/color changes still count; a failed write must retry.
        frame.pixels[0] = 0xff200a;
        assert!(frame.publish_pixels(&dir).is_err());
        assert_eq!(fs::read(dir.join("live.ppm")).unwrap(), original);
        fs::remove_dir(dir.join("live.ppm.tmp")).unwrap();
        assert!(frame.publish_pixels(&dir).unwrap());
        let changed = fs::read(dir.join("live.ppm")).unwrap();
        assert_eq!(&changed[15..18], &[0xff, 0x20, 0x0a]);
        assert!(!dir.join("live.ppm.tmp").exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn failure_codes_are_distinct_from_success() {
        for outcome in [
            Outcome::GuestFailure,
            Outcome::Incomplete,
            Outcome::OutputFailure,
        ] {
            assert_ne!(outcome.code(), 0);
        }
    }
    #[test]
    fn json_escapes_paths_and_control_characters() {
        assert_eq!(json_string("a\\b\"\n\t—"), "\"a\\\\b\\\"\\u000a\\u0009—\"");
    }
}
