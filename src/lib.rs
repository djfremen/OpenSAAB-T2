// SPDX-License-Identifier: MPL-2.0
//! Native adapter protocol components, independent of the emulator and GUI.

#[path = "candi/cpu.rs"]
pub mod candi_cpu;
#[path = "candi/transport.rs"]
pub mod candi_transport;
pub mod chipsoft;
pub mod chipsoft_backend;
pub mod chipsoft_channel;
pub mod chipsoft_probe;
#[cfg(target_os = "macos")]
pub mod chipsoft_serial;
pub mod ibus_dtc;
pub mod ignition;
pub mod seatbelt;
pub mod t8_dtc;
pub mod vcx;
pub mod vin_probe;

pub mod can_adapter;
pub mod candi_nano;
pub mod nano_channel;
pub mod nano_native;
pub mod nano_usb;

pub mod nano_backend;
