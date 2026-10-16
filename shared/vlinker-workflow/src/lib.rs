// SPDX-License-Identifier: MPL-2.0
pub mod adapter_identity;
pub mod connection_startup;
pub mod ownership;
pub mod reports;
pub mod t8_dtc;
pub mod vin_probe;
pub mod vlinker;

pub fn manifest() -> serde_json::Value {
    serde_json::json!({"package":"opensaab-vlinker-workflow", "version":env!("CARGO_PKG_VERSION"),
        "source_pin_sha256":env!("OPENSAAB_VLINKER_SOURCE_PIN"), "workflow_schema":1, "report_schema":1})
}

pub use opensaab_adapter_translation as translation;
