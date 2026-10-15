// SPDX-License-Identifier: MPL-2.0
//! Host CAN bridge trait (goCAN / J2534 / etc.). Stub only.
//!
//! Naming follows the TrionicTuning approach of jacking a userland CAN stack
//! onto intercepted CAN-chip register traffic. No adapter calls are made here.

use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CanFrame {
    pub id: u32,
    pub data: Vec<u8>,
}

#[derive(Debug)]
pub struct GoCanError {
    message: String,
}

impl GoCanError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for GoCanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for GoCanError {}

pub trait GoCanBridge {
    fn send(&mut self, frame: CanFrame) -> Result<(), GoCanError>;
    fn try_recv(&mut self) -> Result<Option<CanFrame>, GoCanError>;
}

/// Default backend: always errors with an explicit unimplemented label.
#[derive(Debug, Default)]
pub struct UnimplementedGoCan;

impl GoCanBridge for UnimplementedGoCan {
    fn send(&mut self, frame: CanFrame) -> Result<(), GoCanError> {
        Err(GoCanError::new(format!(
            "goCAN bridge unimplemented; refused TX id={:#x} len={} (not vehicle traffic)",
            frame.id,
            frame.data.len()
        )))
    }

    fn try_recv(&mut self) -> Result<Option<CanFrame>, GoCanError> {
        Err(GoCanError::new(
            "goCAN bridge unimplemented; no RX path (not an ECU timeout simulation)",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unimplemented_backend_never_looks_like_success() {
        let mut b = UnimplementedGoCan;
        assert!(b
            .send(CanFrame {
                id: 1,
                data: vec![0]
            })
            .is_err());
        assert!(b.try_recv().is_err());
    }
}
