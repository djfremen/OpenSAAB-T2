// SPDX-License-Identifier: MPL-2.0
//! Classic Bosch MDI on Windows. Vendor startup and ISO-TP stay in this bucket.
pub mod init;
pub mod polling;
pub mod readiness;
#[cfg(all(windows, target_arch = "x86"))]
pub mod native;
pub mod profile;
pub mod raw;
pub mod trace;
#[cfg(all(windows, target_arch = "x86"))]
pub mod server;
pub mod worker;
