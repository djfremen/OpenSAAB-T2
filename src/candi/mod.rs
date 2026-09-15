// SPDX-License-Identifier: MPL-2.0
//! Opt-in CANDi secondary CPU research worker.
//!
//! Real Tech2 units talk to an in-cable CANDi MCU over a high-speed serial link;
//! that MCU owns a CAN controller for the vehicle bus. This module sketches the
//! same three layers without claiming live ECU traffic. The worker now runs
//! `tech2_emu::candi_cpu` against the decoded MSI download. `native_link` is an
//! opt-in headless UART experiment. These older surfaces remain unwired:
//!   1. Tech2 ↔ CANDi mailbox (`link`)
//!   2. CAN controller register intercept (`can_mmio`)
//!   3. Host CAN / goCAN bridge (`gocan_bridge`) — stub only
//!
//! Enable with `--candi` or `TECH2_CANDI=1`. Default runs are unchanged.

pub mod can_mmio;
pub mod gocan_bridge;
pub mod link;
pub mod native_link;
pub mod worker;

pub use worker::{maybe_start, CandiConfig};
