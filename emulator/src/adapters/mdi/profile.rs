// SPDX-License-Identifier: MPL-2.0
//! Exact classic-MDI profile inventoried on the EliteBook; MDI2 is separate.
pub const DRIVER: &str =
    r"C:\Program Files (x86)\GM MDI Software\Products\MDI\Dynamic Link Libraries\BVTX4J32.dll";
pub const RUNTIME: &str =
    r"C:\Program Files (x86)\GM MDI Software\Dynamic Link Libraries\BVTX-VCI-RT.dll";
pub const PDU: &str =
    r"C:\Program Files (x86)\GM MDI Software\Products\MDI\Dynamic Link Libraries\BVTX-VCI-PDU.dll";
pub const DRIVER_SHA: &str = "39a75770dfceb65fc7cb5a7201ccaa9fc0eed0c781e465398b594598dd131191";
pub const RUNTIME_SHA: &str = "7dbd55017da86e8ffc861d7dcb409a70f983c3eb44b478e648178e1fffbfcde0";
pub const PDU_SHA: &str = "f128f09b6731d31a0fe7ea72dde6b79b9686c3a69fa646c73f890be8a94891b3";
pub const REGISTRY: &str = r"SOFTWARE\PassThruSupport.04.04\Bosch - MDI";
// KEY_READ and exactly KEY_WOW64_32KEY. Combining both WOW64 view bits is invalid.
pub const REGISTRY_ACCESS: u32 = 0x20019 | 0x200;
pub const PROTOCOL: u32 = 6;
pub const BAUD: u32 = 500_000;
pub const REQUEST_ID: u32 = 0x7e0;
pub const REPLY_ID: u32 = 0x7e8;
pub const HELPER: &str = "mdi-driver-worker.exe";

pub fn validate_image(bytes: &[u8], expected: &str) -> Result<(), String> {
    use sha2::{Digest, Sha256};
    if bytes.len() < 64 || &bytes[..2] != b"MZ" {
        return Err("Invalid MDI Windows image".into());
    }
    let pe = u32::from_le_bytes(bytes[60..64].try_into().unwrap()) as usize;
    if pe < 64
        || pe.checked_add(6).is_none_or(|end| end > bytes.len())
        || &bytes[pe..pe + 4] != b"PE\0\0"
        || &bytes[pe + 4..pe + 6] != [0x4c, 0x01]
    {
        return Err("Classic MDI requires an x86 image".into());
    }
    if format!("{:x}", Sha256::digest(bytes)) != expected {
        return Err("MDI vendor image changed; review the driver profile before use".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    #[test]
    fn registry_lookup_uses_only_the_x86_view() {
        assert_eq!(REGISTRY_ACCESS & 0x300, 0x200);
        assert_eq!(REGISTRY_ACCESS & !0x300, 0x20019);
    }
    #[test]
    fn rejects_wrong_architecture_corruption_and_changed_driver() {
        let mut image = vec![0u8; 80];
        image[..2].copy_from_slice(b"MZ");
        image[60..64].copy_from_slice(&64u32.to_le_bytes());
        image[64..70].copy_from_slice(b"PE\0\0\x4c\x01");
        let pin = format!("{:x}", Sha256::digest(&image));
        assert!(validate_image(&image, &pin).is_ok());
        image[68..70].copy_from_slice(&[0x64, 0x86]);
        assert!(validate_image(&image, &pin).is_err());
        image[68..70].copy_from_slice(&[0x4c, 0x01]);
        image[79] = 1;
        assert!(validate_image(&image, &pin).is_err());
        image[60..64].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(validate_image(&image, &pin).is_err());
    }
}
