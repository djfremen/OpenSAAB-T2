// SPDX-License-Identifier: MPL-2.0
//! Strict decoding for a small, explicit set of standard Mode 01 ECM values.
//! Scaling references: python-OBD command tables and decoder documentation.
use serde_json::{json, Value};
pub const PIDS: [u8; 8] = [0x04, 0x05, 0x0b, 0x0c, 0x0d, 0x0f, 0x10, 0x11];
pub fn supported(mask: u32, pid: u8) -> bool {
    (1..=32).contains(&pid) && mask & (1u32 << (32 - pid)) != 0
}
pub fn decode(pid: u8, data: &[u8]) -> Result<Value, String> {
    let width = if matches!(pid, 0x0c | 0x10) { 2 } else { 1 };
    if !PIDS.contains(&pid) || data.len() != width {
        return Err("Unsupported current-data PID or exact payload width".into());
    }
    let a = data[0] as f64;
    let pair = || u16::from_be_bytes([data[0], data[1]]) as f64;
    let (name, unit, value) = match pid {
        0x04 => ("calculated_engine_load", "%", a * 100.0 / 255.0),
        0x05 => ("coolant_temperature", "degC", a - 40.0),
        0x0b => ("intake_manifold_pressure", "kPa", a),
        0x0c => ("engine_speed", "rpm", pair() / 4.0),
        0x0d => ("vehicle_speed", "km/h", a),
        0x0f => ("intake_air_temperature", "degC", a - 40.0),
        0x10 => ("mass_air_flow", "g/s", pair() / 100.0),
        0x11 => ("absolute_throttle_position", "%", a * 100.0 / 255.0),
        _ => unreachable!(),
    };
    Ok(
        json!({"pid":pid,"name":name,"unit":unit,"value":value,"raw_data_hex":crate::trace::hex(data)}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn support_mask_bit_order_and_boundaries() {
        assert!(supported(0x80000001, 1));
        assert!(supported(0x80000001, 32));
        assert!(!supported(0x80000001, 2));
        assert!(!supported(u32::MAX, 0));
        assert!(!supported(u32::MAX, 33));
    }
    #[test]
    fn temperatures_rpm_and_flow_scaling() {
        assert_eq!(decode(5, &[68]).unwrap()["value"], 28.0);
        assert_eq!(decode(15, &[0]).unwrap()["value"], -40.0);
        assert_eq!(decode(12, &[0x1f, 0x40]).unwrap()["value"], 2000.0);
        assert_eq!(decode(16, &[1, 0]).unwrap()["value"], 2.56);
        assert_eq!(decode(17, &[255]).unwrap()["value"], 100.0);
    }
    #[test]
    fn malformed_or_unselected_values_never_decode() {
        assert!(decode(12, &[1]).is_err());
        assert!(decode(5, &[1, 2]).is_err());
        assert!(decode(0x42, &[1, 2]).is_err());
    }
}
