// SPDX-License-Identifier: MPL-2.0
//! Adapter-specific wire protocols and backends. Shared CAN events stay in can_adapter.
pub mod chipsoft;
pub mod common;
pub mod j2534;
pub mod vcx_nano;
