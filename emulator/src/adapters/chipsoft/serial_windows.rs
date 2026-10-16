// SPDX-License-Identifier: MPL-2.0
//! Windows USB CDC (usbser.sys) COM ports for the native Chipsoft protocol.
//! Same role as the macOS `serial` module: the inbox CDC class driver exposes
//! the adapter as `COMn`; this talks the Chipsoft frames to it directly, with
//! no vendor J2534 library. Pure std: the port is opened as `\\.\COMn`, and a
//! reader thread services the blocking `ReadFile` so the shared
//! `chipsoft_channel::Client` sees the same bounded, non-blocking transport it
//! uses over Android USB and macOS CDC.
//!
//! Line settings are not changed here: a USB CDC device ignores baud, and the
//! macOS path works without asserting DTR/RTS. If a future adapter firmware
//! needs DTR, set it with `mode COMn: dtr=on` before starting the helper.

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use std::os::windows::io::AsRawHandle;
use std::thread;
use std::time::Duration;

/// Only `COMn` names (or `\\.\COMn`) are accepted; no scanning of other ports.
pub fn candidates() -> io::Result<Vec<PathBuf>> {
    // Enumeration by VID/PID lives in the launcher (Win32_PnPEntity). The helper
    // never guesses; it needs an explicit --port.
    Ok(Vec::new())
}

pub fn device_path(name: &str) -> io::Result<String> {
    let bare = name.trim().trim_start_matches(r"\\.\").to_ascii_uppercase();
    let digits = bare.strip_prefix("COM").unwrap_or("");
    if digits.is_empty() || digits.len() > 3 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "select the Chipsoft COM port explicitly, e.g. --port COM5",
        ));
    }
    Ok(format!(r"\\.\{bare}"))
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct CommTimeouts {
    interval: u32, multiplier: u32, constant: u32,
    write_multiplier: u32, write_constant: u32,
}
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetCommTimeouts(handle: *mut std::ffi::c_void, timeouts: *mut CommTimeouts) -> i32;
    fn SetCommTimeouts(handle: *mut std::ffi::c_void, timeouts: *const CommTimeouts) -> i32;
}

fn receive_loop(mut reader: impl Read, tx: mpsc::Sender<io::Result<Vec<u8>>>, stop: &AtomicBool) {
    let mut buffer = [0u8; 512];
    while !stop.load(Ordering::Acquire) {
        let result = match reader.read(&mut buffer) {
            // COM timeouts can successfully return zero bytes; this is not EOF.
            Ok(0) => { thread::sleep(Duration::from_millis(1)); continue; }
            Ok(n) => Ok(buffer[..n].to_vec()),
            Err(e) if matches!(e.kind(), io::ErrorKind::TimedOut | io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock) => continue,
            Err(e) => Err(e),
        };
        let failed = result.is_err();
        if tx.send(result).is_err() || failed { break; }
    }
}

pub struct Port {
    file: File,
    incoming: Receiver<io::Result<Vec<u8>>>,
    reader: Option<thread::JoinHandle<()>>,
    stop: Arc<AtomicBool>,
    original_timeouts: CommTimeouts,
}

impl Port {
    pub fn open(path: &Path) -> io::Result<Self> {
        let device = device_path(&path.to_string_lossy())?;
        // No sharing: a second opener (or a vendor DLL) must not interleave frames.
        let file = OpenOptions::new().read(true).write(true).open(&device)?;
        let mut original_timeouts = CommTimeouts::default();
        if unsafe { GetCommTimeouts(file.as_raw_handle(), &mut original_timeouts) } == 0 {
            return Err(io::Error::last_os_error());
        }
        // Return buffered bytes immediately, or wait at most 20 ms for the
        // first byte. Never inherit another opener's timeout policy.
        // https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-commtimeouts
        let timeouts = CommTimeouts { interval: u32::MAX, multiplier: u32::MAX,
            constant: 20, write_multiplier: 0, write_constant: 1000 };
        let reader_file = file.try_clone()?;
        if unsafe { SetCommTimeouts(file.as_raw_handle(), &timeouts) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let (tx, incoming) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let reader_stop = stop.clone();
        let reader = thread::spawn(move || receive_loop(reader_file, tx, &reader_stop));
        Ok(Self {
            file,
            incoming,
            reader: Some(reader),
            stop,
            original_timeouts,
        })
    }
}

impl Drop for Port {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(reader) = self.reader.take() { let _ = reader.join(); }
        // The reader is stopped before restoring the previous port timeouts.
        unsafe { SetCommTimeouts(self.file.as_raw_handle(), &self.original_timeouts); }
    }
}

impl crate::adapters::common::usb::UsbTransport for Port {
    fn write(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.file.write_all(bytes).map_err(|e| e.to_string())?;
        self.file.flush().map_err(|e| e.to_string())
    }
    fn read(&mut self) -> Result<Vec<u8>, String> {
        // Drain whatever the reader thread has queued; wait briefly when empty so
        // the caller's polling loop does not spin.
        let mut out = Vec::new();
        loop {
            match self.incoming.try_recv() {
                Ok(Ok(chunk)) => out.extend_from_slice(&chunk),
                Ok(Err(e)) => return Err(e.to_string()),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return Err("serial reader stopped".into()),
            }
        }
        if out.is_empty() {
            match self.incoming.recv_timeout(Duration::from_millis(5)) {
                Ok(Ok(chunk)) => out.extend_from_slice(&chunk),
                Ok(Err(e)) => return Err(e.to_string()),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => return Err("serial reader stopped".into()),
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn idle_serial_read_preserves_following_reply_and_disconnect_error() {
        struct Fake { call: u8 }
        impl Read for Fake {
            fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
                self.call += 1;
                match self.call {
                    1 => Ok(0),
                    2 => { out[..3].copy_from_slice(&[1, 2, 3]); Ok(3) }
                    _ => Err(io::Error::new(io::ErrorKind::BrokenPipe, "disconnected")),
                }
            }
        }
        let (tx, rx) = mpsc::channel();
        receive_loop(Fake { call: 0 }, tx, &AtomicBool::new(false));
        assert_eq!(rx.recv().unwrap().unwrap(), vec![1, 2, 3]);
        assert_eq!(rx.recv().unwrap().unwrap_err().kind(), io::ErrorKind::BrokenPipe);
        assert!(rx.recv().is_err());
    }
    #[test]
    fn only_explicit_com_names_are_accepted() {
        assert_eq!(device_path("COM5").unwrap(), r"\\.\COM5");
        assert_eq!(device_path(r"\\.\com12").unwrap(), r"\\.\COM12");
        assert!(device_path("COM").is_err());
        assert!(device_path("/dev/ttyACM0").is_err());
        assert!(device_path("COM1234").is_err());
    }
}
