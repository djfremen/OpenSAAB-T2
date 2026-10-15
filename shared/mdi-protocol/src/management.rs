// SPDX-License-Identifier: MPL-2.0
//! Scoped classic-MDI management transport, observed 2.5.33.154.
//! The private base key is supplied by the caller; no vendor library is loaded.
use crate::wire::{Record, MAX_RECORD};
use blowfish::{
    cipher::{generic_array::GenericArray, BlockDecrypt, BlockEncrypt, KeyInit},
    Blowfish,
};
use serde_json::{json, Value};
use std::{
    io::Read,
    net::{IpAddr, Shutdown, SocketAddr, TcpStream},
    time::{Duration, Instant},
};

fn words(v: &[u32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}
fn blob(v: &[u8]) -> Vec<u8> {
    let mut b = words(&[v.len() as u32]);
    b.extend(v);
    b
}
fn padded(mut b: Vec<u8>) -> Vec<u8> {
    b.resize((b.len() + 7) & !7, 0);
    b
}

pub struct Codec {
    cipher: Blowfish,
}
impl Codec {
    pub fn new(base_key: &[u8], serial: u32) -> Result<Self, String> {
        if base_key.len() != 56 || serial == 0 {
            return Err("Explicit 56-byte private key and nonzero adapter serial required".into());
        }
        let mut derived = Vec::with_capacity(56);
        for word in base_key.chunks_exact(4) {
            derived.extend(
                u32::from_le_bytes(word.try_into().unwrap())
                    .wrapping_add(serial)
                    .to_le_bytes(),
            );
        }
        Ok(Self {
            cipher: Blowfish::new_from_slice(&derived).map_err(|_| "Blowfish key size")?,
        })
    }
    pub fn encode(&self, record: &Record) -> Result<Record, String> {
        if record.opcode & 0x8000 != 0 {
            return Err("Opcode already encrypted".into());
        }
        if record.body.len() > MAX_RECORD - 15 {
            return Err("Management body limit".into());
        }
        let mut b = padded(record.body.clone());
        for block in b.chunks_exact_mut(8) {
            self.cipher
                .encrypt_block(GenericArray::from_mut_slice(block));
        }
        Ok(Record::new(record.opcode | 0x8000, b))
    }
    pub fn decode(&self, record: Record) -> Result<Record, String> {
        if record.opcode & 0x8000 == 0 || record.body.is_empty() || record.body.len() % 8 != 0 {
            return Err("Encrypted management reply framing".into());
        }
        let mut b = record.body;
        for block in b.chunks_exact_mut(8) {
            self.cipher
                .decrypt_block(GenericArray::from_mut_slice(block));
        }
        Ok(Record::new(record.opcode & !0x8000, b))
    }
}

struct Cursor<'a> {
    b: &'a [u8],
    pos: usize,
}
impl<'a> Cursor<'a> {
    fn new(b: &'a [u8]) -> Self {
        Self { b, pos: 0 }
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self.pos.checked_add(n).ok_or("Field overflow")?;
        if end > self.b.len() {
            return Err("Truncated management field".into());
        }
        let b = &self.b[self.pos..end];
        self.pos = end;
        Ok(b)
    }
    fn word(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn expect(&mut self, n: u32) -> Result<(), String> {
        if self.word()? == n {
            Ok(())
        } else {
            Err("Management discriminator mismatch".into())
        }
    }
    fn string(&mut self) -> Result<String, String> {
        let n = self.word()? as usize;
        if n > 4096 {
            return Err("Management string bound".into());
        }
        String::from_utf8(self.take(n)?.to_vec()).map_err(|_| "Management UTF-8".into())
    }
    fn finish(&mut self) -> Result<(), String> {
        self.finish_bound(7)
    }
    fn finish_bound(&mut self, maximum: usize) -> Result<(), String> {
        let tail = &self.b[self.pos..];
        if tail.len() <= maximum && tail.iter().all(|x| *x == 0) {
            Ok(())
        } else {
            Err("Management padding differs".into())
        }
    }
}
fn status(record: &Record, opcode: u32) -> Result<(), String> {
    record.reply(opcode).map(|_| ())
}

/// One port-9000 connection per inventory query; no ownership or vehicle request.
pub fn inventory_request() -> Record {
    Record::new(0x82e, words(&[1]))
}
pub fn inventory_reply(record: &Record) -> Result<Vec<Value>, String> {
    status(record, 0x842)?;
    let mut c = Cursor::new(&record.body);
    c.expect(0)?;
    c.expect(1)?;
    let count = c.word()?;
    if !(1..=16).contains(&count) {
        return Err("Application count boundary".into());
    }
    let mut apps = Vec::new();
    for _ in 0..count {
        c.expect(0x841)?;
        c.expect(1)?;
        c.expect(0x85f)?;
        c.expect(2)?;
        let version = c.string()?;
        let build = c.string()?;
        let start = c.string()?;
        let running = c.string()?;
        let kind = c.string()?;
        let name = c.string()?;
        let stop = c.string()?;
        // Retain observed 34-byte metadata trailer; its wider semantics are not assumed.
        let trailer = c.take(34)?;
        apps.push(json!({"version":version,"build":build,"start":start,"running":running,"kind":kind,"name":name,"stop":stop,"trailer_hex":hex(trailer)}));
    }
    c.finish()?;
    Ok(apps)
}

/// A new owner label and instance name; the response supplies the access handle.
pub fn registration_request(instance: &str) -> Result<Record, String> {
    if instance.is_empty() || instance.len() > 96 || !instance.is_ascii() {
        return Err("Fresh management instance label boundary".into());
    }
    let mut b = words(&[7]);
    b.extend(blob(b"OpenMDI"));
    b.extend(words(&[0]));
    b.extend(blob(instance.as_bytes()));
    b.extend(words(&[0x1241, 0, 1]));
    b.extend([0, 1]);
    b.extend(words(&[0, 0]));
    Ok(Record::new(0x834, b))
}
pub fn registration_reply(record: &Record) -> Result<u32, String> {
    status(record, 0x835)?;
    let mut c = Cursor::new(&record.body);
    c.expect(0)?;
    c.expect(6)?;
    let handle = c.word()?;
    if handle == 0 {
        return Err("Management handle boundary".into());
    }
    c.expect(0)?; // Observed owner flag; another owner's response is rejected.
                  // Version six serializes six metadata strings. These are not ownership flags.
    for _ in 0..6 {
        c.string()?;
    }
    // The native empty-metadata variant has eight extra zero bytes.
    // Accept only that bounded zero extension, not arbitrary trailing data.
    c.finish_bound(8)?;
    Ok(handle)
}
fn properties(items: &[(&str, &str)]) -> Vec<u8> {
    let mut b = words(&[0x84b, 0, 1, 0, items.len() as u32]);
    for (key, value) in items {
        b.extend(words(&[0x84a, 1]));
        b.extend(blob(key.as_bytes()));
        b.extend(blob(value.as_bytes()));
    }
    b
}
pub fn start_request(handle: u32) -> Result<Record, String> {
    if handle == 0 {
        return Err("Fresh management handle required".into());
    }
    let mut b = words(&[2, 0]);
    b.extend(properties(&[("APP_NAME", "PDU"), ("COMMAND", "START")]));
    b.extend(words(&[handle]));
    Ok(Record::new(0x830, b))
}
pub fn start_reply(record: &Record, request: &Record) -> Result<(), String> {
    status(record, 0x831)?;
    let mut expected = request.body.clone();
    expected[..8].copy_from_slice(&words(&[0, 2]));
    // The response echoes the properties, then zero completion status. It does
    // not echo the caller's access handle; that handle is checked on release.
    let tail = expected.len() - 4;
    expected[tail..].fill(0);
    if record.body != padded(expected) {
        return Err("Start acknowledgement/properties differ".into());
    }
    Ok(())
}
pub fn reboot_request(handle: u32) -> Result<Record, String> {
    if handle == 0 {
        return Err("Fresh management handle required".into());
    }
    let mut b = words(&[1, 0x85d, 0, 1, 1, 0x100]);
    b.extend(properties(&[("power-ctrl", "reboot")]));
    b.extend(words(&[handle]));
    Ok(Record::new(0x85e, b))
}
pub fn release_request(handle: u32) -> Record {
    Record::new(0x836, words(&[1, handle, 0]))
}
fn simple_reply(record: &Record, opcode: u32) -> Result<(), String> {
    status(record, opcode)?;
    if record.body != words(&[0, 1]) {
        return Err("Management acknowledgement differs".into());
    }
    Ok(())
}
fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

struct Socket {
    stream: TcpStream,
}
impl Socket {
    fn open(ip: IpAddr, port: u16) -> Result<Self, String> {
        let stream = TcpStream::connect_timeout(&SocketAddr::new(ip, port), Duration::from_secs(2))
            .map_err(|e| format!("Management TCP {port}: {e}"))?;
        stream.set_nodelay(true).map_err(|e| e.to_string())?;
        Ok(Self { stream })
    }
    fn exchange(
        &mut self,
        codec: &Codec,
        request: &Record,
        operations: &mut Vec<Value>,
    ) -> Result<Record, String> {
        let wire = codec.encode(request)?;
        let deadline = Instant::now() + Duration::from_secs(5);
        operations.push(json!({"request":request.opcode,"request_body_hex":hex(&request.body),"request_sent":false}));
        self.stream
            .set_write_timeout(Some(Duration::from_secs(2)))
            .map_err(|e| e.to_string())?;
        wire.write(&mut self.stream)?;
        let last = operations.last_mut().unwrap();
        last["request_sent"] = json!(true);
        let mut exact = |n: usize| -> Result<Vec<u8>, String> {
            let mut b = vec![0; n];
            let mut pos = 0;
            while pos < n {
                let remaining = deadline
                    .checked_duration_since(Instant::now())
                    .filter(|remaining| !remaining.is_zero())
                    .ok_or("Management reply deadline")?;
                self.stream
                    .set_read_timeout(Some(remaining))
                    .map_err(|e| e.to_string())?;
                let size = self
                    .stream
                    .read(&mut b[pos..])
                    .map_err(|e| format!("Management reply: {e}"))?;
                if size == 0 {
                    return Err("Management peer closed inside reply".into());
                }
                pos += size;
            }
            Ok(b)
        };
        let h = exact(8)?;
        let size = u32::from_le_bytes(h[..4].try_into().unwrap()) as usize;
        if !(8..=MAX_RECORD).contains(&size) {
            return Err("Management reply record bound".into());
        }
        let reply = codec.decode(Record::new(
            u32::from_le_bytes(h[4..].try_into().unwrap()),
            exact(size - 8)?,
        ))?;
        last["reply"] = json!(reply.opcode);
        last["reply_body_hex"] = json!(hex(&reply.body));
        Ok(reply)
    }
}
impl Drop for Socket {
    fn drop(&mut self) {
        let _ = self.stream.shutdown(Shutdown::Both);
    }
}

/// Bounded standalone management experiment; mode is query, start, or reboot.
/// It never loads diagnostic DLLs or sends an ECU request. No implicit command retry.
pub fn attempt(ip: IpAddr, serial: u32, base_key: &[u8], mode: &str, instance: &str) -> Value {
    let started = Instant::now();
    let mut operations = Vec::new();
    let mut apps = Vec::new();
    let mut access = None;
    let mut owner = None;
    let mut released = false;
    let mut reboot_commands = 0;
    let mut start_commands = 0;
    let mut acknowledged = false;
    let result = (|| -> Result<(), String> {
        if !matches!(mode, "query" | "start" | "reboot") {
            return Err("Unsupported management mode".into());
        }
        let codec = Codec::new(base_key, serial)?;
        let mut query = Socket::open(ip, 9000)?;
        apps = inventory_reply(&query.exchange(&codec, &inventory_request(), &mut operations)?)?;
        drop(query);
        let pdu = apps
            .iter()
            .find(|a| a["name"] == "PDU")
            .ok_or("PDU application absent")?;
        if pdu["version"] != "2.5.33.154" || pdu["kind"] != "application" {
            return Err("Unqualified PDU profile".into());
        }
        if mode == "query" {
            return Ok(());
        }
        if mode == "start" && pdu["running"] != "false" {
            return Err("PDU was already running; first-start witness required".into());
        }
        let mut socket = Socket::open(ip, 9002)?;
        let reply = socket.exchange(&codec, &registration_request(instance)?, &mut operations)?;
        // Save an allocated handle even when a changed ownership tail fails qualification.
        if reply.opcode == 0x835 && reply.body.len() >= 12 && reply.body[..4] == [0; 4] {
            let token = u32::from_le_bytes(reply.body[8..12].try_into().unwrap());
            if token != 0 {
                access = Some(token);
            }
        }
        owner = Some(socket);
        let handle = registration_reply(&reply)?;
        let (port, request) = if mode == "reboot" {
            (9004, reboot_request(handle)?)
        } else {
            (9000, start_request(handle)?)
        };
        let mut command = Socket::open(ip, port)?;
        if mode == "reboot" {
            reboot_commands += 1;
        } else {
            start_commands += 1;
        }
        let reply = command.exchange(&codec, &request, &mut operations)?;
        if mode == "reboot" {
            simple_reply(&reply, 0x85c)?;
        } else {
            start_reply(&reply, &request)?;
        }
        acknowledged = true;
        Ok(())
    })();
    let mut errors = Vec::new();
    if let Err(error) = result {
        errors.push(error);
    }
    if let (Some(socket), Some(handle), Ok(codec)) =
        (owner.as_mut(), access, Codec::new(base_key, serial))
    {
        match socket
            .exchange(&codec, &release_request(handle), &mut operations)
            .and_then(|r| simple_reply(&r, 0x837))
        {
            Ok(()) => released = true,
            Err(e) => errors.push(format!("Management release: {e}")),
        }
    }
    drop(owner);
    json!({"schema":1,"mode":mode,"success":errors.is_empty(),"elapsed_ms":started.elapsed().as_millis(),
        "vendor_dll_loaded":false,"vehicle_requests":0,"operations":operations,"applications":apps,
        "access_handle":access,"management_released":released,"sockets_closed":true,"acknowledged":acknowledged,
        "reboot_command_count":reboot_commands,"start_command_count":start_commands,"errors":errors})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn standard_blowfish_reference_vector() {
        let cipher: Blowfish = Blowfish::new_from_slice(&[0; 8]).unwrap();
        let mut b = GenericArray::clone_from_slice(&[0; 8]);
        cipher.encrypt_block(&mut b);
        assert_eq!(hex(&b), "4ef997456198dd78");
        cipher.decrypt_block(&mut b);
        assert_eq!(&b[..], &[0; 8]);
    }
    #[test]
    fn encryption_uses_wrapping_little_endian_serial_words() {
        let base = [0xff; 56];
        let codec = Codec::new(&base, 2).unwrap();
        let expected: Blowfish = Blowfish::new_from_slice(&words(&[1; 14])).unwrap();
        let mut block = GenericArray::clone_from_slice(&[1, 0, 0, 0, 0, 0, 0, 0]);
        expected.encrypt_block(&mut block);
        assert_eq!(
            codec.encode(&inventory_request()).unwrap().body,
            block.as_slice()
        );
    }
    #[test]
    fn changing_fresh_handle_changes_start_and_reboot() {
        let a = start_request(0x12345678).unwrap();
        let b = start_request(0x87654321).unwrap();
        assert_ne!(a, b);
        assert_eq!(&a.body[a.body.len() - 4..], &0x12345678u32.to_le_bytes());
        let reboot = reboot_request(0x12345678).unwrap();
        assert!(reboot.body.windows(6).any(|x| x == b"reboot"));
        assert_eq!(
            &reboot.body[reboot.body.len() - 4..],
            &0x12345678u32.to_le_bytes()
        );
        assert!(reboot_request(0).is_err());
        assert!(start_request(0).is_err());
    }
    #[test]
    fn start_reply_checks_echoed_properties_and_final_status() {
        let request = start_request(123).unwrap();
        let mut body = padded(request.body.clone());
        body[..8].copy_from_slice(&words(&[0, 2]));
        body[request.body.len() - 4..].fill(0);
        assert!(start_reply(&Record::new(0x831, body.clone()), &request).is_ok());
        body[request.body.len() - 4] ^= 1;
        assert!(start_reply(&Record::new(0x831, body), &request).is_err());
    }
    #[test]
    fn framing_and_changed_ownership_fail_closed() {
        let codec = Codec::new(&[0; 56], 1).unwrap();
        assert!(codec.decode(Record::new(0x835, vec![0; 48])).is_err());
        assert!(codec.decode(Record::new(0x8835, vec![0; 47])).is_err());
        let mut body = words(&[0, 6, 123]);
        body.resize(48, 0);
        assert_eq!(
            registration_reply(&Record::new(0x835, body.clone())).unwrap(),
            123
        );
        body[12] = 1;
        assert!(registration_reply(&Record::new(0x835, body)).is_err());
    }
    #[test]
    fn ownership_metadata_is_bounded_and_can_be_nonempty() {
        let mut body = words(&[0, 6, 123, 0]);
        for field in ["", "", "", "", "00", ""] {
            body.extend(blob(field.as_bytes()));
        }
        let reply = Record::new(0x835, padded(body));
        assert_eq!(registration_reply(&reply).unwrap(), 123);
        let mut bad = reply.clone();
        bad.body[32..36].copy_from_slice(&4097u32.to_le_bytes());
        assert!(registration_reply(&bad).is_err());
        for n in 0..42 {
            assert!(registration_reply(&Record::new(0x835, reply.body[..n].to_vec())).is_err());
        }
    }
    #[test]
    fn truncated_inventory_creates_no_application() {
        for n in 0..28 {
            assert!(inventory_reply(&Record::new(0x842, vec![0; n])).is_err());
        }
    }
}
