// SPDX-License-Identifier: MPL-2.0
//! Independently authored codec for the GM II 1.2.8.0 USB envelope.
//! Unknown fields/opcodes are retained. USB acceptance is never a CAN ACK.

pub const HEADER_BYTES: usize = 16;
pub const LENGTH_XOR: u16 = 0x51e6;
// monpa432.dll receive parser limits the length excluding its four-byte prefix.
pub const MAX_RECORD_BYTES: usize = 0x1800 + 4;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    pub destination: u16,
    pub source: u16,
    pub opcode: u16,
    pub sequence: u16,
    pub auxiliary: [u8; 4],
    pub body: Vec<u8>,
}

fn u16le(b: &[u8], p: usize) -> u16 {
    u16::from_le_bytes([b[p], b[p + 1]])
}
fn u32le(b: &[u8], p: usize) -> u32 {
    u32::from_le_bytes(b[p..p + 4].try_into().unwrap())
}

impl Record {
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        if !(HEADER_BYTES..=MAX_RECORD_BYTES).contains(&bytes.len()) {
            return Err("Mongoose record size outside bounds".into());
        }
        let length = u16le(bytes, 0);
        if length as usize + 4 != bytes.len() || u16le(bytes, 2) != length ^ LENGTH_XOR {
            return Err("Mongoose length/XOR mismatch".into());
        }
        Ok(Self {
            destination: u16le(bytes, 4),
            source: u16le(bytes, 6),
            opcode: u16le(bytes, 8),
            sequence: u16le(bytes, 10),
            auxiliary: bytes[12..16].try_into().unwrap(),
            body: bytes[16..].to_vec(),
        })
    }
    pub fn encode(&self) -> Result<Vec<u8>, String> {
        let size = HEADER_BYTES
            .checked_add(self.body.len())
            .ok_or("record size overflow")?;
        if size > MAX_RECORD_BYTES {
            return Err("Mongoose record size outside bounds".into());
        }
        let length = (size - 4) as u16;
        let mut bytes = Vec::with_capacity(size);
        for word in [
            length,
            length ^ LENGTH_XOR,
            self.destination,
            self.source,
            self.opcode,
            self.sequence,
        ] {
            bytes.extend(word.to_le_bytes());
        }
        bytes.extend(self.auxiliary);
        bytes.extend(&self.body);
        Ok(bytes)
    }
    /// Correlate addresses, opcode and sequence; sequence numbers wrap and restart.
    /// Caller must also enforce session identity and a finite response deadline.
    pub fn matches_reply(&self, reply: &Self) -> bool {
        self.opcode & 0x8000 == 0
            && reply.opcode == self.opcode | 0x8000
            && reply.sequence == self.sequence
            && reply.destination == self.source
            && reply.source == self.destination
    }
    /// First word of observed replies. Its meaning depends on opcode;
    /// query replies do not necessarily have a status prefix. No conversion
    /// into transmit completion. 0x8008 returns 0x100, not zero.
    pub fn reply_first_word(&self) -> Option<u32> {
        (self.opcode & 0x8000 != 0 && self.body.len() >= 4).then(|| u32le(&self.body, 0))
    }
    /// Observed 0x0009 message layout, including original data and status.
    /// This does NOT assert raw CAN versus an adapter-assembled ISO-TP payload.
    pub fn received_message(&self) -> Result<Option<Message>, String> {
        if self.opcode != 9 {
            return Ok(None);
        }
        if self.body.len() < 12 {
            return Err("Truncated Mongoose message".into());
        }
        let length = u16le(&self.body, 8) as usize;
        let extra_index = u16le(&self.body, 10) as usize;
        if length != self.body.len() - 12 || extra_index > length {
            return Err("Mongoose message data/extra index mismatch".into());
        }
        Ok(Some(Message {
            status: u32le(&self.body, 0),
            timestamp: u32le(&self.body, 4),
            extra_index,
            data: self.body[12..].to_vec(),
        }))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub status: u32,
    pub timestamp: u32,
    pub extra_index: usize,
    /// Address is big endian in observed CAN-shaped messages; bytes remain intact.
    pub data: Vec<u8>,
}

/// Handles split/coalesced bulk reads, with no speculative resynchronization.
#[derive(Default)]
pub struct Decoder {
    pending: Vec<u8>,
    failed: bool,
}
impl Decoder {
    pub fn push(&mut self, bytes: &[u8]) -> Result<Vec<Record>, String> {
        if self.failed {
            return Err("Mongoose decoder is terminal".into());
        }
        let result = self.consume(bytes);
        if result.is_err() {
            self.failed = true;
            self.pending.clear();
        }
        result
    }
    fn consume(&mut self, mut bytes: &[u8]) -> Result<Vec<Record>, String> {
        let mut records = Vec::new();
        while !bytes.is_empty() {
            let target = if self.pending.len() < 4 {
                4
            } else {
                let length = u16le(&self.pending, 0);
                if u16le(&self.pending, 2) != length ^ LENGTH_XOR
                    || !(HEADER_BYTES..=MAX_RECORD_BYTES).contains(&(length as usize + 4))
                {
                    return Err("Invalid Mongoose stream header".into());
                }
                length as usize + 4
            };
            let n = (target - self.pending.len()).min(bytes.len());
            self.pending.extend(&bytes[..n]);
            bytes = &bytes[n..];
            if self.pending.len() == target && target != 4 {
                records.push(Record::decode(&self.pending)?);
                self.pending.clear();
            }
        }
        Ok(records)
    }
    pub fn finish(&self) -> Result<(), String> {
        if self.failed || !self.pending.is_empty() {
            Err("Failed or incomplete Mongoose stream".into())
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request() -> Record {
        Record {
            destination: 1,
            source: 0,
            opcode: 0x100,
            sequence: 1,
            auxiliary: [0; 4],
            body: vec![],
        }
    }
    #[test]
    fn captured_start_envelope_and_fragmented_coalesced_reads() {
        let bytes = [0x0c, 0, 0xea, 0x51, 1, 0, 0, 0, 0, 1, 1, 0, 0, 0, 0xb3, 0];
        let mut r = request();
        r.auxiliary = [0, 0, 0xb3, 0];
        assert_eq!(r.encode().unwrap(), bytes);
        let twice = [bytes.as_slice(), bytes.as_slice()].concat();
        for split in 0..=twice.len() {
            let mut decoder = Decoder::default();
            let mut records = decoder.push(&twice[..split]).unwrap();
            records.extend(decoder.push(&twice[split..]).unwrap());
            decoder.finish().unwrap();
            assert_eq!(records, vec![r.clone(), r.clone()]);
        }
    }
    #[test]
    fn malformed_headers_partial_eof_and_terminal_errors() {
        let bytes = request().encode().unwrap();
        for n in 1..bytes.len() {
            let mut d = Decoder::default();
            d.push(&bytes[..n]).unwrap();
            assert!(d.finish().is_err());
        }
        let mut bad = bytes.clone();
        bad[2] ^= 1;
        let mut d = Decoder::default();
        assert!(d.push(&bad).is_err());
        assert!(d.push(&bytes).is_err());
        let mut oversized = request();
        oversized.body = vec![0; MAX_RECORD_BYTES];
        assert!(oversized.encode().is_err());
    }
    #[test]
    fn replies_require_exact_route_opcode_sequence_and_preserve_status() {
        let r = request();
        let mut reply = r.clone();
        reply.destination = 0;
        reply.source = 1;
        reply.opcode |= 0x8000;
        assert!(r.matches_reply(&reply));
        reply.sequence += 1;
        assert!(!r.matches_reply(&reply));
        reply.sequence -= 1;
        reply.source = 0x601;
        assert!(!r.matches_reply(&reply));
        reply.opcode = 0x8008;
        reply.body = 0x100u32.to_le_bytes().to_vec();
        assert_eq!(reply.reply_first_word(), Some(0x100));
        assert!(!r.matches_reply(&reply));
    }
    #[test]
    fn message_lengths_are_validated_without_inventing_can_frames() {
        let mut r = request();
        r.opcode = 9;
        r.body = [
            0u32.to_le_bytes().as_slice(),
            123u32.to_le_bytes().as_slice(),
            &[12, 0, 12, 0],
            &[0, 0, 5, 0x41],
            &[1; 8],
        ]
        .concat();
        let m = r.received_message().unwrap().unwrap();
        assert_eq!(m.timestamp, 123);
        assert_eq!(m.data.len(), 12);
        r.body[10] = 13;
        assert!(r.received_message().is_err());
        r.body[10] = 12;
        r.body.pop();
        assert!(r.received_message().is_err());
    }
}
