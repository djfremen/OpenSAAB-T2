// SPDX-License-Identifier: MPL-2.0
//! Native adapter protocol components, independent of the emulator and GUI.

#[path = "candi/cpu.rs"]
pub mod candi_cpu;
#[path = "candi/transport.rs"]
pub mod candi_transport;
// Compatibility re-exports keep CLI/API callers stable while code lives in adapter buckets.
pub mod adapters;
pub use adapters::chipsoft::backend as chipsoft_backend;
pub use adapters::chipsoft::channel as chipsoft_channel;
pub use adapters::chipsoft::dtc as chipsoft_dtc;
pub use adapters::chipsoft::probe as chipsoft_probe;
pub use adapters::chipsoft::protocol as chipsoft;
#[cfg(any(target_os = "macos", target_os = "windows"))]
pub use adapters::chipsoft::serial as chipsoft_serial;
pub mod ibus_dtc;
pub mod ignition;
pub mod seatbelt;
pub mod t8_dtc;
pub use adapters::j2534::connection as vcx;
pub mod vin_probe;
pub mod connection_startup;

pub mod can_adapter;
pub use opensaab_vlinker_workflow::translation as adapter_translation;
pub use adapters::j2534::bridge as candi_nano;
pub use adapters::vcx_nano::channel as nano_channel;
pub use adapters::vcx_nano::native as nano_native;
pub use adapters::vcx_nano::protocol as nano_usb;

pub use adapters::vcx_nano::backend as nano_backend;

pub mod security;

pub mod dtc_reports;

pub mod support_reports;
pub mod menu_navigation;
pub mod adapter_failure;

// Embedded hosts reuse the CLI hardware models and exact guest step routine.
// The alias keeps pre-existing adapter imports valid inside this library.
#[cfg(feature = "embedded")]
extern crate self as tech2_emu;
#[cfg(feature = "embedded")]
pub mod artifacts;
#[cfg(feature = "embedded")]
pub mod bus;
#[cfg(feature = "embedded")]
pub mod candi;
#[cfg(feature = "embedded")]
pub mod communication;
#[cfg(feature = "embedded")]
pub mod event_console;
#[cfg(feature = "embedded")]
pub mod guest;
#[cfg(feature = "embedded")]
pub mod lcd;
#[cfg(feature = "embedded")]
pub mod load_binary;
#[cfg(feature = "embedded")]
pub mod logger;
#[cfg(feature = "embedded")]
pub mod options;
#[cfg(feature = "embedded")]
pub mod ssa_flash;
#[cfg(feature = "embedded")]
pub mod tpu;
#[cfg(feature = "embedded")]
pub mod trace;
#[cfg(feature = "embedded")]
pub mod embedded;
#[cfg(all(feature = "embedded", feature = "load-test"))]
mod load_test;
#[cfg(feature = "embedded")]
pub use guest::{service_interrupt, step_guest};
