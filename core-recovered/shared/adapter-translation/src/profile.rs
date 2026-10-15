// SPDX-License-Identifier: MPL-2.0
use crate::Bus;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlowControlOwner {
    Guest,
    Host,
    Adapter,
    None,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    GuestRaw,
    HostIsoTp,
    HostGmReport,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Representation {
    RawCan,
    IsoTpPayload,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandFamily {
    Chipsoft,
    Nano,
    J2534,
    Elm,
    Stn,
    Mic,
    BlueDriver,
    Virtual,
}

/// Exact-profile capabilities supplied after qualification; a brand name supplies none.
#[derive(Clone, Debug)]
pub struct Capabilities {
    pub family: CommandFamily,
    pub buses: Vec<Bus>,
    pub raw_receive: bool,
    pub raw_transmit: bool,
    pub assembled_receive: bool,
    pub adapter_flow_control: bool,
    pub simultaneous_buses: bool,
}
#[derive(Clone, Copy, Debug)]
pub struct SessionProfile {
    pub bus: Bus,
    pub mode: Mode,
    pub representation: Representation,
    pub flow_control: FlowControlOwner,
}
impl Capabilities {
    pub fn validate(&self, p: SessionProfile) -> Result<(), String> {
        if !self.buses.contains(&p.bus) {
            return Err("Bus is not qualified for this adapter profile".into());
        }
        match p.representation {
            Representation::RawCan if !self.raw_receive => {
                return Err("Raw reception unavailable".into())
            }
            Representation::IsoTpPayload if !self.assembled_receive => {
                return Err("Assembled reception unavailable".into())
            }
            _ => {}
        }
        match p.mode {
            Mode::GuestRaw
                if p.representation != Representation::RawCan
                    || p.flow_control != FlowControlOwner::Guest
                    || !self.raw_transmit =>
            {
                Err("Guest requires raw RX/TX and guest-owned CAN flow control".into())
            }
            Mode::HostGmReport
                if p.representation != Representation::RawCan
                    || p.flow_control != FlowControlOwner::None
                    || !self.raw_transmit =>
            {
                Err(
                    "GM A9/81 report requires raw frames with CAN auto-flow-control disabled"
                        .into(),
                )
            }
            Mode::HostIsoTp => match p.flow_control {
                FlowControlOwner::Host
                    if p.representation == Representation::RawCan && self.raw_transmit =>
                {
                    Ok(())
                }
                FlowControlOwner::Adapter if self.adapter_flow_control => Ok(()),
                _ => Err("ISO-TP requires one qualified host or adapter flow-control owner".into()),
            },
            _ => Ok(()),
        }
    }
    pub fn validate_concurrent(&self, sessions: &[SessionProfile]) -> Result<(), String> {
        for p in sessions {
            self.validate(*p)?;
        }
        if !self.simultaneous_buses
            && sessions
                .iter()
                .any(|p| sessions.first().is_some_and(|first| first.bus != p.bus))
        {
            return Err(
                "Adapter requires explicit bus switching; simultaneous buses unqualified".into(),
            );
        }
        Ok(())
    }
}
