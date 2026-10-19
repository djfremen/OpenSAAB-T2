// SPDX-License-Identifier: MPL-2.0
use crate::candi_cpu::CanTransmission;

// Captured only after original Service > Check Ignition Key Status. This is
// a diagnostic DeviceControl request, not a general read-service allowance.
#[derive(Default)]
pub(crate) struct KeyStatusGate {
    pub(crate) enabled: bool,
    pub(crate) sent: u8,
}
impl KeyStatusGate {
    pub(crate) fn allowed(&mut self, controller: usize, tx: &CanTransmission) -> bool {
        if !self.enabled
            || self.sent >= 3
            || controller != 2
            || tx.frame.id != 0x241
            || tx.frame.data != [3, 0xae, 3, 2, 0, 0, 0, 0]
            || crate::can_adapter::electrical_route(controller, tx).is_err()
        {
            return false;
        }
        self.sent += 1;
        true
    }
}
