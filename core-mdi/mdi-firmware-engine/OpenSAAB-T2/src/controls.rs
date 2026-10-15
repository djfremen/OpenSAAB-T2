// SPDX-License-Identifier: MPL-2.0
//! Tech2 Keypad Controls Mapping.
//!
//! Maps host keyboard input (minifb) onto the keypad encoder at $600500.
//!
//! The values pushed here are raw 5-bit *encoder codes*, not ASCII.  The ROM
//! does its own translation: the IRQ1 handler at $0168B8 reads $600500, splits
//! it into `code = byte >> 3` and `active = byte & 4`, acts directly on codes
//! $09 (menu up) and $0C (menu down), and passes everything else through the
//! RAM tables at $100CEA / $100CD0 before latching the result at $1011B4 for
//! the GETKEY service at $017336.
//!
//! - Up:    $09 (hold-repeat)
//! - Down:  $0C (hold-repeat)
//! - Enter: KEY_ENTER_DEFAULT, overridable with TECH2_KEY_ENTER (edge only)
//! - Esc:   $01 (EXIT key, returns from menus/dialogs; Cmd+Q / Ctrl+Q closes window)

use crate::bus::{Tech2Bus, KEY_DOWN, KEY_ENTER_DEFAULT, KEY_EXIT, KEY_UP};
use minifb::{InputCallback, Key, Window};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::env;
use std::rc::Rc;
use std::time::{Duration, Instant};

const INITIAL_REPEAT_DELAY: Duration = Duration::from_millis(350);
const REPEAT_CADENCE: Duration = Duration::from_millis(120);

/// Enter's encoder code is not recoverable from the ROM statically (it goes
/// through a RAM translation table), so allow it to be pinned at run time.
fn enter_code() -> u8 {
    env::var("TECH2_KEY_ENTER")
        .ok()
        .and_then(|v| {
            let v = v.trim().to_ascii_lowercase();
            let (radix, digits) = match v.strip_prefix("0x").or_else(|| v.strip_prefix('$')) {
                Some(rest) => (16, rest),
                None => (10, v.as_str()),
            };
            u8::from_str_radix(digits, radix).ok()
        })
        .map(|c| c & 0x1F)
        .unwrap_or(KEY_ENTER_DEFAULT)
}

/// Use one source of press edges. On macOS minifb's duration-based
/// is_key_pressed can lag is_key_down by a frame; combining both double-fires.
fn rising_edge(down: bool, previous: &mut bool) -> bool {
    let pressed = down && !*previous;
    *previous = down;
    pressed
}

/// One host key that repeats while held.
struct Repeater {
    code: u8,
    held: bool,
    next_fire: Option<Instant>,
}

impl Repeater {
    fn new(code: u8) -> Self {
        Self {
            code,
            held: false,
            next_fire: None,
        }
    }

    fn update(&mut self, down: bool, now: Instant, bus: &mut Tech2Bus) {
        if !down {
            self.held = false;
            self.next_fire = None;
            return;
        }
        if !self.held {
            self.held = true;
            bus.press_key(self.code);
            self.next_fire = Some(now + INITIAL_REPEAT_DELAY);
            return;
        }
        if let Some(at) = self.next_fire {
            if now >= at {
                bus.press_key(self.code);
                self.next_fire = Some(now + REPEAT_CADENCE);
            }
        }
    }
}

/// Preserve both transitions when macOS drains key-down and key-up in one update.
/// Held-state polling alone loses those taps (including remote keyboard input).
#[derive(Clone, Default)]
struct KeyEvents(Rc<RefCell<VecDeque<(Key, bool)>>>);

impl InputCallback for KeyEvents {
    fn add_char(&mut self, _character: u32) {}
    fn set_key_state(&mut self, key: Key, down: bool) {
        self.0.borrow_mut().push_back((key, down));
    }
}

pub struct KeyState {
    events: KeyEvents,
    held_keys: [bool; 512],
    pub quit_requested: bool,
    pub recovery_requested: bool,
    pub console_history: i32,
    pub console_live_requested: bool,
    enter_code: u8,
    enter_down: bool,
    exit_down: bool,
    digit_down: [bool; 10],
    f_down: [bool; 5],
    up: Repeater,
    down: Repeater,
}

impl Default for KeyState {
    fn default() -> Self {
        Self::new()
    }
}

impl KeyState {
    pub fn new() -> Self {
        Self {
            events: KeyEvents::default(),
            held_keys: [false; 512],
            quit_requested: false,
            recovery_requested: false,
            console_history: 0,
            console_live_requested: false,
            enter_code: enter_code(),
            enter_down: false,
            exit_down: false,
            digit_down: [false; 10],
            f_down: [false; 5],
            up: Repeater::new(KEY_UP),
            down: Repeater::new(KEY_DOWN),
        }
    }

    pub fn attach(&self, win: &mut Window) {
        win.set_input_callback(Box::new(self.events.clone()));
    }

    /// Peek a host-only shortcut before deciding which page receives the events.
    pub fn has_pending_key(&self, wanted: Key) -> bool {
        self.events
            .0
            .borrow()
            .iter()
            .any(|&(key, down)| key == wanted && down)
    }

    /// During a host wait, discard guest keys but keep cancel/quit responsive.
    pub fn poll_wait(&mut self, bus: &mut Tech2Bus) -> bool {
        let mut back = false;
        for (key, down) in self.events.0.borrow_mut().drain(..) {
            bus.trace_event(
                "host_wait_key",
                &format!("key={key:?} down={down} guest_delivery=false"),
            );
            self.held_keys[key as usize] = down;
            if down {
                match key {
                    Key::PageUp => self.console_history += 1,
                    Key::PageDown => self.console_history -= 1,
                    Key::End => self.console_live_requested = true,
                    _ => {}
                }
                back |= matches!(key, Key::Escape | Key::Backspace | Key::Left);
                self.recovery_requested |= key == Key::F12;
                self.quit_requested |= key == Key::Q
                    && [
                        Key::LeftSuper,
                        Key::RightSuper,
                        Key::LeftCtrl,
                        Key::RightCtrl,
                    ]
                    .iter()
                    .any(|k| self.held_keys[*k as usize]);
            }
        }
        back
    }

    /// A checkpoint is taken before dispatch, so restoring cannot replay Select.
    pub fn has_pending_selection(&self) -> bool {
        self.events.0.borrow().iter().any(|&(key, down)| {
            down && !self.held_keys[key as usize]
                && matches!(
                    key,
                    Key::Enter
                        | Key::NumPadEnter
                        | Key::Space
                        | Key::Right
                        | Key::Key0
                        | Key::Key1
                        | Key::Key2
                        | Key::Key3
                        | Key::Key4
                        | Key::Key5
                        | Key::Key6
                        | Key::Key7
                        | Key::Key8
                        | Key::Key9
                        | Key::NumPad0
                        | Key::NumPad1
                        | Key::NumPad2
                        | Key::NumPad3
                        | Key::NumPad4
                        | Key::NumPad5
                        | Key::NumPad6
                        | Key::NumPad7
                        | Key::NumPad8
                        | Key::NumPad9
                        | Key::F1
                        | Key::F2
                        | Key::F3
                        | Key::F4
                        | Key::F5
                )
        })
    }

    /// Replay ordered native transitions, then service intentional held repeats.
    pub fn poll(&mut self, bus: &mut Tech2Bus) {
        let events: Vec<_> = self.events.0.borrow_mut().drain(..).collect();
        for (key, down) in events {
            if matches!(key, Key::PageUp | Key::PageDown | Key::End) {
                if down {
                    match key {
                        Key::PageUp => self.console_history += 1,
                        Key::PageDown => self.console_history -= 1,
                        Key::End => self.console_live_requested = true,
                        _ => unreachable!(),
                    }
                }
                continue;
            }
            if bus.trace.enabled() {
                bus.trace_event("host_key", &format!("key={key:?} down={down}"));
            }
            if self.held_keys[key as usize] != down {
                self.held_keys[key as usize] = down;
                self.poll_snapshot(bus);
                if self.recovery_requested || self.quit_requested {
                    return;
                }
            }
        }
        self.poll_snapshot(bus);
    }

    pub fn focus_lost(&mut self) {
        self.events.0.borrow_mut().clear();
        self.held_keys.fill(false);
        self.enter_down = false;
        self.exit_down = false;
        self.digit_down.fill(false);
        self.f_down.fill(false);
        self.up = Repeater::new(KEY_UP);
        self.down = Repeater::new(KEY_DOWN);
    }

    fn poll_snapshot(&mut self, bus: &mut Tech2Bus) {
        let now = Instant::now();
        let held = self.held_keys;
        let is_down = |key: Key| held[key as usize];
        let shortcut = is_down(Key::LeftSuper)
            || is_down(Key::RightSuper)
            || is_down(Key::LeftCtrl)
            || is_down(Key::RightCtrl);
        if shortcut {
            self.quit_requested |= is_down(Key::Q);
            return;
        }

        if is_down(Key::F12)
            || (is_down(Key::Escape)
                && crate::recovery::unavailable_reason(&bus.screen_text()).is_some())
        {
            self.recovery_requested = true;
            return;
        }

        // Enter: Enter, NumPadEnter, Space, Right Arrow
        let enter_down = is_down(Key::Enter)
            || is_down(Key::NumPadEnter)
            || is_down(Key::Space)
            || is_down(Key::Right);

        if rising_edge(enter_down, &mut self.enter_down) {
            let code = if bus.guest_splash_reached() {
                self.enter_code
            } else {
                0x10
            };
            println!("KEYPAD: [ENTER / SELECT] pressed (code {code:#04x})");
            bus.press_key(code);
        }

        // Exit: Escape, Backspace, Delete, Left Arrow
        let exit_down = is_down(Key::Escape)
            || is_down(Key::Backspace)
            || is_down(Key::Delete)
            || is_down(Key::Left);

        if rising_edge(exit_down, &mut self.exit_down) {
            println!("KEYPAD: [EXIT / ESC] pressed (code {KEY_EXIT:#04x})");
            bus.exit_key();
        }

        // Up and Down navigation with repeat support.
        self.up.update(is_down(Key::Up), now, bus);
        self.down.update(is_down(Key::Down), now, bus);

        // Digits 0..9 direct selection
        let digit_map = [
            (0, is_down(Key::Key0) || is_down(Key::NumPad0)),
            (1, is_down(Key::Key1) || is_down(Key::NumPad1)),
            (2, is_down(Key::Key2) || is_down(Key::NumPad2)),
            (3, is_down(Key::Key3) || is_down(Key::NumPad3)),
            (4, is_down(Key::Key4) || is_down(Key::NumPad4)),
            (5, is_down(Key::Key5) || is_down(Key::NumPad5)),
            (6, is_down(Key::Key6) || is_down(Key::NumPad6)),
            (7, is_down(Key::Key7) || is_down(Key::NumPad7)),
            (8, is_down(Key::Key8) || is_down(Key::NumPad8)),
            (9, is_down(Key::Key9) || is_down(Key::NumPad9)),
        ];
        for (d, is_down) in digit_map {
            if is_down && !self.digit_down[d] {
                bus.select_digit(d as u8);
            }
            self.digit_down[d] = is_down;
        }

        // F1..F5 keys direct menu selection (F1 -> F0, F2 -> F1, etc.)
        let f_map = [
            (0, is_down(Key::F1)),
            (1, is_down(Key::F2)),
            (2, is_down(Key::F3)),
            (3, is_down(Key::F4)),
            (4, is_down(Key::F5)),
        ];
        for (idx, is_down) in f_map {
            if is_down && !self.f_down[idx] {
                bus.select_digit(idx as u8);
            }
            self.f_down[idx] = is_down;
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn host_page_retains_short_taps_without_delivering_them_to_guest() {
        use super::*;
        let mut keys = KeyState::new();
        let mut bus = Tech2Bus::new(
            vec![0; 0x40000],
            vec![],
            crate::bus::ExecutionMode::Fidelity,
        );
        for key in [Key::R, Key::Escape] {
            keys.events.set_key_state(key, true);
            keys.events.set_key_state(key, false);
        }
        assert!(keys.has_pending_key(Key::R));
        assert!(keys.poll_wait(&mut bus));
        assert!(!keys.has_pending_key(Key::R));
        assert!(bus.key_queue.is_empty());
    }
    use super::*;
    fn machine() -> Tech2Bus {
        Tech2Bus::new(
            vec![0; 0x40000],
            vec![],
            crate::bus::ExecutionMode::Fidelity,
        )
    }

    #[test]
    fn emergency_escape_does_not_depend_on_guest_key_consumption() {
        let mut bus = machine();
        let mut keys = KeyState::new();
        keys.events.set_key_state(Key::F12, true);
        keys.events.set_key_state(Key::F12, false);
        keys.poll(&mut bus);
        assert!(keys.recovery_requested);
        assert_eq!(bus.key_events, 0);

        let text = b"Checking Key Position Working";
        bus.lcd.vram[..text.len()].copy_from_slice(text);
        let mut keys = KeyState::new();
        keys.events.set_key_state(Key::Escape, true);
        keys.events.set_key_state(Key::Escape, false);
        keys.poll(&mut bus);
        assert!(keys.recovery_requested);
        assert_eq!(bus.key_events, 0);
    }

    #[test]
    fn taps_released_before_frame_are_preserved_in_order() {
        let mut bus = machine();
        let mut keys = KeyState::new();
        let mut callback = keys.events.clone();
        // All down/up events arrive in the same window update, leaving no held keys.
        for key in [Key::Enter, Key::Escape, Key::Key0, Key::Down, Key::F2] {
            callback.set_key_state(key, true);
            callback.set_key_state(key, false);
        }
        keys.poll(&mut bus);
        assert_eq!(bus.key_events, 5);
        let pending: Vec<_> = bus.key_queue.iter().copied().collect();
        // First encoder-down is already latched by the bus.
        assert_eq!(
            pending,
            vec![0x80, 0x0c, 0x08, 0xc4, 0xc0, 0x64, 0x60, 0x24, 0x20]
        );
        keys.poll(&mut bus);
        assert_eq!(bus.key_events, 5);
    }

    #[test]
    fn native_autorepeat_does_not_duplicate_select_and_focus_loss_releases_it() {
        let mut bus = machine();
        let mut keys = KeyState::new();
        for _ in 0..4 {
            keys.events.set_key_state(Key::Enter, true);
        }
        keys.poll(&mut bus);
        assert_eq!(bus.key_events, 1);
        keys.focus_lost();
        keys.poll(&mut bus);
        keys.events.set_key_state(Key::Enter, true);
        keys.poll(&mut bus);
        assert_eq!(bus.key_events, 2);
    }

    #[test]
    fn wait_discards_guest_keys_and_preserves_quick_cancel_and_quit() {
        let mut bus = machine();
        let mut keys = KeyState::new();
        for key in [Key::Enter, Key::Escape] {
            keys.events.set_key_state(key, true);
            keys.events.set_key_state(key, false);
        }
        assert!(keys.has_pending_selection());
        assert!(keys.poll_wait(&mut bus));
        assert!(!keys.has_pending_selection());
        assert_eq!(bus.key_events, 0);
        for key in [Key::LeftSuper, Key::Q] {
            keys.events.set_key_state(key, true);
        }
        keys.poll_wait(&mut bus);
        assert!(keys.quit_requested);
    }

    #[test]
    fn quick_quit_shortcut_is_preserved_without_guest_input() {
        let mut bus = machine();
        let mut keys = KeyState::new();
        for (key, down) in [
            (Key::LeftSuper, true),
            (Key::Q, true),
            (Key::Q, false),
            (Key::LeftSuper, false),
        ] {
            keys.events.set_key_state(key, down);
        }
        keys.poll(&mut bus);
        assert!(keys.quit_requested);
        assert_eq!(bus.key_events, 0);
    }

    #[test]
    fn select_and_exit_fire_once_until_released() {
        for _key in [KEY_ENTER_DEFAULT, KEY_EXIT] {
            let mut previous = false;
            let events: Vec<_> = [false, true, true, true, false, false, true, false]
                .into_iter()
                .map(|down| rising_edge(down, &mut previous))
                .collect();
            assert_eq!(
                events,
                vec![false, true, false, false, false, false, true, false]
            );
        }
    }
}
