// SPDX-License-Identifier: MPL-2.0
// OS process liveness only; security decisions remain shared.
#[cfg(windows)]
pub fn require_stopped(pid: u32) -> super::Result<()> {
    use std::ffi::c_void;
    #[link(name="kernel32")]
    extern "system" {
        fn OpenProcess(access:u32, inherit:i32, pid:u32)->*mut c_void;
        fn WaitForSingleObject(handle:*mut c_void, milliseconds:u32)->u32;
        fn CloseHandle(handle:*mut c_void)->i32;
        fn GetLastError()->u32;
    }
    unsafe {
        let handle=OpenProcess(0x00100000,0,pid);
        if handle.is_null() { return if GetLastError()==87 {Ok(())} else {Err("Cannot confirm firmware process is stopped".into())}; }
        let state=WaitForSingleObject(handle,0);CloseHandle(handle);
        if state==0 {Ok(())} else {Err("Firmware is still running or process state unavailable".into())}
    }
}
