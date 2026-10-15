// SPDX-License-Identifier: LGPL-3.0-only
// Copyright (c) 2026 Erik Fuller and OpenVCX contributors.
// Rust protocol port of OpenVCX dll/device_vcx.c transient PASSTHRU session.
// See licenses/OPENVCX_LGPL_3_0.txt, licenses/OPENVCX_GPL_3_0.txt
// and licenses/OPENVCX_SOURCE_ORIGIN.txt. Proper random DH replaces
// upstream's public-one shortcut; the existing signed device record is selected.
use crate::adapters::vcx_nano::{
    channel::{control, Client, UsbTransport},
    protocol::{identity, Decoder, Frame},
};
use std::time::{SystemTime, UNIX_EPOCH};
pub const PRIME: u64 = u64::MAX - 58;
pub const GENERATOR: u64 = 5;
pub const PASSTHRU_ID: u32 = 0xdfb9_86a5;
const MARKER: u32 = 0xa567_a567;
const DELTA: u32 = 0x9e37_79b9;
const FILETIME_EPOCH: u64 = 116_444_736_000_000_000;
pub const EXPECTED: [(u8, usize); 6] = [
    (0x8c, 0),
    (0xa0, 8),
    (0x84, 32),
    (0xa0, 8),
    (0xa1, 160),
    (0xa2, 32),
];

pub fn pow_mod(mut base: u64, mut exponent: u64) -> u64 {
    let mut result = 1;
    while exponent != 0 {
        if exponent & 1 != 0 {
            result = ((result as u128 * base as u128) % PRIME as u128) as u64;
        }
        base = ((base as u128 * base as u128) % PRIME as u128) as u64;
        exponent >>= 1;
    }
    result
}
fn public_valid(value: u64) -> bool {
    (2..PRIME - 1).contains(&value)
}
#[derive(Clone, Copy)]
pub struct Dh {
    private: u64,
    pub public: u64,
}
impl Dh {
    pub fn generate(
        random: &mut impl FnMut() -> Result<u64, String>,
        prior: Option<&Dh>,
    ) -> Result<Self, String> {
        for _ in 0..128 {
            let private = random()?;
            if !public_valid(private) {
                continue;
            }
            let public = pow_mod(GENERATOR, private);
            if !public_valid(public)
                || prior.is_some_and(|old| old.private == private || old.public == public)
            {
                continue;
            }
            return Ok(Self { private, public });
        }
        Err("CSPRNG did not produce a valid fresh DH exponent".into())
    }
    pub fn key(&self, prefix: &[u8; 8], peer: u64) -> Result<[u8; 16], String> {
        if !public_valid(peer) {
            return Err("Device DH public value outside nondegenerate range".into());
        }
        let shared = pow_mod(peer, self.private);
        if !public_valid(shared) {
            return Err("Degenerate DH shared value rejected".into());
        }
        let mut key = [0; 16];
        key[..8].copy_from_slice(prefix);
        key[8..].copy_from_slice(&shared.to_le_bytes());
        Ok(key)
    }
}
fn mix(z: u32, y: u32, sum: u32, key: &[u32; 4], index: usize, selector: usize) -> u32 {
    ((z >> 5) ^ (y << 2)).wrapping_add((y >> 3) ^ (z << 4))
        ^ (y ^ sum).wrapping_add(z ^ key[(index & 3) ^ selector])
}
pub fn crypt(data: &mut [u8], key_bytes: &[u8; 16], decrypt: bool) -> Result<(), String> {
    if data.len() < 8 || data.len() > 160 || data.len() % 4 != 0 {
        return Err("Invalid corrected XXTEA outer size".into());
    }
    let mut words: Vec<u32> = data
        .chunks_exact(4)
        .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
        .collect();
    let key: [u32; 4] = std::array::from_fn(|i| {
        u32::from_le_bytes(key_bytes[i * 4..i * 4 + 4].try_into().unwrap())
    });
    let n = words.len();
    let rounds = 64 + 52 / n;
    if !decrypt {
        let mut sum = 0u32;
        let mut z = words[n - 1];
        for _ in 0..rounds {
            sum = sum.wrapping_add(DELTA);
            let e = ((sum >> 2) & 3) as usize;
            for i in 0..n - 1 {
                let y = words[i + 1];
                words[i] = words[i].wrapping_add(mix(z, y, sum, &key, i, e));
                z = words[i];
            }
            let y = words[0];
            words[n - 1] = words[n - 1].wrapping_add(mix(z, y, sum, &key, n - 1, e));
            z = words[n - 1];
        }
    } else {
        let mut sum = (rounds as u32).wrapping_mul(DELTA);
        let mut y = words[0];
        while sum != 0 {
            let e = ((sum >> 2) & 3) as usize;
            for i in (1..n).rev() {
                let z = words[i - 1];
                words[i] = words[i].wrapping_sub(mix(z, y, sum, &key, i, e));
                y = words[i];
            }
            let z = words[n - 1];
            words[0] = words[0].wrapping_sub(mix(z, y, sum, &key, 0, e));
            y = words[0];
            sum = sum.wrapping_sub(DELTA);
        }
    }
    for (i, word) in words.iter().enumerate() {
        data[i * 4..i * 4 + 4].copy_from_slice(&word.to_le_bytes());
    }
    Ok(())
}
#[derive(Clone, Copy)]
pub struct Trailer {
    pub filetime: u64,
    pub ticks: u32,
}
pub fn seal(plain: &[u8], key: &[u8; 16], time: Trailer) -> Result<Vec<u8>, String> {
    let mut out = plain.to_vec();
    out.extend_from_slice(&MARKER.to_le_bytes());
    out.extend_from_slice(&time.filetime.to_le_bytes());
    out.extend_from_slice(&time.ticks.to_le_bytes());
    crypt(&mut out, key, false)?;
    Ok(out)
}
pub fn open_record(cipher: &[u8], key: &[u8; 16]) -> Result<[u8; 16], String> {
    if cipher.len() != 32 {
        return Err("PASSTHRU reply sealed width differs".into());
    }
    let mut data = cipher.to_vec();
    crypt(&mut data, key, true)?;
    if data[16..20] != MARKER.to_le_bytes() {
        return Err("PASSTHRU reply trailer marker invalid".into());
    }
    if &data[..8] != b"PASSTHRU" {
        return Err("PASSTHRU reply token differs".into());
    }
    // Published source treats bytes8..16 as returned record metadata. It does
    // not compare or copy these bytes into the independent fixed selector.
    Ok(data[..16].try_into().unwrap())
}
pub fn query() -> [u8; 16] {
    let mut out = [0; 16];
    out[..8].copy_from_slice(b"PASSTHRU");
    out
}
pub fn selector() -> [u8; 144] {
    let mut out = [0; 144];
    out[0x84..0x88].copy_from_slice(&PASSTHRU_ID.to_le_bytes());
    out[0x88..].copy_from_slice(b"PASSTHRU");
    out
}
/// OS entropy only; no seeded PRNG, time seed or fixed exponent fallback.
pub fn fresh_random() -> Result<u64, String> {
    #[cfg(target_os = "macos")]
    {
        let mut bytes = [0u8; 8];
        unsafe { libc::arc4random_buf(bytes.as_mut_ptr().cast(), bytes.len()) };
        Ok(u64::from_le_bytes(bytes))
    }
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        use std::{
            fs::OpenOptions,
            io::Read,
            os::unix::fs::{FileTypeExt, OpenOptionsExt},
        };
        // Android kernels support getrandom even when the old libc API surface
        // does not export it. GRND_NONBLOCK bounds early-boot entropy failure.
        let mut bytes = [0u8; 8];
        let mut filled = 0usize;
        for _ in 0..8 {
            let n = unsafe {
                libc::syscall(
                    libc::SYS_getrandom,
                    bytes[filled..].as_mut_ptr(),
                    bytes.len() - filled,
                    libc::GRND_NONBLOCK,
                )
            };
            if n > 0 {
                filled += n as usize;
                if filled == bytes.len() {
                    return Ok(u64::from_le_bytes(bytes));
                }
                continue;
            }
            if n == 0 {
                return Err("OS secure entropy returned zero bytes".into());
            }
            let error = std::io::Error::last_os_error();
            match error.raw_os_error() {
                Some(libc::EINTR) => continue,
                Some(libc::ENOSYS) if filled == 0 => {
                    // Only a kernel without getrandom uses the real character
                    // device; no fallback after insufficient/unready entropy.
                    let mut file = OpenOptions::new()
                        .read(true)
                        .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK)
                        .open("/dev/urandom")
                        .map_err(|e| e.to_string())?;
                    if !file
                        .metadata()
                        .map_err(|e| e.to_string())?
                        .file_type()
                        .is_char_device()
                    {
                        return Err("OS entropy path is not a character device".into());
                    }
                    let mut filled = 0usize;
                    for _ in 0..8 {
                        match file.read(&mut bytes[filled..]) {
                            Ok(0) => return Err("OS entropy character device returned EOF".into()),
                            Ok(n) => {
                                filled += n;
                                if filled == bytes.len() {
                                    return Ok(u64::from_le_bytes(bytes));
                                }
                            }
                            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                            Err(e) => return Err(e.to_string()),
                        }
                    }
                    return Err("OS entropy character device read bound exceeded".into());
                }
                _ => return Err(format!("OS secure entropy unavailable: {error}")),
            }
        }
        Err("OS secure entropy short-read/interruption bound exceeded".into())
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "android")))]
    {
        Err("Secure entropy platform is unsupported".into())
    }
}
pub fn fresh_time() -> Result<Trailer, String> {
    let since = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?;
    let filetime = u64::try_from(since.as_nanos() / 100)
        .map_err(|_| "FILETIME out of range")?
        .checked_add(FILETIME_EPOCH)
        .ok_or("FILETIME overflow")?;
    #[cfg(unix)]
    {
        let mut ts = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        if unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts) } != 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        if ts.tv_sec < 0 || ts.tv_nsec < 0 {
            return Err("Monotonic clock out of range".into());
        }
        let ticks =
            ((ts.tv_sec as u128 * 1000 + ts.tv_nsec as u128 / 1_000_000) & 0xffff_ffff) as u32;
        Ok(Trailer { filetime, ticks })
    }
    #[cfg(not(unix))]
    {
        let _ = filetime;
        Err("Native Nano clock platform unsupported".into())
    }
}

#[derive(Default)]
pub struct Progress {
    pub query_record_verified: bool,
    pub installed_record_verified: bool,
    pub fresh_dh_exchanges: u8,
}
fn exchange<T: UsbTransport>(
    client: &mut Client<T>,
    opcode: u8,
    payload: &[u8],
    reply_width: usize,
    observed: &mut impl FnMut(&Frame) -> Result<(), String>,
) -> Result<Vec<u8>, String> {
    let reply = client.exchange(&control(0, opcode, payload), &mut |frame: &Frame| {
        println!(
            "CONTROL_RX header={} payload={}",
            hex(&frame.header),
            hex(&frame.payload)
        );
        if frame.header[2] == 0 {
            return Err("Unexpected CAN receive during init-only startup".into());
        }
        observed(frame)
    })?;
    if reply.payload.len() != reply_width + 1 || reply.payload.first() != Some(&0) {
        return Err(format!("Startup opcode{opcode:02X} status/width rejected"));
    }
    Ok(reply.payload[1..].to_vec())
}
/// Firmware and identity must pass before entropy, DH or any channel open.
pub fn verify_identity(reply: &Frame) -> Result<crate::nano_usb::Identity, String> {
    let device = identity(reply)?;
    if device.firmware != "1.9.4.2" {
        return Err(
            "Nano firmware differs from source-supported 1.9.4.2; stopped before fresh DH".into(),
        );
    }
    Ok(device)
}
/// Shared production entry. The caller's actual GetInfo reply is reused once.
pub fn init_handshake<T: UsbTransport>(
    client: &mut Client<T>,
    identity_reply: &Frame,
    observed: &mut impl FnMut(&Frame) -> Result<(), String>,
    progress: &mut Progress,
) -> Result<(), String> {
    initialize_from_identity(
        client,
        identity_reply,
        &mut fresh_random,
        &mut fresh_time,
        progress,
        observed,
    )
}
/// Dependency-injected core is for offline oracle/transport tests only.
pub fn initialize_from_identity<T: UsbTransport>(
    client: &mut Client<T>,
    identity_reply: &Frame,
    random: &mut impl FnMut() -> Result<u64, String>,
    time: &mut impl FnMut() -> Result<Trailer, String>,
    progress: &mut Progress,
    observed: &mut impl FnMut(&Frame) -> Result<(), String>,
) -> Result<(), String> {
    *progress = Progress::default();
    verify_identity(identity_reply)?;
    let info = &identity_reply.payload[1..];
    let prefix: [u8; 8] = info[16..24].try_into().unwrap();
    let phase1 = Dh::generate(random, None)?;
    let reply1 = exchange(client, 0xa0, &phase1.public.to_le_bytes(), 8, observed)?;
    let peer1 = u64::from_le_bytes(reply1.try_into().unwrap());
    let key1 = phase1.key(&prefix, peer1)?;
    progress.fresh_dh_exchanges = 1;
    let sealed = seal(&query(), &key1, time()?)?;
    let record = exchange(client, 0x84, &sealed, 32, observed)?;
    open_record(&record, &key1)?;
    progress.query_record_verified = true;
    let phase2 = Dh::generate(random, Some(&phase1))?;
    let reply2 = exchange(client, 0xa0, &phase2.public.to_le_bytes(), 8, observed)?;
    let peer2 = u64::from_le_bytes(reply2.try_into().unwrap());
    if peer2 == peer1 {
        return Err("Device reused prior DH public value; second-phase freshness unproved".into());
    }
    let key2 = phase2.key(&prefix, peer2)?;
    progress.fresh_dh_exchanges = 2;
    let selection = seal(&selector(), &key2, time()?)?;
    exchange(client, 0xa1, &selection, 0, observed)?;
    let sealed = seal(&query(), &key2, time()?)?;
    let installed = exchange(client, 0xa2, &sealed, 32, observed)?;
    open_record(&installed, &key2)?;
    client.finish_frames()?;
    progress.installed_record_verified = true;
    Ok(())
}
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect()
}
pub struct InitOnly<T> {
    pub inner: T,
    pub completed_writes: usize,
}
impl<T: UsbTransport> UsbTransport for InitOnly<T> {
    fn write(&mut self, bytes: &[u8]) -> Result<(), String> {
        let mut decoder = Decoder::default();
        let frames = decoder.feed(bytes)?;
        decoder.finish()?;
        if frames.len() != 1 || self.completed_writes >= EXPECTED.len() {
            return Err("Init-only TX count/frame bound exceeded".into());
        }
        let frame = &frames[0];
        let (op, width) = EXPECTED[self.completed_writes];
        if frame.header != [0x80, 0, op, 0] || frame.payload.len() != width {
            return Err("Init-only forbids channel/CAN/reset/firmware or replayed order".into());
        }
        self.inner.write(bytes)?;
        println!("USB_TX {} completion=serial-write-only", hex(bytes));
        self.completed_writes += 1;
        Ok(())
    }
    fn read(&mut self) -> Result<Vec<u8>, String> {
        let bytes = self.inner.read()?;
        if !bytes.is_empty() {
            println!("SERIAL_RX {}", hex(&bytes));
        }
        Ok(bytes)
    }
}

#[cfg(test)]
fn initialize<T: UsbTransport>(
    client: &mut Client<T>,
    random: &mut impl FnMut() -> Result<u64, String>,
    time: &mut impl FnMut() -> Result<Trailer, String>,
    progress: &mut Progress,
) -> Result<(), String> {
    let info = exchange(client, 0x8c, &[], 64, &mut |_| Ok(()))?;
    let frame = control(0, 0x8c, &[vec![0], info].concat());
    initialize_from_identity(client, &frame, random, time, progress, &mut |_| Ok(()))
}
#[cfg(test)]
#[path = "init_handshake/tests.rs"]
mod tests;
