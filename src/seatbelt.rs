// SPDX-License-Identifier: MPL-2.0
//! Scoped original-firmware BCM reminder operation; exact baseline and target bytes.
//! Original firmware owns challenge/key exchange and configuration requests.
use crate::candi_cpu::CanTransmission;
use std::{
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
pub struct Policy {
    desired: u8,
    seed: u16,
    key: u16,
    requested: bool,
    matched: bool,
    key_sent: bool,
    authorized: bool,
    read_pending: bool,
    baseline: bool,
    written: bool,
    pub rejection: Option<(u8, u8)>,
    pub acknowledged: bool,
    pub symbol_only: bool,
    pub desired_readback: bool,
}
impl Policy {
    pub fn new(seed: u16, key: u16) -> Result<Self, String> {
        if seed == 0xffff || key == 0xffff {
            return Err("Unfilled BCM authority".into());
        }
        Ok(Self {
            desired: 0,
            seed,
            key,
            requested: false,
            matched: false,
            key_sent: false,
            authorized: false,
            read_pending: false,
            baseline: false,
            written: false,
            rejection: None,
            acknowledged: false,
            symbol_only: false,
            desired_readback: false,
        })
    }
    pub fn audible(seed: u16, key: u16) -> Result<Self, String> {
        let mut p = Self::new(seed, key)?;
        p.desired = 0xc0;
        Ok(p)
    }
    pub fn load(path: &Path) -> Result<Self, String> {
        Self::load_direction(path, false)
    }
    pub fn load_audible(path: &Path) -> Result<Self, String> {
        Self::load_direction(path, true)
    }
    fn load_direction(path: &Path, audible: bool) -> Result<Self, String> {
        let data = std::fs::read(path).map_err(|e| e.to_string())?;
        if data.len() > 1024 {
            return Err("Oversized BCM authority".into());
        }
        let v: serde_json::Value = serde_json::from_slice(&data).map_err(|e| e.to_string())?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_secs();
        let expires = v["expires"].as_u64().ok_or("Missing authority deadline")?;
        if v["operation"]
            != if audible {
                "bcm-audible"
            } else {
                "bcm-symbol-only"
            }
            || v["vin"]
                != if audible {
                    "YS3FD49YX41000001"
                } else {
                    "YS3FH46U681000002"
                }
            || expires <= now
            || expires > now + 1800
        {
            return Err("Invalid/expired BCM reminder authority".into());
        }
        let seed =
            u16::try_from(v["seed"].as_u64().ok_or("Missing seed")?).map_err(|_| "Invalid seed")?;
        let key =
            u16::try_from(v["key"].as_u64().ok_or("Missing key")?).map_err(|_| "Invalid key")?;
        if audible {
            use std::io::{Read, Seek, SeekFrom};
            let mut card = std::fs::File::open(
                path.parent()
                    .ok_or("Missing authority directory")?
                    .join("firmware/card-authorized.bin"),
            )
            .map_err(|e| e.to_string())?;
            if card.metadata().map_err(|e| e.to_string())?.len() != 33554432 {
                return Err("Invalid processed card size".into());
            }
            let mut ssa = [0u8; 714];
            card.seek(SeekFrom::Start(0xfe0000))
                .map_err(|e| e.to_string())?;
            card.read_exact(&mut ssa).map_err(|e| e.to_string())?;
            let at = v["ssa_offset"]
                .as_u64()
                .ok_or("Missing BCM SSA tuple offset")? as usize;
            if ssa[..2] != [0xb1, 0]
                || ssa[0x14..0x25] != *b"YS3FD49YX41000001"
                || !(0x132..=706).contains(&at)
                || (at - 0x132) % 8 != 0
                || ssa[at + 2] != 3
                || u16::from_be_bytes([ssa[at + 4], ssa[at + 5]]) != seed
                || u16::from_be_bytes([ssa[at + 6], ssa[at + 7]]) != key
            {
                return Err("Processed card does not match current VIN and BCM authority".into());
            }
            Self::audible(seed, key)
        } else {
            Self::new(seed, key)
        }
    }
    fn bcm(controller: usize, tx: &CanTransmission) -> bool {
        controller == 2
            && tx.frame.id == 0x242
            && crate::can_adapter::electrical_route(controller, tx).is_ok_and(|r| r.j2534_flags == 0)
    }
    pub fn allows(&self, controller: usize, tx: &CanTransmission) -> bool {
        if self.rejection.is_some() || !Self::bcm(controller, tx) {
            return false;
        }
        let d = tx.frame.data.as_slice();
        if d == [2, 0x27, 1, 0, 0, 0, 0, 0] {
            return !self.key_sent;
        }
        if d == [4, 0x27, 2, (self.key >> 8) as u8, self.key as u8, 0, 0, 0] {
            return self.matched && !self.key_sent;
        }
        d == [4, 0x3b, 1, 0x99, self.desired, 0, 0, 0]
            && self.authorized
            && self.baseline
            && !self.written
    }
    pub fn before(&mut self, controller: usize, tx: &CanTransmission) -> Result<(), String> {
        if !Self::bcm(controller, tx) {
            return Ok(());
        }
        let d = tx.frame.data.as_slice();
        if d.len() >= 3 && (d[1] == 0x27 || d[1] == 0x3b) {
            if !self.allows(controller, tx) {
                return Err("BCM reminder policy rejected request".into());
            }
            if d[1] == 0x3b {
                self.written = true;
            } else if d[2] == 1 {
                self.requested = true;
            } else {
                self.key_sent = true;
            }
        }
        if d == [2, 0x1a, 1, 0, 0, 0, 0, 0] {
            self.read_pending = true;
        }
        Ok(())
    }
    pub fn observe(&mut self, controller: usize, id: u32, d: &[u8]) -> Result<(), String> {
        if controller != 2 || id != 0x642 || d.len() < 3 {
            return Ok(());
        }
        match (d[0], d[1], d[2]) {
            (4, 0x67, 1) if d.len() >= 5 => {
                if !self.requested || u16::from_be_bytes([d[3], d[4]]) != self.seed {
                    return Err(
                        "Fresh BCM seed differs from processed SSA; recollect before authorization"
                            .into(),
                    );
                }
                self.matched = true;
            }
            (2, 0x67, 2) => {
                if !self.key_sent {
                    return Err("Unsolicited BCM authorization".into());
                }
                self.authorized = true;
            }
            (4, 0x5a, 1) if d.len() >= 5 && self.read_pending => {
                self.read_pending = false;
                self.baseline = self.authorized && d[3..5] == [0x99, self.desired ^ 0xc0];
                self.symbol_only = d[3..5] == [0x99, 0];
                self.desired_readback = d[3..5] == [0x99, self.desired];
            }
            (2, 0x7b, 1) => {
                if !self.written {
                    return Err("Unsolicited BCM write acknowledgment".into());
                }
                self.acknowledged = true;
            }
            (3, 0x7f, service)
                if d.len() >= 4 && d[3] != 0x78 && (service == 0x27 || service == 0x3b) =>
            {
                // A real ECU rejection belongs to the guest. Latch the write guard
                // closed, but let the transport deliver this unchanged response.
                self.rejection = Some((service, d[3]));
                self.authorized = false;
                self.baseline = false;
            }
            _ => {}
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::candi_cpu::{CanElectricalState, CanFrame};
    fn tx(d: &[u8]) -> CanTransmission {
        CanTransmission {
            ticket: 1,
            frame: CanFrame {
                id: 0x242,
                extended: false,
                rtr: false,
                dlc: d.len() as u8,
                data: d.to_vec(),
            },
            btr0: 0xdd,
            btr1: 0x36,
            electrical: CanElectricalState::SingleWireGpio {
                latch: 3,
                assignment: 0,
                direction: 3,
            },
        }
    }
    #[test]
    fn audible_authority_must_match_processed_card_and_vehicle() {
        use std::io::{Seek, SeekFrom, Write};
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
        let dir = std::env::temp_dir().join(format!(
            "tech2-authority-{}-{}",
            std::process::id(),
            now.as_nanos()
        ));
        std::fs::create_dir_all(dir.join("firmware")).unwrap();
        let mut card = std::fs::File::create(dir.join("firmware/card-authorized.bin")).unwrap();
        card.set_len(33554432).unwrap();
        let mut ssa = [0xff; 714];
        ssa[..2].copy_from_slice(&[0xb1, 0]);
        ssa[0x14..0x25].copy_from_slice(b"YS3FD49YX41000001");
        ssa[0x132..0x13a].copy_from_slice(&[0, 1, 3, 1, 0x12, 0x34, 0x56, 0x78]);
        card.seek(SeekFrom::Start(0xfe0000)).unwrap();
        card.write_all(&ssa).unwrap();
        drop(card);
        let valid = serde_json::json!({"operation":"bcm-audible","vin":"YS3FD49YX41000001","expires":now.as_secs()+600,"ssa_offset":0x132,"seed":0x1234,"key":0x5678});
        let path = dir.join("authority.json");
        std::fs::write(&path, valid.to_string()).unwrap();
        assert!(Policy::load_audible(&path).is_ok());
        assert!(Policy::load(&path).is_err());
        for (field, value) in [
            ("vin", serde_json::json!("YS3FH46U681000002")),
            ("key", serde_json::json!(0x5679)),
            ("seed", serde_json::json!(0x1235)),
            ("ssa_offset", serde_json::json!(0x133)),
            ("expires", serde_json::json!(0)),
        ] {
            let mut bad = valid.clone();
            bad[field] = value;
            std::fs::write(&path, bad.to_string()).unwrap();
            assert!(Policy::load_audible(&path).is_err());
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn audible_requires_fresh_symbol_baseline_and_exact_single_write() {
        let mut p = Policy::audible(0x1234, 0x5678).unwrap();
        let seed = tx(&[2, 0x27, 1, 0, 0, 0, 0, 0]);
        let key = tx(&[4, 0x27, 2, 0x56, 0x78, 0, 0, 0]);
        let read = tx(&[2, 0x1a, 1, 0, 0, 0, 0, 0]);
        let write = tx(&[4, 0x3b, 1, 0x99, 0xc0, 0, 0, 0]);
        assert!(!p.allows(2, &write));
        p.before(2, &seed).unwrap();
        p.observe(2, 0x642, &[4, 0x67, 1, 0x12, 0x34]).unwrap();
        p.before(2, &key).unwrap();
        p.observe(2, 0x642, &[2, 0x67, 2]).unwrap();
        p.before(2, &read).unwrap();
        p.observe(2, 0x642, &[4, 0x5a, 1, 0x98, 0]).unwrap();
        assert!(!p.allows(2, &write));
        p.before(2, &read).unwrap();
        p.observe(2, 0x642, &[4, 0x5a, 1, 0x99, 0]).unwrap();
        assert!(p.allows(2, &write));
        assert!(!p.desired_readback);
        assert!(!p.allows(0, &write));
        assert!(!p.allows(2, &tx(&[4, 0x3b, 1, 0x99, 0, 0, 0, 0])));
        p.before(2, &write).unwrap();
        assert!(!p.allows(2, &write));
        p.observe(2, 0x642, &[2, 0x7b, 1]).unwrap();
        assert!(p.acknowledged);
        assert!(!p.desired_readback);
        p.before(2, &read).unwrap();
        p.observe(2, 0x642, &[4, 0x5a, 1, 0x99, 0xc0]).unwrap();
        assert!(p.desired_readback);
        assert!(!p.symbol_only);
    }

    #[test]
    fn reversal_requires_fresh_authorization_baseline_and_readback() {
        let mut p = Policy::new(0x1234, 0x5678).unwrap();
        let seed = tx(&[2, 0x27, 1, 0, 0, 0, 0, 0]);
        let key = tx(&[4, 0x27, 2, 0x56, 0x78, 0, 0, 0]);
        let read = tx(&[2, 0x1a, 1, 0, 0, 0, 0, 0]);
        let write = tx(&[4, 0x3b, 1, 0x99, 0, 0, 0, 0]);
        assert!(!p.allows(2, &key));
        assert!(!p.allows(2, &write));
        p.before(2, &seed).unwrap();
        p.observe(2, 0x642, &[4, 0x67, 1, 0x12, 0x34]).unwrap();
        assert!(p.allows(2, &key));
        p.before(2, &key).unwrap();
        assert!(!p.allows(2, &key));
        assert!(!p.allows(2, &write));
        p.observe(2, 0x642, &[2, 0x67, 2]).unwrap();
        p.before(2, &read).unwrap();
        p.observe(2, 0x642, &[4, 0x5a, 1, 0x99, 0xc0]).unwrap();
        assert!(p.allows(2, &write));
        assert!(!p.allows(0, &write));
        assert!(!p.allows(2, &tx(&[4, 0x3b, 1, 0x99, 0xc0, 0, 0, 0])));
        p.before(2, &write).unwrap();
        assert!(!p.allows(2, &write));
        assert!(!p.acknowledged);
        p.observe(2, 0x642, &[2, 0x7b, 1]).unwrap();
        assert!(p.acknowledged);
        assert!(!p.symbol_only);
        p.before(2, &read).unwrap();
        p.observe(2, 0x642, &[4, 0x5a, 1, 0x99, 0]).unwrap();
        assert!(p.symbol_only);
    }
    #[test]
    fn ecu_rejection_reaches_guest_without_reopening_write_authority() {
        let mut p = Policy::new(0x1234, 0x5678).unwrap();
        let seed = tx(&[2, 0x27, 1, 0, 0, 0, 0, 0]);
        let key = tx(&[4, 0x27, 2, 0x56, 0x78, 0, 0, 0]);
        let read = tx(&[2, 0x1a, 1, 0, 0, 0, 0, 0]);
        let write = tx(&[4, 0x3b, 1, 0x99, 0, 0, 0, 0]);
        p.before(2, &seed).unwrap();
        p.observe(2, 0x642, &[4, 0x67, 1, 0x12, 0x34]).unwrap();
        p.before(2, &key).unwrap();
        p.observe(2, 0x642, &[2, 0x67, 2]).unwrap();
        p.before(2, &read).unwrap();
        p.observe(2, 0x642, &[4, 0x5a, 1, 0x99, 0xc0]).unwrap();
        p.before(2, &write).unwrap();
        // Actual Pixel capture: pending followed by request-out-of-range.
        p.observe(2, 0x642, &[3, 0x7f, 0x3b, 0x78, 0, 0, 0, 0])
            .unwrap();
        assert_eq!(p.rejection, None);
        p.observe(2, 0x642, &[3, 0x7f, 0x3b, 0x31, 0, 0, 0, 0])
            .unwrap();
        assert_eq!(p.rejection, Some((0x3b, 0x31)));
        assert!(!p.acknowledged);
        assert!(!p.symbol_only);
        assert!(!p.allows(2, &write));
        assert!(!p.allows(2, &key));
        assert!(!p.allows(2, &seed));
        // A subsequent valid baseline must not reopen the rejected operation.
        p.before(2, &read).unwrap();
        p.observe(2, 0x642, &[4, 0x5a, 1, 0x99, 0xc0]).unwrap();
        assert!(!p.allows(2, &write));
    }
    #[test]
    fn changed_seed_and_unsolicited_authorization_fail() {
        let mut p = Policy::new(1, 2).unwrap();
        assert!(p.observe(2, 0x642, &[2, 0x67, 2]).is_err());
        p.before(2, &tx(&[2, 0x27, 1, 0, 0, 0, 0, 0])).unwrap();
        assert!(p.observe(2, 0x642, &[4, 0x67, 1, 0, 3]).is_err());
    }
}
