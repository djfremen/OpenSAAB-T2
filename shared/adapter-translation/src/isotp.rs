// SPDX-License-Identifier: MPL-2.0
//! Classical CAN, normal addressing, bounded receive assembly. CAN FD/extended addressing unsupported.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    FirstFrame,
    ConsecutiveFrame,
    Complete(Vec<u8>),
}
pub struct Receiver {
    limit: usize,
    length: Option<usize>,
    bytes: Vec<u8>,
    next: u8,
    failed: bool,
    complete: bool,
}
impl Receiver {
    pub fn new(limit: usize) -> Result<Self, String> {
        if !(1..=4095).contains(&limit) {
            return Err("ISO-TP limit must be 1..4095".into());
        }
        Ok(Self {
            limit,
            length: None,
            bytes: Vec::new(),
            next: 1,
            failed: false,
            complete: false,
        })
    }
    pub fn started(&self) -> bool {
        self.length.is_some()
    }
    pub fn push(&mut self, data: &[u8]) -> Result<Step, String> {
        if self.failed || self.complete {
            return Err("ISO-TP receiver is terminal".into());
        }
        let result = self.consume(data);
        if result.is_err() {
            self.failed = true;
            self.bytes.clear();
        }
        result
    }
    fn consume(&mut self, data: &[u8]) -> Result<Step, String> {
        if data.is_empty() || data.len() > 8 {
            return Err("Invalid ISO-TP CAN size".into());
        }
        match data[0] >> 4 {
            0 if !self.started() => {
                let length = (data[0] & 15) as usize;
                if length == 0 || length > 7 || length > self.limit || data.len() < length + 1 {
                    return Err("Invalid ISO-TP single-frame length".into());
                }
                self.length = Some(length);
                self.complete = true;
                Ok(Step::Complete(data[1..1 + length].to_vec()))
            }
            1 if !self.started() => {
                if data.len() != 8 {
                    return Err("Truncated ISO-TP first frame".into());
                }
                let length = (((data[0] & 15) as usize) << 8) | data[1] as usize;
                if length < 8 || length > self.limit {
                    return Err("Invalid ISO-TP multi-frame length".into());
                }
                self.length = Some(length);
                self.bytes.extend_from_slice(&data[2..]);
                Ok(Step::FirstFrame)
            }
            2 if self.started() && data[0] & 15 == self.next => {
                let remaining = self.length.unwrap() - self.bytes.len();
                let count = remaining.min(7);
                if data.len() < count + 1 {
                    return Err("Truncated ISO-TP consecutive frame".into());
                }
                self.bytes.extend_from_slice(&data[1..1 + count]);
                self.next = (self.next + 1) & 15;
                if self.bytes.len() == self.length.unwrap() {
                    self.complete = true;
                    Ok(Step::Complete(std::mem::take(&mut self.bytes)))
                } else {
                    Ok(Step::ConsecutiveFrame)
                }
            }
            _ => Err("Unexpected, duplicate or out-of-sequence ISO-TP frame".into()),
        }
    }
}

/// Compatibility helper for the existing bounded 1A90 host probes.
pub struct VinReply {
    receiver: Receiver,
    vin: Option<String>,
    failed: bool,
}
impl Default for VinReply {
    fn default() -> Self {
        Self {
            receiver: Receiver::new(19).unwrap(),
            vin: None,
            failed: false,
        }
    }
}
impl VinReply {
    /// Returns true only when the caller must send flow control. A service-1A
    /// response-pending reply waits for the first frame within the caller's
    /// existing absolute deadline; it never starts a transfer or requests FC.
    pub fn feed(&mut self, data: &[u8]) -> Result<bool, String> {
        if self.failed || self.vin.is_some() {
            return Err("VIN receiver is terminal".into());
        }
        let result = self.consume(data);
        if result.is_err() {
            self.failed = true;
            self.vin = None;
        }
        result
    }
    fn consume(&mut self, data: &[u8]) -> Result<bool, String> {
        if data.len() != 8 {
            return Err("VIN CAN response must contain eight bytes".into());
        }
        if data[..3] == [3, 0x7f, 0x1a] {
            if data[3] == 0x78 && !self.receiver.started() {
                return Ok(false);
            }
            return Err(format!("ECU rejected VIN request: NRC {:02X}", data[3]));
        }
        if !self.receiver.started() && data[..4] != [0x10, 19, 0x5a, 0x90] {
            return Err("Unexpected VIN first frame".into());
        }
        match self.receiver.push(data)? {
            Step::FirstFrame => Ok(true),
            Step::ConsecutiveFrame => Ok(false),
            Step::Complete(bytes) => {
                if bytes.len() != 19 || bytes[..2] != [0x5a, 0x90] {
                    return Err("Invalid VIN payload".into());
                }
                let vin = &bytes[2..];
                if !vin
                    .iter()
                    .all(|b| b.is_ascii_digit() || b.is_ascii_uppercase() && !b"IOQ".contains(b))
                {
                    return Err("Invalid VIN characters".into());
                }
                self.vin = Some(String::from_utf8(vin.to_vec()).map_err(|e| e.to_string())?);
                Ok(false)
            }
        }
    }
    pub fn vin(&self) -> Result<Option<String>, String> {
        if self.failed {
            return Err("VIN receiver failed".into());
        }
        Ok(self.vin.clone())
    }
}
