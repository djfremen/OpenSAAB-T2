// SPDX-License-Identifier: MPL-2.0
//! Observed startup reference, not a qualified live initialization recipe.
//! Echo, start-firmware, Open, board-info and Close are corroborated in the DLL.
//! Remaining query parameters and all physical behavior need separate qualification.
use super::protocol::Record;

pub const VID: u16 = 0x18e1;
pub const PID: u16 = 0x0107;
pub const BULK_OUT: u8 = 0x01;
pub const BULK_IN: u8 = 0x82;
pub const INTERRUPT_IN: u8 = 0x81;
/// Observed vendor control request: bmRequestType=0x40, request=0xDB,
/// wValue=1/0, wIndex=0, no data. Its internal effect is unqualified.
pub const VENDOR_CONTROL_REQUEST: u8 = 0xdb;

pub const STARTUP_OPCODES: [u16; 11] = [
    0x100, 0x109, 0x0c, 0x111, 0x103, 3, 0x109, 0x0c, 0x0c, 0x13, 0x13,
];

/// Normalizes four opaque auxiliary bytes to zero; captured stack-like values
/// are never copied into an initializer. Physical replay has not been tested.
pub fn startup_reference() -> Vec<Record> {
    STARTUP_OPCODES
        .iter()
        .enumerate()
        .map(|(i, &opcode)| {
            let body = match i {
                2 => 0x2fu32.to_le_bytes().to_vec(),
                7 => 0x2bu32.to_le_bytes().to_vec(),
                8 => 0x2au32.to_le_bytes().to_vec(),
                9 | 10 => 0u32.to_le_bytes().to_vec(),
                _ => vec![],
            };
            Record {
                destination: 1,
                source: 0,
                opcode,
                sequence: (i + 1) as u16,
                auxiliary: [0; 4],
                body,
            }
        })
        .collect()
}

/// Captured channel parameters. Pin assignments are corroborated by op0x12.
/// These describe the native reference; no raw guest capability is implied.
pub fn observed_channel_open(destination: u16, sequence: u16) -> Result<Record, String> {
    let baud: u32 = match destination {
        0x1801 | 0x1701 => 33_333,
        0x0601 => 500_000,
        _ => return Err("Channel absent from Mongoose capture reference".into()),
    };
    Ok(Record {
        destination,
        source: 0,
        opcode: 6,
        sequence,
        auxiliary: [0; 4],
        body: [0x800u32.to_le_bytes(), baud.to_le_bytes()].concat(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn startup_and_channel_parameters_match_reference_order() {
        let r = startup_reference();
        assert_eq!(
            r.iter().map(|r| r.opcode).collect::<Vec<_>>(),
            STARTUP_OPCODES
        );
        assert_eq!(r[2].body, [0x2f, 0, 0, 0]);
        assert_eq!(r[7].body, [0x2b, 0, 0, 0]);
        assert_eq!(r[8].body, [0x2a, 0, 0, 0]);
        assert_eq!(
            observed_channel_open(0x1801, 12).unwrap().body,
            [0, 8, 0, 0, 0x35, 0x82, 0, 0]
        );
        assert_eq!(
            observed_channel_open(0x601, 59).unwrap().body,
            [0, 8, 0, 0, 0x20, 0xa1, 7, 0]
        );
        assert!(observed_channel_open(0x501, 1).is_err());
    }
}
