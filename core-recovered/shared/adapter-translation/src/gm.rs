// SPDX-License-Identifier: MPL-2.0
//! Independently authored GM A9/81 collector, consolidated from OpenSAAB's own decoder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dtc {
    pub code: String,
    pub failure_type: u8,
    pub status: u8,
}
pub struct Report {
    records: Vec<Dtc>,
    complete: bool,
    failed: bool,
    limit: usize,
}
impl Default for Report {
    fn default() -> Self {
        Self::new(128).unwrap()
    }
}
impl Report {
    pub fn new(limit: usize) -> Result<Self, String> {
        if !(1..=2048).contains(&limit) {
            return Err("GM record limit must be 1..2048".into());
        }
        Ok(Self {
            records: Vec::new(),
            complete: false,
            failed: false,
            limit,
        })
    }
    pub fn is_complete(&self) -> bool {
        self.complete && !self.failed
    }
    pub fn receive(&mut self, id: u32, data: &[u8]) -> Result<(), String> {
        self.receive_for_ids(id, data, 0x7e8, 0x5e8)
    }
    pub fn receive_for_ids(
        &mut self,
        id: u32,
        data: &[u8],
        reply: u32,
        report: u32,
    ) -> Result<(), String> {
        if self.failed || self.complete {
            return Err("Traffic supplied after end or failure of DTC report".into());
        }
        let result = self.consume(id, data, reply, report);
        if result.is_err() {
            self.failed = true;
        }
        result
    }
    fn consume(&mut self, id: u32, data: &[u8], reply: u32, report: u32) -> Result<(), String> {
        if id == reply && data.starts_with(&[3, 0x7f, 0xa9]) {
            let nrc = *data.get(3).ok_or("Truncated DTC negative response")?;
            return if nrc == 0x78 {
                Ok(())
            } else {
                Err(format!("DTC request rejected: NRC {nrc:02X}"))
            };
        }
        if id != report || data.first() != Some(&0x81) {
            return Ok(());
        }
        if !(5..=8).contains(&data.len()) {
            return Err("Invalid DTC record length".into());
        }
        if data[1..4] == [0, 0, 0] {
            let valid_end = if report == 0x541 {
                matches!(data[4], 0xff | 0x17)
            } else {
                data[4] == 0xff
            };
            if !valid_end {
                return Err("Invalid DTC report end marker".into());
            }
            self.complete = true;
            return Ok(());
        }
        if self.records.len() >= self.limit {
            return Err(format!("DTC report exceeds {} records", self.limit));
        }
        let a = data[1];
        self.records.push(Dtc {
            code: format!(
                "{}{}{:X}{:02X}",
                b"PCBU"[(a >> 6) as usize] as char,
                (a >> 4) & 3,
                a & 15,
                data[2]
            ),
            failure_type: data[3],
            status: data[4],
        });
        Ok(())
    }
    pub fn finish(self) -> Result<Vec<Dtc>, String> {
        if self.failed || !self.complete {
            return Err("Incomplete DTC report: failure or no end marker".into());
        }
        Ok(self.records)
    }
}
