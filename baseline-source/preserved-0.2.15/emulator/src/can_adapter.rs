// SPDX-License-Identifier: MPL-2.0
//! Platform-neutral native CANdi transport boundary.
//! Completion is a backend/driver indication, not proof of an electrical CAN ACK.
//! Never silently promote USB write success into a hardware-confirmed event.
use crate::candi_cpu::{CanElectricalState, CanFrame, CanTransmission};
use std::collections::VecDeque;
use std::time::Duration;

/// Bounded driver-batch staging. Ordering is preserved within each physical
/// bus; one controller's full FIFO must not stall a different bus.
#[derive(Clone, Default)]
pub struct ReceiveStaging {
    queues: [VecDeque<CanFrame>; 3],
}

impl ReceiveStaging {
    pub fn len(&self) -> usize {
        self.queues.iter().map(VecDeque::len).sum()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn depths(&self) -> [usize; 3] {
        std::array::from_fn(|c| self.queues[c].len())
    }
    pub fn push(&mut self, controller: usize, frame: CanFrame) -> Result<(), String> {
        frame.validate()?;
        if controller >= self.queues.len() {
            return Err("CAN controller is not mapped".into());
        }
        if self.len() >= 4096 {
            return Err("Live CANdi receive staging queue full; no silent frame discard".into());
        }
        self.queues[controller].push_back(frame);
        Ok(())
    }
    /// `deliver` returns true only after consuming the frame (including a
    /// modeled filter rejection/overrun); false retains it for another poll.
    pub fn drain(
        &mut self,
        mut deliver: impl FnMut(usize, &CanFrame) -> Result<bool, String>,
    ) -> Result<u64, String> {
        let mut deferred = 0;
        for (c, queue) in self.queues.iter_mut().enumerate() {
            while let Some(frame) = queue.front() {
                if !deliver(c, frame)? {
                    deferred += 1;
                    break;
                }
                queue.pop_front();
            }
        }
        Ok(deferred)
    }
}

#[cfg(test)]
mod receive_staging_tests {
    use super::*;

    fn frame() -> CanFrame {
        CanFrame {
            id: 0x541,
            extended: false,
            rtr: false,
            dlc: 1,
            data: vec![1],
        }
    }

    #[test]
    fn receive_limit_is_shared_and_failed_delivery_retains_the_frame() {
        let mut staging = ReceiveStaging::default();
        for n in 0..4096 {
            staging.push(n % 3, frame()).unwrap();
        }
        assert!(staging.push(2, frame()).unwrap_err().contains("queue full"));
        assert_eq!(staging.len(), 4096);
        let depths = staging.depths();
        assert_eq!(
            staging
                .drain(|_, _| Err("delivery failed".into()))
                .unwrap_err(),
            "delivery failed"
        );
        assert_eq!(staging.depths(), depths);
        assert_eq!(staging.drain(|_, _| Ok(false)).unwrap(), 3);
        assert_eq!(staging.depths(), depths);
        staging.drain(|_, _| Ok(true)).unwrap();
        assert!(staging.is_empty());
    }

    #[test]
    fn invalid_arrivals_never_enter_staging() {
        let mut staging = ReceiveStaging::default();
        assert!(staging.push(3, frame()).is_err());
        let mut invalid = frame();
        invalid.dlc = 9;
        assert!(staging.push(0, invalid).is_err());
        assert!(staging.is_empty());
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompletionSource {
    J2534SoftwareLoopback,
    VcxUsbWriteCompatibility,
    ChipsoftDeviceReplyCompatibility,
}
impl CompletionSource {
    pub fn label(self) -> &'static str {
        match self {
            Self::J2534SoftwareLoopback => "j2534-software-loopback",
            Self::VcxUsbWriteCompatibility => "vcx-usb-write-compatibility",
            Self::ChipsoftDeviceReplyCompatibility => "chipsoft-device-reply-compatibility",
        }
    }
}
#[derive(Debug, PartialEq, Eq)]
pub enum Event {
    /// Driver-compatible completion, never independent evidence of a CAN ACK.
    Completed {
        controller: usize,
        ticket: u64,
        source: CompletionSource,
    },
    Received(usize, CanFrame),
}

/// Match the existing CANdi PIT/SCI research clock to wall time when attached
/// to real hardware. Offline emulation can run freely; live ECU deadlines cannot.
/// Cycle counts are approximate, not a claim of cycle-accurate CPU32 timing.
pub fn realtime_delay(cycles: u64, elapsed: Duration) -> Duration {
    const HZ: u64 = 16_777_216;
    let virtual_time = Duration::new(cycles / HZ, ((cycles % HZ) * 1_000_000_000 / HZ) as u32);
    let lead = virtual_time.saturating_sub(elapsed);
    if lead < Duration::from_millis(1) {
        Duration::ZERO
    } else {
        lead.min(Duration::from_millis(2))
    }
}

/// Implementations own bounded I/O, deadlines and cleanup. `poll` must not wait
/// for USB or ECU responses on the emulation thread. RX remains real raw CAN;
/// segmentation and diagnostic requests belong to the original guest firmware.
pub trait Backend {
    fn poll(&mut self) -> Result<Vec<Event>, String>;
    fn transmit(&mut self, controller: usize, tx: CanTransmission) -> Result<(), String>;
    fn confirmations(&self) -> u64;
    fn close(&mut self);
}

/// Capture-backed electrical route shared by Chipsoft, VCX Nano and Windows J2534.
/// Flags are deliberately named by layer; J2534 and VCX wire values differ.
#[derive(Debug, PartialEq, Eq)]
pub struct ElectricalRoute {
    pub channel: u8,
    pub j2534_flags: u32,
    pub wire_flags: u32,
}
pub fn electrical_route(
    controller: usize,
    tx: &CanTransmission,
) -> Result<ElectricalRoute, String> {
    tx.frame.validate()?;
    if tx.frame.extended || tx.frame.rtr {
        return Err("Native electrical route supports standard data frames only".into());
    }
    match (controller, tx.btr0, tx.btr1, tx.electrical) {
        (0, 0xc1, 0x36, CanElectricalState::StandardCan) => Ok(ElectricalRoute {
            channel: 0,
            j2534_flags: 0,
            wire_flags: 0,
        }),
        (
            2,
            0xdd,
            0x36,
            CanElectricalState::SingleWireGpio {
                latch,
                assignment: 0,
                direction,
            },
        ) if direction & 3 == 3 => match latch & 3 {
            2 if tx.frame.id == 0x100 && tx.frame.dlc == 0 => Ok(ElectricalRoute {
                channel: 1,
                j2534_flags: 0x400,
                wire_flags: 0x1000,
            }),
            3 => Ok(ElectricalRoute {
                channel: 1,
                j2534_flags: 0,
                wire_flags: 0,
            }),
            _ => Err("Unverified SWCAN electrical state/frame combination".into()),
        },
        _ => Err("Unsupported native CAN controller, timing or electrical route".into()),
    }
}

// Compatibility aliases for callers using the former adapter-specific names.
pub use electrical_route as nano_route;
pub type NanoRoute = ElectricalRoute;
