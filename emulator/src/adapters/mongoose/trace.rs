// SPDX-License-Identifier: MPL-2.0
//! Bounded offline USBPcap reader. Default report contains no VIN, serial or payload.
use super::{
    init,
    protocol::{Decoder, Record},
};
use crate::adapter_translation::isotp::VinReply;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_CAPTURE_BYTES: usize = 64 * 1024 * 1024;
const MAX_USB_RECORDS: usize = 1_000_000;

struct Usb<'a> {
    index: usize,
    at_us: u64,
    status: u32,
    info: u8,
    bus: u16,
    device: u16,
    endpoint: u8,
    transfer: u8,
    body: &'a [u8],
}
fn le16(b: &[u8], i: usize) -> u16 {
    u16::from_le_bytes(b[i..i + 2].try_into().unwrap())
}
fn le32(b: &[u8], i: usize) -> u32 {
    u32::from_le_bytes(b[i..i + 4].try_into().unwrap())
}

fn usb_records(bytes: &[u8]) -> Result<Vec<Usb<'_>>, String> {
    if bytes.len() > MAX_CAPTURE_BYTES || bytes.len() < 24 {
        return Err("PCAP size outside bounds".into());
    }
    let nanos = match &bytes[..4] {
        [0xd4, 0xc3, 0xb2, 0xa1] => false,
        [0x4d, 0x3c, 0xb2, 0xa1] => true,
        _ => return Err("Expected little-endian PCAP".into()),
    };
    if le16(bytes, 4) != 2 || le16(bytes, 6) != 4 || le32(bytes, 20) != 249 {
        return Err("Expected PCAP 2.4 USBPcap linktype 249".into());
    }
    let snap = le32(bytes, 16) as usize;
    let mut offset = 24;
    let mut out = Vec::new();
    while offset < bytes.len() {
        if out.len() == MAX_USB_RECORDS || bytes.len() - offset < 16 {
            return Err("PCAP record limit/truncated header".into());
        }
        let h = &bytes[offset..offset + 16];
        let n = le32(h, 8) as usize;
        let frac = le32(h, 4) as u64;
        if n != le32(h, 12) as usize
            || n > snap
            || n < 27
            || frac >= if nanos { 1_000_000_000 } else { 1_000_000 }
        {
            return Err("Truncated or malformed PCAP record".into());
        }
        offset += 16;
        let end = offset
            .checked_add(n)
            .filter(|&e| e <= bytes.len())
            .ok_or("PCAP payload outside file")?;
        let p = &bytes[offset..end];
        offset = end;
        let header = le16(p, 0) as usize;
        let length = le32(p, 23) as usize;
        if header < 27 || header > p.len() || length != p.len() - header {
            return Err("USBPcap header/data length mismatch".into());
        }
        out.push(Usb {
            index: out.len() + 1,
            at_us: le32(h, 0) as u64 * 1_000_000 + if nanos { frac / 1000 } else { frac },
            status: le32(p, 10),
            info: p[16],
            bus: le16(p, 17),
            device: le16(p, 19),
            endpoint: p[21],
            transfer: p[22],
            body: &p[header..],
        });
    }
    Ok(out)
}

/// Select the unique GM II descriptor identity. Do not guess from hub/address/name.
pub fn review(bytes: &[u8]) -> Result<Value, String> {
    let usb = usb_records(bytes)?;
    let identities: BTreeSet<_> = usb
        .iter()
        .filter(|r| {
            r.transfer == 2
                && r.info & 1 == 1
                && r.status == 0
                && r.body.len() == 18
                && r.body[..2] == [18, 1]
                && le16(r.body, 8) == init::VID
                && le16(r.body, 10) == init::PID
        })
        .map(|r| (r.bus, r.device))
        .collect();
    if identities.len() != 1 {
        return Err("Capture must identify exactly one MongoosePro GM II descriptor".into());
    }
    let identity = *identities.iter().next().unwrap();
    let selected: Vec<_> = usb
        .iter()
        .filter(|r| (r.bus, r.device) == identity)
        .collect();
    let mut decoders = [Decoder::default(), Decoder::default()];
    let mut opcodes = BTreeMap::<String, usize>::new();
    let mut statuses = BTreeMap::<String, usize>::new();
    let mut reply_statuses = BTreeMap::<String, usize>::new();
    let mut pending = BTreeMap::<(u16, u16, u16, u16), Record>::new();
    let mut decoded = 0;
    let mut matched = 0;
    let mut unsolicited = 0;
    let mut rx = 0;
    let mut bulk_bytes = 0;
    let mut bulk_payloads = 0;
    let mut errors = 0;
    let mut controls = Vec::new();
    let mut lifecycle = Vec::new();
    let mut opens = Vec::new();
    let mut startup = Vec::<Record>::new();
    let mut startups = Vec::<Vec<Record>>::new();
    let mut in_startup = false;
    let mut vins = BTreeMap::<(u16, u32), (u64, VinReply)>::new();
    let mut vin_values = BTreeSet::<String>::new();
    let mut vin_complete = 0;
    let mut vin_failed = 0;
    let start = usb.first().ok_or("Empty PCAP")?.at_us;
    for r in selected {
        *statuses.entry(format!("0x{:08x}", r.status)).or_default() += 1;
        if r.status != 0 {
            errors += 1;
            continue;
        }
        if r.transfer == 2
            && r.info & 1 == 0
            && r.body.len() == 8
            && r.body[0] == 0x40
            && r.body[1] == init::VENDOR_CONTROL_REQUEST
        {
            controls.push(json!({"packet":r.index,"at_us":r.at_us.saturating_sub(start),
                "request":219,"value":le16(r.body,2),"index":le16(r.body,4),"length":le16(r.body,6)}));
        }
        if r.transfer != 3 || r.body.is_empty() {
            continue;
        }
        let direction = if r.endpoint == init::BULK_OUT && r.info & 1 == 0 {
            0
        } else if r.endpoint == init::BULK_IN && r.info & 1 == 1 {
            1
        } else {
            return Err(
                "Unexpected bulk endpoint/direction or duplicate completion payload".into(),
            );
        };
        bulk_bytes += r.body.len();
        bulk_payloads += 1;
        for record in decoders[direction].push(r.body)? {
            decoded += 1;
            *opcodes
                .entry(format!(
                    "{}:0x{:04x}",
                    if direction == 0 { "out" } else { "in" },
                    record.opcode
                ))
                .or_default() += 1;
            if direction == 0 {
                let key = (
                    record.destination,
                    record.source,
                    record.opcode,
                    record.sequence,
                );
                if pending.insert(key, record.clone()).is_some() {
                    return Err("Reused Mongoose pending request key".into());
                }
                if record.opcode == 0x100 {
                    if !startup.is_empty() {
                        startups.push(std::mem::take(&mut startup));
                    }
                    in_startup = true;
                }
                if in_startup && record.opcode != 5 && record.opcode != 6 {
                    let mut normalized = record.clone();
                    normalized.auxiliary = [0; 4];
                    startup.push(normalized);
                }
                if record.opcode == 5 || record.opcode == 6 {
                    if !startup.is_empty() {
                        startups.push(std::mem::take(&mut startup));
                    }
                    in_startup = false;
                }
                if [0x100, 3, 5, 6, 7, 0x12].contains(&record.opcode) {
                    let mut entry = json!({"packet":r.index,"at_us":r.at_us.saturating_sub(start),
                        "opcode":record.opcode,"destination":record.destination,"sequence":record.sequence});
                    if record.opcode == 6 && record.body.len() == 8 {
                        entry["flags"] = json!(le32(&record.body, 0));
                        entry["baud"] = json!(le32(&record.body, 4));
                        opens.push(entry.clone());
                    }
                    if record.opcode == 0x12 && record.body.len() == 12 {
                        entry["parameters"] = json!([
                            le32(&record.body, 0),
                            le32(&record.body, 4),
                            le32(&record.body, 8)
                        ]);
                    }
                    lifecycle.push(entry);
                }
            } else if record.opcode & 0x8000 != 0 {
                let key = (
                    record.source,
                    record.destination,
                    record.opcode & 0x7fff,
                    record.sequence,
                );
                let request = pending.remove(&key).ok_or("Unmatched Mongoose reply")?;
                if !request.matches_reply(&record) {
                    return Err("Mongoose reply correlation failed".into());
                }
                matched += 1;
                if let Some(status) = record.reply_first_word() {
                    *reply_statuses
                        .entry(format!("0x{:04x}:0x{status:08x}", record.opcode))
                        .or_default() += 1;
                }
            } else {
                unsolicited += 1;
                if let Some(m) = record.received_message()? {
                    rx += 1;
                    // Narrow ISO-TP evidence only: expected 19-byte 5A90 response,
                    // standard address, eight CAN-shaped bytes, zero reported status.
                    if m.status != 0 || m.data.len() != 12 || m.extra_index != 12 {
                        continue;
                    }
                    let id = u32::from_be_bytes(m.data[..4].try_into().unwrap());
                    if id > 0x7ff {
                        continue;
                    }
                    let data = &m.data[4..];
                    let key = (record.source, id);
                    if data[..4] == [0x10, 19, 0x5a, 0x90] {
                        if vins.insert(key, (r.at_us, VinReply::default())).is_some() {
                            vin_failed += 1;
                        }
                    } else if data[0] >> 4 != 2 {
                        continue;
                    }
                    if let Some((at, receiver)) = vins.get_mut(&key) {
                        let failed = r.at_us < *at
                            || r.at_us - *at > 1_000_000
                            || receiver.feed(data).is_err();
                        if failed {
                            vins.remove(&key);
                            vin_failed += 1;
                        } else if let Some(vin) = receiver.vin()? {
                            vin_values.insert(vin);
                            vin_complete += 1;
                            vins.remove(&key);
                        }
                    }
                }
            }
        }
    }
    for d in &decoders {
        d.finish()?;
    }
    if decoded == 0 {
        return Err(
            "No selected Mongoose bulk traffic; descriptor-only capture cannot establish startup"
                .into(),
        );
    }
    if !startup.is_empty() {
        startups.push(startup);
    }
    let reference = init::startup_reference();
    Ok(
        json!({"schema":1,"profile":"mongoose-pro-gm-ii-1.2.8.0-capture-research",
        "pcap_records":usb.len(),"selected_bulk_payload_records":bulk_payloads,"selected_bulk_payload_bytes":bulk_bytes,
        "decoded_rpc_records":decoded,"matched_rpc_replies":matched,"pending_rpc_requests":pending.len(),
        "unsolicited_records":unsolicited,"received_messages":rx,"opcodes":opcodes,
        "selected_usb_status_counts":statuses,"selected_usb_error_records":errors,"reply_first_word_counts":reply_statuses,
        "vendor_controls":controls,"startup_sequences":startups.iter().map(|s|s.iter().map(|r|r.opcode).collect::<Vec<_>>()).collect::<Vec<_>>(),
        "startup_sequences_matching_reference":startups.iter().filter(|s|**s==reference).count(),
        "channel_opens":opens,"lifecycle":lifecycle,
        "complete_vin_responses":vin_complete,"unique_vin_values":vin_values.len(),
        "incomplete_or_invalid_vin_sequences":vin_failed+vins.len(),
        "raw_payloads_included":false,"hardware_tested":false,
        "limits":"Offline structure and narrow VIN reassembly only. No live backend, cold/wake proof, complete DTC report or installed-product qualification. USB/RPC status does not prove electrical CAN ACK."}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires private contributor USBPcap; never commit raw input"]
    fn captured_startup_replies_drive_the_beta_session() {
        use crate::adapters::{
            common::usb::UsbTransport,
            mongoose::live::{DeviceIo, Mode},
        };
        use std::collections::VecDeque;
        struct Replay {
            pairs: VecDeque<(Record, Record)>,
            rx: VecDeque<Vec<u8>>,
        }
        impl UsbTransport for Replay {
            fn write(&mut self, b: &[u8]) -> Result<(), String> {
                let actual = Record::decode(b)?;
                let (mut expected, reply) =
                    self.pairs.pop_front().ok_or("Unexpected beta request")?;
                expected.auxiliary = [0; 4];
                if actual != expected {
                    return Err("Beta request differs from captured startup".into());
                }
                self.rx.push_back(reply.encode()?);
                Ok(())
            }
            fn read(&mut self) -> Result<Vec<u8>, String> {
                Ok(self.rx.pop_front().unwrap_or_default())
            }
        }
        impl DeviceIo for Replay {
            fn control(&mut self, _: u16) -> Result<(), String> {
                Ok(())
            }
            fn close_usb(&mut self) -> Result<(), String> {
                Ok(())
            }
        }
        let path = std::env::var("OPENSAAB_MONGOOSE_CAPTURE").expect("Set private capture path");
        let bytes = std::fs::read(path).unwrap();
        let usb = usb_records(&bytes).unwrap();
        let mut started = false;
        let mut requests = Vec::<Record>::new();
        let mut pairs = VecDeque::new();
        for r in usb {
            if r.device != 1 || r.transfer != 3 || r.body.is_empty() {
                continue;
            }
            let record = Record::decode(r.body).unwrap();
            if r.endpoint == 1 && record.opcode == 0x100 {
                started = true;
            }
            if !started {
                continue;
            }
            if r.endpoint == 1 {
                requests.push(record);
            } else if r.endpoint == 0x82 && record.opcode & 0x8000 != 0 {
                let n = requests
                    .iter()
                    .position(|p| p.matches_reply(&record))
                    .unwrap();
                let request = requests.remove(n);
                let is_close = request.opcode == 5;
                pairs.push_back((request, record));
                if is_close {
                    break;
                }
            }
        }
        assert_eq!(pairs.len(), 12);
        let mut io = Replay {
            pairs,
            rx: VecDeque::new(),
        };
        let result = crate::adapters::mongoose::live::run(&mut io, Mode::Startup);
        assert_eq!(result["status"], "passed");
        assert_eq!(result["events"].as_array().unwrap().len(), 12);
        assert!(io.pairs.is_empty());
        assert_eq!(result["application_vehicle_transmits"], 0);
    }
    fn capture(packets: &[(u8, u8, u8, Vec<u8>)]) -> Vec<u8> {
        let mut b = vec![0; 24];
        b[..4].copy_from_slice(&[0xd4, 0xc3, 0xb2, 0xa1]);
        b[4] = 2;
        b[6] = 4;
        b[16..20].copy_from_slice(&65535u32.to_le_bytes());
        b[20] = 249;
        for (i, (endpoint, transfer, info, body)) in packets.iter().enumerate() {
            let mut p = vec![0; 27];
            p[..2].copy_from_slice(&27u16.to_le_bytes());
            p[16] = *info;
            p[17] = 1;
            p[19] = 1;
            p[21] = *endpoint;
            p[22] = *transfer;
            p[23..27].copy_from_slice(&(body.len() as u32).to_le_bytes());
            p.extend(body);
            b.extend(1u32.to_le_bytes());
            b.extend((i as u32 * 1000).to_le_bytes());
            b.extend((p.len() as u32).to_le_bytes());
            b.extend((p.len() as u32).to_le_bytes());
            b.extend(p);
        }
        b
    }
    fn identity() -> (u8, u8, u8, Vec<u8>) {
        let mut d = vec![0; 18];
        d[0] = 18;
        d[1] = 1;
        d[8..10].copy_from_slice(&init::VID.to_le_bytes());
        d[10..12].copy_from_slice(&init::PID.to_le_bytes());
        (0x80, 2, 1, d)
    }
    #[test]
    fn segmented_vin_uses_one_route_and_safe_report_does_not_leak_value() {
        let vin = b"YS3FD49Y671000001";
        let payload = [&[0x5a, 0x90][..], vin].concat();
        let mut frames = vec![vec![0x10, 19]];
        frames[0].extend(&payload[..6]);
        frames.push([&[0x21][..], &payload[6..13]].concat());
        frames.push([&[0x22][..], &payload[13..], &[0][..]].concat());
        let mut packets = vec![identity()];
        for data in frames {
            let body = [
                0u32.to_le_bytes().as_slice(),
                123u32.to_le_bytes().as_slice(),
                &[12, 0, 12, 0],
                &[0, 0, 7, 0xe8],
                &data,
            ]
            .concat();
            let r = Record {
                destination: 0,
                source: 0x601,
                opcode: 9,
                sequence: 0,
                auxiliary: [0; 4],
                body,
            };
            packets.push((0x82, 3, 1, r.encode().unwrap()));
        }
        let result = review(&capture(&packets)).unwrap();
        assert_eq!(result["complete_vin_responses"], 1);
        assert_eq!(result["incomplete_or_invalid_vin_sequences"], 0);
        assert!(!result
            .to_string()
            .contains(std::str::from_utf8(vin).unwrap()));
        // Move the last consecutive frame to a different route; no completion.
        packets[3].3[6..8].copy_from_slice(&0x1701u16.to_le_bytes());
        let result = review(&capture(&packets)).unwrap();
        assert_eq!(result["complete_vin_responses"], 0);
        assert_eq!(result["incomplete_or_invalid_vin_sequences"], 1);
    }
    #[test]
    fn unmatched_replies_wrong_identity_and_bulk_errors_are_not_success() {
        let mut id = identity();
        id.3[10] = 0;
        assert!(review(&capture(&[id])).is_err());
        let reply = Record {
            destination: 0,
            source: 1,
            opcode: 0x8100,
            sequence: 1,
            auxiliary: [0; 4],
            body: vec![],
        };
        assert!(review(&capture(&[
            identity(),
            (0x82, 3, 1, reply.encode().unwrap())
        ]))
        .unwrap_err()
        .contains("Unmatched"));
        assert!(review(&capture(&[
            identity(),
            (0x83, 3, 1, reply.encode().unwrap())
        ]))
        .unwrap_err()
        .contains("endpoint"));
    }
    #[test]
    #[ignore = "requires the private saab_tastic capture via OPENSAAB_MONGOOSE_CAPTURE"]
    fn original_capture_has_eight_startups_and_all_replies_and_vins() {
        let bytes = std::fs::read(
            std::env::var_os("OPENSAAB_MONGOOSE_CAPTURE").expect("capture path required"),
        )
        .unwrap();
        let r = review(&bytes).unwrap();
        for (key, value) in [
            ("pcap_records", 42498),
            ("decoded_rpc_records", 18076),
            ("matched_rpc_replies", 8749),
            ("pending_rpc_requests", 0),
            ("startup_sequences_matching_reference", 8),
            ("complete_vin_responses", 25),
            ("unique_vin_values", 1),
            ("incomplete_or_invalid_vin_sequences", 0),
        ] {
            assert_eq!(r[key], value, "{key}");
        }
    }
    #[test]
    fn wrong_empty_and_truncated_captures_fail() {
        assert!(review(&[]).is_err());
        let mut p = vec![0; 24];
        p[..4].copy_from_slice(&[0xd4, 0xc3, 0xb2, 0xa1]);
        p[4] = 2;
        p[6] = 4;
        p[20] = 249;
        assert!(review(&p).unwrap_err().contains("exactly one"));
        p.push(0);
        assert!(review(&p).unwrap_err().contains("truncated header"));
    }
}
