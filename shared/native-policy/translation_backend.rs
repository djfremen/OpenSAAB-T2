// SPDX-License-Identifier: MPL-2.0
use super::{Backend, Event, FailureContext};
use crate::candi_cpu::{CanFrame, CanTransmission};

/// Common raw-frame contract for every guest backend. This facade leaves all
/// adapter setup, wire encoding, timing, guards and completion provenance intact.
pub struct TranslationBackend {
    inner: Box<dyn Backend>,
    closed: bool,
}
impl TranslationBackend {
    pub fn new(inner: Box<dyn Backend>) -> Self {
        Self {
            inner,
            closed: false,
        }
    }
    fn validate(controller: usize, frame: &CanFrame) -> Result<(), String> {
        use crate::adapter_translation::{Bus, Packet, RawFrame};
        let bus = match controller {
            0 => Bus::HsCan,
            2 => Bus::SwCan,
            1 => Bus::Other(1),
            _ => return Err("Translation layer: unknown CAN controller".into()),
        };
        Packet::Raw(RawFrame {
            bus,
            id: frame.id,
            extended: frame.extended,
            rtr: frame.rtr,
            dlc: frame.dlc,
            data: frame.data.clone(),
        })
        .guest_frame()?;
        Ok(())
    }
}
impl Backend for TranslationBackend {
    fn poll(&mut self) -> Result<Vec<Event>, String> {
        if self.closed {
            return Err("Translation backend closed".into());
        }
        let events = self.inner.poll()?;
        for event in &events {
            match event {
                Event::Received(controller, frame) => Self::validate(*controller, frame)?,
                Event::Completed { controller, .. } if *controller > 2 => {
                    return Err("Translation layer: unknown completion controller".into())
                }
                _ => {}
            }
        }
        Ok(events)
    }
    fn transmit(&mut self, controller: usize, tx: CanTransmission) -> Result<(), String> {
        if self.closed {
            return Err("Translation backend closed".into());
        }
        Self::validate(controller, &tx.frame)?;
        self.inner.transmit(controller, tx)
    }
    fn confirmations(&self) -> u64 {
        self.inner.confirmations()
    }
    fn failure_context(&self) -> Option<FailureContext> {
        self.inner.failure_context()
    }
    fn close(&mut self) {
        if !self.closed {
            self.closed = true;
            self.inner.close();
        }
    }
}
impl Drop for TranslationBackend {
    fn drop(&mut self) {
        self.close();
    }
}

