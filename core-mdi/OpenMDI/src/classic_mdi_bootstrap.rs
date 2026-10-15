// SPDX-License-Identifier: MPL-2.0
//! Automatic bootstrap for the observed classic-MDI 2.5.33.154 management protocol.
//! This fixed protocol material is present in the inspected general Windows
//! J2534 and D-PDU runtimes. It is not an owner credential or an ECU security key.
//! No vendor executable or recorded device identity is needed at runtime.
use crate::management;
use serde_json::Value;
use std::net::IpAddr;

const BASE_KEY: [u8; 56] = [
    0x43, 0x29, 0x7f, 0xad, 0x38, 0xe3, 0x73, 0xfe, 0x07, 0xa7, 0x13, 0x70, 0x45, 0xda, 0x2a, 0x16,
    0x04, 0x68, 0x91, 0x04, 0xc2, 0xfd, 0x3b, 0x2f, 0x37, 0xd0, 0x6b, 0xb5, 0x16, 0xcb, 0x75, 0x46,
    0x1f, 0x08, 0x26, 0x0d, 0x1a, 0xc2, 0x46, 0x5e, 0x58, 0x40, 0x23, 0x64, 0x1a, 0xba, 0x61, 0x76,
    0x02, 0x58, 0x16, 0x16, 0x46, 0x29, 0xb0, 0x07,
];

/// Use a freshly validated announcement serial. Inventory validation in the
/// shared management core rejects other PDU versions before registration/START.
pub fn attempt(ip: IpAddr, serial: u32, action: &str, instance: &str) -> Value {
    let mut report = management::attempt(ip, serial, &BASE_KEY, action, instance);
    report["management_bootstrap"] = "built_in_classic_mdi_2.5.33.154".into();
    report["imported_profile_used"] = false.into();
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn automatic_inventory_matches_independent_blowfish_vector() {
        // Synthetic serial; expected bytes computed independently with PyCryptodome.
        let codec = management::Codec::new(&BASE_KEY, 0x12345678).unwrap();
        let encoded = codec.encode(&management::inventory_request()).unwrap();
        assert_eq!(encoded.opcode, 0x882e);
        assert_eq!(
            encoded.body,
            [0xc3, 0xe5, 0x08, 0xa2, 0x13, 0x12, 0xfa, 0x0a]
        );
        let other = management::Codec::new(&BASE_KEY, 0x12345679).unwrap();
        assert_ne!(
            other.encode(&management::inventory_request()).unwrap().body,
            encoded.body
        );
    }
    #[test]
    fn missing_identity_cannot_open_a_management_connection() {
        let report = attempt("127.0.0.1".parse().unwrap(), 0, "start", "test");
        assert_eq!(report["success"], false);
        assert_eq!(report["operations"], serde_json::json!([]));
        assert_eq!(report["start_command_count"], 0);
        assert_eq!(report["vehicle_requests"], 0);
        assert_eq!(report["imported_profile_used"], false);
    }
}
