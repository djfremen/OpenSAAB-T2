// SPDX-License-Identifier: MPL-2.0
//! Chipsoft-only startup and shutdown recipes; never inherit Nano USB controls.
use super::protocol::{command, words, Frame};

pub const PROTOCOLS: [u32; 2] = [5, 0x8008];
pub fn setup(protocol: u32) -> Result<Vec<Frame>, String> {
    let baud = match protocol {
        5 => 500000,
        0x8008 => 33333,
        _ => return Err("Unsupported raw protocol".into()),
    };
    let mut out = vec![command(4, words(&[protocol, 0, baud]))];
    if protocol == 0x8008 {
        out.push(command(0xb, words(&[protocol, 0x8001, 0x0100])));
    }
    let mut filter = words(&[protocol, 1]);
    for n in 0..3 {
        filter.extend([0; 12]);
        filter.push(if n == 2 { 0 } else { 4 });
        filter.extend(protocol.to_le_bytes());
        filter.extend([0; 4]);
    }
    out.push(command(0x17, filter));
    // J2534 CLEAR_RX_BUFFER. Historical ARM_CHANNEL label was misleading.
    out.push(command(0x12, words(&[protocol])));
    Ok(out)
}
/// Release one opened Chipsoft protocol. Caller still verifies the reply.
pub fn disconnect(protocol: u32) -> Frame {
    command(5, words(&[protocol]))
}

/// Release the Chipsoft device after channel cleanup.
pub fn terminate() -> Frame {
    command(0x20, vec![])
}
