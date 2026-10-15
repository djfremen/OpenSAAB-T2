// SPDX-License-Identifier: MPL-2.0
/// Captured 9440 functional tester-present: extended address FE, one-byte
/// service 3E, unchanged CAN padding. Opt-in host startup experiment only.
/// This is diagnostic keep-alive, not ECU security access or programming.
pub const SAAB_9440_TESTER_PRESENT: [u8; 8] = [0xfe, 1, 0x3e, 0, 0, 0, 0, 0];
pub const SAAB_9440_VIN_PROFILE: &str = "saab9440-hs-tester-present-1";

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
