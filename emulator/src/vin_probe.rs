// SPDX-License-Identifier: MPL-2.0
//! Bounded read-only VIN transport helpers shared by USB adapters.
pub use opensaab_vlinker_workflow::translation::isotp::VinReply;

pub use opensaab_vlinker_workflow::vin_probe::saab_tech2_year;

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
