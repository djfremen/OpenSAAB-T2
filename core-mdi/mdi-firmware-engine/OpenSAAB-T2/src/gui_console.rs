// SPDX-License-Identifier: MPL-2.0
//! Host console below the 2x scanner display. Scrolling freezes a bounded snapshot.
use crate::lcd::{draw_text_to_buffer as text, fill_rect_stride as fill};
use minifb::{MouseButton, MouseMode, Window};

pub const WIDTH: usize = 640;
pub const HEIGHT: usize = 720;
const TOP: usize = 480;
const LINES: usize = 19;

#[derive(Default)]
pub struct ConsoleView {
    history: Option<Vec<(u32, String)>>,
    offset: usize,
    mouse_down: bool,
    /// Host-side CANDi secondary worker is running (opt-in `--candi`).
    pub candi_active: bool,
    pub vcx_available: bool,
    pub native_link: bool,
    pub link_only: bool,
}

fn lines(limit: usize, link_only: bool) -> Vec<(u32, String)> {
    crate::logger::get_logger()
        .lock()
        .map(|l| {
            if link_only {
                l.get_link_console_lines(78, limit)
            } else {
                l.get_console_lines(78, limit)
            }
        })
        .unwrap_or_default()
}

impl ConsoleView {
    pub fn apply_keys(&mut self, keys: &mut crate::controls::KeyState) {
        let delta = std::mem::take(&mut keys.console_history);
        if delta != 0 {
            self.scroll(delta as f32 * 5.0);
        }
        if std::mem::take(&mut keys.console_live_requested) {
            self.history = None;
            self.offset = 0;
        }
    }
    pub fn poll(&mut self, win: &Window) {
        let down = win.get_mouse_down(MouseButton::Left);
        let position = win.get_mouse_pos(MouseMode::Discard);
        if down
            && !self.mouse_down
            && position.is_some_and(|(x, y)| x >= 530.0 && (480.0..505.0).contains(&y))
        {
            self.history = None;
            self.offset = 0;
        }
        if position.is_some_and(|(_, y)| y >= TOP as f32) {
            if let Some((_, delta)) = win.get_scroll_wheel() {
                if delta.abs() > 0.0 {
                    self.scroll(delta);
                }
            }
        }
        if down
            && !self.mouse_down
            && self.native_link
            && position.is_some_and(|(x, y)| x < 320.0 && (480.0..505.0).contains(&y))
        {
            self.link_only = !self.link_only;
            self.history = None;
            self.offset = 0;
        }
        self.mouse_down = down;
    }

    fn scroll(&mut self, delta: f32) {
        let history = self
            .history
            .get_or_insert_with(|| lines(4096, self.link_only));
        let amount = (delta.abs() * 3.0).ceil().min(100.0) as usize;
        self.offset = if delta > 0.0 {
            self.offset.saturating_add(amount)
        } else {
            self.offset.saturating_sub(amount)
        };
        self.offset = self.offset.min(history.len().saturating_sub(LINES));
    }

    pub fn draw(&self, frame: &mut [u32]) {
        fill(frame, WIDTH, 0, TOP, WIDTH, HEIGHT - TOP, 0x000d1117);
        fill(frame, WIDTH, 0, TOP, WIDTH, 25, 0x00212c3c);
        text(
            frame,
            WIDTH,
            if self.link_only {
                "TECH2 <-> CANdi [ALL]"
            } else if self.native_link {
                "EVENT CONSOLE [LINK]"
            } else {
                "EVENT CONSOLE"
            },
            8,
            TOP + 8,
            0x00e6edf3,
        );
        text(
            frame,
            WIDTH,
            if self.history.is_some() {
                "PAUSED"
            } else {
                "LIVE"
            },
            400,
            TOP + 8,
            0x0058a6ff,
        );
        text(frame, WIDTH, "[END: LIVE]", 480, TOP + 8, 0x009bd7ff);
        if self.candi_active {
            // Compact status pill — green means the secondary worker thread is up,
            // not that vehicle CAN/goCAN is live.
            fill(frame, WIDTH, 568, TOP + 4, 64, 17, 0x001a3d2a);
            text(frame, WIDTH, "CANDI", 578, TOP + 8, 0x003fb950);
        }
        let (owned, offset) = if self.history.is_some() {
            (Vec::new(), self.offset)
        } else {
            (lines(LINES, self.link_only), 0)
        };
        let history = self.history.as_ref().unwrap_or(&owned);
        let end = history.len().saturating_sub(offset);
        let start = end.saturating_sub(LINES);
        for (row, (color, message)) in history[start..end].iter().enumerate() {
            text(frame, WIDTH, message, 8, TOP + 33 + row * 10, *color);
        }
        fill(frame, WIDTH, 0, HEIGHT - 18, WIDTH, 18, 0x00161b22);
        text(
            frame,
            WIDTH,
            if self.native_link {
                "Native CANdi link | Adapter TX/RX: see events | PgUp/PgDn: history"
            } else if self.vcx_available {
                "VCX: host VIN available | Guest transport: pending | PgUp/PgDn: logs"
            } else if self.candi_active {
                "PgUp/PgDn: history | End: live | tech2.log | J2534: off | CANDI: on"
            } else {
                "PgUp/PgDn: history | End: live | tech2.log | J2534: unimplemented"
            },
            8,
            HEIGHT - 13,
            0x008b949e,
        );
    }
}

/// Keep the original guest pixels intact while doubling them for desktop display.
pub fn scale_lcd(frame: &mut [u32], lcd: &[u32]) {
    for y in 0..240 {
        for x in 0..320 {
            let color = lcd[y * 320 + x];
            let at = y * 2 * WIDTH + x * 2;
            frame[at] = color;
            frame[at + 1] = color;
            frame[at + WIDTH] = color;
            frame[at + WIDTH + 1] = color;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scanner_and_console_do_not_overwrite_each_other() {
        let mut frame = vec![0xabcdef; WIDTH * HEIGHT];
        let lcd = (0..320 * 240).map(|v| v as u32).collect::<Vec<_>>();
        scale_lcd(&mut frame, &lcd);
        let scanner = frame[..WIDTH * TOP].to_vec();
        let console = ConsoleView::default();
        console.draw(&mut frame);
        assert_eq!(frame[..WIDTH * TOP], scanner);
        assert_eq!(frame[WIDTH * 479 + 639], lcd[320 * 240 - 1]);
        assert_eq!(lcd[0], 0);
    }
    #[test]
    fn candi_badge_pixels_appear_only_when_active() {
        let mut off = vec![0u32; WIDTH * HEIGHT];
        let mut on = vec![0u32; WIDTH * HEIGHT];
        ConsoleView::default().draw(&mut off);
        ConsoleView {
            candi_active: true,
            ..Default::default()
        }
        .draw(&mut on);
        // Pill fill at header right (x=570, y=TOP+5).
        let i = (TOP + 5) * WIDTH + 570;
        assert_ne!(off[i], on[i]);
        assert_eq!(on[i], 0x001a3d2a);
    }

    #[test]
    fn paused_history_stays_bounded_while_scrolling() {
        let mut view = ConsoleView {
            history: Some(vec![(0, "old".into()); 40]),
            ..Default::default()
        };
        view.scroll(100.0);
        assert_eq!(view.offset, 40 - LINES);
        view.scroll(-100.0);
        assert_eq!(view.offset, 0);
        assert!(view.history.is_some()); // Only Live resumes the moving tail.
    }
}
