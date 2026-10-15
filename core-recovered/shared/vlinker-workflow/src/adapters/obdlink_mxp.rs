// SPDX-License-Identifier: MPL-2.0
//! MX+ STN2255 5.12.4 HS-CAN profile; other firmware remains unqualified.
//! OBDLink FRPM revision F: AT flow-control mode 1, STPR/STPBRR readback.
use super::{mic, Profile};
pub const STARTUP: &[&str] = &[
    "ATZ", "ATE0", "STDI", "STI", "ATRV", "ATSP6", "STPR", "STPBRR",
    "ATL0", "ATS1", "ATH1", "ATD0", "ATCAF0", "ATCFC1", "ATAT0", "ATST32",
    "ATSH7E0", "ATCRA7E8", "ATFCSH7E0", "ATFCSD300000", "ATFCSM1",
    mic::VIN_REQUEST, "STPC",
];
fn matches_product(product: &str) -> bool { mic::product_matches(product, "OBDLINK MX+", false) }
fn firmware(firmware: &str) -> bool { firmware == "STN2255 v5.12.4" }
pub const PROFILE: Profile = Profile {
    id: "obdlink_mxp", label: "OBDLink MX+",
    vin_profile: "obdlink_mxp_stn2255_hscan_vin_v1",
    info_profile: "obdlink_mxp_stn2255_hscan_ecm_info_v1",
    dtc_profile: "obdlink_mxp_stn2255_hscan_ecm_dtc_v1",
    startup: STARTUP, matches_product, vin_firmware: firmware, probe_firmware: firmware,
};
