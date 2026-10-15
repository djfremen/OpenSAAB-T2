// SPDX-License-Identifier: MPL-2.0
//! One persistent usbfs IN request. Idle polls never cancel or discard data.
use std::{
    io,
    time::{Duration, Instant},
};

const SUBMIT: libc::c_ulong = 0x8038550a;
const DISCARD: libc::c_ulong = 0x550b;
const REAP: libc::c_ulong = 0x4008550d;

// Android NDK r29 linux/usbdevice_fs.h, aarch64 ABI. Verified independently
// with C static assertions against that header, including ioctl values.
#[repr(C)]
struct Urb {
    kind: u8,
    endpoint: u8,
    status: i32,
    flags: u32,
    buffer: *mut libc::c_void,
    buffer_length: i32,
    actual_length: i32,
    start_frame: i32,
    packets: i32,
    error_count: i32,
    signr: u32,
    context: *mut libc::c_void,
}

pub struct Input {
    fd: i32,
    urb: Option<Box<Urb>>,
    buffer: Option<Box<[u8]>>,
    submitted: bool,
}
impl Input {
    pub fn new(fd: i32) -> Result<Self, String> {
        Self::with_capacity(fd, 64)
    }
    pub fn with_capacity(fd: i32, capacity: usize) -> Result<Self, String> {
        if !matches!(capacity, 64 | 2048) {
            return Err("Unqualified USB input capacity".into());
        }
        if std::mem::size_of::<Urb>() != 56 {
            return Err("Unsupported asynchronous USB ABI".into());
        }
        let fd = unsafe { libc::dup(fd) };
        if fd < 0 {
            return Err(format!(
                "Async USB descriptor: {}",
                io::Error::last_os_error()
            ));
        }
        let mut buffer = vec![0; capacity].into_boxed_slice();
        let urb = Box::new(Urb {
            kind: 3,
            endpoint: 0x81,
            status: 0,
            flags: 0,
            buffer: buffer.as_mut_ptr().cast(),
            buffer_length: capacity as i32,
            actual_length: 0,
            start_frame: 0,
            packets: 0,
            error_count: 0,
            signr: 0,
            context: std::ptr::null_mut(),
        });
        Ok(Self {
            fd,
            urb: Some(urb),
            buffer: Some(buffer),
            submitted: false,
        })
    }
    fn submit(&mut self) -> Result<(), String> {
        let u = self.urb.as_deref_mut().ok_or("USB input released")?;
        u.status = 0;
        u.actual_length = 0;
        if unsafe { libc::ioctl(self.fd, SUBMIT as _, u as *mut Urb) } < 0 {
            return Err(format!("USB submit: {}", io::Error::last_os_error()));
        }
        self.submitted = true;
        Ok(())
    }
    fn reap(&mut self) -> Result<Option<(i32, Vec<u8>)>, String> {
        let mut returned: *mut Urb = std::ptr::null_mut();
        if unsafe { libc::ioctl(self.fd, REAP as _, &mut returned) } < 0 {
            let e = io::Error::last_os_error();
            return if e.raw_os_error() == Some(libc::EAGAIN) {
                Ok(None)
            } else {
                Err(format!("USB reap: {e}"))
            };
        }
        let u = self.urb.as_deref_mut().ok_or("USB input released")?;
        if !std::ptr::eq(returned, u) {
            return Err("Unexpected completed USB request".into());
        }
        self.submitted = false;
        let n = checked_length(u.actual_length, u.buffer_length as usize)?;
        Ok(Some((
            u.status,
            self.buffer.as_deref().ok_or("USB buffer released")?[..n].to_vec(),
        )))
    }
    pub fn poll(&mut self) -> Result<Option<Vec<u8>>, String> {
        if !self.submitted {
            self.submit()?;
        }
        let Some((status, data)) = self.reap()? else {
            return Ok(None);
        };
        if status != 0 {
            return Err(format!(
                "USB IN completion status {status}, actual bytes {}",
                data.len()
            ));
        }
        Ok(Some(data))
    }
    // Keep buffers alive until the queued request is returned. If the device
    // disappears and reaping cannot be confirmed, Drop deliberately retains
    // this tiny allocation until process exit instead of freeing kernel targets.
    pub fn stop(&mut self) -> Result<Option<(i32, Vec<u8>)>, String> {
        if !self.submitted {
            return Ok(None);
        }
        let ptr = self.urb.as_deref_mut().ok_or("USB input released")? as *mut Urb;
        if unsafe { libc::ioctl(self.fd, DISCARD as _, ptr) } < 0 {
            let e = io::Error::last_os_error();
            if !matches!(e.raw_os_error(), Some(libc::EINVAL) | Some(libc::ENODEV)) {
                return Err(format!("USB discard: {e}"));
            }
        }
        let deadline = Instant::now() + Duration::from_secs(1);
        loop {
            if let Some((status, bytes)) = self.reap()? {
                if !matches!(status, 0 | -2 | -104 | -19 | -108) {
                    return Err(format!("Cancelled USB status {status}"));
                }
                return Ok(Some((status, bytes)));
            }
            if Instant::now() >= deadline {
                return Err("USB cancel/reap deadline".into());
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}
impl Drop for Input {
    fn drop(&mut self) {
        if self.submitted && self.stop().is_err() {
            if let Some(u) = self.urb.take() {
                std::mem::forget(u);
            }
            if let Some(b) = self.buffer.take() {
                std::mem::forget(b);
            }
        }
        unsafe {
            libc::close(self.fd);
        }
    }
}
fn checked_length(n: i32, capacity: usize) -> Result<usize, String> {
    if n < 0 || n as usize > capacity {
        return Err("USB completion actual_length boundary".into());
    }
    Ok(n as usize)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn abi_and_completed_length_boundaries() {
        assert_eq!(std::mem::size_of::<Urb>(), 56);
        assert_eq!(std::mem::offset_of!(Urb, buffer), 16);
        assert_eq!(std::mem::offset_of!(Urb, actual_length), 28);
        assert_eq!(std::mem::offset_of!(Urb, context), 48);
        assert_eq!(checked_length(0, 64).unwrap(), 0);
        assert_eq!(checked_length(64, 64).unwrap(), 64);
        assert!(checked_length(-1, 64).is_err());
        assert!(checked_length(65, 64).is_err());
        assert_eq!(checked_length(2048, 2048).unwrap(), 2048);
        assert!(checked_length(2049, 2048).is_err());
    }
}
