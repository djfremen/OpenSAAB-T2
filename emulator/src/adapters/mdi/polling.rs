// SPDX-License-Identifier: MPL-2.0
//! Capture-scoped body polling treatment for the private MDI discriminator.
//! The default build retains the shared Read gate. No other adapter changes.
use crate::{can_adapter::{electrical_route, ElectricalMode}, candi_cpu::CanTransmission};

pub const ENABLED: bool = cfg!(feature = "mdi-body-read-experiment");
pub const IDS: [u32; 4] = [0x242, 0x24f, 0x241, 0x246];
pub const STOP: [u8; 8] = [2, 0xaa, 0, 0, 0, 0, 0, 0];
const STARTS: [[u8; 8]; 4] = [
    [3, 0xaa, 3, 3, 0, 0, 0, 0],
    [5, 0xaa, 3, 4, 5, 10, 0, 0],
    [4, 0xaa, 3, 2, 1, 0, 0, 0],
    [3, 0xaa, 3, 4, 0, 0, 0, 0],
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action { Start(usize), Stop(usize) }

/// Validate the complete captured raw frame, including PCI and padding.
pub fn classify(controller: usize, tx: &CanTransmission) -> Option<Action> {
    if controller != 2 || tx.frame.extended || tx.frame.rtr || tx.frame.dlc != 8
        || electrical_route(controller, tx).ok()?.mode != ElectricalMode::Data {
        return None;
    }
    let index = IDS.iter().position(|id| *id == tx.frame.id)?;
    if tx.frame.data.as_slice() == STARTS[index] {
        Some(Action::Start(index))
    } else if tx.frame.data.as_slice() == STOP {
        Some(Action::Stop(index))
    } else { None }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::mdi::raw::{hex, parse_tx};
    #[test]
    fn captured_start_stop_pairs_and_every_byte_mutation_are_bounded() {
        for (index, id) in IDS.iter().enumerate() {
            for (payload, action) in [(STARTS[index], Action::Start(index)), (STOP, Action::Stop(index))] {
                let (_, _, message, tx) = parse_tx(&format!("TX 2 1 0000 {id:03X} {}", hex(&payload))).unwrap();
                assert_eq!(classify(2, &tx), Some(action));
                assert_eq!(&message.data[4..12], &payload);
                assert_eq!(classify(0, &tx), None);
                for offset in 0..8 {
                    let mut bad = tx.clone(); bad.frame.data[offset] ^= 0x80;
                    assert_eq!(classify(2, &bad), None);
                }
                let mut bad = tx.clone(); bad.frame.id = 0x243;
                assert_eq!(classify(2, &bad), None);
                bad = tx.clone(); bad.frame.data.pop(); bad.frame.dlc = 7;
                assert_eq!(classify(2, &bad), None);
            }
        }
    }
}
