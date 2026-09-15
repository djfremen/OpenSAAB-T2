// SPDX-License-Identifier: MPL-2.0
//! Mailbox between the Tech2 guest side and the CANDi worker.
//!
//! Frames here are host-side IPC bytes, not decoded vehicle CAN frames and not
//! proof of J2534 PassThru traffic.

use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};

/// Direction relative to the CANDi worker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkDirection {
    /// Bytes the Tech2 side would push toward CANDi (master → slave).
    Tech2ToCandi,
    /// Bytes the CANDi side would push toward Tech2 (slave → master).
    CandiToTech2,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkFrame {
    pub direction: LinkDirection,
    pub bytes: Vec<u8>,
}

/// Bidirectional, non-blocking mailbox used by the scaffold worker.
pub struct CandiLink {
    to_candi_tx: Sender<LinkFrame>,
    to_candi_rx: Receiver<LinkFrame>,
    to_tech2_tx: Sender<LinkFrame>,
    to_tech2_rx: Receiver<LinkFrame>,
}

impl CandiLink {
    pub fn new() -> Self {
        let (to_candi_tx, to_candi_rx) = mpsc::channel();
        let (to_tech2_tx, to_tech2_rx) = mpsc::channel();
        Self {
            to_candi_tx,
            to_candi_rx,
            to_tech2_tx,
            to_tech2_rx,
        }
    }

    /// Split into endpoints owned by each side of the scaffold.
    pub fn split(self) -> (Tech2Endpoint, CandiEndpoint) {
        (
            Tech2Endpoint {
                tx: self.to_candi_tx,
                rx: self.to_tech2_rx,
            },
            CandiEndpoint {
                tx: self.to_tech2_tx,
                rx: self.to_candi_rx,
            },
        )
    }
}

impl Default for CandiLink {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub struct Tech2Endpoint {
    tx: Sender<LinkFrame>,
    rx: Receiver<LinkFrame>,
}

impl Tech2Endpoint {
    pub fn send_to_candi(&self, bytes: Vec<u8>) -> Result<(), String> {
        self.tx
            .send(LinkFrame {
                direction: LinkDirection::Tech2ToCandi,
                bytes,
            })
            .map_err(|_| "CANDi worker mailbox closed".into())
    }

    pub fn try_recv_from_candi(&self) -> Result<Option<LinkFrame>, String> {
        match self.rx.try_recv() {
            Ok(frame) => Ok(Some(frame)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Err("CANDi worker mailbox closed".into()),
        }
    }
}

#[derive(Debug)]
pub struct CandiEndpoint {
    tx: Sender<LinkFrame>,
    rx: Receiver<LinkFrame>,
}

impl CandiEndpoint {
    pub fn try_recv_from_tech2(&self) -> Result<Option<LinkFrame>, String> {
        match self.rx.try_recv() {
            Ok(frame) => Ok(Some(frame)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Err("Tech2 mailbox closed".into()),
        }
    }

    pub fn send_to_tech2(&self, bytes: Vec<u8>) -> Result<(), String> {
        self.tx
            .send(LinkFrame {
                direction: LinkDirection::CandiToTech2,
                bytes,
            })
            .map_err(|_| "Tech2 mailbox closed".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mailbox_round_trip_preserves_bytes_and_direction() {
        let (tech2, candi) = CandiLink::new().split();
        tech2.send_to_candi(vec![0x01, 0x02]).unwrap();
        let got = candi.try_recv_from_tech2().unwrap().unwrap();
        assert_eq!(got.direction, LinkDirection::Tech2ToCandi);
        assert_eq!(got.bytes, vec![0x01, 0x02]);
        candi.send_to_tech2(vec![0xaa]).unwrap();
        let back = tech2.try_recv_from_candi().unwrap().unwrap();
        assert_eq!(back.direction, LinkDirection::CandiToTech2);
        assert_eq!(back.bytes, vec![0xaa]);
        assert!(tech2.try_recv_from_candi().unwrap().is_none());
    }
}
