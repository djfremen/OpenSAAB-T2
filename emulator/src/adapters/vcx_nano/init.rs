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
    // HS-CAN (channel 0) carries ~970 fps of 0x0C1..0x381 broadcast frames on
    // 2008 ME9.6 vehicles, exceeding the Nano's ~828 fps internal 230.4 kbaud
    // UART ceiling and overflowing its 64-frame ring buffer. All HS-CAN UUDT/USDT
    // diagnostic replies (0x540..0x7FF) and 0x410 status traffic have bit 10 set
    // (0x400..0x7FF), so pattern=0x0000_0400 / mask=0x0000_0400 filters out
    // 0x000..0x3FF in hardware while keeping normal CAN mode (ACK active).
    frames.push(control(
        channel,
        0x48,
        if channel == 0 {
            &[1, 0, 1, 1, 4, 0, 0, 4, 0, 0, 0, 4, 0]
        } else {
            &[1, 0, 1, 1, 4, 0, 0, 0, 0, 0, 0, 0, 0]
        },
    ));
    Ok(frames)
}
pub fn shutdown(channel: u8) -> [Frame; 3] {
    [
        // Delete the installed slot, rather than submit a zero-width PASS
        // record. OpenVCX filters_replay encodes count/slot/type/flag/size
        // as 1/0/0/0/0 for the single slot installed by setup(). A nonzero
        // reply still fails cleanup; this record does not qualify status 96.
        control(channel, 0x48, &[1, 0, 0, 0, 0]),
        control(channel, 0x43, &[]),
        control(channel, 0x41, &[]),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn setup_installs_high_id_pass_filter_on_hs_can_and_pass_all_on_sw_can() {
        let hs = setup(0).unwrap();
        assert_eq!(
            hs.last().unwrap(),
            &control(0, 0x48, &[1, 0, 1, 1, 4, 0, 0, 4, 0, 0, 0, 4, 0])
        );
        let sw = setup(1).unwrap();
        assert_eq!(
            sw.last().unwrap(),
            &control(1, 0x48, &[1, 0, 1, 1, 4, 0, 0, 0, 0, 0, 0, 0, 0])
        );
    }
    #[test]
    fn shutdown_deletes_only_the_installed_slot_then_stops_and_closes() {
        for channel in [0, 1] {
            let frames = shutdown(channel);
            assert_eq!(frames[0], control(channel, 0x48, &[1, 0, 0, 0, 0]));
            assert_eq!(frames[1], control(channel, 0x43, &[]));
            assert_eq!(frames[2], control(channel, 0x41, &[]));
            assert_ne!(frames[0].payload, [1, 0, 1, 1, 0]);
        }
    }
}
