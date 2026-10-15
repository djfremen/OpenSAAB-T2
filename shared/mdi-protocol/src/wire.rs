// SPDX-License-Identifier: MPL-2.0
//! Typed generation for the observed classic-MDI TCP protocol.
//! Values are scoped to 8.5.33.154. No saved records or vendor DLL are consumed.
use std::io::{Read, Write};

pub const MAX_RECORD: usize = 65_536;

fn words(values: &[u32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}
fn blob(value: &[u8]) -> Vec<u8> {
    let mut b = words(&[value.len() as u32]);
    b.extend(value);
    b
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub opcode: u32,
    pub body: Vec<u8>,
}
impl Record {
    pub fn new(opcode: u32, body: Vec<u8>) -> Self {
        Self { opcode, body }
    }
    pub fn encoded(&self) -> Result<Vec<u8>, String> {
        if self.body.len() > MAX_RECORD - 8 {
            return Err("MDI record size limit".into());
        }
        let mut b = words(&[(self.body.len() + 8) as u32, self.opcode]);
        b.extend(&self.body);
        Ok(b)
    }
    pub fn read(reader: &mut impl Read) -> Result<Self, String> {
        let mut h = [0; 8];
        reader
            .read_exact(&mut h)
            .map_err(|e| format!("MDI header: {e}"))?;
        let n = u32::from_le_bytes(h[..4].try_into().unwrap()) as usize;
        if !(8..=MAX_RECORD).contains(&n) {
            return Err("MDI record size limit".into());
        }
        let mut body = vec![0; n - 8];
        reader
            .read_exact(&mut body)
            .map_err(|e| format!("MDI body: {e}"))?;
        Ok(Self {
            opcode: u32::from_le_bytes(h[4..].try_into().unwrap()),
            body,
        })
    }
    pub fn write(&self, writer: &mut impl Write) -> Result<(), String> {
        writer
            .write_all(&self.encoded()?)
            .map_err(|e| format!("MDI send: {e}"))
    }
    pub fn reply(&self, expected: u32) -> Result<&[u8], String> {
        if self.opcode != expected || self.body.len() < 4 {
            return Err("Unexpected MDI reply".into());
        }
        let status = u32::from_le_bytes(self.body[..4].try_into().unwrap());
        if status != 0 {
            return Err(format!("MDI RPC status 0x{status:08x}"));
        }
        Ok(&self.body[4..])
    }
}

/// Zero allocates a session; the second socket must use the newly returned token.
pub fn register(session_token: u32) -> Record {
    Record::new(0x884, words(&[1, session_token, 5000]))
}
pub fn registration_reply(record: &Record, expected_token: Option<u32>) -> Result<u32, String> {
    let tail = record.reply(0x885)?;
    if tail.len() != 12 {
        return Err("Registration reply length".into());
    }
    let v: Vec<u32> = tail
        .chunks_exact(4)
        .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
        .collect();
    if v[0] != 1 || v[1] != 0 || v[2] == 0 || expected_token.is_some_and(|n| n != v[2]) {
        return Err("Registration module/token mismatch".into());
    }
    Ok(v[2])
}
/// Empty protocol-owner heartbeat observed in the native Windows command socket.
pub fn heartbeat() -> Record {
    Record::new(0x888, Vec::new())
}
pub fn heartbeat_reply(record: &Record) -> Result<(), String> {
    if !record.reply(0x889)?.is_empty() {
        return Err("Heartbeat reply has unexpected tail".into());
    }
    Ok(())
}
pub fn release_session(token: u32) -> Record {
    Record::new(0x886, words(&[1, token]))
}
pub fn session_released(record: &Record) -> Result<(), String> {
    if record.reply(0x887)? != 1u32.to_le_bytes() {
        return Err("Session release tail differs".into());
    }
    Ok(())
}

/// Opaque tags are allocated by this host within the observed client namespace.
#[derive(Debug)]
pub struct Tags {
    next_link: u32,
    next_primitive: u32,
}
impl Tags {
    pub fn for_session(token: u32) -> Self {
        let seed = (token & 0xfff) << 4;
        Self {
            next_link: 0x02000000 | seed | 1,
            next_primitive: 0x02100000 | seed | 1,
        }
    }
    pub fn link(&mut self) -> u32 {
        let t = self.next_link;
        self.next_link += 1;
        t
    }
    pub fn primitive(&mut self) -> u32 {
        let t = self.next_primitive;
        self.next_primitive += 1;
        t
    }
}

pub fn create_hs_iso15765(link: u32) -> Record {
    let mut b = words(&[link]);
    b.push(1);
    b.extend(words(&[3, 36, 2, 6, 1, 14, 2, 0xfffffffe]));
    b.push(1);
    b.extend(blob(&[0xa0, 1, 0, 0]));
    Record::new(0x44f, b)
}
/// Native reference: ISO15765 on pin1 SAE J2411. Not a raw guest channel.
pub fn create_sw_iso15765(link: u32) -> Record {
    let mut b = words(&[link]);
    b.push(1);
    b.extend(words(&[14, 16, 1, 1, 9, 0xfffffffe]));
    b.push(1);
    b.extend(blob(&[0x80, 0, 0, 0]));
    Record::new(0x44f, b)
}
pub fn create_sw_raw(link: u32) -> Record {
    let mut r = create_sw_iso15765(link);
    r.body[9..13].copy_from_slice(&6u32.to_le_bytes());
    // The retained raw J2534 run uses different flags from Tech2Win's
    // paired ISO15765/wake links. Keep the two profiles distinct.
    let n = r.body.len();
    r.body[n - 4..].copy_from_slice(&[0xa0, 1, 0, 0]);
    r
}
pub fn create_hs_raw(link: u32) -> Record {
    let mut r = create_hs_iso15765(link);
    r.body[9..13].copy_from_slice(&6u32.to_le_bytes());
    r
}
/// Forward original CAN bytes. The caller owns diagnostic PCI and flow control.
pub fn raw_frame(
    link: u32,
    token: u32,
    id: u32,
    data: &[u8],
    wake: bool,
) -> Result<Record, String> {
    if id > 0x7ff || data.len() > 8 || (wake && (id != 0x100 || !data.is_empty())) {
        return Err("Raw CAN frame/electrical boundary".into());
    }
    let mut bytes = id.to_be_bytes().to_vec();
    bytes.extend(data);
    let mut b = words(&[link, token, 0x8004]);
    b.extend(blob(&bytes));
    b.push(1);
    b.extend(words(&[0, 1, 0, u32::from(wake)]));
    b.extend(blob(&[if wake { 0x20 } else { 0 }, 0, 0, 0]));
    b.extend(words(&[0]));
    Ok(Record::new(0x463, b))
}
pub fn link_operation(op: u32, link: u32) -> Result<Record, String> {
    if !matches!(op, 0x45f | 0x461 | 0x451) {
        return Err("Unsupported link operation".into());
    }
    Ok(Record::new(op, words(&[link])))
}
pub fn set_unum32(link: u32, id: u32, class: u32, value: u32) -> Record {
    let mut b = words(&[link, 0x105, 0x1200]);
    b.push(1);
    b.extend(words(&[0x1200, id, 0x105, class]));
    b.push(1);
    b.extend(words(&[value]));
    Record::new(0x459, b)
}
pub type Parameter = (u32, u32, u32);
pub fn response_table(link: u32, entries: &[(u32, Vec<Parameter>)]) -> Result<Record, String> {
    if entries.len() > 4096 || entries.iter().any(|(_, p)| p.len() > 4096) {
        return Err("Response table size limit".into());
    }
    let mut b = words(&[link, 0x1700, 0x1700, entries.len() as u32]);
    for (unique, params) in entries {
        b.extend(words(&[*unique, params.len() as u32]));
        for (id, class, value) in params {
            b.extend(words(&[0x105, 0x1200, *id, 0x105, *class]));
            b.push(1);
            b.extend(words(&[*value]));
        }
    }
    Ok(Record::new(0x45d, b))
}
pub fn primitive(
    link: u32,
    token: u32,
    kind: u32,
    data: &[u8],
    flags: &[u8],
    send: i32,
    receive: i32,
    acceptance: Option<u32>,
) -> Result<Record, String> {
    if !matches!(kind, 0x8001 | 0x8003 | 0x8004)
        || data.len() > 4128
        || flags.len() > 4
        || send < 0
        || receive < -1
    {
        return Err("Unsupported scoped primitive".into());
    }
    let mut b = words(&[link, token, kind]);
    b.extend(blob(data));
    b.push(1);
    b.extend(words(&[0, send as u32, receive as u32, 0]));
    b.extend(blob(flags));
    b.extend(words(&[u32::from(acceptance.is_some())]));
    if let Some(id) = acceptance {
        b.extend(words(&[0, id, 0, 0]));
    }
    Ok(Record::new(0x463, b))
}
pub fn cancel(link: u32, token: u32) -> Record {
    Record::new(0x465, words(&[link, token]))
}
pub fn receive_on_id(link: u32, token: u32, id: u32) -> Result<Record, String> {
    if id > 0x7ff {
        return Err("Standard receive ID bounds".into());
    }
    let mut b = words(&[link, token, 0x8004]);
    b.extend(blob(&[]));
    b.push(1);
    b.extend(words(&[0, 0, u32::MAX, 0]));
    b.extend(blob(&[]));
    b.extend(words(&[1, 0, 0, 4]));
    b.extend(0x1fffffffu32.to_be_bytes());
    b.extend(id.to_be_bytes());
    b.extend(words(&[0]));
    Ok(Record::new(0x463, b))
}
pub fn spa_preconditions(link: u32, token: u32) -> Record {
    let mut b = words(&[link, token, 0x8004]);
    b.extend(blob(&[0, 0, 2, 0x5b, 0x1a, 0x9a]));
    b.push(1);
    b.extend(words(&[500, 1, 0, 0]));
    b.extend(blob(&[0, 0, 0, 0x40]));
    b.extend(words(&[0]));
    Record::new(0x463, b)
}
pub fn spa_raw_preconditions(link: u32, token: u32) -> Record {
    let mut b = words(&[link, token, 0x8004]);
    b.extend(blob(&[0, 0, 2, 0x5b, 2, 0x1a, 0x9a, 0, 0, 0, 0, 0]));
    b.push(1);
    b.extend(words(&[500, 1, 0, 0]));
    b.extend(blob(&[0; 4]));
    b.extend(words(&[0]));
    Record::new(0x463, b)
}
pub fn start_communication(link: u32, token: u32) -> Record {
    let mut b = words(&[link, token, 0x8001]);
    b.extend(blob(&[]));
    b.push(1);
    b.extend(words(&[100, 0, 0, 0]));
    b.extend(blob(&[]));
    b.extend(words(&[0]));
    Record::new(0x463, b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    #[test]
    fn fragmented_reads_and_poisoned_lengths() {
        struct Slow(Cursor<Vec<u8>>);
        impl Read for Slow {
            fn read(&mut self, b: &mut [u8]) -> std::io::Result<usize> {
                let n = b.len().min(1);
                self.0.read(&mut b[..n])
            }
        }
        let original = register(73);
        assert_eq!(
            Record::read(&mut Slow(Cursor::new(original.encoded().unwrap()))).unwrap(),
            original
        );
        for n in [0, 7, (MAX_RECORD + 1) as u32, u32::MAX] {
            assert!(Record::read(&mut Cursor::new(words(&[n, 0x885]))).is_err());
        }
        let mut short = original.encoded().unwrap();
        short.pop();
        assert!(Record::read(&mut Cursor::new(short)).is_err());
    }
    #[test]
    fn live_token_linkage_and_release_are_checked() {
        let r = Record::new(0x885, words(&[0, 1, 0, 73]));
        assert_eq!(registration_reply(&r, None).unwrap(), 73);
        assert_eq!(registration_reply(&r, Some(73)).unwrap(), 73);
        assert!(registration_reply(&r, Some(2)).is_err());
        for body in [
            words(&[1, 1, 0, 73]),
            words(&[0, 1, 0, 0]),
            words(&[0, 2, 0, 73]),
            words(&[0, 1, 0]),
        ] {
            assert!(registration_reply(&Record::new(0x885, body), None).is_err());
        }
        assert!(session_released(&Record::new(0x887, words(&[0, 1]))).is_ok());
        assert!(session_released(&Record::new(0x887, words(&[0, 2]))).is_err());
    }
    #[test]
    fn generated_commands_pass_independent_structural_decoder() {
        let link = 0x02000491;
        let commands = [
            create_hs_iso15765(link),
            set_unum32(link, 9, 5, 500000),
            response_table(link, &[(0, vec![(34, 6, 0x7e8), (31, 6, 0x7e0)])]).unwrap(),
            primitive(
                link,
                0x02100491,
                0x8004,
                &[0, 0, 7, 0xe0, 0x1a, 0x90],
                &[0x22, 0, 0, 0x40],
                1,
                0,
                None,
            )
            .unwrap(),
            primitive(
                link,
                0x02100492,
                0x8004,
                &[],
                &[0xa0, 1, 0, 0],
                0,
                -1,
                Some(0x2534),
            )
            .unwrap(),
        ];
        let bytes: Vec<u8> = commands.iter().flat_map(|x| x.encoded().unwrap()).collect();
        let decoded = crate::trace::decode_stream(&bytes).unwrap();
        assert_eq!(decoded[0]["decoded"]["resource"]["pins"][1]["pin"], 14);
        assert_eq!(decoded[1]["decoded"]["value"], 500000);
        assert_eq!(decoded[3]["decoded"]["data_hex"], "000007e01a90");
        assert_eq!(decoded[4]["decoded"]["control"]["receive_cycles"], -1);
        assert!(primitive(link, 1, 0x9999, &[], &[], 0, 0, None).is_err());
        assert!(primitive(link, 1, 0x8004, &[0; 4129], &[], 1, 0, None).is_err());
    }
    #[test]
    fn tags_are_allocated_without_saved_handles() {
        let mut a = Tags::for_session(73);
        let mut b = Tags::for_session(74);
        assert_ne!(a.link(), b.link());
        assert_ne!(a.primitive(), b.primitive());
        let t = a.primitive();
        assert_eq!(a.primitive(), t + 1);
    }
}
#[test]
fn spa_generated_route_receiver_and_read_match_native_structure() {
    let commands = [
        create_sw_iso15765(3),
        receive_on_id(3, 5, 0x65b).unwrap(),
        spa_preconditions(3, 6),
    ];
    let bytes: Vec<u8> = commands.iter().flat_map(|r| r.encoded().unwrap()).collect();
    let d = crate::trace::decode_stream(&bytes).unwrap();
    assert_eq!(d[0]["decoded"]["resource"]["bus"], 14);
    assert_eq!(d[0]["decoded"]["resource"]["protocol"], 16);
    assert_eq!(d[0]["decoded"]["resource"]["pins"][0]["pin"], 1);
    let c = &d[1]["decoded"]["control"];
    assert_eq!(c["receive_cycles"], -1);
    assert_eq!(c["expected_responses"][0]["pattern_hex"], "0000065b");
    assert_eq!(d[2]["decoded"]["data_hex"], "0000025b1a9a");
    assert_eq!(d[2]["decoded"]["control"]["time_ms"], 500);
    assert!(receive_on_id(3, 5, 0x800).is_err());
    let bytes: Vec<u8> = [
        create_sw_raw(3),
        create_hs_raw(4),
        raw_frame(3, 5, 0x25b, &[2, 0x1a, 0x9a, 0, 0, 0, 0, 0], false).unwrap(),
        raw_frame(3, 6, 0x100, &[], true).unwrap(),
    ]
    .iter()
    .flat_map(|r| r.encoded().unwrap())
    .collect();
    let d = crate::trace::decode_stream(&bytes).unwrap();
    assert_eq!(d[0]["decoded"]["flags_hex"], "a0010000");
    assert_eq!(d[1]["decoded"]["resource"]["protocol"], 6);
    assert_eq!(d[2]["decoded"]["data_hex"], "0000025b021a9a0000000000");
    assert_eq!(d[3]["decoded"]["control"]["temporary_params"], 1);
    assert_eq!(d[3]["decoded"]["control"]["flags_hex"], "20000000");
    assert!(raw_frame(3, 6, 0x101, &[], true).is_err());
    assert!(raw_frame(3, 6, 0x800, &[0], false).is_err());
    assert!(raw_frame(3, 6, 0x100, &[0], true).is_err());
}
