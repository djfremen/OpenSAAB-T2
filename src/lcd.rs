// SPDX-License-Identifier: MPL-2.0
//! SED1335 LCD Controller and Font Matrix Renderer.
#![allow(dead_code)]

pub const VRAM_SIZE: usize = 64 * 1024;

#[derive(Clone, Debug)]
pub struct Sed1335 {
    pub vram: Vec<u8>,
    pub cursor: u32,
    pub cmd: u8,
    pub params: Vec<u8>,
    pub params_left: u8,
    pub status: u8,
    pub data_wr: usize,
    pub cmd_wr: usize,
    pub sad1: usize,
    pub sad2: usize,
    pub ap: usize, // address pitch (bytes per row, from SYSTEM SET P7+P8)
    pub fx: u8,    // character width (from SYSTEM SET P2)
    pub fy: u8,    // character height (from SYSTEM SET P3)
    pub system_set: u8,
    pub cgram_addr: u16,
    pub overlay_mode: u8,
    pub cursor_direction: u8,
    pub display_on: bool,
    pub display_mode: u8, // DISPLAY ON/OFF parameter from command 0x59
    pub read_phase: u8,   // byte phase for multi-byte read commands (e.g. 0x47 CSRR)
}

impl Sed1335 {
    pub fn new() -> Self {
        Self {
            vram: vec![0; VRAM_SIZE],
            cursor: 0,
            cmd: 0,
            params: Vec::new(),
            params_left: 0,
            status: 0x00,
            data_wr: 0,
            cmd_wr: 0,
            sad1: 0x0000,
            sad2: 0x1000,
            ap: 40,
            fx: 8,
            fy: 8,
            system_set: 0x30,
            cgram_addr: 0,
            overlay_mode: 0,
            cursor_direction: 0,
            display_on: true,
            display_mode: 0x14,
            read_phase: 0,
        }
    }

    pub fn write_cmd(&mut self, v: u8) {
        let verbose = lcd_log_enabled();
        if verbose && !self.params.is_empty() {
            println!(
                "SED1335 CMD prev {:#04x} params={:02x?}",
                self.cmd, self.params
            );
        }
        if verbose && v != 0x46 {
            println!("SED1335 CMD {:#04x}", v);
        }
        // A SCROLL command is either the three-byte single-screen form or
        // the six-byte two-screen form.  POST uses the former and switches
        // command after its third byte; treating the unused second screen as
        // the reset-time $1000 graphics page created the vertical bars.
        if self.cmd == 0x44 && !self.params.is_empty() && self.params_left > 0 {
            if self.params.len() >= 2 {
                let new_sad1 = (self.params[0] as usize) | ((self.params[1] as usize) << 8);
                if new_sad1 != self.sad1 {
                    self.sad1 = new_sad1;
                    if verbose {
                        println!(
                            "SED1335 0x44 SCROLL partial ({}B): sad1={:#06x}",
                            self.params.len(),
                            self.sad1
                        );
                    }
                }
            }
            if self.params.len() == 3 {
                // Single-screen SCROLL has no graphics layer.
                self.sad2 = self.sad1;
            } else if self.params.len() >= 5 {
                self.sad2 = (self.params[3] as usize) | ((self.params[4] as usize) << 8);
            }
        }
        if (0x4c..=0x4f).contains(&v) {
            self.cursor_direction = v & 3;
        }
        if v == 0x58 {
            self.display_on = false;
        }
        if v == 0x59 {
            self.display_on = true;
        }
        self.cmd_wr += 1;
        self.cmd = v;
        self.params.clear();
        self.read_phase = 0;
        self.params_left = match v {
            0x40 => 8,
            0x44 => 6, // 3-param single-screen or 6-param dual-screen
            0x46 => 2,
            0x58..=0x5B => 1,
            0x5C..=0x5D => 2,
            _ => 0,
        };
        if v == 0x42 || v == 0x43 || v == 0x47 || (0x4C..=0x4F).contains(&v) {
            self.params_left = 0;
        }
    }

    fn advance_cursor(&mut self) {
        let delta = match self.cursor_direction {
            0 => 1,
            1 => -1,
            2 => -(self.ap as i32),
            _ => self.ap as i32,
        };
        self.cursor = self.cursor.wrapping_add_signed(delta) & 0xffff;
    }

    pub fn text_dimensions(&self) -> (usize, usize) {
        (
            320 / self.fx.max(1) as usize,
            240usize.div_ceil(self.fy.max(1) as usize),
        )
    }

    pub fn text_enabled(&self) -> bool {
        self.display_on && self.display_mode & 0x0c != 0 && self.overlay_mode & 0x04 == 0
    }

    /// Original emulator.exe decodes FP1 from bits 2..3, FP2 from 4..5,
    /// and OVLAY's low bits as OR/XOR/AND. XOR is the guest's selection bar.
    pub fn pixel(&self, x: usize, y: usize) -> bool {
        if !self.display_on {
            return false;
        }
        let byte = |address: usize| self.vram[address % self.vram.len()];
        let mut first = false;
        if self.display_mode & 0x0c != 0 {
            if self.overlay_mode & 0x04 != 0 {
                first = byte(self.sad1 + y * self.ap + x / 8) & (0x80 >> (x % 8)) != 0;
            } else {
                let fx = self.fx.clamp(1, 8) as usize;
                let fy = self.fy.max(1) as usize;
                let c = byte(self.sad1 + y / fy * self.ap + x / fx);
                let glyph_y = y % fy;
                let glyph_x = 7 - x % fx;
                first = if self.system_set & 1 != 0 {
                    let glyph_stride = if fy <= 8 { 8 } else { 16 };
                    glyph_y < glyph_stride
                        && byte(self.cgram_addr as usize + c as usize * glyph_stride + glyph_y)
                            & (1 << glyph_x)
                            != 0
                } else {
                    glyph_y < 8 && get_font_bit(c, glyph_x, glyph_y)
                };
            }
        }
        let second = self.display_mode & 0x30 != 0
            && self.sad2 != self.sad1
            && byte(self.sad2 + y * self.ap + x / 8) & (0x80 >> (x % 8)) != 0;
        if self.display_mode & 0x0c == 0 || self.display_mode & 0x30 == 0 || self.sad2 == self.sad1
        {
            return first | second;
        }
        match self.overlay_mode & 3 {
            1 => first ^ second,
            2 => first & second,
            _ => first | second,
        }
    }

    pub fn read_data(&mut self) -> u8 {
        match self.cmd {
            0x47 => {
                // CSRR: Cursor Read (first low byte, then high byte)
                if self.read_phase == 0 {
                    self.read_phase = 1;
                    (self.cursor & 0xFF) as u8
                } else {
                    self.read_phase = 0;
                    ((self.cursor >> 8) & 0xFF) as u8
                }
            }
            0x43 => {
                // MREAD: Memory Read
                let i = (self.cursor as usize) % self.vram.len();
                let val = self.vram[i];
                self.advance_cursor();
                val
            }
            _ => 0x00,
        }
    }

    pub fn write_data(&mut self, v: u8) {
        let verbose = lcd_log_enabled();
        self.data_wr += 1;
        if self.params_left > 0 {
            self.params.push(v);
            self.params_left -= 1;
            // SYSTEM SET (0x40): parse when all 8 bytes received
            if self.cmd == 0x40 && self.params.len() == 8 {
                self.system_set = self.params[0];
                self.fx = (self.params[1] & 0x07) + 1;
                self.fy = (self.params[2] & 0x1F) + 1;
                self.ap = (self.params[6] as usize) | ((self.params[7] as usize) << 8);
                if verbose {
                    println!(
                        "SED1335 0x40 SYSTEM SET: fx={} fy={} ap={} params={:02x?}",
                        self.fx, self.fy, self.ap, self.params
                    );
                }
            }
            // SCROLL (0x44): update sad1 as soon as we have 2 bytes, sad2 at 5 bytes
            if self.cmd == 0x44 {
                if self.params.len() == 2 {
                    self.sad1 = (self.params[0] as usize) | ((self.params[1] as usize) << 8);
                }
                if self.params.len() == 5 {
                    self.sad2 = (self.params[3] as usize) | ((self.params[4] as usize) << 8);
                    if verbose {
                        println!(
                            "SED1335 0x44 SCROLL: sad1={:#06x} sad2={:#06x}",
                            self.sad1, self.sad2
                        );
                    }
                }
            }
            if self.cmd == 0x5b {
                self.overlay_mode = self.params[0];
            }
            if self.cmd == 0x5c && self.params.len() == 2 {
                self.cgram_addr = u16::from_le_bytes([self.params[0], self.params[1]]);
            }
            if self.cmd == 0x59 && !self.params.is_empty() {
                self.display_mode = self.params[0];
            }
            if self.cmd == 0x46 && self.params.len() == 2 {
                self.cursor = (self.params[0] as u32) | ((self.params[1] as u32) << 8);
            }
            return;
        }
        // Command parameters and unexpected data must not leak into VRAM.
        if self.cmd != 0x42 {
            return;
        }
        let i = (self.cursor as usize) % self.vram.len();
        let ch = if (32..127).contains(&v) {
            v as char
        } else {
            '?'
        };
        if lcd_log_enabled() {
            println!(
                "VRAM write at {:#06x} (row={}, col={}): {:#04x} ('{}')",
                i,
                i / 40,
                i % 40,
                v,
                ch
            );
        }
        self.vram[i] = v;
        self.advance_cursor();
    }
}

fn draw_text_scaled(buf: &mut [u32], text: &str, x: usize, y: usize, scale: usize, color: u32) {
    draw_text_scaled_stretched(buf, text, x, y, scale, scale, color);
}

fn draw_text_scaled_stretched(
    buf: &mut [u32],
    text: &str,
    x: usize,
    y: usize,
    x_scale: usize,
    y_scale: usize,
    color: u32,
) {
    for (char_i, ch) in text.bytes().enumerate() {
        for glyph_y in 0..8 {
            for glyph_x in 0..8 {
                if !get_font_bit(ch, 7 - glyph_x, glyph_y) {
                    continue;
                }
                let px = x + (char_i * 8 + glyph_x) * x_scale;
                let py = y + glyph_y * y_scale;
                for sy in 0..y_scale {
                    for sx in 0..x_scale {
                        if px + sx < 320 && py + sy < 240 {
                            buf[(py + sy) * 320 + px + sx] = color;
                        }
                    }
                }
            }
        }
    }
}

/// 8x8 character font bit lookup matrix.
pub fn get_font_bit(c: u8, font_x: usize, font_y: usize) -> bool {
    let glyph: [u8; 8] = match c {
        b' ' => [0, 0, 0, 0, 0, 0, 0, 0],
        b'A' => [0, 0x18, 0x3c, 0x66, 0x7e, 0x66, 0x66, 0],
        b'B' => [0, 0x7c, 0x66, 0x7c, 0x66, 0x66, 0x7c, 0],
        b'C' => [0, 0x3c, 0x66, 0x60, 0x60, 0x66, 0x3c, 0],
        b'D' => [0, 0x78, 0x6c, 0x66, 0x66, 0x6c, 0x78, 0],
        b'E' => [0, 0x7e, 0x60, 0x7c, 0x60, 0x60, 0x7e, 0],
        b'F' => [0, 0x7e, 0x60, 0x7c, 0x60, 0x60, 0x60, 0],
        b'G' => [0, 0x3c, 0x66, 0x60, 0x6e, 0x66, 0x3c, 0],
        b'H' => [0, 0x66, 0x66, 0x7e, 0x66, 0x66, 0x66, 0],
        b'I' => [0, 0x3c, 0x18, 0x18, 0x18, 0x18, 0x3c, 0],
        b'J' => [0, 0x1e, 0x0c, 0x0c, 0x0c, 0x6c, 0x38, 0],
        b'K' => [0, 0x66, 0x6c, 0x78, 0x78, 0x6c, 0x66, 0],
        b'L' => [0, 0x60, 0x60, 0x60, 0x60, 0x60, 0x7e, 0],
        b'M' => [0, 0x63, 0x77, 0x7f, 0x6b, 0x63, 0x63, 0],
        b'N' => [0, 0x66, 0x76, 0x7e, 0x6e, 0x66, 0x66, 0],
        b'O' => [0, 0x3c, 0x66, 0x66, 0x66, 0x66, 0x3c, 0],
        b'P' => [0, 0x7c, 0x66, 0x66, 0x7c, 0x60, 0x60, 0],
        b'Q' => [0, 0x3c, 0x66, 0x66, 0x66, 0x6c, 0x36, 0],
        b'R' => [0, 0x7c, 0x66, 0x66, 0x7c, 0x6c, 0x66, 0],
        b'S' => [0, 0x3c, 0x66, 0x30, 0x0c, 0x66, 0x3c, 0],
        b'T' => [0, 0x7e, 0x18, 0x18, 0x18, 0x18, 0x18, 0],
        b'U' => [0, 0x66, 0x66, 0x66, 0x66, 0x66, 0x3c, 0],
        b'V' => [0, 0x66, 0x66, 0x66, 0x66, 0x3c, 0x18, 0],
        b'W' => [0, 0x63, 0x63, 0x6b, 0x7f, 0x77, 0x63, 0],
        b'X' => [0, 0x66, 0x66, 0x3c, 0x18, 0x3c, 0x66, 0],
        b'Y' => [0, 0x66, 0x66, 0x3c, 0x18, 0x18, 0x18, 0],
        b'Z' => [0, 0x7e, 0x0c, 0x18, 0x30, 0x60, 0x7e, 0],
        b'a' => [0, 0, 0x3c, 0x06, 0x3e, 0x66, 0x3e, 0],
        b'b' => [0, 0x60, 0x60, 0x7c, 0x66, 0x66, 0x7c, 0],
        b'c' => [0, 0, 0x3c, 0x60, 0x60, 0x60, 0x3c, 0],
        b'd' => [0, 0x06, 0x06, 0x3e, 0x66, 0x66, 0x3e, 0],
        b'e' => [0, 0, 0x3c, 0x66, 0x7e, 0x60, 0x3c, 0],
        b'f' => [0, 0x1c, 0x30, 0x7c, 0x30, 0x30, 0x30, 0],
        b'g' => [0, 0, 0x3e, 0x66, 0x66, 0x3e, 0x06, 0x7c],
        b'h' => [0, 0x60, 0x60, 0x7c, 0x66, 0x66, 0x66, 0],
        b'i' => [0, 0x18, 0, 0x38, 0x18, 0x18, 0x3c, 0],
        b'j' => [0, 0x0c, 0, 0x0c, 0x0c, 0x0c, 0x4c, 0x38],
        b'k' => [0, 0x60, 0x60, 0x66, 0x6c, 0x78, 0x66, 0],
        b'l' => [0, 0x38, 0x18, 0x18, 0x18, 0x18, 0x3c, 0],
        b'm' => [0, 0, 0x66, 0x7f, 0x7f, 0x6b, 0x63, 0],
        b'n' => [0, 0, 0x7c, 0x66, 0x66, 0x66, 0x66, 0],
        b'o' => [0, 0, 0x3c, 0x66, 0x66, 0x66, 0x3c, 0],
        b'p' => [0, 0, 0x7c, 0x66, 0x66, 0x7c, 0x60, 0x60],
        b'q' => [0, 0, 0x3e, 0x66, 0x66, 0x3e, 0x06, 0x06],
        b'r' => [0, 0, 0x3c, 0x66, 0x60, 0x60, 0x60, 0],
        b's' => [0, 0, 0x3e, 0x60, 0x3c, 0x06, 0x7c, 0],
        b't' => [0, 0x18, 0x7c, 0x18, 0x18, 0x18, 0x0c, 0],
        b'u' => [0, 0, 0x66, 0x66, 0x66, 0x66, 0x3e, 0],
        b'v' => [0, 0, 0x66, 0x66, 0x66, 0x3c, 0x18, 0],
        b'w' => [0, 0, 0x63, 0x6b, 0x7f, 0x3e, 0x1c, 0],
        b'x' => [0, 0, 0x66, 0x3c, 0x18, 0x3c, 0x66, 0],
        b'y' => [0, 0, 0x66, 0x66, 0x66, 0x3e, 0x06, 0x7c],
        b'z' => [0, 0, 0x7e, 0x0c, 0x18, 0x30, 0x7e, 0],
        b'*' => [0, 0, 0x28, 0x10, 0x28, 0, 0, 0],
        b'.' => [0, 0, 0, 0, 0, 0, 0x18, 0],
        b':' => [0, 0, 0x18, 0, 0x18, 0, 0, 0],
        b'\'' => [0x0c, 0x0c, 0x18, 0, 0, 0, 0, 0],
        b'[' => [0, 0x1c, 0x10, 0x10, 0x10, 0x10, 0x1c, 0],
        b']' => [0, 0x38, 0x08, 0x08, 0x08, 0x08, 0x38, 0],
        b'0' => [0, 0x3c, 0x66, 0x6e, 0x76, 0x66, 0x3c, 0],
        b'1' => [0, 0x18, 0x38, 0x18, 0x18, 0x18, 0x7e, 0],
        b'2' => [0, 0x3c, 0x66, 0x0c, 0x18, 0x30, 0x7e, 0],
        b'3' => [0, 0x7e, 0x0c, 0x18, 0x0c, 0x66, 0x3c, 0],
        b'4' => [0, 0x0c, 0x1c, 0x3c, 0x6c, 0x7e, 0x0c, 0],
        b'5' => [0, 0x7e, 0x60, 0x7c, 0x06, 0x66, 0x3c, 0],
        b'6' => [0, 0x3c, 0x60, 0x7c, 0x66, 0x66, 0x3c, 0],
        b'7' => [0, 0x7e, 0x06, 0x0c, 0x18, 0x30, 0x30, 0],
        b'8' => [0, 0x3c, 0x66, 0x3c, 0x66, 0x66, 0x3c, 0],
        b'9' => [0, 0x3c, 0x66, 0x3e, 0x06, 0x0c, 0x38, 0],
        b'-' => [0, 0, 0, 0x7e, 0, 0, 0, 0],
        b'>' => [0, 0x60, 0x30, 0x18, 0x30, 0x60, 0, 0],
        b'<' => [0, 0x06, 0x0c, 0x18, 0x0c, 0x06, 0, 0],
        b'/' => [0, 0x06, 0x0c, 0x18, 0x30, 0x60, 0, 0],
        b'(' => [0, 0x0c, 0x18, 0x18, 0x18, 0x0c, 0, 0],
        b')' => [0, 0x30, 0x18, 0x18, 0x18, 0x30, 0, 0],
        b',' => [0, 0, 0, 0, 0, 0x18, 0x18, 0x30],
        b'!' => [0, 0x18, 0x18, 0x18, 0x18, 0, 0x18, 0],
        b'=' => [0, 0, 0x7e, 0, 0x7e, 0, 0, 0],
        b'_' => [0, 0, 0, 0, 0, 0, 0x7e, 0],
        b'&' => [0, 0x38, 0x6c, 0x38, 0x76, 0xcc, 0x7b, 0],
        b'+' => [0, 0x18, 0x18, 0x7e, 0x18, 0x18, 0, 0],
        b'?' => [0, 0x3c, 0x66, 0x0c, 0x18, 0, 0x18, 0],
        b'%' => [0, 0x62, 0x64, 0x08, 0x10, 0x26, 0x46, 0],
        b'@' => [0, 0x3c, 0x42, 0x99, 0xa5, 0x9e, 0x40, 0x3c],
        b'#' => [0, 0x24, 0x7e, 0x24, 0x7e, 0x24, 0, 0],
        b'$' => [0, 0x18, 0x3e, 0x60, 0x3c, 0x06, 0x7c, 0x18],
        b'"' => [0, 0x66, 0x66, 0x44, 0, 0, 0, 0],
        _ => [0, 0, 0, 0, 0, 0, 0, 0],
    };
    (glyph[font_y] >> font_x) & 1 == 1
}

pub fn fill_rect(buf: &mut [u32], x: usize, y: usize, w: usize, h: usize, color: u32) {
    for py in y..y.saturating_add(h).min(240) {
        for px in x..x.saturating_add(w).min(320) {
            buf[py * 320 + px] = color;
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum UiScreen {
    Splash,
    MainMenu,
    Diagnostics,
    ServiceProgramming,
    ViewStoredData,
    ToolOptions,
    Download,
}

#[derive(Clone, Debug)]
pub struct Tech2Ui {
    pub screen: UiScreen,
    pub menu_selected: usize,
    pub diag_selected: usize,
    pub sps_selected: usize,
    pub data_selected: usize,
    pub tool_selected: usize,
    pub dl_selected: usize,
}

impl Default for Tech2Ui {
    fn default() -> Self {
        Self::new()
    }
}

impl Tech2Ui {
    pub fn new() -> Self {
        Self {
            screen: UiScreen::Splash,
            menu_selected: 0,
            diag_selected: 0,
            sps_selected: 0,
            data_selected: 0,
            tool_selected: 0,
            dl_selected: 0,
        }
    }
}

pub fn render_menu_screen(
    buf: &mut [u32],
    title: &str,
    items: &[&str],
    selected: usize,
    nav_help: &str,
    fg_color: u32,
    bg_color: u32,
) {
    buf.fill(bg_color);

    // Title banner: 320x20 at top of display
    fill_rect(buf, 0, 0, 320, 20, 0x0000_00B0);
    let title_x = (320usize.saturating_sub(title.len() * 8)) / 2;
    draw_text_scaled_stretched(buf, title, title_x, 2, 1, 2, fg_color);

    // Menu rows: 40-column x 15-row SED1335 hardware format
    let spacing = if items.len() <= 4 {
        36
    } else if items.len() <= 5 {
        30
    } else {
        26
    };
    let start_y = if items.len() <= 4 {
        46
    } else if items.len() <= 5 {
        38
    } else {
        30
    };

    for (i, item) in items.iter().enumerate() {
        let y = start_y + i * spacing;
        if i == selected {
            fill_rect(buf, 8, y - 2, 304, 20, fg_color);
            draw_text_scaled_stretched(buf, &format!(" > {item}"), 12, y, 1, 2, bg_color);
        } else {
            draw_text_scaled_stretched(buf, &format!("   {item}"), 12, y, 1, 2, fg_color);
        }
    }

    // Bottom navigation bar
    fill_rect(buf, 0, 220, 320, 20, 0x0000_00B0);
    let nav_x = (320usize.saturating_sub(nav_help.len() * 8)) / 2;
    draw_text_scaled(buf, nav_help, nav_x, 226, 1, fg_color);
}

pub fn render_main_menu(buf: &mut [u32], selected: usize, fg_color: u32, bg_color: u32) {
    let items = [
        "F0: Diagnostics",
        "F1: Service Programming System",
        "F2: View Stored Data",
        "F3: Tool Options",
        "F4: Download",
    ];
    render_menu_screen(
        buf,
        "< MAIN MENU >",
        &items,
        selected,
        "[ENTER] Select   [EXIT] Back   [UP/DN] Move",
        fg_color,
        bg_color,
    );
}

pub fn render_diagnostics_menu(buf: &mut [u32], selected: usize, fg_color: u32, bg_color: u32) {
    let items = [
        "1. Vehicle Identification",
        "2. Diagnostic Trouble Codes",
        "3. Clear DTC Information",
        "4. Engine & Transmission",
        "5. Body & Chassis Control",
        "6. Infotainment & Audio",
    ];
    render_menu_screen(
        buf,
        "< DIAGNOSTICS >",
        &items,
        selected,
        "[ENTER] Select        [EXIT] Main Menu",
        fg_color,
        bg_color,
    );
}

pub fn render_service_programming(buf: &mut [u32], selected: usize, fg_color: u32, bg_color: u32) {
    let items = [
        "1. Request Info (VIN / Cal ID)",
        "2. Program System (ECU)",
        "3. Replace & Reprogram ECU",
        "4. Calibration History",
    ];
    render_menu_screen(
        buf,
        "< SERVICE PROGRAMMING >",
        &items,
        selected,
        "[ENTER] Select        [EXIT] Main Menu",
        fg_color,
        bg_color,
    );
}

pub fn render_view_stored_data(buf: &mut [u32], selected: usize, fg_color: u32, bg_color: u32) {
    let items = [
        "1. Diagnostic Trouble Codes (DTC)",
        "2. Freeze Frame Data",
        "3. Failure Records",
        "4. DTC Status & Symptoms",
    ];
    render_menu_screen(
        buf,
        "< VIEW STORED DATA >",
        &items,
        selected,
        "[ENTER] Select        [EXIT] Main Menu",
        fg_color,
        bg_color,
    );
}

pub fn render_tool_options(buf: &mut [u32], selected: usize, fg_color: u32, bg_color: u32) {
    let items = [
        "1. Set Clock / RTC",
        "2. Hardware POST: 10/10 PASS",
        "3. CPU: MC68EC020 @ 16 MHz",
        "4. PCMCIA Card: NAO 32MB Active",
    ];
    render_menu_screen(
        buf,
        "< TOOL OPTIONS >",
        &items,
        selected,
        "[EXIT] Main Menu            [UP/DN] Move",
        fg_color,
        bg_color,
    );
}

pub fn render_download(buf: &mut [u32], selected: usize, fg_color: u32, bg_color: u32) {
    let items = [
        "1. RS232 / PC Link (115200)",
        "2. PCMCIA Card Flash Update",
        "3. System Software Backup",
    ];
    render_menu_screen(
        buf,
        "< DOWNLOAD >",
        &items,
        selected,
        "[ENTER] Select        [EXIT] Main Menu",
        fg_color,
        bg_color,
    );
}

/// Draw text into any arbitrary pixel buffer with a given stride (width).
pub fn draw_text_to_buffer(
    buf: &mut [u32],
    stride: usize,
    text: &str,
    x: usize,
    y: usize,
    color: u32,
) {
    for (char_i, ch) in text.bytes().enumerate() {
        for glyph_y in 0..8 {
            for glyph_x in 0..8 {
                if !get_font_bit(ch, 7 - glyph_x, glyph_y) {
                    continue;
                }
                let px = x + char_i * 8 + glyph_x;
                let py = y + glyph_y;
                let idx = py * stride + px;
                if idx < buf.len() && px < stride {
                    buf[idx] = color;
                }
            }
        }
    }
}

/// Fill a rectangle in a buffer with a given stride.
pub fn fill_rect_stride(
    buf: &mut [u32],
    stride: usize,
    x: usize,
    y: usize,
    w: usize,
    h: usize,
    color: u32,
) {
    for r in y..y + h {
        for c in x..x + w {
            let idx = r * stride + c;
            if idx < buf.len() && c < stride {
                buf[idx] = color;
            }
        }
    }
}

#[cfg(test)]
mod display_tests {
    use super::*;

    fn command(lcd: &mut Sed1335, cmd: u8, params: &[u8]) {
        lcd.write_cmd(cmd);
        for byte in params {
            lcd.write_data(*byte);
        }
    }

    fn menu_display() -> Sed1335 {
        let mut lcd = Sed1335::new();
        command(&mut lcd, 0x40, &[0x35, 7, 14, 39, 60, 239, 40, 0]);
        command(&mut lcd, 0x44, &[0, 0, 239, 0x80, 2, 239]);
        command(&mut lcd, 0x5c, &[0, 0xf0]);
        command(&mut lcd, 0x5b, &[1]);
        command(&mut lcd, 0x59, &[0x14]);
        lcd
    }

    #[test]
    fn guest_font_and_xor_graphics_draw_selection_in_the_requested_row() {
        let mut lcd = menu_display();
        assert_eq!(lcd.text_dimensions(), (40, 16));
        lcd.vram[40] = b'A'; // second 15-pixel row
        lcd.vram[0xf000 + b'A' as usize * 16 + 2] = 0x80;
        assert!(lcd.pixel(0, 17));
        assert!(!lcd.pixel(1, 17));
        assert!(!lcd.pixel(0, 10)); // not the old 8-pixel row
        lcd.vram[0x280 + 17 * 40] = 0xff;
        assert!(!lcd.pixel(0, 17)); // white glyph becomes blue
        assert!(lcd.pixel(1, 17)); // blue background becomes white
        command(&mut lcd, 0x5b, &[0]);
        assert!(lcd.pixel(0, 17)); // OR makes both white
        command(&mut lcd, 0x5b, &[2]);
        assert!(lcd.pixel(0, 17));
        assert!(!lcd.pixel(1, 17)); // AND
        command(&mut lcd, 0x59, &[4]); // text only; graphics disabled
        assert!(lcd.pixel(0, 17));
        assert!(!lcd.pixel(1, 17));
        command(&mut lcd, 0x58, &[0]);
        assert!(!lcd.pixel(0, 17));
        assert!(!lcd.text_enabled());
    }

    #[test]
    fn graphics_page_at_0280_is_active_and_cgram_does_not_alias_lower_vram() {
        let mut lcd = menu_display();
        command(&mut lcd, 0x46, &[0, 0xf0]);
        command(&mut lcd, 0x42, &[0xab]);
        assert_eq!(lcd.vram[0xf000], 0xab);
        assert_eq!(lcd.vram[0x7000], 0);
        lcd.vram[0] = b' '; // keep the test pixel clear of the modified glyph 0
        lcd.vram[0x280] = 0x80;
        assert!(lcd.pixel(0, 0));
        command(&mut lcd, 0x59, &[4]);
        assert!(!lcd.pixel(0, 0));
    }

    #[test]
    fn read_and_write_cursor_follow_guest_direction_and_wrap_at_16_bits() {
        let mut lcd = menu_display();
        for (cmd, start, expected) in [
            (0x4c, 0xffffu16, 0),
            (0x4d, 0, 0xffff),
            (0x4e, 100, 60),
            (0x4f, 100, 140),
        ] {
            command(&mut lcd, 0x46, &start.to_le_bytes());
            command(&mut lcd, cmd, &[]);
            command(&mut lcd, 0x42, &[0x5a]);
            assert_eq!(lcd.vram[start as usize], 0x5a);
            assert_eq!(lcd.cursor, expected);
            command(&mut lcd, 0x46, &start.to_le_bytes());
            command(&mut lcd, 0x43, &[]);
            assert_eq!(lcd.read_data(), 0x5a);
            assert_eq!(lcd.cursor, expected);
        }
        let cursor = lcd.cursor;
        command(&mut lcd, 0x5b, &[1, 0xff]);
        assert_eq!(lcd.cursor, cursor); // excess parameters are not VRAM data
    }
}

fn lcd_log_enabled() -> bool {
    #[cfg(feature = "load-test")]
    { static VALUE: std::sync::LazyLock<bool> = std::sync::LazyLock::new(|| crate::options::env_flag("LCD_LOG")); *VALUE }
    #[cfg(not(feature = "load-test"))]
    { crate::options::env_flag("LCD_LOG") }
}
