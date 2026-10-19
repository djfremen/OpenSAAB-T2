// SPDX-License-Identifier: MPL-2.0
//! Adapter-specific wire protocols and backends. Shared CAN events stay in can_adapter.
pub mod chipsoft;
pub mod common;
pub mod j2534;
pub mod vcx_nano;
// One STN wire implementation is shared with the recovered runtime.
#[path = "../../core-recovered/emulator/src/adapters/obdlink_guest.rs"]
pub mod obdlink_guest;
#[path = "../../core-recovered/emulator/src/adapters/stn_parser.rs"]
pub mod stn_parser;
