// SPDX-License-Identifier: MPL-2.0
//! Nano-only channel initialization and cleanup. No independent cold initializer
//! is qualified yet; the October 4 read evidence followed Windows conditioning.
use super::protocol::{control, Frame};

/// Captured OEM raw-CAN startup. These are normal channels, not silent listen-only.
pub fn setup(channel: u8) -> Result<Vec<Frame>, String> {
    let hs = [0, 3, 0, 7, 0xa1, 0x20];
    let sw = [0, 3, 0, 0, 0x82, 0x35];
    let params: &[u8] = match channel {
        0 => &[0, 3, 0, 7, 0xa1, 0x20, 0, 2, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0],
        1 => &[0, 3, 0, 0, 0x82, 0x35, 0, 2, 1, 0, 0, 0, 1, 0, 0, 0, 0, 2],
        _ => return Err("Unsupported Nano raw channel".into()),
    };
    let mut frames = vec![
        control(channel, 0x40, &[0, 0, 0x81, 1]),
        control(channel, 0x45, if channel == 0 { &hs } else { &sw }),
        control(channel, 0x47, &[1]),
    ];
    if channel == 1 {
        frames.push(control(channel, 0x43, &[]));
        frames.push(control(channel, 0x45, params));
    }
    frames.push(control(channel, 0x42, &[0]));
    frames.push(control(channel, 0x45, params));
    frames.push(control(
        channel,
        0x48,
        &[1, 0, 1, 1, 4, 0, 0, 0, 0, 0, 0, 0, 0],
    ));
    Ok(frames)
}
pub fn shutdown(channel: u8) -> [Frame; 3] {
    [
        control(channel, 0x48, &[1, 0, 1, 1, 0]),
        control(channel, 0x43, &[]),
        control(channel, 0x41, &[]),
    ]
}
