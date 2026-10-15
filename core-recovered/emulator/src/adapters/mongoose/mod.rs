// SPDX-License-Identifier: MPL-2.0
//! MongoosePro GM II capture research and experimental adapter-only live test.
//! No original-firmware guest backend or hardware qualification.
//! USB, initialization and reply interpretation stay inside this adapter bucket.
pub mod init;
pub mod live;
pub mod protocol;
pub mod trace;
pub mod vin;
