// SPDX-License-Identifier: MPL-2.0
//! Thread-confined C boundary for native apps. Offline-only initial profile.
//! No firmware, process launch, network client or live adapter is part of this API.
use crate::{
    bus::{ExecutionMode, Tech2Bus},
    candi, load_binary,
    options::Options,
};
use m68k::{CpuCore, CpuType, StepResult};
use sha2::{Digest, Sha256};
use std::{
    ffi::{c_char, CStr, CString},
    panic::{catch_unwind, AssertUnwindSafe},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

pub struct OfflineSession {
    cpu: CpuCore,
    bus: Tech2Bus,
    instructions: u64,
    started: Instant,
    welcome_ms: Option<u128>,
    failure: Option<String>,
    pixels: Vec<u32>,
}

pub fn validate_component(kind: &str, path: &Path) -> Result<(), String> {
    // The native picker uses a private staging filename. Show the selected
    // component and the useful validation reason, not the app-container path.
    validate_component_inner(kind, path).map_err(|e| e.replace(&path.display().to_string(), kind))
}
fn validate_component_inner(kind: &str, path: &Path) -> Result<(), String> {
    match kind {
        "eprom.bin" => {
            load_binary::load_boot(Some(path))?;
        }
        "opsys.dwn" => {
            load_binary::load_opsys(Some(path))?;
        }
        "candi.bin" => {
            crate::candi_cpu::Firmware::load(path)?;
        }
        "card.bin" => {
            let (_, data) = load_binary::load_nao(Some(path))?;
            if !data.starts_with(b"T2  ") {
                return Err("Select a Tech 2 diagnostic card image (T2 header required)".into());
            }
            return Ok(());
        }
        _ => return Err("Unknown firmware component".into()),
    }
    // Same support profile as Android, hashes only: no proprietary bytes shipped.
    let profile: serde_json::Value = serde_json::from_str(include_str!("../ui/support-files.json"))
        .map_err(|e| e.to_string())?;
    let data = std::fs::read(path).map_err(|e| e.to_string())?;
    let hash = format!("{:x}", Sha256::digest(&data));
    if profile[kind]["sha256"].as_str() != Some(&hash) {
        return Err(format!(
            "{kind} does not match the supported component checksum"
        ));
    }
    Ok(())
}
impl OfflineSession {
    pub fn open(directory: &Path, output: &Path) -> Result<Self, String> {
        for kind in ["eprom.bin", "opsys.dwn", "candi.bin", "card.bin"] {
            validate_component(kind, &directory.join(kind))?;
        }
        let (_, boot) = load_binary::load_boot(Some(&directory.join("eprom.bin")))?;
        let (_, card) = load_binary::load_nao(Some(&directory.join("card.bin")))?;
        let (_, opsys) = load_binary::load_opsys(Some(&directory.join("opsys.dwn")))?;
        std::fs::create_dir_all(output).map_err(|e| e.to_string())?;
        let opts = Options::parse(
            [
                "--interactive-headless",
                "--research-harness",
                "--candi-native-link",
                "--candi-on-demand",
                "--candi-firmware",
                directory
                    .join("candi.bin")
                    .to_str()
                    .ok_or("Invalid component path")?,
                "--output-dir",
                output.to_str().ok_or("Invalid session path")?,
            ]
            .into_iter()
            .map(str::to_owned),
            |_| None,
        )?;
        let mut bus = Tech2Bus::new(boot, card, ExecutionMode::ResearchHarness);
        bus.load_opsys_ram(&opsys);
        bus.flash[0x8018..0x40000].copy_from_slice(&opsys);
        load_binary::restore_rom_modules(&mut bus.flash, &opsys);
        bus.pending_candi = Some(candi::demand::Pending::from_options(&opts));
        let mut cpu = CpuCore::new();
        cpu.set_cpu_type(CpuType::M68EC020);
        cpu.sr_mask = 0xa71f;
        cpu.reset(&mut bus);
        Ok(Self {
            cpu,
            bus,
            instructions: 0,
            started: Instant::now(),
            welcome_ms: None,
            failure: None,
            pixels: vec![0; 320 * 240],
        })
    }
    pub fn advance(&mut self, milliseconds: u32) -> bool {
        if self.failure.is_some() {
            return false;
        }
        let deadline = Instant::now() + Duration::from_millis(milliseconds.clamp(1, 50) as u64);
        for i in 0..2_000_000 {
            if i % 2048 == 0 && Instant::now() >= deadline {
                break;
            }
            #[cfg(feature = "load-test")]
            if self.welcome_ms.is_none() && crate::load_test::candidate(&self.cpu) {
                let bound = (self.instructions / 50_000 + 1) * 50_000;
                let skipped = crate::load_test::advance(
                    &mut self.cpu,
                    &mut self.bus,
                    self.instructions,
                    bound,
                );
                if skipped != 0 {
                    self.instructions += skipped;
                    continue;
                }
            }
            let result = crate::step_guest(&mut self.cpu, &mut self.bus, self.instructions);
            self.instructions += 1;
            if !matches!(result, StepResult::Ok { .. }) {
                self.failure = Some(
                    self.bus
                        .failure_reason
                        .clone()
                        .unwrap_or_else(|| format!("Emulation stopped: {result:?}")),
                );
                break;
            }
            if self.instructions % 50_000 == 0 {
                if self.bus.guest_boot_failed() {
                    self.failure = Some("Original firmware reported a bootstrap failure".into());
                    break;
                }
                if self.welcome_ms.is_none() && self.bus.guest_splash_reached() {
                    self.welcome_ms = Some(self.started.elapsed().as_millis());
                    self.bus.demand_boot_complete = self.bus.pending_candi.is_some();
                }
                if self.bus.failure_reason.is_some() {
                    self.failure = self.bus.failure_reason.clone();
                    break;
                }
            }
        }
        self.failure.is_none()
    }
    pub fn key(&mut self, code: u8) -> bool {
        if code > 31 || self.failure.is_some() {
            return false;
        }
        if code == 1 {
            self.bus.exit_key();
        } else {
            self.bus.press_key(code);
        }
        true
    }
    pub fn snapshot(&self) -> String {
        serde_json::json!({"mode":"offline","vehicle_connected":false,"instructions":self.instructions,
            "welcome_ms":self.welcome_ms,"screen":self.bus.screen_text(),"failure":self.failure,
            "candi_started":self.bus.candi_link.is_some(),"external_tx":false,
            "profile":"existing-android-research-harness; deferred CANdi; no live backend"}).to_string()
    }
}

unsafe fn string(pointer: *const c_char) -> Result<String, String> {
    if pointer.is_null() {
        return Err("Missing path".into());
    }
    CStr::from_ptr(pointer)
        .to_str()
        .map(str::to_owned)
        .map_err(|_| "Invalid UTF-8".into())
}
unsafe fn error(buffer: *mut c_char, capacity: usize, message: &str) {
    if buffer.is_null() || capacity == 0 {
        return;
    }
    let n = message.len().min(capacity - 1);
    std::ptr::copy_nonoverlapping(message.as_ptr(), buffer.cast::<u8>(), n);
    *buffer.add(n) = 0;
}
/// Caller owns buffers/handles; all calls for a handle must stay on one serial queue.
#[no_mangle]
pub unsafe extern "C" fn opensaab_validate_component(
    kind: *const c_char,
    path: *const c_char,
    output: *mut c_char,
    capacity: usize,
) -> i32 {
    match catch_unwind(|| validate_component(&string(kind)?, &PathBuf::from(string(path)?))) {
        Ok(Ok(())) => 0,
        Ok(Err(e)) => {
            error(output, capacity, &e);
            -1
        }
        Err(_) => {
            error(output, capacity, "Firmware validation failed safely");
            -1
        }
    }
}
#[no_mangle]
pub unsafe extern "C" fn opensaab_create_offline(
    directory: *const c_char,
    output: *const c_char,
    err: *mut c_char,
    capacity: usize,
) -> *mut OfflineSession {
    match catch_unwind(|| {
        OfflineSession::open(
            &PathBuf::from(string(directory)?),
            &PathBuf::from(string(output)?),
        )
    }) {
        Ok(Ok(session)) => Box::into_raw(Box::new(session)),
        Ok(Err(e)) => {
            error(err, capacity, &e);
            std::ptr::null_mut()
        }
        Err(_) => {
            error(err, capacity, "Emulator initialization failed safely");
            std::ptr::null_mut()
        }
    }
}
#[no_mangle]
pub unsafe extern "C" fn opensaab_advance(session: *mut OfflineSession, milliseconds: u32) -> i32 {
    let Some(s) = session.as_mut() else {
        return -1;
    };
    match catch_unwind(AssertUnwindSafe(|| s.advance(milliseconds))) {
        Ok(true) => 0,
        Ok(false) => -1,
        Err(_) => {
            s.failure = Some("Emulator panic; session stopped".into());
            -1
        }
    }
}
#[no_mangle]
pub unsafe extern "C" fn opensaab_key(session: *mut OfflineSession, code: u8) -> i32 {
    let Some(s) = session.as_mut() else {
        return -1;
    };
    match catch_unwind(AssertUnwindSafe(|| s.key(code))) {
        Ok(true) => 0,
        _ => -1,
    }
}
#[no_mangle]
pub unsafe extern "C" fn opensaab_copy_rgba(
    session: *mut OfflineSession,
    output: *mut u8,
    capacity: usize,
) -> i32 {
    if output.is_null() || capacity < 320 * 240 * 4 {
        return -1;
    }
    let Some(s) = session.as_mut() else {
        return -1;
    };
    catch_unwind(AssertUnwindSafe(|| {
        s.bus.render_lcd_pixels(&mut s.pixels);
        for (i, pixel) in s.pixels.iter().enumerate() {
            let p = output.add(i * 4);
            *p = (pixel >> 16) as u8;
            *p.add(1) = (pixel >> 8) as u8;
            *p.add(2) = *pixel as u8;
            *p.add(3) = 255;
        }
        0
    }))
    .unwrap_or(-1)
}
#[no_mangle]
pub unsafe extern "C" fn opensaab_snapshot(session: *const OfflineSession) -> *mut c_char {
    let Some(s) = session.as_ref() else {
        return std::ptr::null_mut();
    };
    catch_unwind(AssertUnwindSafe(|| {
        CString::new(s.snapshot())
            .map(CString::into_raw)
            .unwrap_or(std::ptr::null_mut())
    }))
    .unwrap_or(std::ptr::null_mut())
}
#[no_mangle]
pub unsafe extern "C" fn opensaab_free_string(value: *mut c_char) {
    if !value.is_null() {
        drop(CString::from_raw(value));
    }
}
#[no_mangle]
pub unsafe extern "C" fn opensaab_destroy(session: *mut OfflineSession) {
    if !session.is_null() {
        drop(Box::from_raw(session));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_input_never_produces_an_engine() {
        let d = CString::new("/no-such-ios-firmware").unwrap();
        let mut err = [0i8; 256];
        let p =
            unsafe { opensaab_create_offline(d.as_ptr(), d.as_ptr(), err.as_mut_ptr(), err.len()) };
        assert!(p.is_null());
        assert_ne!(err[0], 0);
        assert_eq!(unsafe { opensaab_advance(p, 25) }, -1);
        assert_eq!(unsafe { opensaab_key(p, 16) }, -1);
    }
    #[test]
    fn malformed_component_is_rejected_without_touching_source() {
        let p = std::env::temp_dir().join(format!("opensaab-ios-bad-{}", std::process::id()));
        std::fs::write(&p, b"not a firmware image").unwrap();
        assert!(validate_component("card.bin", &p).is_err());
        assert!(validate_component("eprom.bin", &p).is_err());
        let message = validate_component("eprom.bin", &p).unwrap_err();
        assert!(message.contains("eprom.bin"));
        assert!(!message.contains(&p.display().to_string()));
        assert_eq!(std::fs::read(&p).unwrap(), b"not a firmware image");
        std::fs::remove_file(p).unwrap();
    }
}
