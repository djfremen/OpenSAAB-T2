// SPDX-License-Identifier: MPL-2.0
//! Bounded read-only VIN transport helpers shared by USB adapters.
pub use opensaab_vlinker_workflow::translation::isotp::VinReply;

pub use opensaab_vlinker_workflow::vin_probe::saab_tech2_year;

/// Validate a complete assembled Saab 1A90 reply without fabricating raw frames.
pub fn assembled_saab_vin(payload: &[u8]) -> Result<String, String> {
    if payload.len() != 19 || !payload.starts_with(&[0x5a, 0x90])
        || !payload[2..].starts_with(b"YS3")
        || !payload[2..].iter().all(|b| b.is_ascii_digit() || (b.is_ascii_uppercase() && !b"IOQ".contains(b))) {
        return Err("Invalid complete Saab VIN payload".into());
    }
    String::from_utf8(payload[2..].to_vec()).map_err(|_| "Invalid VIN encoding".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn year_is_read_from_the_tenth_character_not_the_vin_suffix() {
        assert_eq!(saab_tech2_year("YS3FD49YX41000001").unwrap(), 2004);
        assert_eq!(saab_tech2_year("YS3FH46U681000002").unwrap(), 2008);
        assert!(saab_tech2_year("2017").is_err());
    }
}
