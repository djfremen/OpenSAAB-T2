// SPDX-License-Identifier: MPL-2.0
//! Host recovery decisions. These never represent successful guest operations.
#[cfg(feature = "gui")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Restart,
    Close,
    Resume,
}

/// The native guest link is separate from the working host-side Nano queries.
pub fn guest_link_error(screen: &str) -> Option<&'static str> {
    let lower = screen.to_ascii_lowercase();
    if lower.contains("no candi communication established") {
        Some("Guest reports: No CANDI Communication Established. The native CANdi link is unavailable; host-side Nano/J2534 queries are a separate path.")
    } else {
        None
    }
}

/// Stop interactive waits before the guest exhausts retries and asserts.
pub fn unavailable_reason(screen: &str) -> Option<&'static str> {
    if let Some(reason) = guest_link_error(screen) {
        return Some(reason);
    }
    let lower = screen.to_ascii_lowercase();
    if lower.contains("checking") && lower.contains("working") {
        Some("The original firmware's adapter link is not connected. Host-side Nano/J2534 reads are separate. Return to the previous menu, restart, or quit.")
    } else if [
        "program not found",
        "no program found",
        "software not found",
        "card not present",
    ]
    .iter()
    .any(|s| lower.contains(s))
    {
        Some("The required program or card was not found. Check the selected image files, then restart the scanner, or quit.")
    } else {
        None
    }
}

/// Only capture interactive menus, never a splash or an in-progress operation.
pub fn can_checkpoint(bus: &crate::bus::Tech2Bus) -> bool {
    let screen = bus.screen_text().to_ascii_lowercase();
    !bus.compatibility_splash
        && !bus.guest_splash_reached()
        && bus.key_queue.is_empty()
        && bus.key_active.is_none()
        && !screen.contains("working")
        && unavailable_reason(&screen).is_none()
        && (screen.contains("f0:") || screen.contains("select") || screen.contains("main menu"))
}

pub struct Checkpoint {
    cpu: m68k::CpuCore,
    bus: crate::bus::Tech2Bus,
}

impl Checkpoint {
    pub fn capture(
        cpu: &m68k::CpuCore,
        bus: &crate::bus::Tech2Bus,
    ) -> Result<Self, serde_json::Error> {
        // Use the CPU crate's supported save-state format; decode tables and
        // runtime pointers are rebuilt, rather than copied unsafely.
        let cpu = serde_json::from_slice(&serde_json::to_vec(cpu)?)?;
        let link = bus
            .candi_link
            .as_ref()
            .map(|link| link.checkpoint())
            .transpose()?;
        let mut snapshot = bus.recovery_snapshot();
        snapshot.candi_link = link;
        Ok(Self { cpu, bus: snapshot })
    }

    pub fn restore(self, cpu: &mut m68k::CpuCore, bus: &mut crate::bus::Tech2Bus) {
        *cpu = self.cpu;
        bus.restore_recovery_snapshot(self.bus);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn connection_and_missing_program_have_recovery_without_blocking_menus() {
        assert!(unavailable_reason("Checking Key Position Working").is_some());
        assert!(unavailable_reason("PROGRAM NOT FOUND").is_some());
        assert!(unavailable_reason("Main Menu F0: Diagnostics").is_none());
        assert!(unavailable_reason("Diagnostic Trouble Codes Read DTCs").is_none());
        assert!(guest_link_error("Help\nNo CANDI Communication Established").is_some());
        assert!(guest_link_error("Checking Key Position Working").is_none());
        assert!(guest_link_error("CANdi Information").is_none());
    }
}
