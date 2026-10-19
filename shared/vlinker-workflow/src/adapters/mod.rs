// SPDX-License-Identifier: MPL-2.0
//! Model-owned identity, firmware qualification and initialization profiles.
//! MIC framing and diagnostic decoding remain shared between these MIC products.
pub mod carista_evo;
pub mod mic;
pub mod obdlink_mxp;
pub mod vlinker_mc_plus;

pub struct Profile {
    pub id: &'static str,
    pub label: &'static str,
    pub vin_profile: &'static str,
    pub info_profile: &'static str,
    pub dtc_profile: &'static str,
    pub startup: &'static [&'static str],
    pub matches_product: fn(&str) -> bool,
    pub vin_firmware: fn(&str) -> bool,
    pub probe_firmware: fn(&str) -> bool,
}
