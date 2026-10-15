// SPDX-License-Identifier: MPL-2.0
//! IOKit wrapper; all RNDIS/MDI protocol behavior is in the shared workflow.
use crate::UsbTransport;
use std::ffi::c_void;
unsafe extern "C" {
    fn mdi_usb_open(location: u32, identity: u64, error: *mut i32) -> *mut c_void;
    fn mdi_usb_control(
        usb: *mut c_void,
        kind: u8,
        request: u8,
        b: *mut u8,
        n: u16,
        actual: *mut u32,
    ) -> i32;
    fn mdi_usb_write(usb: *mut c_void, b: *mut u8, n: u32) -> i32;
    fn mdi_usb_start(usb: *mut c_void) -> i32;
    fn mdi_usb_poll(usb: *mut c_void, b: *mut u8, n: *mut u32) -> i32;
    fn mdi_usb_stop(usb: *mut c_void, b: *mut u8, n: *mut u32, status: *mut i32) -> i32;
    fn mdi_usb_close(usb: *mut c_void) -> i32;
}
pub fn target(value: &str) -> Result<(u32, u64), String> {
    let fields: Vec<_> = value.split(':').collect();
    if fields.len() != 3 || fields[0] != "mdi-usb" || fields[1].len() != 8 {
        return Err("Invalid selected MDI USB target".into());
    }
    let location = u32::from_str_radix(fields[1], 16).map_err(|_| "Invalid MDI USB location")?;
    let identity = fields[2]
        .parse::<u64>()
        .map_err(|_| "Invalid MDI registry identity")?;
    if identity == 0 {
        return Err("Missing MDI registry identity".into());
    }
    Ok((location, identity))
}
fn check(rc: i32, operation: &str) -> Result<(), String> {
    if rc == 0 {
        Ok(())
    } else {
        Err(format!("MDI USB {operation}: IOKit 0x{:08X}", rc as u32))
    }
}
pub struct MacUsb {
    handle: *mut c_void,
}
impl MacUsb {
    pub fn open(selected: &str) -> Result<Self, String> {
        let (location, identity) = target(selected)?;
        let mut rc = 0;
        let handle = unsafe { mdi_usb_open(location, identity, &mut rc) };
        check(rc, "open/configuration")?;
        if handle.is_null() {
            return Err("MDI USB owner unavailable".into());
        }
        Ok(Self { handle })
    }
}
impl UsbTransport for MacUsb {
    fn control(&mut self, k: u8, r: u8, b: &mut [u8]) -> Result<usize, String> {
        let length = u16::try_from(b.len()).map_err(|_| "USB control length")?;
        let mut n = 0;
        check(
            unsafe { mdi_usb_control(self.handle, k, r, b.as_mut_ptr(), length, &mut n) },
            "control",
        )?;
        if n as usize > b.len() {
            return Err("USB control completion overflow".into());
        }
        Ok(n as usize)
    }
    fn bulk_out(&mut self, b: &mut [u8]) -> Result<usize, String> {
        check(
            unsafe { mdi_usb_write(self.handle, b.as_mut_ptr(), b.len() as u32) },
            "OUT",
        )?;
        Ok(b.len())
    }
    fn start_input(&mut self) -> Result<(), String> {
        check(unsafe { mdi_usb_start(self.handle) }, "IN start")
    }
    fn poll_input(&mut self) -> Result<Option<Vec<u8>>, String> {
        let mut b = vec![0; 2048];
        let mut n = 0;
        let rc = unsafe { mdi_usb_poll(self.handle, b.as_mut_ptr(), &mut n) };
        if rc == 0 {
            return Ok(None);
        }
        if rc != 1 {
            check(rc, "IN")?;
        }
        if n > 2048 {
            return Err("USB IN completion overflow".into());
        }
        b.truncate(n as usize);
        Ok(Some(b))
    }
    fn stop_input(&mut self) -> Result<Option<(i32, Vec<u8>)>, String> {
        let mut b = vec![0; 2048];
        let mut n = 0;
        let mut status = 0;
        check(
            unsafe { mdi_usb_stop(self.handle, b.as_mut_ptr(), &mut n, &mut status) },
            "IN cancel/reap",
        )?;
        if n > 2048 {
            return Err("USB terminal completion overflow".into());
        }
        b.truncate(n as usize);
        Ok(Some((status, b)))
    }
    fn close(&mut self) -> Result<(), String> {
        let handle = std::mem::replace(&mut self.handle, std::ptr::null_mut());
        check(unsafe { mdi_usb_close(handle) }, "release")
    }
}
impl Drop for MacUsb {
    fn drop(&mut self) {
        let _ = self.close();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selected_device_identity_is_required_before_USB_open() {
        assert_eq!(
            target("mdi-usb:14300000:4294974292").unwrap(),
            (0x14300000, 4294974292)
        );
        for s in [
            "mdi-usb:14300000:0",
            "/dev/cu.usbmodem1",
            "mdi-usb:xyz:42",
            "mdi-usb:14300000:42:extra",
            "mdi-usb:14300000:saved",
        ] {
            assert!(target(s).is_err());
        }
    }
}
