// SPDX-License-Identifier: MPL-2.0
//! Common MIC wire settings; model profiles explicitly opt into this recipe.
pub const VIN_REQUEST: &str = "021A900000000000";
pub const STARTUP: [&str; 13] = [
    "ATZ",
    "ATE0",
    "VTI",
    "VTVERS",
    "ATRV",
    "ATSP6",
    "VTPRON",
    "VTPBRD",
    "VTSWGP CAF0,CFC1,H1,S1,D0,L0,AT0,GR0",
    "VTSET_HD 7E0,7E8,FF",
    "VTSET_CAN_FC 300000,1,7E0",
    VIN_REQUEST,
    "VTPC",
];
pub fn product_matches(product: &str, prefix: &str, allow_plus: bool) -> bool {
    let upper = product.trim().to_ascii_uppercase();
    upper.strip_prefix(prefix).is_some_and(|rest| {
        rest.is_empty()
            || rest.starts_with(char::is_whitespace)
            || allow_plus && (rest == "+" || rest.starts_with("+ "))
    })
}
pub fn firmware_has_token(firmware: &str, allowed: &[&str]) -> bool {
    firmware
        .to_ascii_uppercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .any(|token| allowed.contains(&token))
}
