// SPDX-License-Identifier: MPL-2.0
//! Private API-stage controls. No application CAN read/write or wake calls.
//! Vendor side effects still require separate wire observation.
use super::{init::Driver, profile};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    OpenOnly,
    DeviceInfo,
    ChannelOnly,
}
impl Stage {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "open-only" => Ok(Self::OpenOnly),
            "device-info" => Ok(Self::DeviceInfo),
            "channel-only" => Ok(Self::ChannelOnly),
            _ => Err("MDI readiness profile must be open-only, device-info or channel-only".into()),
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::OpenOnly => "open-only",
            Self::DeviceInfo => "device-info",
            Self::ChannelOnly => "channel-only",
        }
    }
}
#[derive(Debug)]
pub struct Outcome {
    pub stage: Stage,
    pub completed: bool,
    pub cleanup_ok: bool,
    pub calls: Vec<&'static str>,
    pub version: Option<[String; 3]>,
    pub voltage_mv: Option<u32>,
    pub error: Option<String>,
}
impl Outcome {
    pub fn success(&self) -> bool {
        self.completed && self.cleanup_ok && self.error.is_none()
    }
    pub fn json(&self) -> serde_json::Value {
        serde_json::json!({"schema":1,"adapter":"bosch-mdi-classic",
            "profile":self.stage.name(),"status":if self.success(){"profile_completed"}else{"failed"},
            "completed":self.completed,"cleanup_ok":self.cleanup_ok,"calls":self.calls,
            "version":self.version,"voltage_mv":self.voltage_mv,"error":self.error,
            "application_can_writes":0,"application_can_reads":0,"application_wake_calls":0,
            "implicit_vendor_bus_traffic":"not_assessed","cold_start_qualified":false,
            "driver_sha256":profile::DRIVER_SHA,"runtime_sha256":profile::RUNTIME_SHA,
            "pdu_sha256":profile::PDU_SHA,"process_bits":usize::BITS})
    }
}
pub fn run(driver: &mut impl Driver, stage: Stage) -> Outcome {
    let mut result = Outcome {
        stage,
        completed: false,
        cleanup_ok: true,
        calls: vec![],
        version: None,
        voltage_mv: None,
        error: None,
    };
    let (mut device, mut channel, mut filter) = (None, None, None);
    let operation = (|| -> Result<(), String> {
        result.calls.push("open");
        let d = driver.open()?;
        device = Some(d);
        result.calls.push("version");
        let version = driver.version(d)?;
        if version[1] != "2.5.33.154" || version[2] != "04.04" {
            return Err("MDI driver/API identity differs from selected profile".into());
        }
        result.version = Some(version);
        if stage == Stage::OpenOnly {
            return Ok(());
        }
        result.calls.push("voltage");
        result.voltage_mv = Some(driver.voltage(d)?);
        if stage == Stage::DeviceInfo {
            return Ok(());
        }
        result.calls.push("connect");
        let c = driver.connect(d)?;
        channel = Some(c);
        result.calls.push("filter");
        filter = Some(driver.filter(c)?);
        result.calls.push("clear_rx");
        driver.clear_rx(c)?;
        Ok(())
    })();
    result.completed = operation.is_ok();
    result.error = operation.err();
    let mut errors = vec![];
    if let (Some(c), Some(f)) = (channel, filter) {
        result.calls.push("stop_filter");
        if let Err(e) = driver.stop_filter(c, f) {
            errors.push(e);
        }
    }
    if let Some(c) = channel {
        result.calls.push("disconnect");
        if let Err(e) = driver.disconnect(c) {
            errors.push(e);
        }
    }
    if let Some(d) = device {
        result.calls.push("close");
        if let Err(e) = driver.close(d) {
            errors.push(e);
        }
    }
    if !errors.is_empty() {
        result.cleanup_ok = false;
        result.error = Some(format!(
            "{}; cleanup: {}",
            result.error.unwrap_or_default(),
            errors.join("; ")
        ));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::super::init::Message;
    use super::*;
    struct Fake {
        calls: Vec<&'static str>,
        fail: Option<&'static str>,
        wrong_version: bool,
    }
    impl Fake {
        fn new() -> Self {
            Self {
                calls: vec![],
                fail: None,
                wrong_version: false,
            }
        }
        fn step(&mut self, name: &'static str) -> Result<(), String> {
            self.calls.push(name);
            if self.fail == Some(name) {
                Err(name.into())
            } else {
                Ok(())
            }
        }
    }
    impl Driver for Fake {
        fn open(&mut self) -> Result<u32, String> {
            self.step("open")?;
            Ok(10)
        }
        fn version(&mut self, d: u32) -> Result<[String; 3], String> {
            assert_eq!(d, 10);
            self.step("version")?;
            Ok([
                "8.5.33".into(),
                if self.wrong_version {
                    "changed"
                } else {
                    "2.5.33.154"
                }
                .into(),
                "04.04".into(),
            ])
        }
        fn voltage(&mut self, d: u32) -> Result<u32, String> {
            assert_eq!(d, 10);
            self.step("voltage")?;
            Ok(14118)
        }
        fn connect(&mut self, d: u32) -> Result<u32, String> {
            assert_eq!(d, 10);
            self.step("connect")?;
            Ok(20)
        }
        fn filter(&mut self, c: u32) -> Result<u32, String> {
            assert_eq!(c, 20);
            self.step("filter")?;
            Ok(30)
        }
        fn clear_rx(&mut self, c: u32) -> Result<(), String> {
            assert_eq!(c, 20);
            self.step("clear_rx")
        }
        fn write(&mut self, _: u32, _: &mut Message) -> Result<u32, String> {
            panic!("readiness profile must not transmit")
        }
        fn read(&mut self, _: u32) -> Result<Option<Message>, String> {
            panic!("readiness profile must not read CAN")
        }
        fn stop_filter(&mut self, c: u32, f: u32) -> Result<(), String> {
            assert_eq!((c, f), (20, 30));
            self.step("stop_filter")
        }
        fn disconnect(&mut self, c: u32) -> Result<(), String> {
            assert_eq!(c, 20);
            self.step("disconnect")
        }
        fn close(&mut self, d: u32) -> Result<(), String> {
            assert_eq!(d, 10);
            self.step("close")
        }
    }
    #[test]
    fn distinct_stage_boundaries_never_read_or_write_can() {
        for (stage, expected) in [
            (Stage::OpenOnly, vec!["open", "version", "close"]),
            (
                Stage::DeviceInfo,
                vec!["open", "version", "voltage", "close"],
            ),
            (
                Stage::ChannelOnly,
                vec![
                    "open",
                    "version",
                    "voltage",
                    "connect",
                    "filter",
                    "clear_rx",
                    "stop_filter",
                    "disconnect",
                    "close",
                ],
            ),
        ] {
            let mut d = Fake::new();
            let out = run(&mut d, stage);
            assert!(out.success());
            assert_eq!(d.calls, expected);
            assert_eq!(out.calls, d.calls);
            assert_eq!(out.json()["cold_start_qualified"], false);
            assert_eq!(out.json()["implicit_vendor_bus_traffic"], "not_assessed");
        }
    }
    #[test]
    fn every_acquisition_failure_closes_only_acquired_resources() {
        for (fail, releases) in [
            ("open", vec![]),
            ("version", vec!["close"]),
            ("voltage", vec!["close"]),
            ("connect", vec!["close"]),
            ("filter", vec!["disconnect", "close"]),
            ("clear_rx", vec!["stop_filter", "disconnect", "close"]),
        ] {
            let mut d = Fake::new();
            d.fail = Some(fail);
            let out = run(&mut d, Stage::ChannelOnly);
            assert!(!out.success());
            assert!(!out.completed);
            assert!(out.cleanup_ok);
            let tail = d.calls.iter().position(|x| *x == fail).unwrap() + 1;
            assert_eq!(d.calls[tail..], releases);
            assert_eq!(out.calls, d.calls);
        }
    }
    #[test]
    fn uncertain_release_is_not_retried_and_remaining_resources_close() {
        for fail in ["stop_filter", "disconnect", "close"] {
            let mut d = Fake::new();
            d.fail = Some(fail);
            let out = run(&mut d, Stage::ChannelOnly);
            assert!(out.completed);
            assert!(!out.cleanup_ok);
            assert!(!out.success());
            assert_eq!(d.calls.iter().filter(|x| **x == fail).count(), 1);
            assert_eq!(
                &d.calls[d.calls.len() - 3..],
                ["stop_filter", "disconnect", "close"]
            );
        }
    }
    #[test]
    fn changed_driver_identity_stops_before_voltage_or_channel_setup() {
        let mut d = Fake::new();
        d.wrong_version = true;
        let out = run(&mut d, Stage::ChannelOnly);
        assert!(!out.success());
        assert_eq!(d.calls, ["open", "version", "close"]);
        for bad in ["", "vin", "wake", "channel-only --vin"] {
            assert!(Stage::parse(bad).is_err());
        }
    }
}
