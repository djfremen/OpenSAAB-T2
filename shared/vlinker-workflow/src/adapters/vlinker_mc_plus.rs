// SPDX-License-Identifier: MPL-2.0
//! vLinker MC+ identity and its existing MIC firmware/startup qualification.
use super::{mic, Profile};
fn matches_product(product: &str) -> bool {
    mic::product_matches(product, "VLINKER MC", true)
}
fn vin_firmware(firmware: &str) -> bool {
    mic::firmware_has_token(firmware, &["MIC3313", "MIC331X", "MIC3X1X"])
}
pub const PROFILE: Profile = Profile {
    id: "vlinker_mc_plus",
    label: "vLinker MC+",
    vin_profile: "vlinker_mc_mic_hscan_vin_v1",
    info_profile: "vlinker_mc_mic_hscan_ecm_info_v1",
    dtc_profile: "vlinker_mc_mic_hscan_ecm_dtc_v1",
    startup: &mic::STARTUP,
    matches_product,
    vin_firmware,
    probe_firmware: vin_firmware,
};
