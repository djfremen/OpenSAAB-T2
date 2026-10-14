// SPDX-License-Identifier: MPL-2.0
//! Advice from bounded observed identity only. Never changes adapter permission.
use serde_json::{json, Value};
pub const CHIPSOFT_INFO: &str = "https://www.chipsoft.com.ua/2021/01/13/chipsoft-j2534-152.html";
pub const CHIPSOFT_PRO_PACKAGE: &str = "https://chipsoft.com.ua/download/chipsoft_j2534_pro_kline_canhacker_tech2win_drivers_152.zip";
pub const TESTED_FIRMWARE: &str = "1.5.2";
fn numeric_version(s: &str) -> Option<[u16; 3]> {
    let parts: Vec<_> = s.split('.').collect();
    if !(2..=3).contains(&parts.len()) { return None; }
    let mut result = [0; 3];
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() || part.len() > 3 || !part.bytes().all(|b| b.is_ascii_digit()) { return None; }
        result[i] = part.parse().ok()?;
    }
    Some(result)
}
pub fn chipsoft(v: &Value) -> Option<Value> {
    if v["family"] != "chipsoft" || v["compatibility"] != "rejected" || v["failure_kind"] != "adapter_firmware_not_validated" { return None; }
    let model = v["model"].as_str().filter(|s| ["Chipsoft J2534 Pro", "Chipsoft J2534 Mid", "Chipsoft J2534 Lite"].contains(s));
    let firmware = v["firmware_version"].as_str().filter(|s| crate::adapter_diagnostics::version(s));
    let tested = [1, 5, 2];
    let numeric = firmware.and_then(numeric_version);
    let (code, title, explanation, url) = match model {
        Some("Chipsoft J2534 Mid" | "Chipsoft J2534 Lite") => (
            "model_not_validated", "Chipsoft model not validated",
            "This native mode does not enable Mid/Lite. A firmware update does not add support for another model. Send a reviewed issue report with CRASH DETAILS.", CHIPSOFT_INFO),
        Some("Chipsoft J2534 Pro") if numeric.is_some_and(|n| n < tested) => (
            "older_pro_firmware", "Older Chipsoft Pro firmware detected",
            "Use Chipsoft's official Windows Pro driver/firmware package for your genuine Pro adapter and follow the manufacturer's update instructions. Reconnect and identify again afterward. Installing a Windows driver alone does not confirm the adapter firmware changed.", CHIPSOFT_PRO_PACKAGE),
        Some("Chipsoft J2534 Pro") if numeric == Some(tested) => (
            "identity_not_validated", "Chipsoft identification not validated",
            "The reported firmware matches the tested version, so this error does not establish outdated firmware. Send a reviewed issue report with CRASH DETAILS before changing firmware.", CHIPSOFT_INFO),
        Some("Chipsoft J2534 Pro") if numeric.is_some_and(|n| n > tested) => (
            "firmware_not_validated", "Chipsoft firmware not validated",
            "The reported firmware is newer than the tested version. Newer firmware is not automatically validated. Do not downgrade solely because of this message; send a reviewed issue report with CRASH DETAILS.", CHIPSOFT_INFO),
        _ => (
            "identity_unknown", "Chipsoft identification not validated",
            "The adapter model or firmware could not be verified. This does not establish outdated firmware. Send a reviewed issue report with CRASH DETAILS and confirm the exact adapter model.", CHIPSOFT_INFO),
    };
    Some(json!({"code":code, "title":title, "message":format!(
        "Detected: {} | firmware {}.\nTested by this native mode: Chipsoft J2534 Pro firmware {TESTED_FIRMWARE}.\n\n{explanation}\n\nAndroid connects directly over USB; it does not use the Windows J2534 driver.",
        model.unwrap_or("unavailable"),firmware.unwrap_or("unavailable")), "url":url}))
}
#[cfg(test)]
mod tests {
    use super::*;
    fn rejected(model: &str, fw: &str) -> Value { json!({"family":"chipsoft","compatibility":"rejected","failure_kind":"adapter_firmware_not_validated","model":model,"firmware_version":fw}) }
    #[test]
    fn update_advice_requires_observed_older_pro_firmware() {
        for fw in ["1.4", "1.4.0", "1.5.1"] { let advice=chipsoft(&rejected("Chipsoft J2534 Pro",fw)).unwrap(); assert_eq!(advice["code"],"older_pro_firmware"); assert_eq!(advice["url"],CHIPSOFT_PRO_PACKAGE); }
        for fw in ["1.5.2", "1.5.3", "1.52", "unknown", "1.5.1-test", "1.5.1.0"] { let advice=chipsoft(&rejected("Chipsoft J2534 Pro",fw)).unwrap(); assert_ne!(advice["code"],"older_pro_firmware"); assert_eq!(advice["url"],CHIPSOFT_INFO); }
    }
    #[test]
    fn mid_lite_unknown_and_accepted_are_not_update_cases() {
        for model in ["Chipsoft J2534 Mid","Chipsoft J2534 Lite"] { let advice=chipsoft(&rejected(model,"1.5.1")).unwrap(); assert_eq!(advice["code"],"model_not_validated"); assert_eq!(advice["url"],CHIPSOFT_INFO); }
        let mut v=rejected("RAW_PRIVATE_ID","PRIVATE_SERIAL"); let advice=chipsoft(&v).unwrap(); assert_eq!(advice["code"],"identity_unknown"); assert!(!advice.to_string().contains("PRIVATE"));
        v["compatibility"]=json!("accepted"); assert!(chipsoft(&v).is_none()); v["compatibility"]=json!("rejected"); v["family"]=json!("nano"); assert!(chipsoft(&v).is_none());
    }
    #[test]
    fn matching_firmware_rejection_is_not_called_outdated() {
        let advice=chipsoft(&rejected("Chipsoft J2534 Pro","1.5.2")).unwrap(); assert_eq!(advice["code"],"identity_not_validated"); assert!(advice["message"].as_str().unwrap().contains("does not establish outdated firmware"));
    }
}
