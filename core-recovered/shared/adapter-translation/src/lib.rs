// SPDX-License-Identifier: MPL-2.0
//! Shared protocol behavior. No adapter I/O, vehicle commands or platform UI.
pub mod gm;
pub mod host;
pub mod isotp;
pub mod profile;

pub fn source_pin() -> &'static str {
    env!("OPENSAAB_TRANSLATION_SOURCE_PIN")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Bus {
    HsCan,
    SwCan,
    Other(u16),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawFrame {
    pub bus: Bus,
    pub id: u32,
    pub extended: bool,
    pub rtr: bool,
    pub dlc: u8,
    pub data: Vec<u8>,
}
impl RawFrame {
    pub fn standard(bus: Bus, id: u32, data: Vec<u8>) -> Result<Self, String> {
        if data.len() > 8 {
            return Err("Classical CAN exceeds eight bytes".into());
        }
        let frame = Self {
            bus,
            id,
            extended: false,
            rtr: false,
            dlc: data.len() as u8,
            data,
        };
        frame.validate()?;
        Ok(frame)
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.id > if self.extended { 0x1fff_ffff } else { 0x7ff }
            || self.dlc > 8
            || self.data.len() != if self.rtr { 0 } else { self.dlc as usize }
        {
            return Err("Invalid classical CAN frame".into());
        }
        Ok(())
    }
}

/// An assembled payload has no original PCI/segmentation and cannot become a raw guest frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Packet {
    Raw(RawFrame),
    IsoTpPayload {
        bus: Bus,
        id: u32,
        extended: bool,
        data: Vec<u8>,
    },
}
impl Packet {
    pub fn guest_frame(&self) -> Result<&RawFrame, String> {
        match self {
            Self::Raw(frame) => {
                frame.validate()?;
                Ok(frame)
            }
            Self::IsoTpPayload { .. } => {
                Err("Guest transport requires original raw CAN frames".into())
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    Physical,
    Replay,
    BehaviorModel,
    FirmwareEmulation,
}

/// `at_us` is the caller's normalized monotonic session clock, not guessed wire latency.
#[derive(Clone, Debug)]
pub struct Observation {
    pub session_id: u64,
    pub at_us: u64,
    pub device_timestamp: Option<u64>,
    pub origin: Origin,
    pub packet: Packet,
}
