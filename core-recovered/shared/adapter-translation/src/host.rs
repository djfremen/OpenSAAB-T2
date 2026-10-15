// SPDX-License-Identifier: MPL-2.0
//! One bounded host transaction per ECU/session. The caller owns I/O and cleanup.
use crate::{
    gm::{Dtc, Report},
    isotp::{Receiver, Step},
    profile::{Capabilities, FlowControlOwner, Mode, Representation, SessionProfile},
    Bus, Observation, Packet, RawFrame,
};

#[derive(Clone, Debug)]
pub enum Protocol {
    IsoTp {
        service: u8,
        positive_prefix: Vec<u8>,
    },
    GmA9Report {
        report_id: u32,
    },
}
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub payload_bytes: usize,
    pub report_records: usize,
    pub idle_us: u64,
    pub total_us: u64,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            payload_bytes: 512,
            report_records: 128,
            idle_us: 1_000_000,
            total_us: 10_000_000,
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct FlowControl {
    pub block_size: u8,
    pub st_min: u8,
}
impl FlowControl {
    fn validate(self) -> Result<(), String> {
        if self.st_min > 0x7f && !(0xf1..=0xf9).contains(&self.st_min) {
            return Err("Reserved ISO-TP separation time".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug)]
pub struct Config {
    pub session_id: u64,
    pub profile: SessionProfile,
    pub request_id: u32,
    pub reply_id: u32,
    pub extended: bool,
    pub protocol: Protocol,
    pub limits: Limits,
    pub host_flow_control: FlowControl,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResultData {
    Payload(Vec<u8>),
    GmReport(Vec<Dtc>),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Ignored,
    Pending,
    Progress,
    SendFlowControl(RawFrame),
    Complete(ResultData),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Active,
    Complete,
    Failed,
    Cancelled,
}

pub struct Session {
    config: Config,
    iso: Receiver,
    gm: Option<Report>,
    start_us: u64,
    last_us: u64,
    progress_us: u64,
    block_count: u8,
    state: State,
}
impl Session {
    pub fn new(config: Config, capabilities: &Capabilities, start_us: u64) -> Result<Self, String> {
        capabilities.validate(config.profile)?;
        if config.profile.mode == Mode::GuestRaw {
            return Err("Guest traffic must bypass host assembly".into());
        }
        if config.limits.idle_us == 0
            || config.limits.total_us == 0
            || config.limits.idle_us > config.limits.total_us
        {
            return Err("Invalid host transaction deadlines".into());
        }
        config.host_flow_control.validate()?;
        let max_id = if config.extended { 0x1fff_ffff } else { 0x7ff };
        if config.request_id > max_id || config.reply_id > max_id {
            return Err("Invalid transaction CAN address".into());
        }
        let gm = match &config.protocol {
            Protocol::GmA9Report { report_id } => {
                if config.profile.mode != Mode::HostGmReport
                    || *report_id > max_id
                    || *report_id == config.reply_id
                {
                    return Err("Invalid GM transaction profile/addresses".into());
                }
                Some(Report::new(config.limits.report_records)?)
            }
            Protocol::IsoTp {
                positive_prefix, ..
            } => {
                if config.profile.mode != Mode::HostIsoTp
                    || positive_prefix.is_empty()
                    || positive_prefix.len() > config.limits.payload_bytes
                {
                    return Err("Invalid ISO-TP transaction profile/expected reply".into());
                }
                None
            }
        };
        let iso = Receiver::new(config.limits.payload_bytes)?;
        Ok(Self {
            config,
            iso,
            gm,
            start_us,
            last_us: start_us,
            progress_us: start_us,
            block_count: 0,
            state: State::Active,
        })
    }
    pub fn state(&self) -> State {
        self.state
    }
    pub fn cancel(&mut self) {
        if self.state == State::Active {
            self.state = State::Cancelled;
            self.gm = None;
        }
    }
    pub fn tick(&mut self, now_us: u64) -> Result<(), String> {
        if self.state != State::Active {
            return Err("Host transaction is terminal".into());
        }
        if now_us < self.last_us {
            self.state = State::Failed;
            return Err("Session clock moved backwards".into());
        }
        self.last_us = now_us;
        if now_us - self.start_us >= self.config.limits.total_us
            || now_us - self.progress_us >= self.config.limits.idle_us
        {
            self.state = State::Failed;
            self.gm = None;
            return Err("Incomplete transaction: bounded deadline expired".into());
        }
        Ok(())
    }
    pub fn feed(&mut self, observation: &Observation) -> Result<Action, String> {
        self.tick(observation.at_us)?;
        if observation.session_id != self.config.session_id {
            return Ok(Action::Ignored);
        }
        let result = self.consume(&observation.packet);
        match &result {
            Ok(Action::Ignored) => {}
            Ok(Action::Complete(_)) => {
                self.progress_us = observation.at_us;
                self.state = State::Complete;
            }
            Ok(_) => self.progress_us = observation.at_us,
            Err(_) => {
                self.state = State::Failed;
                self.gm = None;
            }
        }
        result
    }
    fn selected(&self, bus: Bus, id: u32, extended: bool) -> bool {
        bus == self.config.profile.bus
            && extended == self.config.extended
            && (id == self.config.reply_id
                || matches!(self.config.protocol,Protocol::GmA9Report {report_id} if report_id==id))
    }
    fn consume(&mut self, packet: &Packet) -> Result<Action, String> {
        let data = match packet {
            Packet::Raw(frame) => {
                frame.validate()?;
                if !self.selected(frame.bus, frame.id, frame.extended) {
                    return Ok(Action::Ignored);
                }
                if frame.rtr {
                    return Err("RTR cannot carry a diagnostic reply".into());
                }
                if self.config.profile.representation != Representation::RawCan {
                    return Err("Unexpected raw representation".into());
                }
                if matches!(self.config.protocol, Protocol::GmA9Report { .. }) {
                    let Protocol::GmA9Report { report_id } = self.config.protocol else {
                        unreachable!()
                    };
                    if frame.id == report_id && frame.data.first() != Some(&0x81)
                        || frame.id == self.config.reply_id
                            && !frame.data.starts_with(&[3, 0x7f, 0xa9])
                    {
                        return Err("Unexpected GM report/control frame".into());
                    }
                    let report = self.gm.as_mut().unwrap();
                    report.receive_for_ids(
                        frame.id,
                        &frame.data,
                        self.config.reply_id,
                        report_id,
                    )?;
                    return if report.is_complete() {
                        Ok(Action::Complete(ResultData::GmReport(
                            self.gm.take().unwrap().finish()?,
                        )))
                    } else if frame.id == self.config.reply_id {
                        Ok(Action::Pending)
                    } else {
                        Ok(Action::Progress)
                    };
                }
                let Protocol::IsoTp { service, .. } = self.config.protocol else {
                    unreachable!()
                };
                if frame.data.starts_with(&[3, 0x7f]) {
                    if frame.data.len() < 4 || frame.data[2] != service {
                        return Err("Negative reply does not match active service".into());
                    }
                    if frame.data[3] == 0x78 && !self.iso.started() {
                        return Ok(Action::Pending);
                    }
                    return Err(format!(
                        "ECU rejected active service: NRC {:02X}",
                        frame.data[3]
                    ));
                }
                match self.iso.push(&frame.data)? {
                    Step::FirstFrame => {
                        self.block_count = 0;
                        return if self.config.profile.flow_control == FlowControlOwner::Host {
                            self.flow_control()
                        } else {
                            Ok(Action::Progress)
                        };
                    }
                    Step::ConsecutiveFrame => {
                        if self.config.profile.flow_control == FlowControlOwner::Host
                            && self.config.host_flow_control.block_size != 0
                        {
                            self.block_count += 1;
                            if self.block_count == self.config.host_flow_control.block_size {
                                self.block_count = 0;
                                return self.flow_control();
                            }
                        }
                        return Ok(Action::Progress);
                    }
                    Step::Complete(data) => data,
                }
            }
            Packet::IsoTpPayload {
                bus,
                id,
                extended,
                data,
            } => {
                if !self.selected(*bus, *id, *extended) {
                    return Ok(Action::Ignored);
                }
                if self.config.profile.representation != Representation::IsoTpPayload
                    || !matches!(self.config.protocol, Protocol::IsoTp { .. })
                {
                    return Err("Assembled payload cannot supply raw CAN/GM report evidence".into());
                }
                if data.is_empty() || data.len() > self.config.limits.payload_bytes {
                    return Err("Invalid assembled payload length".into());
                }
                let Protocol::IsoTp { service, .. } = self.config.protocol else {
                    unreachable!()
                };
                if data.first() == Some(&0x7f) {
                    if data.len() != 3 || data[1] != service {
                        return Err("Invalid negative payload for active service".into());
                    }
                    if data[2] == 0x78 {
                        return Ok(Action::Pending);
                    }
                    return Err(format!("ECU rejected active service: NRC {:02X}", data[2]));
                }
                data.clone()
            }
        };
        let Protocol::IsoTp {
            positive_prefix, ..
        } = &self.config.protocol
        else {
            unreachable!()
        };
        if !data.starts_with(positive_prefix) {
            return Err("Completed payload does not match active request".into());
        }
        Ok(Action::Complete(ResultData::Payload(data)))
    }
    fn flow_control(&self) -> Result<Action, String> {
        Ok(Action::SendFlowControl(RawFrame {
            bus: self.config.profile.bus,
            id: self.config.request_id,
            extended: self.config.extended,
            rtr: false,
            dlc: 8,
            data: vec![
                0x30,
                self.config.host_flow_control.block_size,
                self.config.host_flow_control.st_min,
                0,
                0,
                0,
                0,
                0,
            ],
        }))
    }
}
