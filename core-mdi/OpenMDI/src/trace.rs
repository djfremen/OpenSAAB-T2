// SPDX-License-Identifier: MPL-2.0
//! Offline decoder for the observed classic MDI 8.5.33.154 TCP serialization.
//! Input must already be reassembled, ordered TCP bytes from one direction.
//! This is not a driver, PCAP parser, command encoder, or CAN-frame converter.
//! Unknown opcodes remain opaque; status trailers retain their unknown meaning.
use serde_json::{json, Value};

pub const MAX_STREAM_BYTES: usize = 32 * 1024 * 1024;
const MAX_RECORD_BYTES: usize = 1024 * 1024;
const MAX_ARRAY_ITEMS: usize = 4096;
const MAX_RECORDS: usize = 65_536;

#[derive(Debug, PartialEq, Eq)]
pub struct DecodeError {
    pub offset: usize,
    pub reason: &'static str,
}

pub fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(DIGITS[(b >> 4) as usize] as char);
        out.push(DIGITS[(b & 15) as usize] as char);
    }
    out
}

struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
    base: usize,
}
impl<'a> Cursor<'a> {
    fn fail(&self, reason: &'static str) -> DecodeError {
        DecodeError {
            offset: self.base + self.pos,
            reason,
        }
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8], DecodeError> {
        let end = self
            .pos
            .checked_add(n)
            .ok_or_else(|| self.fail("length overflow"))?;
        let data = self
            .bytes
            .get(self.pos..end)
            .ok_or_else(|| self.fail("truncated field"))?;
        self.pos = end;
        Ok(data)
    }
    fn u32(&mut self) -> Result<u32, DecodeError> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn boolean(&mut self) -> Result<bool, DecodeError> {
        match self.take(1)?[0] {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(self.fail("invalid presence byte")),
        }
    }
    fn expect(&mut self, n: u32) -> Result<(), DecodeError> {
        if self.u32()? != n {
            return Err(self.fail("unexpected discriminator"));
        }
        Ok(())
    }
    fn count(&mut self) -> Result<usize, DecodeError> {
        let n = self.u32()? as usize;
        if n > MAX_ARRAY_ITEMS {
            return Err(self.fail("array limit exceeded"));
        }
        Ok(n)
    }
    fn blob(&mut self) -> Result<&'a [u8], DecodeError> {
        let n = self.u32()? as usize;
        self.take(n)
    }
    fn finish(&self) -> Result<(), DecodeError> {
        if self.pos != self.bytes.len() {
            return Err(self.fail("unexpected trailing bytes"));
        }
        Ok(())
    }
}

/// Decode complete records. A partial header/body is an error, never a success.
pub fn decode_stream(bytes: &[u8]) -> Result<Vec<Value>, DecodeError> {
    if bytes.len() > MAX_STREAM_BYTES {
        return Err(DecodeError {
            offset: 0,
            reason: "stream limit exceeded",
        });
    }
    let mut records = Vec::new();
    let mut offset = 0;
    while offset < bytes.len() {
        if records.len() == MAX_RECORDS {
            return Err(DecodeError {
                offset,
                reason: "record count limit exceeded",
            });
        }
        let mut c = Cursor {
            bytes: &bytes[offset..],
            pos: 0,
            base: offset,
        };
        let length = c.u32()? as usize;
        let opcode = c.u32()?;
        if !(8..=MAX_RECORD_BYTES).contains(&length) {
            return Err(c.fail("invalid record length"));
        }
        let body = c.take(length - 8)?;
        let decoded = decode_body(opcode, body, offset + 8)?;
        records.push(json!({"offset": offset, "length": length, "opcode": opcode,
            "decoded": decoded}));
        offset += length;
    }
    Ok(records)
}

fn decode_body(opcode: u32, bytes: &[u8], base: usize) -> Result<Value, DecodeError> {
    let mut c = Cursor {
        bytes,
        pos: 0,
        base,
    };
    let value = match opcode {
        0x44f => {
            let link = c.u32()?;
            let resource = if c.boolean()? {
                let bus = c.u32()?;
                let protocol = c.u32()?;
                let n = c.count()?;
                let mut pins = Vec::new();
                for _ in 0..n {
                    pins.push(json!({"pin": c.u32()?, "type": c.u32()?}));
                }
                json!({"bus": bus, "protocol": protocol, "pins": pins})
            } else {
                Value::Null
            };
            let resource_id = c.u32()?;
            let flags = if c.boolean()? {
                json!(hex(c.blob()?))
            } else {
                Value::Null
            };
            json!({"kind": "create_link", "link": link, "resource": resource,
                "resource_id": resource_id, "flags_hex": flags})
        }
        0x459 => {
            let link = c.u32()?;
            c.expect(0x105)?;
            c.expect(0x1200)?;
            if !c.boolean()? {
                return Err(c.fail("UNUM32 parameter item absent"));
            }
            c.expect(0x1200)?;
            let id = c.u32()?;
            c.expect(0x105)?;
            let class = c.u32()?;
            if !c.boolean()? {
                return Err(c.fail("UNUM32 parameter value absent"));
            }
            json!({"kind": "set_unum32_param", "link": link, "id": id,
                "class": class, "value": c.u32()?})
        }
        0x45d => {
            let link = c.u32()?;
            c.expect(0x1700)?;
            c.expect(0x1700)?;
            let n = c.count()?;
            let mut entries = Vec::new();
            for _ in 0..n {
                let unique_response_id = c.u32()?;
                let np = c.count()?;
                let mut params = Vec::new();
                for _ in 0..np {
                    c.expect(0x105)?;
                    c.expect(0x1200)?;
                    let id = c.u32()?;
                    c.expect(0x105)?;
                    let class = c.u32()?;
                    if !c.boolean()? {
                        return Err(c.fail("table parameter value absent"));
                    }
                    params.push(json!({"id": id, "class": class, "value": c.u32()?}));
                }
                entries.push(json!({"unique_response_id": unique_response_id, "params": params}));
            }
            json!({"kind": "unique_response_table", "link": link, "entries": entries})
        }
        0x463 => {
            let link = c.u32()?;
            let token = c.u32()?;
            let primitive_type = c.u32()?;
            let data = hex(c.blob()?);
            let control = if c.boolean()? {
                let time_ms = c.u32()?;
                let send_cycles = c.u32()? as i32;
                let receive_cycles = c.u32()? as i32;
                let temporary_params = c.u32()?;
                let flags = hex(c.blob()?);
                let n = c.count()?;
                let mut expected = Vec::new();
                for _ in 0..n {
                    let response_type = c.u32()?;
                    let acceptance_id = c.u32()?;
                    let nmask = c.u32()? as usize;
                    let mask = hex(c.take(nmask)?);
                    let pattern = hex(c.take(nmask)?);
                    let nids = c.count()?;
                    let mut ids = Vec::new();
                    for _ in 0..nids {
                        ids.push(c.u32()?);
                    }
                    expected.push(json!({"response_type": response_type,
                        "acceptance_id": acceptance_id, "mask_hex": mask,
                        "pattern_hex": pattern, "unique_response_ids": ids}));
                }
                json!({"time_ms": time_ms, "send_cycles": send_cycles,
                    "receive_cycles": receive_cycles, "temporary_params": temporary_params,
                    "flags_hex": flags, "expected_responses": expected})
            } else {
                Value::Null
            };
            json!({"kind": "start_primitive", "link": link, "token": token,
                "primitive_type": primitive_type, "data_hex": data, "control": control})
        }
        0x451 | 0x45f | 0x461 => {
            let kind = match opcode {
                0x451 => "destroy_link",
                0x45f => "connect",
                _ => "disconnect",
            };
            json!({"kind": kind, "link": c.u32()?})
        }
        0x465 => json!({"kind": "cancel_primitive", "link": c.u32()?, "token": c.u32()?}),
        0x863 | 0x5e1 | 0x5e3 | 0x5df => event(opcode, &mut c)?,
        // Reply prefix only; no interpretation of the operation-specific tail.
        0x44e | 0x450 | 0x452 | 0x45a | 0x45e | 0x460 | 0x462 | 0x464 | 0x466 | 0x468 | 0x885
        | 0x887 | 0x889 => {
            let status = c.u32()?;
            let tail = hex(c.take(bytes.len() - c.pos)?);
            json!({"kind": "rpc_reply", "status": status, "opaque_tail_hex": tail})
        }
        _ => {
            c.pos = bytes.len();
            json!({"kind": "opaque", "body_hex": hex(bytes)})
        }
    };
    c.finish()?;
    Ok(value)
}

fn event(opcode: u32, c: &mut Cursor<'_>) -> Result<Value, DecodeError> {
    let link = c.u32()?;
    let count = if opcode == 0x863 { c.count()? } else { 1 };
    if count == 0 {
        return Err(c.fail("empty event batch"));
    }
    let mut items = Vec::with_capacity(count);
    for _ in 0..count {
        items.push(event_item(opcode, link, c)?);
    }
    if count == 1 {
        Ok(items.pop().unwrap())
    } else {
        Ok(json!({"kind": "event_batch", "link": link, "items": items}))
    }
}

fn event_item(opcode: u32, link: u32, c: &mut Cursor<'_>) -> Result<Value, DecodeError> {
    let item_type = c.u32()?;
    c.expect(item_type)?;
    if opcode == 0x863 {
        c.expect(item_type)?;
    }
    let token = c.u32()?;
    let timestamp = c.u32()?;
    if !c.boolean()? {
        return Err(c.fail("event data absent"));
    }
    let data = match (opcode, item_type) {
        (0x863 | 0x5e1, 0x1300) => {
            let rx_flags = hex(c.blob()?);
            let unique_response_id = c.u32()?;
            let acceptance_id = c.u32()?;
            let timestamp_flags = hex(c.blob()?);
            let tx_done_timestamp = c.u32()?;
            let start_timestamp = c.u32()?;
            let extra = if c.boolean()? {
                let nh = c.u32()? as usize;
                let nf = c.u32()? as usize;
                json!({"header_hex": hex(c.take(nh)?), "footer_hex": hex(c.take(nf)?)})
            } else {
                Value::Null
            };
            json!({"kind": "result", "rx_flags_hex": rx_flags,
                "unique_response_id": unique_response_id, "acceptance_id": acceptance_id,
                "timestamp_flags_hex": timestamp_flags, "tx_done_timestamp": tx_done_timestamp,
                "start_timestamp": start_timestamp, "extra": extra, "data_hex": hex(c.blob()?)})
        }
        (0x863, 0x1301) => json!({"kind": "status", "status": c.u32()?,
            "unknown_trailer_hex": hex(c.take(4)?)}),
        (0x5e3, 0x1303) | (0x5df, 0x1302) => json!({
            "kind": if item_type == 0x1302 { "error" } else { "info" },
            "code": c.u32()?, "extra": c.u32()?}),
        _ => return Err(c.fail("unsupported event opcode/type combination")),
    };
    Ok(json!({"kind": "event", "link": link, "token": token,
        "device_timestamp": timestamp, "item_type": item_type, "data": data}))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn record(op: u32, body: &[u8]) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend(((body.len() + 8) as u32).to_le_bytes());
        b.extend(op.to_le_bytes());
        b.extend(body);
        b
    }
    fn words(v: &[u32]) -> Vec<u8> {
        v.iter().flat_map(|n| n.to_le_bytes()).collect()
    }
    #[test]
    fn bounds_and_partial_records_fail() {
        for b in [
            vec![0; 7],
            words(&[0, 0]),
            words(&[u32::MAX, 0]),
            words(&[9, 0]),
        ] {
            assert!(decode_stream(&b).is_err());
        }
        assert!(decode_stream(&vec![0; MAX_STREAM_BYTES + 1]).is_err());
        let mut b = record(0xffff, &[1]);
        b.push(0);
        assert_eq!(decode_stream(&b).unwrap_err().offset, 9);
    }
    #[test]
    fn opaque_and_reply_remain_distinct() {
        let mut b = record(0x467, &[1, 2]);
        b.extend(record(0x450, &words(&[5, 9])));
        let r = decode_stream(&b).unwrap();
        assert_eq!(r[0]["decoded"]["kind"], "opaque");
        assert_eq!(r[1]["decoded"]["status"], 5);
        assert_eq!(r[1]["decoded"]["opaque_tail_hex"], "09000000");
    }
    #[test]
    fn presence_counts_and_trailing_bytes_are_strict() {
        let mut b = words(&[1]);
        b.push(2);
        assert!(decode_stream(&record(0x44f, &b)).is_err());
        b[4] = 1;
        b.extend(words(&[14, 16, 4097]));
        assert!(decode_stream(&record(0x44f, &b)).is_err());
        assert!(decode_stream(&record(0x45f, &words(&[1, 2]))).is_err());
    }
    #[test]
    fn receive_cycles_and_expected_mask_are_preserved() {
        let mut b = words(&[1, 2, 0x8004, 0]);
        b.push(1);
        b.extend(words(&[0, 0, u32::MAX, 0, 0, 1, 0, 3, 4]));
        b.extend([0x1f, 0xff, 0xff, 0xff, 0, 0, 6, 0x41]);
        b.extend(words(&[0]));
        let r = decode_stream(&record(0x463, &b)).unwrap();
        let ctrl = &r[0]["decoded"]["control"];
        assert_eq!(ctrl["receive_cycles"], -1);
        assert_eq!(ctrl["expected_responses"][0]["pattern_hex"], "00000641");
        b.pop();
        assert!(decode_stream(&record(0x463, &b)).is_err());
    }
    #[test]
    fn unique_response_table_keeps_request_and_reply_ids() {
        let mut b = words(&[1, 0x1700, 0x1700, 1, 0, 2]);
        for (id, value) in [(31, 0x241), (34, 0x641)] {
            b.extend(words(&[0x105, 0x1200, id, 0x105, 6]));
            b.push(1);
            b.extend(words(&[value]));
        }
        let r = decode_stream(&record(0x45d, &b)).unwrap();
        let p = &r[0]["decoded"]["entries"][0]["params"];
        assert_eq!(p[0]["value"], 0x241);
        assert_eq!(p[1]["value"], 0x641);
        b.pop();
        assert!(decode_stream(&record(0x45d, &b)).is_err());
    }
    #[test]
    fn zero_payload_wake_and_unknown_status_trailer_are_preserved() {
        let mut b = words(&[3, 2, 0x8004, 4]);
        b.extend([0, 0, 1, 0]);
        b.push(1);
        b.extend(words(&[500, 1, 0, 1, 4]));
        b.extend([0; 4]);
        b.extend(words(&[0]));
        let r = decode_stream(&record(0x463, &b)).unwrap();
        assert_eq!(r[0]["decoded"]["data_hex"], "00000100");
        assert_eq!(r[0]["decoded"]["control"]["temporary_params"], 1);
        let mut s = words(&[3, 1, 0x1301, 0x1301, 0x1301, 2, 100]);
        s.push(1);
        s.extend(words(&[0x8012, 0x12345678]));
        let r = decode_stream(&record(0x863, &s)).unwrap();
        assert_eq!(r[0]["decoded"]["data"]["unknown_trailer_hex"], "78563412");
        s[4] = 2;
        assert!(decode_stream(&record(0x863, &s)).is_err());
    }
    #[test]
    fn async_error_is_not_rpc_success_or_timeout() {
        let mut b = words(&[3, 0x1302, 0x1302, 2, 100]);
        b.push(1);
        b.extend(words(&[0x101, 5]));
        let r = decode_stream(&record(0x5df, &b)).unwrap();
        assert_eq!(r[0]["decoded"]["data"]["kind"], "error");
        assert_eq!(r[0]["decoded"]["data"]["code"], 0x101);
        b[8] = 3;
        assert!(decode_stream(&record(0x5df, &b)).is_err());
    }
    #[test]
    fn assembled_result_remains_pdu_bytes() {
        let mut b = words(&[1, 1, 0x1300, 0x1300, 0x1300, 2, 100]);
        b.push(1);
        b.extend(words(&[4]));
        b.extend([0, 0, 0, 0x40]);
        b.extend(words(&[0, 0, 0, 0, 0]));
        b.push(0);
        b.extend(words(&[7]));
        b.extend([0, 0, 6, 0x41, 0x5a, 0x90, 0x41]);
        let r = decode_stream(&record(0x863, &b)).unwrap();
        assert_eq!(r[0]["decoded"]["data"]["data_hex"], "000006415a9041");
        assert!(r[0]["decoded"]["data"].get("can_frame").is_none());
    }
    #[test]
    fn batched_status_items_are_complete_and_bounded() {
        let mut body = words(&[3, 2]);
        for token in [5, 6] {
            body.extend(words(&[0x1301, 0x1301, 0x1301, token, 100]));
            body.push(1);
            body.extend(words(&[0x8012, 0]));
        }
        let r = decode_stream(&record(0x863, &body)).unwrap();
        assert_eq!(r[0]["decoded"]["items"][1]["token"], 6);
        body.pop();
        assert!(decode_stream(&record(0x863, &body)).is_err());
        for count in [0, 4097, u32::MAX] {
            assert!(decode_stream(&record(0x863, &words(&[3, count]))).is_err());
        }
    }
    #[test]
    fn standalone_result_uses_two_type_discriminators() {
        let mut body = words(&[3, 0x1300, 0x1300, 5, 100]);
        body.push(1);
        body.extend(words(&[4]));
        body.extend([0, 0, 0, 0x40]);
        body.extend(words(&[0, 0x2534, 0, 0, 0]));
        body.push(0);
        body.extend(words(&[6]));
        body.extend([0, 0, 7, 0xe8, 0x5a, 0x90]);
        let r = decode_stream(&record(0x5e1, &body)).unwrap();
        assert_eq!(r[0]["decoded"]["data"]["data_hex"], "000007e85a90");
        body[8] = 1;
        assert!(decode_stream(&record(0x5e1, &body)).is_err());
    }
}
