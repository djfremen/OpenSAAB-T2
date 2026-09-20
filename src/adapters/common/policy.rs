// SPDX-License-Identifier: MPL-2.0
//! Shared original-firmware CAN request policy. No adapter USB frames or opcodes.
use crate::{can_adapter::electrical_route, candi_cpu::CanTransmission};
use std::time::{Duration, Instant};

/// Explicit, mutually exclusive permissions for original firmware requests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Profile {
    Read,
    Seeds,
    ClearDtc,
}
impl Profile {
    pub fn collection(seeds: bool) -> Self {
        if seeds {
            Self::Seeds
        } else {
            Self::Read
        }
    }
}

/// The read-diagnostics subset exercised by the successful native firmware run.
/// Security/seed requests are intentionally outside this Android DTC profile.
pub fn diagnostic_allowed(controller: usize, tx: &CanTransmission) -> bool {
    allowed_for_session(controller, tx, false)
}

/// Captured collection profile: request seeds only; never submit a key.
pub fn allowed_for_session(controller: usize, tx: &CanTransmission, seeds: bool) -> bool {
    allowed_for_profile(controller, tx, Profile::collection(seeds))
}
pub fn allowed_for_profile(controller: usize, tx: &CanTransmission, profile: Profile) -> bool {
    let seeds = profile == Profile::Seeds;
    let Ok(route) = electrical_route(controller, tx) else {
        return false;
    };
    let id = tx.frame.id;
    let data = &tx.frame.data;
    if route.wire_flags == 0x1000 {
        return controller == 2 && id == 0x100 && data.is_empty();
    }
    if data.is_empty()
        || !(id == 0x101
            || id == 0x7e0
            || (controller == 0 && id == 0x7e1)
            || (0x240..=0x25f).contains(&id))
    {
        return false;
    }
    let offset = usize::from(data[0] == 0xfe);
    let Some(&pci) = data.get(offset) else {
        return false;
    };
    if pci == 0x30 {
        return data.len() - offset >= 3;
    } // Guest-owned ISO-TP flow control.
    let n = pci as usize;
    if !(1..=7).contains(&n) || offset + 1 + n > data.len() {
        return false;
    }
    let request = &data[offset + 1..offset + 1 + n];
    if seeds && offset == 0 {
        match request {
            [0xae, 0] if controller == 2 && id == 0x241 => return true,
            [0x27, 1]
                if (controller == 0
                    && ([0x7e0, 0x7e1].contains(&id) || (0x240..=0x25f).contains(&id)))
                    || (controller == 2 && (0x240..=0x25f).contains(&id)) =>
            {
                return true
            }
            [0x27, 0x0b] if controller == 2 && id == 0x241 => return true,
            _ => {}
        }
    }
    if profile == Profile::ClearDtc && (request == [0x04] || request == [0xa9, 0x81, 0x0a]) {
        // GMW3110 ClearDiagnosticInformation: one-byte service, guest-selected
        // physical module or functional address, with unchanged guest padding.
        // The native clear workflow also checks DTC status using mask0A.
        return true;
    }
    match request {
        // Captured BCM Add "Checking SPS": GMW3110 ReportProgrammingState.
        // Query only; do not admit ProgrammingMode, downloads or writes.
        [0xa2] => {
            controller == 0 && id == 0x101 && data.as_slice() == [0xfe, 1, 0xa2, 0, 0, 0, 0, 0]
        }
        [0x1a, _] | [0x3e] | [0x20] => true,
        [0xaa, 1, _] => true,
        [0xa9, 0x81, 0x10 | 0x12 | 0x0c] => true,
        [0x10, 1 | 2] => true,
        // GMW3110 8.10/8.19: define read packets by two-byte parameter IDs. This
        // configures diagnostic reporting; no memory/download payload is allowed.
        [0x2c, 0xf3..=0xfe, pids @ ..] => {
            controller == 0 && id == 0x7e0 && !pids.is_empty() && pids.len() % 2 == 0
        }
        [0xaa, rate @ 0..=4, dpids @ ..] => {
            controller == 0
                && id == 0x7e0
                && (*rate == 0 || !dpids.is_empty())
                && dpids.iter().all(|d| matches!(d, 1..=0x7f | 0x90..=0xfe))
        }
        _ => false,
    }
}

#[derive(Clone, Copy)]
struct PacketContinuation {
    remaining: usize,
    sequence: u8,
    deadline: Instant,
    profile: Profile,
    packet_read: bool,
}

/// Engine dynamic packet definitions and periodic reads may span CAN frames.
/// Unsolicited/out-of-order continuations can never authorize another service.
#[derive(Default)]
pub struct CommandGate {
    packet: Option<PacketContinuation>,
}
impl CommandGate {
    pub fn allowed(
        &mut self,
        controller: usize,
        tx: &CanTransmission,
        profile: Profile,
        now: Instant,
    ) -> bool {
        if electrical_route(controller, tx).is_err() {
            return false;
        }
        let d = &tx.frame.data;
        let engine = controller == 0 && tx.frame.id == 0x7e0;
        if engine && d.len() == 8 && d[0] & 0xf0 == 0x10 {
            let length = (((d[0] & 15) as usize) << 8) | d[1] as usize;
            let packet_read = d[2] == 0xaa
                && d[3] <= 4
                && (8..=32).contains(&length)
                && d[4..].iter().all(|v| matches!(v, 1..=0x7f | 0x90..=0xfe));
            let definition = d[2] == 0x2c
                && (0xf3..=0xfe).contains(&d[3])
                && (8..=16).contains(&length)
                && length % 2 == 0;
            if self.packet.is_some() || !(packet_read || definition) {
                return false;
            }
            self.packet = Some(PacketContinuation {
                remaining: length - 6,
                sequence: 1,
                deadline: now + Duration::from_secs(3),
                profile,
                packet_read,
            });
            return true;
        }
        if d.first().is_some_and(|b| b & 0xf0 == 0x20) {
            let Some(mut p) = self.packet else {
                return false;
            };
            if !engine
                || d.len() != 8
                || now >= p.deadline
                || profile != p.profile
                || d[0] != 0x20 | p.sequence
            {
                return false;
            }
            if p.packet_read
                && !d[1..1 + p.remaining.min(7)]
                    .iter()
                    .all(|v| matches!(v, 1..=0x7f | 0x90..=0xfe))
            {
                return false;
            }
            p.remaining = p.remaining.saturating_sub(7);
            p.sequence = (p.sequence + 1) & 15;
            p.deadline = now + Duration::from_secs(3);
            self.packet = if p.remaining == 0 { None } else { Some(p) };
            return true;
        }
        if engine && self.packet.is_some() {
            return false;
        }
        allowed_for_profile(controller, tx, profile)
    }
}
