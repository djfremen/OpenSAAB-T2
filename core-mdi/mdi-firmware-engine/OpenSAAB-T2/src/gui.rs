// SPDX-License-Identifier: MPL-2.0
//! Desktop GUI Window and Keyboard Input Handler.

use crate::bus::Tech2Bus;
use crate::controls::KeyState;
use crate::gui_console::{scale_lcd, ConsoleView, HEIGHT, WIDTH};
use crate::recovery::Action;
use crate::step_guest;
use m68k::{AddressBus, CpuCore, StepResult};
use minifb::{InputCallback, Key, MouseButton, MouseMode, Scale, Window, WindowOptions};
use std::sync::mpsc::{self, Sender};
use std::thread;
use std::time::{Duration, Instant};

/// minifb 0.25's macOS is_active() inverts the native bool; its notification
/// handler also marks resign-main as active. Ask the live NSWindow directly.
#[cfg(target_os = "macos")]
fn window_has_focus(win: &mut Window) -> bool {
    use std::ffi::{c_char, c_void};
    #[link(name = "objc")]
    extern "C" {
        fn sel_registerName(name: *const c_char) -> *const c_void;
        #[link_name = "objc_msgSend"]
        fn send_bool(receiver: *mut c_void, selector: *const c_void) -> i8;
    }
    // SAFETY: minifb's macOS handle is its live NSWindow. This is invoked on
    // the GUI/main thread while Window owns it. isKeyWindow takes no arguments
    // and returns Objective-C BOOL; no ownership is transferred.
    unsafe {
        send_bool(
            win.get_window_handle(),
            sel_registerName(c"isKeyWindow".as_ptr()),
        ) != 0
    }
}

#[cfg(not(target_os = "macos"))]
fn window_has_focus(win: &mut Window) -> bool {
    win.is_active()
}

pub struct GuiError {
    pub outcome: crate::artifacts::Outcome,
    pub message: String,
    pub restart: bool,
}

impl GuiError {
    fn output(message: &str) -> Self {
        Self {
            outcome: crate::artifacts::Outcome::OutputFailure,
            message: message.into(),
            restart: false,
        }
    }
}

pub struct GuiRunner {
    pub window: Option<Window>,
    pub frame_buf: Vec<u32>,
    pub enabled: bool,
    pub controls: KeyState,
    width: usize,
    scanner_buf: Vec<u32>,
    console: ConsoleView,
    candi_active: bool,
    candi_running: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    ecm: Option<crate::ecm_info::EcmInfo>,
    drive_ecm: bool,
}

fn window_title(candi_active: bool, focused: bool) -> &'static str {
    match (candi_active, focused) {
        (true, true) => "TECH2-SCREEN · CANDI",
        (true, false) => "TECH2-SCREEN · CANDI — click to focus",
        (false, true) => "TECH2-SCREEN",
        (false, false) => "TECH2-SCREEN — click to focus keyboard",
    }
}

fn draw_candi_badge(frame: &mut [u32]) {
    use crate::lcd::{draw_text_to_buffer as text, fill_rect_stride as fill};
    // Top-right of the 2x scanner pane (above the event console).
    fill(frame, WIDTH, 552, 6, 80, 18, 0x001a3d2a);
    text(frame, WIDTH, "CANDI", 568, 10, 0x003fb950);
}

impl GuiRunner {
    /// Create with colours from the Tech2Win config file.
    #[allow(dead_code)]
    pub fn new_with_colors(_bg: u32, _fg: u32) -> Self {
        Self::new_with_options(_bg, _fg, false)
    }

    pub fn new_with_options(_bg: u32, _fg: u32, candi_active: bool) -> Self {
        let headless = crate::options::env_flag("HEADLESS");
        let mut window = None;
        let enabled = !headless;
        let width = WIDTH;
        let title = if candi_active {
            "TECH2-SCREEN · CANDI"
        } else {
            "TECH2-SCREEN"
        };

        if enabled {
            let opts = WindowOptions {
                scale: Scale::X1,
                ..WindowOptions::default()
            };
            if let Ok(win) = Window::new(title, width, HEIGHT, opts) {
                window = Some(win);
            }
        }

        let mut console = ConsoleView::default();
        console.candi_active = candi_active;
        Self {
            window,
            frame_buf: vec![0u32; width * HEIGHT],
            enabled,
            controls: KeyState::new(),
            width,
            scanner_buf: vec![0; 320 * 240],
            console,
            candi_active,
            candi_running: None,
            ecm: None,
            drive_ecm: false,
        }
    }

    pub fn set_candi_worker(&mut self, running: std::sync::Arc<std::sync::atomic::AtomicBool>) {
        self.candi_active = running.load(std::sync::atomic::Ordering::Acquire);
        self.console.candi_active = self.candi_active;
        self.candi_running = Some(running);
    }

    pub fn set_vcx(
        &mut self,
        connection: tech2_emu::vcx::Connection,
        output: std::path::PathBuf,
        drive: bool,
    ) {
        self.ecm = Some(crate::ecm_info::EcmInfo::new(connection, output));
        self.console.vcx_available = true;
        self.drive_ecm = drive;
    }

    /// Run the interactive desktop GUI loop.
    ///
    /// Per frame:
    /// 1. Poll host keys via controls::poll -> Tech2Bus::press_key ($600500)
    /// 2. Step guest CPU (200,000 instructions per frame) via step_guest
    /// 3. Render SED1335 LCD VRAM above the live event console
    /// 4. Present frame to the minifb window at ~60 Hz
    pub fn run(
        &mut self,
        cpu: &mut CpuCore,
        bus: &mut Tech2Bus,
        total_insns: &mut u64,
    ) -> Result<(), GuiError> {
        let mut insns = *total_insns;
        if !self.enabled {
            return Ok(());
        }
        let Some(win) = &mut self.window else {
            return Err(GuiError::output("could not create TECH2-SCREEN window"));
        };

        let insns_per_frame: u64 = if cfg!(debug_assertions) {
            25_000
        } else {
            200_000
        };
        const TARGET_FRAME_TIME: Duration = Duration::from_micros(16_666);

        self.controls.attach(win);
        bus.trace.enable_console();
        self.console.native_link = bus.candi_link.is_some();
        self.console.link_only = self.console.native_link;
        if self.console.native_link {
            bus.trace_event("candi_link_status", "Live Tech2 <-> CANdi frames. Click console header for all events, including adapter TX/RX when the native Nano bridge is attached.");
        }
        crate::log_info!(
            "CONSOLE",
            insns,
            "Live events enabled. Internal requests are not vehicle transmissions."
        );
        crate::log_warn!(
            "J2534",
            insns,
            "Original guest pass-through is pending. Host ECM Information is available only with --vcx-ssh."
        );
        if self.candi_active {
            crate::log_info!(
                "CANDI",
                insns,
                "Indicator on: secondary CANDi worker active (not vehicle TX / goCAN)."
            );
        }
        bus.trace_screen();

        // The caller has just verified the guest's splash in LCD VRAM. Draw
        // it before executing another guest instruction so the handoff cannot
        // advance past the exact frame that qualified the GUI launch.
        bus.render_lcd_pixels(&mut self.scanner_buf);
        scale_lcd(&mut self.frame_buf, &self.scanner_buf);
        if self.candi_active {
            draw_candi_badge(&mut self.frame_buf);
        }
        self.console.draw(&mut self.frame_buf);
        if win
            .update_with_buffer(&self.frame_buf, self.width, HEIGHT)
            .is_err()
        {
            return Err(GuiError::output("GUI window update failed"));
        }

        let mut previous_focus = None;
        let mut frames: u64 = 0;
        let mut checkpoint = None;
        let mut waiting: Option<(Instant, &'static str)> = None;
        let mut notice_until = None;
        let mut driver = self
            .drive_ecm
            .then(|| crate::harness::Harness::new(crate::options::HarnessTarget::Ecm, insns));
        let mut ecm_mouse_down = false;
        while win.is_open() {
            if let Some(running) = &self.candi_running {
                let active = running.load(std::sync::atomic::Ordering::Acquire);
                if self.candi_active != active {
                    self.candi_active = active;
                    self.console.candi_active = active;
                    let focused = window_has_focus(win);
                    win.set_title(window_title(active, focused));
                }
            }
            let quit_cmd = (win.is_key_down(Key::LeftSuper) || win.is_key_down(Key::RightSuper))
                && win.is_key_down(Key::Q);
            let quit_ctrl = (win.is_key_down(Key::LeftCtrl) || win.is_key_down(Key::RightCtrl))
                && win.is_key_down(Key::Q);
            if quit_cmd || quit_ctrl {
                break;
            }

            self.console.poll(win);
            self.console.apply_keys(&mut self.controls);

            let frame_start = Instant::now();
            crate::logger::check_file()
                .map_err(|e| GuiError::output(&format!("console log persistence: {e}")))?;

            let focused = window_has_focus(win);
            if previous_focus != Some(focused) {
                bus.trace_event("window_focus", &format!("focused={focused}"));
                win.set_title(window_title(self.candi_active, focused));
                previous_focus = Some(focused);
            }
            if let Some(ecm) = &mut self.ecm {
                let was_visible = ecm.visible;
                ecm.poll(insns);
                let mouse = win.get_mouse_down(MouseButton::Left);
                let clicked = focused
                    && mouse
                    && !ecm_mouse_down
                    && win
                        .get_mouse_pos(MouseMode::Discard)
                        .is_some_and(|(_, y)| (432.0..480.0).contains(&y));
                ecm_mouse_down = mouse;
                let eligible = crate::ecm_info::available(&bus.screen_text());
                if !ecm.visible
                    && eligible
                    && (clicked || (focused && self.controls.has_pending_key(Key::F9)))
                {
                    ecm.open(insns);
                } else if ecm.visible && focused {
                    let refresh = self.controls.has_pending_key(Key::R);
                    let identity = self.controls.has_pending_key(Key::I);
                    let back = self.controls.poll_wait(bus);
                    if self.controls.quit_requested {
                        break;
                    }
                    if clicked || back || self.controls.recovery_requested {
                        ecm.close();
                    } else if identity {
                        ecm.open_profile(insns, true);
                    } else if refresh {
                        ecm.open(insns);
                    }
                } else if ecm.visible {
                    self.controls.focus_lost();
                }
                if let Some(auto) = &mut driver {
                    match auto.observe(&bus.screen_text(), insns) {
                        Ok(Some(event)) => {
                            if let Some(key) = event.key {
                                bus.press_key(key);
                            }
                            if event.complete {
                                ecm.open(insns);
                                driver = None;
                            }
                        }
                        Err(error) => {
                            crate::log_error!(
                                "GUI",
                                insns,
                                "ECM navigation stopped at {}: {error}",
                                auto.waiting_for()
                            );
                            driver = None;
                        }
                        _ => {}
                    }
                }
                if ecm.visible || was_visible {
                    // These host-page keys must never leak into the paused guest.
                    if ecm.visible != was_visible {
                        self.controls = KeyState::new();
                        self.controls.attach(win);
                    }
                    if ecm.visible {
                        ecm.draw(&mut self.scanner_buf);
                    } else {
                        bus.render_lcd_pixels(&mut self.scanner_buf);
                    }
                    scale_lcd(&mut self.frame_buf, &self.scanner_buf);
                    self.console.draw(&mut self.frame_buf);
                    win.update_with_buffer(&self.frame_buf, self.width, HEIGHT)
                        .map_err(|_| GuiError::output("ECM Information window update failed"))?;
                    thread::sleep(TARGET_FRAME_TIME);
                    continue;
                }
            }
            if let Some((started, reason)) = waiting {
                let back = if focused {
                    self.controls.poll_wait(bus)
                } else {
                    self.controls.focus_lost();
                    false
                };
                if self.controls.quit_requested {
                    break;
                }
                let expired = started.elapsed() >= Duration::from_secs(5);
                if back || expired || self.controls.recovery_requested {
                    let manual_recovery = self.controls.recovery_requested;
                    bus.trace_event(
                        "host_wait_end",
                        &format!("expired={expired} back={back} recovery={manual_recovery}"),
                    );
                    if !manual_recovery && checkpoint.is_some() {
                        let saved: crate::recovery::Checkpoint = checkpoint.take().unwrap();
                        saved.restore(cpu, bus);
                        notice_until = Some(Instant::now() + Duration::from_secs(4));
                        win.set_title(
                            "TECH2-SCREEN — operation cancelled; returned to previous menu",
                        );
                    } else {
                        let action =
                            recovery_dialog(win, &mut self.frame_buf, self.width, reason, false)?;
                        bus.trace_event("host_recovery_action", &format!("{action:?}"));
                        return Err(recovery_exit(reason, action));
                    }
                    waiting = None;
                    self.controls = KeyState::new();
                    self.controls.attach(win);
                } else {
                    let remaining = 5u64.saturating_sub(started.elapsed().as_secs());
                    win.set_title(&format!("TECH2-SCREEN — no response; back in {remaining}s (Esc: back, F12: recovery)"));
                }
                // No transport exists in this build. Keep pumping native events
                // while pausing guest retries, so they cannot race into an assert.
                bus.render_lcd_pixels(&mut self.scanner_buf);
                if waiting.is_some() {
                    crate::lcd::fill_rect_stride(
                        &mut self.scanner_buf,
                        320,
                        0,
                        222,
                        320,
                        18,
                        0x002a4059,
                    );
                    let remaining = 5 - started.elapsed().as_secs().min(5);
                    crate::lcd::draw_text_to_buffer(
                        &mut self.scanner_buf,
                        320,
                        &format!("HOST: no response. Back in {remaining}s. Esc"),
                        4,
                        227,
                        0x00ffffff,
                    );
                }
                scale_lcd(&mut self.frame_buf, &self.scanner_buf);
                if self.candi_active {
                    draw_candi_badge(&mut self.frame_buf);
                }
                self.console.draw(&mut self.frame_buf);
                bus.trace
                    .check()
                    .map_err(|e| GuiError::output(&format!("trace persistence: {e}")))?;
                win.update_with_buffer(&self.frame_buf, self.width, HEIGHT)
                    .map_err(|_| GuiError::output("GUI window update failed"))?;
                thread::sleep(TARGET_FRAME_TIME);
                continue;
            }
            if notice_until.is_some_and(|until| Instant::now() >= until) {
                win.set_title(window_title(self.candi_active, focused));
                notice_until = None;
            }
            if focused {
                if self.controls.has_pending_selection() {
                    // Replace even on capture failure: never return to a stale,
                    // unrelated menu after a later selection.
                    checkpoint = None;
                    if crate::recovery::can_checkpoint(bus) {
                        match crate::recovery::Checkpoint::capture(cpu, bus) {
                            Ok(saved) => {
                                checkpoint = Some(saved);
                                bus.trace_event("menu_checkpoint", "before host selection");
                            }
                            Err(error) => {
                                bus.trace_event("menu_checkpoint_failed", &error.to_string())
                            }
                        }
                    }
                }
                self.controls.poll(bus);
            } else {
                self.controls.focus_lost();
            }
            if self.controls.quit_requested {
                break;
            }
            if self.controls.recovery_requested {
                let blocked = crate::recovery::unavailable_reason(&bus.screen_text());
                let reason = blocked.unwrap_or(
                    "Scanner paused. Continue, restart to clear the current operation, or quit.",
                );
                bus.trace_event("host_recovery", reason);
                bus.trace
                    .check()
                    .map_err(|e| GuiError::output(&format!("trace persistence: {e}")))?;
                let action = recovery_dialog(
                    win,
                    &mut self.frame_buf,
                    self.width,
                    reason,
                    blocked.is_none(),
                )?;
                bus.trace_event("host_recovery_action", &format!("{action:?}"));
                if action != Action::Resume {
                    return Err(recovery_exit(reason, action));
                }
                self.controls = KeyState::new();
                self.controls.attach(win);
                previous_focus = None;
            }

            for _ in 0..insns_per_frame {
                match step_guest(cpu, bus, insns) {
                    StepResult::Ok { .. } => {}
                    res => {
                        let d_regs: [u32; 8] = cpu.dar[0..8].try_into().unwrap();
                        let a_regs: [u32; 8] = cpu.dar[8..16].try_into().unwrap();
                        crate::logger::record_crash_report(
                            "CPU",
                            insns,
                            &bus.failure_reason
                                .clone()
                                .unwrap_or_else(|| format!("Step stopped: {res:?}")),
                            cpu.pc,
                            cpu.sp(),
                            &d_regs,
                            &a_regs,
                            &bus.recent_pc_history(),
                        );
                        return Err(GuiError {
                            restart: false,
                            outcome: crate::artifacts::Outcome::GuestFailure,
                            message: bus.failure_reason.clone().unwrap_or_else(|| {
                                format!("CPU stopped at PC={:#010x}: {res:?}", cpu.pc)
                            }),
                        });
                    }
                }
                insns += 1;
                *total_insns = insns;
            }

            bus.current_insns = insns;
            bus.current_pc = cpu.pc;
            bus.trace_screen();
            if let Err(message) = bus.trace.check() {
                return Err(GuiError {
                    restart: false,
                    outcome: crate::artifacts::Outcome::OutputFailure,
                    message: format!("trace persistence: {message}"),
                });
            }
            if let Some(reason) = crate::recovery::unavailable_reason(&bus.screen_text()) {
                bus.trace_event("host_recovery", reason);
                bus.trace
                    .check()
                    .map_err(|e| GuiError::output(&format!("trace persistence: {e}")))?;
                bus.trace_event(
                    "host_wait_start",
                    &format!(
                        "timeout_seconds=5 checkpoint_available={} reason={reason}",
                        checkpoint.is_some()
                    ),
                );
                waiting = Some((Instant::now(), reason));
            }
            if bus.guest_boot_failed() {
                return Err(GuiError {
                    restart: false,
                    outcome: crate::artifacts::Outcome::GuestFailure,
                    message: "guest reported processor halt or bootstrap failure".into(),
                });
            }
            if frames.is_multiple_of(120) {
                let menu = bus.read_word(crate::bus::KEY_MENU_IDX);
                println!(
                    "GUI frame={frames} insns={insns} key_events={} key_consumed={} menu_idx={menu:#06x}",
                    bus.key_events, bus.key_consumed
                );
            }
            frames += 1;

            bus.render_lcd_pixels(&mut self.scanner_buf);
            if self.ecm.is_some() && crate::ecm_info::available(&bus.screen_text()) {
                crate::ecm_info::draw_entry(&mut self.scanner_buf);
            }
            scale_lcd(&mut self.frame_buf, &self.scanner_buf);
            if self.candi_active {
                draw_candi_badge(&mut self.frame_buf);
            }
            self.console.draw(&mut self.frame_buf);
            if win
                .update_with_buffer(&self.frame_buf, self.width, HEIGHT)
                .is_err()
            {
                return Err(GuiError::output("GUI window update failed"));
            }

            let elapsed = frame_start.elapsed();
            if elapsed < TARGET_FRAME_TIME {
                thread::sleep(TARGET_FRAME_TIME - elapsed);
            }
        }
        Ok(())
    }
}

fn recovery_exit(reason: &str, action: Action) -> GuiError {
    GuiError {
        outcome: crate::artifacts::Outcome::Incomplete,
        message: format!("operation cancelled by host recovery ({action:?}): {reason}"),
        restart: action == Action::Restart,
    }
}

struct DialogInput(Sender<Key>);
impl InputCallback for DialogInput {
    fn add_char(&mut self, _: u32) {}
    fn set_key_state(&mut self, key: Key, down: bool) {
        if down {
            let _ = self.0.send(key);
        }
    }
}

fn dialog_key(key: Key, resume: bool) -> Option<Action> {
    match key {
        Key::Escape | Key::Q => Some(Action::Close),
        Key::R => Some(Action::Restart),
        Key::Enter | Key::NumPadEnter => Some(if resume {
            Action::Resume
        } else {
            Action::Restart
        }),
        _ => None,
    }
}

/// Host-owned framebuffer, never guest VRAM. CPU execution is paused here.
fn recovery_dialog(
    win: &mut Window,
    output: &mut [u32],
    output_width: usize,
    reason: &str,
    resume: bool,
) -> Result<Action, GuiError> {
    win.set_title("Tech2 Emulator — Recovery");
    let (sender, receiver) = mpsc::channel();
    win.set_input_callback(Box::new(DialogInput(sender)));
    let mut dialog = vec![0x00151d29; 320 * 240];
    let width = 320;
    let frame = &mut dialog;
    crate::lcd::fill_rect_stride(frame, width, 0, 0, width, 34, 0x002a4059);
    crate::lcd::draw_text_to_buffer(
        frame,
        width,
        "TECH2 EMULATOR - RECOVERY",
        12,
        12,
        0x00ffffff,
    );
    let mut rows = vec![String::new()];
    for word in reason.split_whitespace() {
        let row = rows.last_mut().unwrap();
        if row.chars().count() + word.chars().count() + 1 > 36 {
            rows.push(String::new());
        }
        let row = rows.last_mut().unwrap();
        if !row.is_empty() {
            row.push(' ');
        }
        // Long file paths are continued across lines rather than drawn offscreen.
        for ch in word.chars() {
            if rows.last().unwrap().chars().count() == 36 {
                rows.push(String::new());
            }
            rows.last_mut().unwrap().push(ch);
        }
    }
    for (line, row) in rows.iter().take(9).enumerate() {
        crate::lcd::draw_text_to_buffer(frame, width, row, 12, 48 + line * 13, 0x00e2e8f0);
    }
    if rows.len() > 9 {
        crate::lcd::draw_text_to_buffer(
            frame,
            width,
            "Full details in console/log.",
            12,
            168,
            0x00b6c3d4,
        );
    }
    crate::lcd::fill_rect_stride(frame, width, 12, 186, 142, 26, 0x003b6385);
    crate::lcd::fill_rect_stride(frame, width, 166, 186, 142, 26, 0x003b6385);
    crate::lcd::draw_text_to_buffer(frame, width, "R: RESTART", 20, 195, 0x00ffffff);
    crate::lcd::draw_text_to_buffer(frame, width, "ESC: QUIT", 180, 195, 0x00ffffff);
    crate::lcd::draw_text_to_buffer(
        frame,
        width,
        if resume {
            "ENTER: Continue scanner"
        } else {
            "ENTER: Retry / restart"
        },
        12,
        222,
        0x00b6c3d4,
    );
    let mut mouse_down = win.get_mouse_down(MouseButton::Left);
    while win.is_open() {
        let height = output.len() / output_width;
        if output_width == WIDTH {
            scale_lcd(output, frame);
            ConsoleView::default().draw(output);
        } else {
            output.copy_from_slice(frame);
        }
        win.update_with_buffer(output, output_width, height)
            .map_err(|_| GuiError::output("recovery window update failed"))?;
        for key in receiver.try_iter() {
            if let Some(action) = dialog_key(key, resume) {
                return Ok(action);
            }
        }
        let down = win.get_mouse_down(MouseButton::Left);
        if down && !mouse_down {
            if let Some((x, y)) = win.get_mouse_pos(MouseMode::Discard) {
                let scale = if output_width == WIDTH { 2.0 } else { 1.0 };
                let (x, y) = (x / scale, y / scale);
                if (186.0..212.0).contains(&y) {
                    if (12.0..154.0).contains(&x) {
                        return Ok(Action::Restart);
                    }
                    if (166.0..308.0).contains(&x) {
                        return Ok(Action::Close);
                    }
                }
            }
        }
        mouse_down = down;
        thread::sleep(Duration::from_millis(16));
    }
    Ok(Action::Close)
}

pub fn startup_recovery(reason: &str) -> Result<Action, GuiError> {
    let mut win = Window::new(
        "Tech2 Emulator — Recovery",
        320,
        240,
        WindowOptions {
            scale: Scale::X2,
            ..WindowOptions::default()
        },
    )
    .map_err(|e| GuiError::output(&format!("recovery window: {e}")))?;
    recovery_dialog(&mut win, &mut vec![0; 320 * 240], 320, reason, false)
}

#[cfg(test)]
mod recovery_tests {
    use super::*;
    #[test]
    fn recovery_keys_offer_real_exit_and_restart() {
        assert_eq!(dialog_key(Key::Escape, false), Some(Action::Close));
        assert_eq!(dialog_key(Key::R, false), Some(Action::Restart));
        assert_eq!(dialog_key(Key::Enter, false), Some(Action::Restart));
        assert_eq!(dialog_key(Key::Enter, true), Some(Action::Resume));
    }
}
