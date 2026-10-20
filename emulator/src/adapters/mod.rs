// SPDX-License-Identifier: MPL-2.0
//! Adapter-specific wire protocols and backends. Shared CAN events stay in can_adapter.
pub mod chipsoft;
pub mod common;
pub mod j2534;
pub mod mdi;
pub mod mongoose;
pub mod vcx_nano;
pub mod vlinker;

// Shared Android preview.18 MX+ transport, adopted via private core f0addf2.
pub mod obdlink_guest;
pub mod stn_parser;
