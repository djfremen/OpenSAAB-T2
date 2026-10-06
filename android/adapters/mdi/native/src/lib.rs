// SPDX-License-Identifier: MPL-2.0
//! Android MDI module: USB carrier and packaged firmware bridge. Protocol stays in openmdi.
mod async_input;
mod discovery;
mod usb_configuration;
use serde_json::{json, Value};
use smoltcp::{
    iface::{Config, Interface, SocketHandle, SocketSet},
    phy::{Device, DeviceCapabilities, Medium, RxToken, TxToken},
    socket::tcp,
    time::Instant,
    wire::{EthernetAddress, IpAddress, IpCidr},
};
use std::{
    collections::VecDeque,
    fs::{File, OpenOptions},
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread,
    time::{Duration, Instant as WallInstant, SystemTime, UNIX_EPOCH},
};

fn word(b: &[u8], n: usize) -> Result<u32, String> {
    b.get(n..n + 4)
        .map(|x| u32::from_le_bytes(x.try_into().unwrap()))
        .ok_or_else(|| "Truncated RNDIS word".into())
}
fn words(w: &[u32]) -> Vec<u8> {
    w.iter().flat_map(|x| x.to_le_bytes()).collect()
}
fn completion(b: &[u8], kind: u32, id: u32, min: usize) -> Result<&[u8], String> {
    if b.len() < min
        || word(b, 0)? != (kind | 0x80000000)
        || word(b, 4)? as usize != b.len()
        || word(b, 8)? != id
        || word(b, 12)? != 0
    {
        return Err("RNDIS completion correlation/status/length failed".into());
    }
    Ok(b)
}
fn query_data(b: &[u8], id: u32) -> Result<&[u8], String> {
    completion(b, 4, id, 24)?;
    let n = word(b, 16)? as usize;
    let p = (word(b, 20)? as usize)
        .checked_add(8)
        .ok_or("Query offset overflow")?;
    if p < 24 {
        return Err("Query data overlaps header".into());
    }
    b.get(p..p.checked_add(n).ok_or("Query length overflow")?)
        .ok_or_else(|| "Query payload out of bounds".into())
}
fn packet(frame: &[u8]) -> Result<Vec<u8>, String> {
    if !(14..=1514).contains(&frame.len()) {
        return Err("Ethernet frame size boundary".into());
    }
    let mut b = words(&[
        1,
        (44 + frame.len()) as u32,
        36,
        frame.len() as u32,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
    ]);
    b.extend(frame);
    Ok(b)
}
fn frames(b: &[u8]) -> Result<Vec<Vec<u8>>, String> {
    let mut result = Vec::new();
    let mut p = 0;
    while p < b.len() {
        // Observed classic-MDI full-speed gadget appends one arbitrary byte to
        // avoid a terminating zero-length USB packet at a 64-byte boundary.
        // The byte is outside MessageLength and is never Ethernet payload.
        if p > 0 && p % 64 == 0 && b.len() - p == 1 {
            break;
        }
        if b[p..].iter().all(|x| *x == 0) {
            break;
        }
        let q = &b[p..];
        if q.len() < 44 || word(q, 0)? != 1 {
            return Err("Invalid RNDIS packet header".into());
        }
        let total = word(q, 4)? as usize;
        let offset = (word(q, 8)? as usize)
            .checked_add(8)
            .ok_or("Data offset overflow")?;
        let len = word(q, 12)? as usize;
        if total < 44
            || total > q.len()
            || offset < 44
            || offset % 4 != 0
            || !(14..=1514).contains(&len)
            || offset.checked_add(len).ok_or("Data length overflow")? > total
            || q[16..44].iter().any(|x| *x != 0)
        {
            return Err("RNDIS packet bounds/unsupported metadata".into());
        }
        result.push(q[offset..offset + len].to_vec());
        p += total;
        if result.len() > 32 {
            return Err("RNDIS bundle boundary".into());
        }
    }
    Ok(result)
}
#[derive(Default)]
struct BulkInput {
    pending: Vec<u8>,
}
impl BulkInput {
    fn push(&mut self, chunk: &[u8]) -> Result<Vec<Vec<u8>>, String> {
        if chunk.len() > 64 || self.pending.len() + chunk.len() > 1580 {
            return Err("USB input assembly boundary".into());
        }
        self.pending.extend_from_slice(chunk);
        if chunk.len() == 64 {
            return Ok(Vec::new());
        }
        // A short packet (including ZLP) terminates this USB transfer. Keep
        // complete full-speed packets across idle poll timeouts until then.
        let result = frames(&self.pending);
        self.pending.clear();
        result
    }
}
fn new_file(p: &Path) -> Result<File, String> {
    use std::os::unix::fs::OpenOptionsExt;
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(p)
        .map_err(|e| format!("Exclusive evidence file: {e}"))
}
fn save(p: &Path, v: &Value) -> Result<(), String> {
    let mut f = new_file(p)?;
    f.write_all(&serde_json::to_vec_pretty(v).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}
struct Evidence {
    capture: bool,
    controls: File,
    usb: File,
    pcap: File,
    bytes: usize,
    limit: usize,
    rx: u64,
    tx: u64,
}
impl Evidence {
    fn new(p: &Path, firmware: bool, capture: bool) -> Result<Self, String> {
        let mut pcap = new_file(&p.join("ethernet-private.pcap"))?;
        pcap.write_all(&words(&[0xa1b2c3d4, 0x00040002, 0, 0, 65535, 1]))
            .map_err(|e| e.to_string())?;
        Ok(Self {
            capture,
            controls: new_file(&p.join("control-private.jsonl"))?,
            usb: new_file(&p.join("rndis-private.bin"))?,
            pcap,
            bytes: 24,
            limit: if firmware {
                128 * 1024 * 1024
            } else {
                10 * 1024 * 1024
            },
            rx: 0,
            tx: 0,
        })
    }
    fn raw(&mut self, kind: u32, b: &[u8]) -> Result<(), String> {
        if !self.capture {
            return Ok(());
        }
        if self.bytes + b.len() + 16 > self.limit {
            return Err("Private trace limit reached".into());
        }
        self.usb
            .write_all(&words(&[kind, b.len() as u32]))
            .and_then(|_| self.usb.write_all(b))
            .map_err(|e| e.to_string())?;
        self.bytes += b.len() + 8;
        Ok(())
    }
    fn ethernet(&mut self, out: bool, b: &[u8]) -> Result<(), String> {
        if !self.capture {
            if out {
                self.tx += 1;
            } else {
                self.rx += 1;
            }
            return Ok(());
        }
        let t = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?;
        self.pcap
            .write_all(&words(&[
                t.as_secs() as u32,
                t.subsec_micros(),
                b.len() as u32,
                b.len() as u32,
            ]))
            .and_then(|_| self.pcap.write_all(b))
            .map_err(|e| e.to_string())?;
        self.bytes += b.len() + 16;
        if out {
            self.tx += 1
        } else {
            self.rx += 1
        }
        Ok(())
    }
    fn control(&mut self, request: &[u8], reply: &[u8]) -> Result<(), String> {
        self.raw(0, request)?;
        self.raw(1, reply)?;
        let v = json!({"request_words":request.as_chunks::<4>().0.iter().map(|x|u32::from_le_bytes(*x)).collect::<Vec<_>>(),"reply_words":reply.as_chunks::<4>().0.iter().map(|x|u32::from_le_bytes(*x)).collect::<Vec<_>>()});
        writeln!(self.controls, "{v}").map_err(|e| e.to_string())
    }
}
#[repr(C)]
struct Control {
    kind: u8,
    request: u8,
    value: u16,
    index: u16,
    length: u16,
    timeout: u32,
    data: *mut libc::c_void,
}
#[repr(C)]
struct Bulk {
    endpoint: u32,
    length: u32,
    timeout: u32,
    data: *mut libc::c_void,
}
fn ioctl_request(n: u32, size: usize) -> libc::c_ulong {
    ((3u32 << 30) | ((size as u32) << 16) | (0x55 << 8) | n) as libc::c_ulong
}
struct Usb {
    fd: i32,
    id: u32,
    evidence: Evidence,
}
impl Drop for Usb {
    fn drop(&mut self) {
        unsafe {
            libc::close(self.fd);
        }
    }
}
impl Usb {
    fn control(&self, kind: u8, request: u8, b: &mut [u8]) -> Result<usize, String> {
        let mut c = Control {
            kind,
            request,
            value: 0,
            index: 0,
            length: b.len() as u16,
            timeout: 1000,
            data: b.as_mut_ptr().cast(),
        };
        let n = unsafe {
            libc::ioctl(
                self.fd,
                ioctl_request(0, std::mem::size_of::<Control>()) as _,
                &mut c,
            )
        };
        if n < 0 {
            Err(format!("USB control: {}", std::io::Error::last_os_error()))
        } else {
            Ok(n as usize)
        }
    }
    fn exchange(&mut self, mut b: Vec<u8>) -> Result<Vec<u8>, String> {
        if self.control(0x21, 0, &mut b)? != b.len() {
            return Err("Short USB control send".into());
        }
        let deadline = WallInstant::now() + Duration::from_secs(2);
        loop {
            let mut reply = vec![0; 4096];
            let n = self.control(0xa1, 1, &mut reply)?;
            reply.truncate(n);
            if n >= 8 && word(&reply, 0)? == 0x80000007 {
                self.evidence.raw(1, &reply)?;
                continue;
            }
            if n >= 16 {
                self.evidence.control(&b, &reply)?;
                return Ok(reply);
            }
            if WallInstant::now() >= deadline {
                return Err("RNDIS response deadline".into());
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
    fn query(&mut self, oid: u32) -> Result<Vec<u8>, String> {
        self.id += 1;
        let id = self.id;
        let b = self.exchange(words(&[4, 28, id, oid, 0, 20, 0]))?;
        Ok(query_data(&b, id)?.to_vec())
    }
    fn filter(&mut self, value: u32) -> Result<(), String> {
        self.id += 1;
        let id = self.id;
        let b = self.exchange(words(&[5, 32, id, 0x0001010e, 4, 20, 0, value]))?;
        completion(&b, 5, id, 16)?;
        Ok(())
    }
    fn keepalive(&mut self) -> Result<(), String> {
        self.id += 1;
        let id = self.id;
        let b = self.exchange(words(&[8, 12, id]))?;
        completion(&b, 8, id, 16)?;
        Ok(())
    }
    fn bulk(&self, endpoint: u32, b: &mut [u8], timeout: u32) -> Result<usize, std::io::Error> {
        let mut x = Bulk {
            endpoint,
            length: b.len() as u32,
            timeout,
            data: b.as_mut_ptr().cast(),
        };
        let n = unsafe {
            libc::ioctl(
                self.fd,
                ioctl_request(2, std::mem::size_of::<Bulk>()) as _,
                &mut x,
            )
        };
        if n < 0 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(n as usize)
        }
    }
    fn send(&mut self, frame: &[u8]) -> Result<(), String> {
        let mut b = packet(frame)?;
        self.evidence.raw(2, &b)?;
        self.evidence.ethernet(true, frame)?;
        if self
            .bulk(2, &mut b, 1000)
            .map_err(|e| format!("Bulk OUT: {e}"))?
            != b.len()
        {
            return Err("Short bulk OUT".into());
        }
        Ok(())
    }
    fn initialize(&mut self) -> Result<Value, String> {
        self.id += 1;
        let id = self.id;
        let b = self.exchange(words(&[2, 24, id, 1, 0, 1580]))?;
        completion(&b, 2, id, 52)?;
        if word(&b, 16)? != 1 || word(&b, 20)? != 0 || word(&b, 28)? != 0 || word(&b, 36)? < 1580 {
            return Err("Unsupported RNDIS initialization profile".into());
        }
        let mac = self.query(0x01010102)?;
        if mac.len() != 6 || mac.iter().all(|x| *x == 0) {
            return Err("Invalid current MAC".into());
        }
        let frame = self.query(0x00010106)?;
        let media = self.query(0x00010114)?;
        let speed = self.query(0x00010107)?;
        Ok(
            json!({"version":"1.0","max_transfer_size":word(&b,36)?,"adapter_mac_private":mac,"max_frame_size":word(&frame,0)?,"media_state":word(&media,0)?,"link_speed_100bps":word(&speed,0)?,"filter":15,"discovery_multicast":"all_multicast_software_validated"}),
        )
    }
}
struct Carrier {
    usb: Usb,
    queue: VecDeque<Vec<u8>>,
    input: BulkInput,
    receiver: async_input::Input,
    error: Option<String>,
    complete_transfers: bool,
    identity_sender: Option<mpsc::SyncSender<u32>>,
}
impl Carrier {
    fn ingest(&mut self, bytes: &[u8]) -> Result<Vec<Vec<u8>>, String> {
        if self.complete_transfers {
            let mut record = words(&[2048, bytes.len() as u32]);
            record.extend(bytes);
            self.usb.evidence.raw(6, &record)?;
            complete_transfer_frames(bytes)
        } else {
            self.usb.evidence.raw(3, bytes)?;
            self.input.push(bytes)
        }
    }
}
fn complete_transfer_frames(bytes: &[u8]) -> Result<Vec<Vec<u8>>, String> {
    // The 2048-byte request exceeds the negotiated 1580-byte RNDIS transfer.
    // A successful shorter completion includes the terminating short packet/ZLP.
    // Journal kind6 records the whole URB; it is not an external packet capture.
    if bytes.len() > 1580 {
        return Err("Whole USB transfer exceeds negotiated size".into());
    }
    frames(bytes)
}
struct Rx(Vec<u8>);
struct Tx<'a>(&'a mut Carrier);
impl RxToken for Rx {
    fn consume<R, F>(self, f: F) -> R
    where
        F: FnOnce(&[u8]) -> R,
    {
        f(&self.0)
    }
}
impl TxToken for Tx<'_> {
    fn consume<R, F>(self, len: usize, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        let mut b = vec![0; len];
        let r = f(&mut b);
        if let Err(e) = self.0.usb.send(&b) {
            self.0.error = Some(e)
        }
        r
    }
}
impl Device for Carrier {
    type RxToken<'a>
        = Rx
    where
        Self: 'a;
    type TxToken<'a>
        = Tx<'a>
    where
        Self: 'a;
    fn capabilities(&self) -> DeviceCapabilities {
        let mut c = DeviceCapabilities::default();
        c.medium = Medium::Ethernet;
        c.max_transmission_unit = 1514;
        c.max_burst_size = Some(1);
        c
    }
    fn receive(&mut self, _: Instant) -> Option<(Rx, Tx<'_>)> {
        if self.error.is_some() {
            return None;
        }
        if self.queue.is_empty() {
            // Retain the queued IN URB across idle polls. Synchronous ioctl
            // cancellation can hide actual_length even for a 64-byte request.
            match self.receiver.poll() {
                Ok(Some(b)) => {
                    let outcome = self.ingest(&b);
                    match outcome {
                        Ok(fs) => {
                            for f in fs {
                                if let Err(e) = self.usb.evidence.ethernet(false, &f) {
                                    self.error = Some(e);
                                    return None;
                                }
                                if let Some(serial) = discovery::announced_serial(&f) {
                                    if let Some(sender) = self.identity_sender.take() {
                                        let _ = sender.try_send(serial);
                                    }
                                }
                                self.queue.push_back(f)
                            }
                        }
                        Err(e) => self.error = Some(e),
                    }
                }
                Ok(None) => {}
                Err(e) => self.error = Some(format!("Bulk IN: {e}")),
            }
        }
        self.queue.pop_front().map(|f| (Rx(f), Tx(self)))
    }
    fn transmit(&mut self, _: Instant) -> Option<Tx<'_>> {
        if self.error.is_some() {
            None
        } else {
            Some(Tx(self))
        }
    }
}
struct Forward {
    local: TcpStream,
    handle: SocketHandle,
    to_device: VecDeque<u8>,
    to_local: VecDeque<u8>,
    eof: bool,
    created: WallInstant,
    was_established: bool,
}
#[allow(clippy::too_many_arguments)] // Thin JNI binding supplies explicit app-owned session inputs.
fn run(
    fd: i32,
    mode: i32,
    dir: &Path,
    executable: &Path,
    firmware: &Path,
    authority: &str,
    capture: bool,
) -> Result<Value, String> {
    if !(0..=6).contains(&mode) {
        return Err("Explicit mode required".into());
    }
    if mode != 6 || !matches!(authority, "full" | "seeds") {
        return Err("Exclusive MDI firmware module configuration required".into());
    }
    phase(dir, "opening", "Initializing MDI USB transport")?;
    let evidence = Evidence::new(dir, true, capture)?;
    let copied = unsafe { libc::dup(fd) };
    if copied < 0 {
        return Err("USB descriptor duplication failed".into());
    }
    let mut usb = Usb {
        fd: copied,
        id: 0x60000000,
        evidence,
    };
    let initialized = usb.initialize()?;
    // Establish a transfer boundary before opening any management/TCP client.
    // Old endpoint tails are quarantined and recorded, never parsed as new data.
    usb.filter(0)?;
    let mut receiver = if mode == 6 {
        async_input::Input::with_capacity(copied, 2048)?
    } else {
        async_input::Input::new(copied)?
    };
    let deadline = WallInstant::now() + Duration::from_millis(150);
    let mut idle = WallInstant::now();
    let mut startup_bytes = 0;
    let mut at_boundary = true;
    loop {
        if let Some(bytes) = receiver.poll()? {
            if mode == 6 {
                let mut record = words(&[2048, bytes.len() as u32]);
                record.extend(&bytes);
                usb.evidence.raw(7, &record)?;
            } else {
                usb.evidence.raw(4, &bytes)?;
            }
            startup_bytes += bytes.len();
            at_boundary = bytes.len() < if mode == 6 { 2048 } else { 64 };
            idle = WallInstant::now();
            if startup_bytes > 25280 {
                return Err("Startup USB drain byte boundary".into());
            }
        }
        if at_boundary && idle.elapsed() >= Duration::from_millis(10) {
            break;
        }
        if WallInstant::now() >= deadline {
            return Err("Startup USB drain deadline".into());
        }
        thread::sleep(Duration::from_millis(1));
    }
    // Native Windows startup ends with directed/multicast/all-multicast/broadcast
    // (15). Filter 11 alone depends on a pre-existing multicast address list;
    // a new initializer must receive discovery without inheriting that state.
    // Only validated announcements from this USB adapter supply its identity.
    usb.filter(15)?;
    save(&dir.join("rndis-init-private.json"), &initialized)?;
    let (identity_sender, identity_receiver) = mpsc::sync_channel(1);
    let mut carrier = Carrier {
        usb,
        queue: VecDeque::new(),
        input: BulkInput::default(),
        receiver,
        error: None,
        complete_transfers: mode == 6,
        identity_sender: Some(identity_sender),
    };
    // An isolated userspace subnet. No Android kernel route or root privilege is needed.
    let mut config = Config::new(EthernetAddress([0x02, 0x4f, 0x4d, 0x44, 0x49, 0x07]).into());
    config.random_seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos() as u64;
    let mut iface = Interface::new(config, &mut carrier, Instant::from_millis(0));
    iface.update_ip_addrs(|a| {
        a.push(IpCidr::new(IpAddress::v4(192, 168, 171, 30), 24))
            .unwrap()
    });
    // Windows discovery joins this group. Passive reception can depend on
    // membership retained by the adapter's bridge; announce our membership.
    join_discovery(&mut iface)?;
    let mut sockets = SocketSet::new(vec![]);
    let ports = [9000u16, 9002, 9004, 10123];
    let mut listeners = Vec::new();
    for port in ports {
        let listener = TcpListener::bind(("127.0.0.1", port))
            .map_err(|e| format!("Exclusive loopback port {port}: {e}"))?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        listeners.push((port, listener));
    }
    let done = Arc::new(AtomicBool::new(false));
    let completed = done.clone();
    let aborted = Arc::new(AtomicBool::new(false));
    let worker_aborted = aborted.clone();
    let executable = executable.to_path_buf();
    let worker_dir = dir.to_path_buf();
    let firmware = firmware.to_path_buf();
    let authority = authority.to_owned();
    let worker = thread::spawn(move || {
        let result = (|| -> Result<Value, String> {
            let ip = "127.0.0.1".parse().unwrap();
            phase(
                &worker_dir,
                "adapter_identity",
                "Retrieving MDI serial number",
            )?;
            let identity_started = WallInstant::now();
            let serial = loop {
                if worker_dir.join("session-stop").exists()
                    || worker_aborted.load(Ordering::Acquire)
                {
                    return Err("Stopped during MDI identity retrieval".into());
                }
                if identity_started.elapsed() >= Duration::from_secs(8) {
                    return Err(
                        "MDI serial retrieval timed out; no management or vehicle request sent"
                            .into(),
                    );
                }
                match identity_receiver.recv_timeout(Duration::from_millis(50)) {
                    Ok(serial) => break serial,
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                        return Err("MDI identity transport ended".into())
                    }
                }
            };
            save(
                &worker_dir.join("identity-private.json"),
                &json!({"schema":1,"serial":serial,
                "source":"fresh_classic_mdi_announcement","opcode":0x86d,"version":7,
                "elapsed_ms":identity_started.elapsed().as_millis(),"saved_serial_used":false,
                "USB_descriptor_serial_used":false,"vehicle_requests":0,"management_requests":0,
                "management_bootstrap":"built_in_classic_mdi_2.5.33.154","imported_profile_used":false}),
            )?;
            if let Err(error) = validate_firmware(&executable, &firmware) {
                return Ok(
                    json!({"success":false,"setup_needed":"firmware","error":error,"adapter_identity_retrieved":true,"vehicle_requests":0,"management_requests":0}),
                );
            }
            if worker_dir.join("session-stop").exists() {
                return Err("Stopped before management handshake".into());
            }
            phase(&worker_dir, "handshake", "Initializing MDI connection")?;
            let label = format!(
                "OpenMDI-Android-{}",
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_err(|e| e.to_string())?
                    .as_nanos()
            );
            let action = if mode == 0 {
                "query"
            } else if matches!(mode, 1 | 3 | 4 | 5 | 6) {
                "start"
            } else {
                "reboot"
            };
            let management = openmdi::classic_mdi_bootstrap::attempt(ip, serial, action, &label);
            save(&worker_dir.join("management-private.json"), &management)?;
            if management["success"] != true {
                return Ok(json!({"success":false,"management":management,"vehicle_requests":0}));
            }
            if worker_dir.join("session-stop").exists() {
                return Err("Stopped before vehicle identification".into());
            }
            phase(
                &worker_dir,
                "identifying",
                "MDI connected · Reading a fresh vehicle VIN",
            )?;
            if matches!(mode, 1 | 3 | 4 | 5 | 6) {
                let attempt = openmdi::client::read_vin(std::net::SocketAddr::new(ip, 10123));
                save(&worker_dir.join("vin-private.json"), &attempt.report)?;
                if mode == 6 && attempt.passed {
                    if worker_dir.join("session-stop").exists() {
                        return Err("Stopped before firmware launch".into());
                    }
                    phase(
                        &worker_dir,
                        "firmware",
                        "Vehicle identified · Starting original firmware",
                    )?;
                    let native = run_firmware(
                        &worker_dir,
                        &executable,
                        &firmware,
                        &authority,
                        &worker_aborted,
                    )?;
                    Ok(
                        json!({"success":native["session_end_ok"]==true,"native_firmware":native,"management":management,"vin":attempt.report,"security_grant":false,"spa_add_confirmed":false}),
                    )
                } else if mode == 5 && attempt.passed {
                    let spa = openmdi::client::read_spa_raw(std::net::SocketAddr::new(ip, 10123));
                    save(&worker_dir.join("spa-raw-private.json"), &spa.report)?;
                    Ok(
                        json!({"success":spa.passed,"management":management,"vin":attempt.report,"spa_raw":spa.report}),
                    )
                } else if mode == 4 && attempt.passed {
                    let spa = openmdi::client::read_spa_preconditions(std::net::SocketAddr::new(
                        ip, 10123,
                    ));
                    save(
                        &worker_dir.join("spa-preconditions-private.json"),
                        &spa.report,
                    )?;
                    Ok(
                        json!({"success":spa.passed,"management":management,"vin":attempt.report,"spa":spa.report}),
                    )
                } else if mode == 3 && attempt.passed {
                    let live = openmdi::client::read_ecm_live(std::net::SocketAddr::new(ip, 10123));
                    save(&worker_dir.join("ecm-live-private.json"), &live.report)?;
                    Ok(
                        json!({"success":live.passed,"management":management,"vin":attempt.report,"ecm_live":live.report}),
                    )
                } else {
                    Ok(
                        json!({"success":attempt.passed,"management":management,"vin":attempt.report}),
                    )
                }
            } else {
                Ok(json!({"success":true,"management":management,"vehicle_requests":0}))
            }
        })();
        completed.store(true, Ordering::Release);
        result
    });
    let origin = WallInstant::now();
    let mut forwards = Vec::<Forward>::new();
    let mut ephemeral = 38000u16;
    let mut keepalive = origin;
    let mut drain = None;
    let mut connections = 0;
    let mut established = 0;
    loop {
        if origin.elapsed() > Duration::from_secs(if mode == 6 { 900 } else { 40 }) {
            carrier.error = Some("Whole probe deadline reached".into());
            break;
        }
        // Keep the carrier alive while the guest performs its normal protocol cleanup.
        if dir.join("session-stop").exists() {
            aborted.store(true, Ordering::Release);
        }
        let now = Instant::from_millis(origin.elapsed().as_millis() as i64);
        for (port, listener) in &listeners {
            match listener.accept() {
                Ok((local, _)) => {
                    if forwards.len() >= 8 {
                        carrier.error = Some("Connection count boundary".into());
                        break;
                    }
                    local.set_nonblocking(true).map_err(|e| e.to_string())?;
                    local.set_nodelay(true).map_err(|e| e.to_string())?;
                    let mut socket = tcp::Socket::new(
                        tcp::SocketBuffer::new(vec![0; 32768]),
                        tcp::SocketBuffer::new(vec![0; 32768]),
                    );
                    socket.set_timeout(Some(smoltcp::time::Duration::from_secs(12)));
                    if mode == 6 {
                        socket.set_keep_alive(Some(smoltcp::time::Duration::from_secs(3)));
                    }
                    socket.set_nagle_enabled(false);
                    ephemeral += 1;
                    socket
                        .connect(
                            iface.context(),
                            (IpAddress::v4(192, 168, 171, 2), *port),
                            ephemeral,
                        )
                        .map_err(|e| format!("Userspace TCP connect: {e:?}"))?;
                    let handle = sockets.add(socket);
                    forwards.push(Forward {
                        local,
                        handle,
                        to_device: VecDeque::new(),
                        to_local: VecDeque::new(),
                        eof: false,
                        created: WallInstant::now(),
                        was_established: false,
                    });
                    connections += 1;
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(e) => {
                    carrier.error = Some(format!("Loopback accept: {e}"));
                    break;
                }
            }
        }
        iface.poll(now, &mut carrier, &mut sockets);
        for f in &mut forwards {
            let socket = sockets.get_mut::<tcp::Socket>(f.handle);
            if socket.state() == tcp::State::Established && !f.was_established {
                f.was_established = true;
                established += 1
            }
            if !f.eof && f.to_device.len() < 32768 {
                let mut buf = [0; 4096];
                match f.local.read(&mut buf) {
                    Ok(0) => f.eof = true,
                    Ok(n) => f.to_device.extend(&buf[..n]),
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(_) => {
                        f.eof = true;
                        socket.abort();
                    }
                }
            }
            if !f.to_device.is_empty() && socket.can_send() {
                let b = f.to_device.make_contiguous();
                let n = socket
                    .send_slice(b)
                    .map_err(|e| format!("Userspace send: {e:?}"))?;
                f.to_device.drain(..n);
            }
            if socket.can_recv() && f.to_local.len() < 32768 {
                socket
                    .recv(|b| {
                        f.to_local.extend(b.iter().copied());
                        (b.len(), ())
                    })
                    .map_err(|e| format!("Userspace receive: {e:?}"))?;
            }
            if !f.to_local.is_empty() {
                match f.local.write(f.to_local.make_contiguous()) {
                    Ok(n) => {
                        f.to_local.drain(..n);
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(_) => socket.abort(),
                }
            }
            if f.eof && f.to_device.is_empty() && socket.may_send() {
                socket.close()
            }
            if !socket.is_open() && f.created.elapsed() > Duration::from_millis(200) {
                let _ = f.local.shutdown(std::net::Shutdown::Both);
            }
        }
        forwards.retain(|f| {
            let keep = sockets.get::<tcp::Socket>(f.handle).is_open()
                || f.created.elapsed() < Duration::from_millis(200);
            if !keep {
                sockets.remove(f.handle);
            }
            keep
        });
        iface.poll(now, &mut carrier, &mut sockets);
        if carrier.error.is_some() {
            break;
        }
        if keepalive.elapsed() > Duration::from_secs(3) && !done.load(Ordering::Acquire) {
            if let Err(e) = carrier.usb.keepalive() {
                carrier.error = Some(e);
                break;
            }
            keepalive = WallInstant::now();
        }
        if done.load(Ordering::Acquire) {
            let start = drain.get_or_insert_with(WallInstant::now);
            if forwards.is_empty() || start.elapsed() > Duration::from_secs(2) {
                break;
            }
        }
        thread::sleep(Duration::from_millis(1));
    }
    aborted.store(true, Ordering::Release);
    // Close local streams before joining to unblock a failed client. Never replay an application request.
    for f in forwards {
        let _ = f.local.shutdown(std::net::Shutdown::Both);
    }
    drop(listeners);
    let workflow = worker.join().map_err(|_| "Shared core thread panicked")?;
    let multicast_tx_before_leave = carrier.usb.evidence.tx;
    let discovery_leave_requested = iface
        .leave_multicast_group(IpAddress::v4(225, 1, 1, 1))
        .is_ok();
    if discovery_leave_requested {
        iface.poll(
            Instant::from_millis(origin.elapsed().as_millis() as i64),
            &mut carrier,
            &mut sockets,
        );
    }
    let discovery_left = discovery_leave_requested
        && carrier.usb.evidence.tx > multicast_tx_before_leave
        && carrier.error.is_none();
    let filter_off = carrier.usb.filter(0).is_ok();
    let mut input_reaped = false;
    match carrier.receiver.stop() {
        Ok(bytes) => {
            input_reaped = true;
            if let Some((status, bytes)) = bytes {
                if status == 0 {
                    match carrier.ingest(&bytes) {
                        Ok(fs) => {
                            for f in fs {
                                carrier.usb.evidence.ethernet(false, &f)?;
                            }
                        }
                        Err(e) => {
                            if carrier.error.is_none() {
                                carrier.error = Some(e);
                            }
                        }
                    }
                } else {
                    let mut completion = words(&[status as u32, bytes.len() as u32]);
                    completion.extend(&bytes);
                    carrier.usb.evidence.raw(5, &completion)?;
                    // Cancellation with no bytes is not a USB zero-length
                    // packet and must never complete a pending transfer.
                    if !bytes.is_empty() && carrier.error.is_none() {
                        carrier.error =
                            Some("Terminal USB cancellation retained partial data".into());
                    }
                }
            }
        }
        Err(e) => {
            if carrier.error.is_none() {
                carrier.error = Some(e);
            }
        }
    }
    if !carrier.input.pending.is_empty() && carrier.error.is_none() {
        carrier.error = Some("Incomplete terminal USB transfer".into());
    }
    let mut halt = words(&[3, 12, 0]);
    let halt_sent = carrier
        .usb
        .control(0x21, 0, &mut halt)
        .map(|n| n == halt.len())
        .unwrap_or(false);
    carrier.usb.evidence.raw(0, &halt)?;
    let outcome = workflow.unwrap_or_else(|e| json!({"success":false,"error":e}));
    let result = json!({"schema":1,"mode":mode,"success":outcome["success"]==true&&carrier.error.is_none()&&filter_off&&halt_sent&&input_reaped&&discovery_left,"elapsed_ms":origin.elapsed().as_millis(),"rndis":initialized,"tcp_connections":connections,"tcp_established":established,"ethernet_rx":carrier.usb.evidence.rx,"ethernet_tx":carrier.usb.evidence.tx,"carrier_error":carrier.error,"filter_disabled":filter_off,"halt_sent":halt_sent,"USB_IN_cancel_reaped":input_reaped,"discovery_multicast_joined":true,"discovery_multicast_left":discovery_left,"startup_quarantined_bytes":startup_bytes,"workflow":outcome,"receive_journal_granularity":if mode==6 {"whole-2048-byte-URB-completion"} else {"64-byte-read-completion"},"kernel_network_route_used":false,"vendor_driver_used":false,"root_used":false,"structured_capture_enabled":capture});
    Ok(result)
}

fn join_discovery(iface: &mut Interface) -> Result<(), String> {
    iface
        .join_multicast_group(IpAddress::v4(225, 1, 1, 1))
        .map_err(|e| format!("MDI discovery multicast join: {e}"))
}

fn phase(directory: &Path, state: &str, message: &str) -> Result<(), String> {
    let temporary = directory.join("session-state.tmp");
    std::fs::write(
        &temporary,
        serde_json::to_vec(&json!({"state":state,"message":message})).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    std::fs::rename(temporary, directory.join("session-state.json")).map_err(|e| e.to_string())
}
fn validate_firmware(executable: &Path, firmware: &Path) -> Result<(), String> {
    if executable.file_name().and_then(|s| s.to_str()) != Some("libtech2_mdi.so")
        || !executable.is_file()
    {
        return Err("Packaged MDI firmware bridge missing".into());
    }
    for (name, length) in [
        ("eprom.bin", 262144u64),
        ("opsys.dwn", 229352),
        ("candi.bin", 65544),
        ("card.bin", 33554432),
    ] {
        if std::fs::metadata(firmware.join(name))
            .map_err(|_| format!("Firmware missing: {name}"))?
            .len()
            != length
        {
            return Err(format!("Firmware length boundary: {name}"));
        }
    }
    Ok(())
}

fn run_firmware(
    directory: &Path,
    executable: &Path,
    firmware: &Path,
    authority: &str,
    aborted: &AtomicBool,
) -> Result<Value, String> {
    use std::process::{Command, Stdio};
    let output = directory.join("native");
    std::fs::create_dir_all(&output).map_err(|e| e.to_string())?;
    let log = new_file(&directory.join("native-process-private.log"))?;
    let err = log.try_clone().map_err(|e| e.to_string())?;
    let mut child = Command::new(executable)
        .args([
            "--test-harness",
            "--harness-target",
            "native-manual",
            "--headless",
            "--candi-native-link",
            "--candi-mdi-mode",
            authority,
        ])
        .arg("--candi-firmware")
        .arg(firmware.join("candi.bin"))
        .arg("--boot")
        .arg(firmware.join("eprom.bin"))
        .arg("--opsys")
        .arg(firmware.join("opsys.dwn"))
        .args(["--max-insns", "50000000000", "--output-dir"])
        .arg(&output)
        .arg(firmware.join("card.bin"))
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(err))
        .spawn()
        .map_err(|e| format!("Original firmware launch: {e}"))?;
    let start = WallInstant::now();
    let mut stopped = None;
    let mut forced = false;
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            break status;
        }
        if (aborted.load(Ordering::Acquire) || start.elapsed() > Duration::from_secs(870))
            && stopped.is_none()
        {
            std::fs::write(output.join("native-stop"), b"stop\n").map_err(|e| e.to_string())?;
            stopped = Some(WallInstant::now());
        }
        if stopped.is_some_and(|t| t.elapsed() > Duration::from_secs(5)) {
            child.kill().map_err(|e| e.to_string())?;
            forced = true;
            break child.wait().map_err(|e| e.to_string())?;
        }
        thread::sleep(Duration::from_millis(25));
    };
    let cleanup: Value = serde_json::from_slice(
        &std::fs::read(output.join("mdi-cleanup-private.json")).unwrap_or_default(),
    )
    .unwrap_or(Value::Null);
    let report: Value =
        serde_json::from_slice(&std::fs::read(output.join("report.json")).unwrap_or_default())
            .unwrap_or(Value::Null);
    let expected_stop = directory.join("session-stop").exists()
        && report["reason"]
            .as_str()
            .is_some_and(|s| s.contains("Native Android session stopped by operator"));
    let clean = !forced && cleanup["cleanup_ok"] == true;
    Ok(
        json!({"process_exit":status.code(),"forced_kill":forced,"elapsed_ms":start.elapsed().as_millis(),
        "transport_cleanup_ok":clean,"session_end_ok":clean && (status.success() || expected_stop),
        "operator_stop_requested":directory.join("session-stop").exists(),"firmware_reason":report["reason"],
        "cleanup":cleanup,"original_result_requires_review":true}),
    )
}

#[no_mangle]
pub extern "system" fn Java_com_opensaab_usb_MdiNative_nativeSelectConfiguration(
    _env: jni::JNIEnv,
    _class: jni::objects::JClass,
    fd: i32,
    configuration: i32,
) -> i32 {
    usb_configuration::select(fd, configuration)
}

#[no_mangle]
pub extern "system" fn Java_com_opensaab_usb_MdiNative_nativeReleaseForConfiguration(
    _env: jni::JNIEnv,
    _class: jni::objects::JClass,
    fd: i32,
    interface: i32,
) -> i32 {
    usb_configuration::release(fd, interface)
}

#[no_mangle]
pub extern "system" fn Java_com_opensaab_usb_MdiNative_nativeRun(
    mut env: jni::JNIEnv,
    _class: jni::objects::JClass,
    fd: i32,
    directory: jni::objects::JString,
    executable: jni::objects::JString,
    firmware: jni::objects::JString,
    authority: jni::objects::JString,
    capture: jni::sys::jboolean,
) -> i32 {
    let dir: PathBuf = match env.get_string(&directory) {
        Ok(s) => PathBuf::from(String::from(s)),
        Err(_) => return 2,
    };
    let executable: PathBuf = match env.get_string(&executable) {
        Ok(s) => PathBuf::from(String::from(s)),
        Err(_) => return 2,
    };
    let firmware: PathBuf = match env.get_string(&firmware) {
        Ok(s) => PathBuf::from(String::from(s)),
        Err(_) => return 2,
    };
    let authority: String = match env.get_string(&authority) {
        Ok(s) => String::from(s),
        Err(_) => return 2,
    };
    let result = std::panic::catch_unwind(|| {
        run(
            fd,
            6,
            &dir,
            &executable,
            &firmware,
            &authority,
            capture != 0,
        )
    })
    .unwrap_or_else(|_| Err("Native probe panic".into()));
    let report = match result {
        Ok(r) => r,
        Err(e) => json!({"success":false,"error":e}),
    };
    let code = if report["success"] == true { 0 } else { 1 };
    if save(&dir.join("result-private.json"), &report).is_err() {
        return 2;
    }
    code
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn packet_roundtrip_and_bundle() {
        let f = vec![0x41; 60];
        let b = packet(&f).unwrap();
        assert_eq!(frames(&b).unwrap(), vec![f.clone()]);
        let mut combined = b.clone();
        combined.extend(b);
        assert_eq!(frames(&combined).unwrap(), vec![f.clone(), f]);
    }
    #[test]
    fn offsets_and_metadata_rejected() {
        let mut b = packet(&[0; 60]).unwrap();
        b[8..12].copy_from_slice(&32u32.to_le_bytes());
        assert!(frames(&b).is_err());
        let mut b = packet(&[0; 60]).unwrap();
        b[16] = 1;
        assert!(frames(&b).is_err());
        let mut b = packet(&[0; 60]).unwrap();
        b[4..8].copy_from_slice(&10000u32.to_le_bytes());
        assert!(frames(&b).is_err());
    }
    #[test]
    fn completion_matches_type_id_status_and_length() {
        let b = words(&[0x80000004, 28, 123, 0, 4, 16, 1500]);
        assert_eq!(word(query_data(&b, 123).unwrap(), 0).unwrap(), 1500);
        assert!(query_data(&b, 124).is_err());
        let mut bad = b;
        bad[12] = 1;
        assert!(query_data(&bad, 123).is_err());
    }
    #[test]
    fn query_header_overlap_rejected() {
        assert!(query_data(&words(&[0x80000004, 28, 1, 0, 4, 0, 0]), 1).is_err());
    }
    #[test]
    fn truncated_and_overlong_frames_rejected() {
        assert!(frames(&[1; 43]).is_err());
        assert!(packet(&[0; 1515]).is_err());
        assert!(packet(&[0; 13]).is_err());
    }
    #[test]
    fn only_one_terminal_usb_padding_byte_at_full_packet_boundary_is_accepted() {
        let frame = vec![0x41; 148];
        let mut b = packet(&frame).unwrap();
        assert_eq!(b.len(), 192);
        b.push(0xb8);
        assert_eq!(frames(&b).unwrap(), vec![frame]);
        b.push(0xb8);
        assert!(frames(&b).is_err());
        let mut non_boundary = packet(&[0x41; 149]).unwrap();
        non_boundary.push(0xb8);
        assert!(frames(&non_boundary).is_err());
        assert!(frames(&[0xb8]).is_err());
    }
    #[test]
    fn full_speed_fragments_survive_idle_gaps_until_short_packet() {
        let frame = vec![0x41; 148];
        let mut bytes = packet(&frame).unwrap();
        bytes.push(0xb8);
        let mut input = BulkInput::default();
        for chunk in bytes[..192].chunks(64) {
            assert!(input.push(chunk).unwrap().is_empty());
        }
        assert_eq!(input.pending.len(), 192);
        assert_eq!(input.push(&bytes[192..]).unwrap(), vec![frame]);
        assert!(input.pending.is_empty());
        let aligned = packet(&[0x41; 84]).unwrap();
        for chunk in aligned.chunks(64) {
            assert!(input.push(chunk).unwrap().is_empty());
        }
        assert_eq!(input.push(&[]).unwrap(), vec![vec![0x41; 84]]);
        assert!(input.push(&[1; 65]).is_err());
        for _ in 0..24 {
            input.push(&[1; 64]).unwrap();
        }
        assert!(input.push(&[1; 64]).is_err());
    }
}

#[test]
fn whole_transfer_framing_is_complete_and_negotiated_size_bounded() {
    let frame = vec![0x41; 148];
    let mut bytes = packet(&frame).unwrap();
    bytes.push(0xb8);
    assert_eq!(complete_transfer_frames(&bytes).unwrap(), vec![frame]);
    assert!(complete_transfer_frames(&bytes[..64]).is_err());
    assert!(complete_transfer_frames(&[0; 1581]).is_err());
    assert!(complete_transfer_frames(&[]).unwrap().is_empty());
}

#[cfg(test)]
mod module_tests {
    use super::*;
    #[test]
    fn fresh_interface_sends_discovery_membership_and_leave_without_tcp() {
        use smoltcp::wire::{IgmpPacket, IpProtocol, Ipv4Address, Ipv4Packet};
        let mut device = smoltcp::phy::Loopback::new(Medium::Ethernet);
        let config = Config::new(EthernetAddress([2, 1, 2, 3, 4, 5]).into());
        let mut iface = Interface::new(config, &mut device, Instant::from_millis(0));
        iface.update_ip_addrs(|a| {
            a.push(IpCidr::new(IpAddress::v4(192, 168, 171, 30), 24))
                .unwrap();
        });
        let group = IpAddress::v4(225, 1, 1, 1);
        let mut sockets = SocketSet::new(vec![]);
        assert!(!iface.has_multicast_group(group));
        join_discovery(&mut iface).unwrap();
        iface.poll(Instant::from_millis(0), &mut device, &mut sockets);
        let (rx, _) = device.receive(Instant::from_millis(0)).unwrap();
        let report = rx.consume(|f| f.to_vec());
        assert_eq!(&report[..6], &[1, 0, 0x5e, 1, 1, 1]);
        let ip = Ipv4Packet::new_checked(&report[14..]).unwrap();
        assert_eq!(ip.next_header(), IpProtocol::Igmp);
        assert_eq!(ip.hop_limit(), 1);
        assert_eq!(ip.dst_addr(), Ipv4Address::new(225, 1, 1, 1));
        let igmp = IgmpPacket::new_checked(ip.payload()).unwrap();
        assert!(igmp.verify_checksum());
        assert_eq!(ip.payload()[0], 0x16); // RFC2236 membership report.
        assert_eq!(&ip.payload()[4..8], &[225, 1, 1, 1]);
        assert!(iface.has_multicast_group(group));
        iface.leave_multicast_group(group).unwrap();
        iface.poll(Instant::from_millis(1), &mut device, &mut sockets);
        let (rx, _) = device.receive(Instant::from_millis(1)).unwrap();
        let leave = rx.consume(|f| f.to_vec());
        let ip = Ipv4Packet::new_checked(&leave[14..]).unwrap();
        assert_eq!(ip.dst_addr(), Ipv4Address::new(224, 0, 0, 2));
        let igmp = IgmpPacket::new_checked(ip.payload()).unwrap();
        assert!(igmp.verify_checksum());
        assert_eq!(ip.payload()[0], 0x17); // RFC2236 leave group.
        assert_eq!(&ip.payload()[4..8], &[225, 1, 1, 1]);
        assert!(!iface.has_multicast_group(group));
        assert!(sockets.iter().next().is_none());
    }
    fn temporary() -> PathBuf {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let p = std::env::temp_dir().join(format!(
            "opensaab-mdi-module-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&p).unwrap();
        p
    }
    #[test]
    fn unsupported_modes_and_authorities_rejected_before_usb_or_files() {
        let missing = Path::new("/unprovisioned-mdimodule-test");
        for (mode, authority) in [(0, "full"), (6, "read")] {
            let error = run(-1, mode, missing, missing, missing, authority, false).unwrap_err();
            assert!(error.contains("configuration"));
        }
    }
    #[test]
    fn normal_sessions_do_not_record_payloads_or_hit_trace_capacity() {
        let p = temporary();
        let mut evidence = Evidence::new(&p, true, false).unwrap();
        evidence.limit = 1;
        for _ in 0..4 {
            evidence.raw(6, &[0; 2048]).unwrap();
            evidence.ethernet(false, &[0; 1514]).unwrap();
        }
        assert_eq!(evidence.rx, 4);
        assert_eq!(evidence.bytes, 24);
        drop(evidence);
        assert_eq!(
            std::fs::metadata(p.join("rndis-private.bin"))
                .unwrap()
                .len(),
            0
        );
        assert_eq!(
            std::fs::metadata(p.join("ethernet-private.pcap"))
                .unwrap()
                .len(),
            24
        );
        std::fs::remove_dir_all(p).unwrap();
    }
    #[test]
    fn phase_updates_replace_complete_json_without_private_profile() {
        let p = temporary();
        phase(&p, "handshake", "Initializing MDI connection").unwrap();
        phase(&p, "firmware", "Starting firmware").unwrap();
        let state: Value =
            serde_json::from_slice(&std::fs::read(p.join("session-state.json")).unwrap()).unwrap();
        assert_eq!(state["state"], "firmware");
        assert_eq!(state.as_object().unwrap().len(), 2);
        assert!(!p.join("session-state.tmp").exists());
        std::fs::remove_dir_all(p).unwrap();
    }
}
