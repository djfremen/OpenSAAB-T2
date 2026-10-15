// SPDX-License-Identifier: MPL-2.0
//! Carista EVO identity and owned MIC3413 qualification. No MC+ firmware fallback.
use super::{mic, Profile};
fn matches_product(product: &str) -> bool {
    mic::product_matches(product, "CARISTA EVO", false)
}
fn vin_firmware(firmware: &str) -> bool {
    mic::firmware_has_token(firmware, &["MIC3413"])
}
fn probe_firmware(firmware: &str) -> bool {
    firmware.trim().eq_ignore_ascii_case("MIC3413 V2.0.84")
}
pub const PROFILE: Profile = Profile {
    id: "carista_evo",
    label: "Carista EVO",
    vin_profile: "carista_evo_mic3413_hscan_vin_v1",
    info_profile: "carista_evo_mic3413_hscan_ecm_info_v1",
    dtc_profile: "carista_evo_mic3413_hscan_ecm_dtc_v1",
    startup: &mic::STARTUP,
    matches_product,
    vin_firmware,
    probe_firmware,
};
