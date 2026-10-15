// SPDX-License-Identifier: MPL-2.0
//! Bounded read-only VIN transport helpers shared by USB adapters.
#[derive(Default)]
pub struct VinReply {
    bytes: Vec<u8>,
    next: u8,
    failed: bool,
}
impl VinReply {
    /// Return true exactly when a valid first frame needs one flow-control frame.
    pub fn feed(&mut self, data: &[u8]) -> Result<bool, String> {
        if self.failed || self.bytes.len() == 19 {
            return Err("VIN receiver is terminal".into());
        }
        let result = self.consume(data);
        if result.is_err() {
            self.failed = true;
            self.bytes.clear();
        }
        result
    }
    fn consume(&mut self, data: &[u8]) -> Result<bool, String> {
        if data.len() != 8 {
            return Err("VIN CAN response must contain eight bytes".into());
        }
        if data[0] == 3 && data[1..3] == [0x7f, 0x1a] {
            // Service-1A response pending precedes the VIN first frame. The
            // caller keeps its absolute deadline; no flow control is due yet.
            if data[3] == 0x78 && self.bytes.is_empty() {
                return Ok(false);
            }
            return Err(format!("ECU rejected VIN request: NRC {:02X}", data[3]));
        }
        if data[..4] == [0x10, 19, 0x5a, 0x90] && self.bytes.is_empty() {
            self.bytes.extend_from_slice(&data[2..]);
            self.next = 1;
            return Ok(true);
        }
        if data[0] == (0x20 | self.next) && self.next > 0 && self.bytes.len() < 19 {
            let needed = (19 - self.bytes.len()).min(7);
            self.bytes.extend_from_slice(&data[1..1 + needed]);
            self.next += 1;
            return Ok(false);
        }
        Err("Unexpected, duplicate or out-of-sequence VIN frame".into())
    }
    pub fn vin(&self) -> Result<Option<String>, String> {
        if self.failed {
            return Err("VIN receiver failed".into());
        }
        if self.bytes.len() != 19 {
            return Ok(None);
        }
        let vin = &self.bytes[2..];
        if !vin
            .iter()
            .all(|b| b.is_ascii_digit() || (b.is_ascii_uppercase() && !b"IOQ".contains(b)))
        {
            return Err("Invalid VIN characters".into());
        }
        Ok(Some(
            String::from_utf8(vin.to_vec()).map_err(|e| e.to_string())?,
        ))
    }
}

pub fn saab_tech2_year(vin: &str) -> Result<u16, String> {
    if vin.len() != 17 || !vin.starts_with("YS3") {
        return Err("Expected a 17-character Saab VIN".into());
    }
    match vin.as_bytes()[9] {
        b'W' => Ok(1998),
        b'X' => Ok(1999),
        b'Y' => Ok(2000),
        b'1'..=b'9' => Ok(2000 + (vin.as_bytes()[9] - b'0') as u16),
        b'A'..=b'C' => Ok(2010 + (vin.as_bytes()[9] - b'A') as u16),
        _ => Err("VIN year is outside this Saab Tech2 card's supported era".into()),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    const PENDING: [u8; 8] = [3, 0x7f, 0x1a, 0x78, 0, 0, 0, 0];
    const FIRST: [u8; 8] = [0x10, 19, 0x5a, 0x90, b'Y', b'S', b'3', b'F'];
    #[test]
    fn pending_replies_wait_without_vin_or_flow_control_then_assemble() {
        let mut reply = VinReply::default();
        for _ in 0..3 {
            assert!(!reply.feed(&PENDING).unwrap());
            assert_eq!(reply.vin().unwrap(), None);
        }
        assert!(reply.feed(&FIRST).unwrap());
        assert!(!reply.feed(&[0x21, b'H', b'4', b'6', b'U', b'6', b'8', b'1']).unwrap());
        assert!(!reply.feed(&[0x22, b'0', b'0', b'0', b'0', b'0', b'1', 0]).unwrap());
        assert_eq!(reply.vin().unwrap().as_deref(), Some("YS3FH46U681000001"));
        assert!(reply.feed(&PENDING).is_err());
    }
    #[test]
    fn pending_during_segmented_reply_fails_terminally() {
        let mut reply = VinReply::default();
        assert!(reply.feed(&FIRST).unwrap());
        assert!(reply.feed(&PENDING).is_err());
        assert!(reply.vin().is_err());
        assert!(reply.feed(&FIRST).is_err());
    }
    #[test]
    fn wrong_service_nrc_prefix_size_and_order_remain_errors() {
        for bad in [
            vec![3, 0x7f, 0x22, 0x78, 0, 0, 0, 0],
            vec![3, 0x7f, 0x1a, 0x22, 0, 0, 0, 0],
            vec![4, 0x7f, 0x1a, 0x78, 0, 0, 0, 0],
            PENDING[..7].to_vec(),
            vec![0x21, 0, 0, 0, 0, 0, 0, 0],
        ] {
            let mut reply = VinReply::default();
            assert!(reply.feed(&bad).is_err());
            assert!(reply.feed(&PENDING).is_err());
            assert!(reply.feed(&FIRST).is_err());
            assert!(reply.vin().is_err());
        }
    }
    #[test]
    fn year_is_read_from_the_tenth_character_not_the_vin_suffix() {
        assert_eq!(saab_tech2_year("YS3FD49YX41000001").unwrap(), 2004);
        assert_eq!(saab_tech2_year("YS3FH46U681000002").unwrap(), 2008);
        assert!(saab_tech2_year("2017").is_err());
    }
}
