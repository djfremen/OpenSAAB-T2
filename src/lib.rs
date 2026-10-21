// SPDX-License-Identifier: MPL-2.0
//! Native adapter protocol components, independent of the emulator and GUI.

#[path = "candi/cpu.rs"]
pub mod candi_cpu;
#[path = "candi/transport.rs"]
pub mod candi_transport;
// Compatibility re-exports keep CLI/API callers stable while code lives in adapter buckets.
pub mod adapters;
pub use adapters::chipsoft::dtc as chipsoft_dtc;
pub use adapters::chipsoft::backend as chipsoft_backend;
pub use adapters::chipsoft::channel as chipsoft_channel;
pub use adapters::chipsoft::probe as chipsoft_probe;
pub use adapters::chipsoft::protocol as chipsoft;
#[cfg(target_os = "macos")]
pub use adapters::chipsoft::serial as chipsoft_serial;
pub mod ibus_dtc;
pub mod ignition;
pub mod seatbelt;
pub mod t8_dtc;
pub use adapters::j2534::connection as vcx;
pub mod vin_probe;

pub mod adapter_diagnostics;
pub mod adapter_compatibility;
pub mod can_adapter;
#[path = "../core-recovered/emulator/src/adapter_failure.rs"]
pub mod adapter_failure;
pub use adapters::j2534::bridge as candi_nano;
pub use adapters::vcx_nano::channel as nano_channel;
pub use adapters::vcx_nano::native as nano_native;
pub use adapters::vcx_nano::protocol as nano_usb;

pub use adapters::vcx_nano::backend as nano_backend;

pub mod native_menu;

pub use opensaab_adapter_translation as adapter_translation;

pub mod security_prompt;
