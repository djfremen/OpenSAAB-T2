// SPDX-License-Identifier: MPL-2.0
//! CAN controller register intercept surface for a future CANDi firmware CPU.
//!
//! When firmware is loaded, MMIO reads/writes to the CAN chip window should go
//! through [`CanChipBackend`] so a goCAN (or recording) backend can observe them.
//! This file does not invent a silicon map; offsets are opaque until RE lands.

use super::gocan_bridge::{CanFrame, GoCanBridge, GoCanError};

/// One logged register access. Never treated as vehicle TX by itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MmioAccess {
    pub write: bool,
    pub offset: u32,
    pub value: u8,
}

/// Backend behind CAN-chip MMIO. Implementations must not claim ECU success.
pub trait CanChipBackend {
    fn read8(&mut self, offset: u32) -> u8;
    fn write8(&mut self, offset: u32, value: u8);
    fn drain_log(&mut self) -> Vec<MmioAccess>;
}

/// Records every access and keeps a flat byte window. No side effects on a bus.
#[derive(Debug)]
pub struct RecordingCanChip {
    window: [u8; 256],
    log: Vec<MmioAccess>,
}

impl RecordingCanChip {
    pub fn new() -> Self {
        Self {
            window: [0; 256],
            log: Vec::new(),
        }
    }
}

impl Default for RecordingCanChip {
    fn default() -> Self {
        Self::new()
    }
}

impl CanChipBackend for RecordingCanChip {
    fn read8(&mut self, offset: u32) -> u8 {
        let idx = (offset as usize) % self.window.len();
        let value = self.window[idx];
        self.log.push(MmioAccess {
            write: false,
            offset,
            value,
        });
        value
    }

    fn write8(&mut self, offset: u32, value: u8) {
        let idx = (offset as usize) % self.window.len();
        self.window[idx] = value;
        self.log.push(MmioAccess {
            write: true,
            offset,
            value,
        });
    }

    fn drain_log(&mut self) -> Vec<MmioAccess> {
        std::mem::take(&mut self.log)
    }
}

/// Optional bridge: when a backend wants to forward a composed frame, it uses
/// [`GoCanBridge`]. The recording chip never auto-sends; callers decide.
pub fn forward_frame(bridge: &mut dyn GoCanBridge, frame: CanFrame) -> Result<(), GoCanError> {
    bridge.send(frame)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::candi::gocan_bridge::UnimplementedGoCan;

    #[test]
    fn recording_chip_logs_reads_and_writes() {
        let mut chip = RecordingCanChip::new();
        chip.write8(0x10, 0x5a);
        assert_eq!(chip.read8(0x10), 0x5a);
        let log = chip.drain_log();
        assert_eq!(
            log,
            vec![
                MmioAccess {
                    write: true,
                    offset: 0x10,
                    value: 0x5a
                },
                MmioAccess {
                    write: false,
                    offset: 0x10,
                    value: 0x5a
                },
            ]
        );
    }

    #[test]
    fn forward_through_unimplemented_gocan_is_explicit_error() {
        let mut bridge = UnimplementedGoCan;
        let err = forward_frame(
            &mut bridge,
            CanFrame {
                id: 0x24f,
                data: vec![0x01],
            },
        )
        .unwrap_err();
        assert!(err.to_string().contains("unimplemented"));
    }
}
