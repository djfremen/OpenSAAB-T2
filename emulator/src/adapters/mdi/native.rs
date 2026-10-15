// SPDX-License-Identifier: MPL-2.0
//! Thin x86 Windows ABI binding. No ECU decisions or alternate adapter fallback.
use super::{
    init::{Driver, Message},
    profile,
};
use std::{
    ffi::{c_char, c_void, CStr},
    fs, mem, ptr,
};
type Handle = isize;
#[link(name = "kernel32")]
unsafe extern "system" {
    fn LoadLibraryExW(path: *const u16, file: Handle, flags: u32) -> Handle;
    fn GetProcAddress(module: Handle, name: *const c_char) -> *const c_void;
    fn GetModuleFileNameW(module: Handle, path: *mut u16, size: u32) -> u32;
    fn GetLastError() -> u32;
    fn CreateFileW(
        path: *const u16,
        access: u32,
        share: u32,
        security: *const c_void,
        creation: u32,
        flags: u32,
        template: Handle,
    ) -> Handle;
    fn CloseHandle(handle: *mut c_void) -> i32;
    fn CreateToolhelp32Snapshot(flags: u32, process: u32) -> Handle;
    fn Process32FirstW(snapshot: Handle, entry: *mut ProcessEntry) -> i32;
    fn Process32NextW(snapshot: Handle, entry: *mut ProcessEntry) -> i32;
}
#[repr(C)]
struct Config {
    parameter: u32,
    value: u32,
}
#[repr(C)]
struct ConfigList {
    count: u32,
    items: *mut Config,
}
impl super::raw::RawDriver for Native {
    fn raw_connect(&mut self, d: u32, protocol: u32, baud: u32) -> Result<u32, String> {
        let mut ch = 0;
        self.check(
            &format!("RAW_CONNECT protocol={protocol} baud={baud} flags=0"),
            unsafe { (self.connect)(d, protocol, 0, baud, &mut ch) },
        )?;
        Ok(ch)
    }
    fn config(&mut self, c: u32, parameter: u32, value: u32) -> Result<(), String> {
        let mut item = Config { parameter, value };
        let list = ConfigList {
            count: 1,
            items: &mut item,
        };
        self.check(&format!("SET_CONFIG {parameter:08X}={value:08X}"), unsafe {
            (self.ioctl)(c, 2, (&list as *const ConfigList).cast(), ptr::null_mut())
        })?;
        item.value = u32::MAX;
        self.check(&format!("GET_CONFIG {parameter:08X}"), unsafe {
            (self.ioctl)(c, 1, (&list as *const ConfigList).cast(), ptr::null_mut())
        })?;
        if item.value != value {
            return Err(format!(
                "MDI config readback mismatch {parameter:X}: expected {value:X}, observed {:X}",
                item.value
            ));
        }
        println!(
            "BRIDGE CALL CONFIG_READBACK parameter={parameter:08X} value={:08X}",
            item.value
        );
        Ok(())
    }
    fn raw_filter(&mut self, c: u32, protocol: u32) -> Result<u32, String> {
        let mut mask = Message::new(0, &[]);
        mask.protocol = protocol;
        let mut pattern = mask.clone();
        let mut id = 0;
        self.check("RAW_PASS_FILTER all-standard", unsafe {
            (self.filter)(c, 1, &mut mask, &mut pattern, ptr::null_mut(), &mut id)
        })?;
        Ok(id)
    }
    fn raw_read(&mut self, c: u32) -> Result<Option<Message>, String> {
        let mut m = Message::default();
        let mut n = 1;
        let rc = unsafe { (self.read)(c, &mut m, &mut n, 0) };
        if (rc == 9 || rc == 16) && n == 0 {
            return Ok(None);
        }
        // No Nano/Chipsoft return-code compatibility applies to Bosch.
        if rc != 0 {
            self.check("RAW_READ", rc)?;
        }
        match n {
            0 => Ok(None),
            1 => Ok(Some(m)),
            _ => Err("MDI raw read exceeds allocated buffer".into()),
        }
    }
    fn raw_write(&mut self, c: u32, m: &mut Message) -> Result<u32, String> {
        let mut n = 1;
        self.check("RAW_WRITE", unsafe { (self.write)(c, m, &mut n, 100) })?;
        Ok(n)
    }
}
#[link(name = "advapi32")]
unsafe extern "system" {
    fn RegOpenKeyExW(
        key: Handle,
        subkey: *const u16,
        options: u32,
        access: u32,
        result: *mut Handle,
    ) -> i32;
    fn RegQueryValueExW(
        key: Handle,
        name: *const u16,
        reserved: *mut u32,
        kind: *mut u32,
        data: *mut u8,
        size: *mut u32,
    ) -> i32;
    fn RegCloseKey(key: Handle) -> i32;
}
#[repr(C)]
struct ProcessEntry {
    size: u32,
    usage: u32,
    pid: u32,
    heap: usize,
    module: u32,
    threads: u32,
    parent: u32,
    priority: i32,
    flags: u32,
    name: [u16; 260],
}
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}
pub struct Guard(Handle);
impl Drop for Guard {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0 as *mut c_void);
        }
    }
}
fn exclusive() -> Result<Guard, String> {
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(2, 0);
        if snapshot == -1 {
            return Err(format!(
                "Cannot inventory diagnostic owners: {}",
                GetLastError()
            ));
        }
        let guard = Guard(snapshot);
        let mut entry: ProcessEntry = mem::zeroed();
        entry.size = mem::size_of::<ProcessEntry>() as u32;
        let mut more = Process32FirstW(guard.0, &mut entry);
        if more == 0 {
            return Err("Cannot read diagnostic owner inventory".into());
        }
        while more != 0 {
            let end = entry.name.iter().position(|&c| c == 0).unwrap_or(260);
            let name = String::from_utf16_lossy(&entry.name[..end]).to_ascii_lowercase();
            if [
                "tech2win.exe",
                "gds2.exe",
                "mdim.exe",
                "mdimanager.exe",
                "gm_mdi_manager.exe",
                "j2534configapp.exe",
            ]
            .contains(&name.as_str())
            {
                return Err(format!("Close diagnostic owner before opening MDI: {name}"));
            }
            more = Process32NextW(guard.0, &mut entry);
        }
        let path = std::env::temp_dir().join("opensaab-mdi-read.lock");
        let path = wide(path.to_str().ok_or("Non-Unicode MDI lock path")?);
        let handle = CreateFileW(path.as_ptr(), 0xc0000000, 0, ptr::null(), 3, 0x80, 0);
        if handle == -1 {
            return Err(format!(
                "MDI is already owned or lock unavailable: {}",
                GetLastError()
            ));
        }
        Ok(Guard(handle))
    }
}
fn registered_driver() -> Result<String, String> {
    unsafe {
        let mut key = 0;
        // This module is x86; explicitly require the 32-bit registry view as well.
        let status = RegOpenKeyExW(
            0x80000002u32 as i32 as isize,
            wide(profile::REGISTRY).as_ptr(),
            0,
            profile::REGISTRY_ACCESS,
            &mut key,
        );
        if status != 0 {
            return Err(format!("Classic MDI registration unavailable: {status}"));
        }
        let mut kind = 0;
        let mut buffer = [0u16; 1024];
        let mut size = 2048;
        let status = RegQueryValueExW(
            key,
            wide("FunctionLibrary").as_ptr(),
            ptr::null_mut(),
            &mut kind,
            buffer.as_mut_ptr().cast(),
            &mut size,
        );
        RegCloseKey(key);
        if status != 0 || kind != 1 || size < 2 || size > 2048 || size % 2 != 0 {
            return Err("Invalid classic MDI provider registration".into());
        }
        let length = size as usize / 2;
        if buffer[length - 1] != 0 {
            return Err("Unterminated MDI provider registration".into());
        }
        String::from_utf16(&buffer[..length - 1]).map_err(|_| "Invalid MDI provider path".into())
    }
}
fn load(path: &str) -> Result<Handle, String> {
    unsafe {
        // Exact absolute module plus its directory and standard Windows directories.
        let module = LoadLibraryExW(wide(path).as_ptr(), 0, 0x1100);
        if module == 0 {
            return Err(format!(
                "MDI module load failed: {} ({path})",
                GetLastError()
            ));
        }
        let mut buffer = [0u16; 2048];
        let n = GetModuleFileNameW(module, buffer.as_mut_ptr(), buffer.len() as u32);
        if n == 0
            || n >= buffer.len() as u32
            || !String::from_utf16_lossy(&buffer[..n as usize]).eq_ignore_ascii_case(path)
        {
            return Err("Loaded MDI module differs from selected path".into());
        }
        Ok(module)
    }
}
type Open = unsafe extern "system" fn(*const c_void, *mut u32) -> u32;
type Close = unsafe extern "system" fn(u32) -> u32;
type Connect = unsafe extern "system" fn(u32, u32, u32, u32, *mut u32) -> u32;
type Filter =
    unsafe extern "system" fn(u32, u32, *mut Message, *mut Message, *mut Message, *mut u32) -> u32;
type Stop = unsafe extern "system" fn(u32, u32) -> u32;
type Messages = unsafe extern "system" fn(u32, *mut Message, *mut u32, u32) -> u32;
type Version = unsafe extern "system" fn(u32, *mut c_char, *mut c_char, *mut c_char) -> u32;
type Ioctl = unsafe extern "system" fn(u32, u32, *const c_void, *mut c_void) -> u32;
type LastError = unsafe extern "system" fn(*mut c_char) -> u32;
pub struct Native {
    _guard: Guard,
    open: Open,
    close: Close,
    connect: Connect,
    disconnect: Close,
    filter: Filter,
    stop: Stop,
    read: Messages,
    write: Messages,
    version: Version,
    ioctl: Ioctl,
    last_error: LastError,
}
impl Native {
    pub fn new() -> Result<Self, String> {
        let guard = exclusive()?;
        if !registered_driver()?.eq_ignore_ascii_case(profile::DRIVER) {
            return Err("Registered provider is not the selected classic MDI profile".into());
        }
        for (path, pin) in [
            (profile::DRIVER, profile::DRIVER_SHA),
            (profile::RUNTIME, profile::RUNTIME_SHA),
            (profile::PDU, profile::PDU_SHA),
        ] {
            profile::validate_image(&fs::read(path).map_err(|e| format!("MDI image: {e}"))?, pin)?;
        }
        if mem::size_of::<Message>() != 4152 {
            return Err("Invalid J2534 ABI".into());
        }
        let _runtime = load(profile::RUNTIME)?;
        let module = load(profile::DRIVER)?;
        // Keep modules loaded until process exit, after the vendor closes all resources.
        macro_rules! symbol {
            ($name:literal,$ty:ty) => {{
                let pointer =
                    unsafe { GetProcAddress(module, concat!($name, "\0").as_ptr().cast()) };
                if pointer.is_null() {
                    return Err(concat!("Missing MDI export: ", $name).into());
                }
                unsafe { mem::transmute::<*const c_void, $ty>(pointer) }
            }};
        }
        println!(
            "MDI_PROFILE classic x86 pinned_driver={} pinned_runtime={}",
            profile::DRIVER_SHA,
            profile::RUNTIME_SHA
        );
        Ok(Self {
            _guard: guard,
            open: symbol!("PassThruOpen", Open),
            close: symbol!("PassThruClose", Close),
            connect: symbol!("PassThruConnect", Connect),
            disconnect: symbol!("PassThruDisconnect", Close),
            filter: symbol!("PassThruStartMsgFilter", Filter),
            stop: symbol!("PassThruStopMsgFilter", Stop),
            read: symbol!("PassThruReadMsgs", Messages),
            write: symbol!("PassThruWriteMsgs", Messages),
            version: symbol!("PassThruReadVersion", Version),
            ioctl: symbol!("PassThruIoctl", Ioctl),
            last_error: symbol!("PassThruGetLastError", LastError),
        })
    }
    fn check(&self, call: &str, rc: u32) -> Result<(), String> {
        println!("{call} rc=0x{rc:08X}");
        if rc == 0 {
            return Ok(());
        }
        let mut error = [0 as c_char; 1024];
        unsafe {
            (self.last_error)(error.as_mut_ptr());
        }
        error[1023] = 0;
        let text = unsafe { CStr::from_ptr(error.as_ptr()) }.to_string_lossy();
        Err(format!("{call} rc=0x{rc:08X}: {text}"))
    }
}
impl Driver for Native {
    fn open(&mut self) -> Result<u32, String> {
        let mut id = 0;
        self.check("PassThruOpen", unsafe { (self.open)(ptr::null(), &mut id) })?;
        Ok(id)
    }
    fn version(&mut self, d: u32) -> Result<[String; 3], String> {
        let mut fw = [0 as c_char; 256];
        let mut dll = fw;
        let mut api = fw;
        self.check("PassThruReadVersion", unsafe {
            (self.version)(d, fw.as_mut_ptr(), dll.as_mut_ptr(), api.as_mut_ptr())
        })?;
        let mut values = [String::new(), String::new(), String::new()];
        for (i, buffer) in [&mut fw, &mut dll, &mut api].into_iter().enumerate() {
            buffer[255] = 0;
            values[i] = unsafe { CStr::from_ptr(buffer.as_ptr()) }
                .to_string_lossy()
                .into_owned();
        }
        Ok(values)
    }
    fn voltage(&mut self, d: u32) -> Result<u32, String> {
        let mut mv = 0u32;
        self.check("READ_VBATT", unsafe {
            (self.ioctl)(d, 3, ptr::null(), (&mut mv as *mut u32).cast())
        })?;
        Ok(mv)
    }
    fn connect(&mut self, d: u32) -> Result<u32, String> {
        let mut id = 0;
        self.check("PassThruConnect ISO15765 flags=0 baud=500000", unsafe {
            (self.connect)(d, profile::PROTOCOL, 0, profile::BAUD, &mut id)
        })?;
        Ok(id)
    }
    fn filter(&mut self, c: u32) -> Result<u32, String> {
        let mut mask = Message::new(0x7ff, &[]);
        let mut pattern = Message::new(profile::REPLY_ID, &[]);
        let mut flow = Message::new(profile::REQUEST_ID, &[]);
        let mut id = 0;
        self.check("PassThruStartMsgFilter FLOW_CONTROL", unsafe {
            (self.filter)(c, 3, &mut mask, &mut pattern, &mut flow, &mut id)
        })?;
        Ok(id)
    }
    fn clear_rx(&mut self, c: u32) -> Result<(), String> {
        self.check("CLEAR_RX_BUFFER", unsafe {
            (self.ioctl)(c, 8, ptr::null(), ptr::null_mut())
        })
    }
    fn write(&mut self, c: u32, m: &mut Message) -> Result<u32, String> {
        let mut count = 1;
        println!("REQUEST CAN=7E0 payload=1A90 ISO-TP=driver retries=0");
        self.check("PassThruWriteMsgs", unsafe {
            (self.write)(c, m, &mut count, 500)
        })?;
        Ok(count)
    }
    fn read(&mut self, c: u32) -> Result<Option<Message>, String> {
        let mut message = Message::default();
        let mut count = 1;
        let rc = unsafe { (self.read)(c, &mut message, &mut count, 250) };
        if rc == 9 || rc == 16 {
            return Ok(None);
        }
        self.check("PassThruReadMsgs", rc)?;
        if count > 1 {
            return Err("MDI receive count exceeds allocated buffer".into());
        }
        if count == 0 {
            return Ok(None);
        }
        println!(
            "RX protocol={} status=0x{:08X} timestamp={} size={} extra={} data={}",
            message.protocol,
            message.rx_status,
            message.timestamp,
            message.data_size,
            message.extra_data_index,
            message.data[..(message.data_size as usize).min(4128)]
                .iter()
                .map(|v| format!("{v:02X}"))
                .collect::<String>()
        );
        Ok(Some(message))
    }
    fn stop_filter(&mut self, c: u32, f: u32) -> Result<(), String> {
        self.check("PassThruStopMsgFilter", unsafe { (self.stop)(c, f) })
    }
    fn disconnect(&mut self, c: u32) -> Result<(), String> {
        self.check("PassThruDisconnect", unsafe { (self.disconnect)(c) })
    }
    fn close(&mut self, d: u32) -> Result<(), String> {
        self.check("PassThruClose", unsafe { (self.close)(d) })
    }
}
