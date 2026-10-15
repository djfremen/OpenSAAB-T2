// SPDX-License-Identifier: MPL-2.0
//! T8 A9/81 DTC record decoding, independent of J2534 and the platform UI.
pub use opensaab_adapter_translation::gm::{Dtc, Report};

/// Independently verify the bounded Windows helper transcript from raw RX bytes.
pub fn parse_log(log: &str) -> Result<Vec<Dtc>, String> {
    if log.len() > 65536 {
        return Err("DTC log exceeds 64 KiB".into());
    }
    for required in [
        "PassThruConnect CAN flags=0 baud=500000 rc=0x00000000",
        "SESSION_STARTED=1",
        "SESSION_STOPPED=1",
        "PassThruDisconnect rc=0x00000000",
        "PassThruClose rc=0x00000000",
    ] {
        if !log.contains(required) {
            return Err(format!("Missing {required}"));
        }
    }
    for required in ["PROBE_RESULT=0", "HELPER_EXIT=0"] {
        if !log.lines().any(|line| line.trim() == required) {
            return Err(format!("Missing {required}"));
        }
    }
    if log
        .lines()
        .filter(|line| line.contains("PassThruStopMsgFilter rc=0x00000000"))
        .count()
        != 2
    {
        return Err("Both raw CAN filters must close successfully".into());
    }
    let mut report = Report::default();
    let (mut active, mut started, mut stopped, mut requested) = (false, false, false, false);
    for line in log.lines() {
        if line.contains("CALL PassThruWriteMsgs")
            && line.ends_with("bytes=00-00-07-E0-03-A9-81-12-00-00-00-00")
        {
            if !started || requested {
                return Err("DTC request outside verified session or duplicate request".into());
            }
            active = true;
            requested = true;
        }
        if line.contains("CALL PassThruWriteMsgs")
            && line.ends_with("bytes=00-00-07-E0-01-20-00-00-00-00-00-00")
        {
            active = false;
        }
        if !line.contains("RX status=0x00000000 protocol=5 ") {
            continue;
        }
        let hex = line.split_once(" bytes=").ok_or("RX has no bytes")?.1;
        let bytes = hex
            .trim()
            .split('-')
            .map(|p| u8::from_str_radix(p, 16))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| "Invalid RX hex")?;
        if bytes.len() < 4 || bytes.len() > 12 {
            return Err("Invalid raw CAN message size".into());
        }
        let id = u32::from_be_bytes(bytes[..4].try_into().unwrap());
        let data = &bytes[4..];
        if id == 0x7e8 && data.starts_with(&[1, 0x50]) && !requested {
            started = true;
        }
        if id == 0x7e8 && data.starts_with(&[1, 0x60]) && requested && !active {
            stopped = true;
        }
        if active {
            report.receive(id, data)?;
        }
    }
    if !requested || !stopped {
        return Err("Missing verified request or stop-session reply".into());
    }
    report.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn transcript_requires_real_end_and_cleanup() {
        let log = "PassThruConnect CAN flags=0 baud=500000 rc=0x00000000\nSESSION_STARTED=1\nRX status=0x00000000 protocol=5 timestamp_us=1 bytes=00-00-07-E8-01-50-00-00-00-00-00-00\nCALL PassThruWriteMsgs bytes=00-00-07-E0-03-A9-81-12-00-00-00-00\nRX status=0x00000000 protocol=5 timestamp_us=2 bytes=00-00-05-E8-81-01-07-00-6F-00-00-00\nRX status=0x00000000 protocol=5 timestamp_us=3 bytes=00-00-05-E8-81-00-00-00-FF-00-00-00\nCALL PassThruWriteMsgs bytes=00-00-07-E0-01-20-00-00-00-00-00-00\nRX status=0x00000000 protocol=5 timestamp_us=4 bytes=00-00-07-E8-01-60-00-00-00-00-00-00\nSESSION_STOPPED=1\nPassThruStopMsgFilter rc=0x00000000\nPassThruStopMsgFilter rc=0x00000000\nPassThruDisconnect rc=0x00000000\nPassThruClose rc=0x00000000\nPROBE_RESULT=0\nHELPER_EXIT=0\n";
        assert_eq!(parse_log(log).unwrap()[0].code, "P0107");
        for broken in [
            log.replace("81-00-00-00-FF", "82-00-00-00-FF"),
            log.replace("PassThruClose rc=0x00000000", "PassThruClose rc=0x00000001"),
            log.replace("status=0x00000000", "status=0x00000009"),
            log.replace("HELPER_EXIT=0", "HELPER_EXIT=1"),
            log.replace("01-60", "01-50"),
        ] {
            assert!(parse_log(&broken).is_err());
        }
    }

    #[test]
    fn pending_and_silence_are_not_empty_success() {
        let mut report = Report::default();
        report
            .receive(0x7e8, &[3, 0x7f, 0xa9, 0x78, 0, 0, 0, 0])
            .unwrap();
        report
            .receive(0x123, &[0x81, 0, 0, 0, 0xff, 0, 0, 0])
            .unwrap();
        assert!(report.finish().is_err());
        assert!(Report::default().receive(0x5e8, &[0x81, 0, 0, 0]).is_err());
        assert!(Report::default()
            .receive(0x7e8, &[3, 0x7f, 0xa9, 0x31])
            .is_err());
    }
    #[test]
    fn records_preserve_status_and_require_explicit_end() {
        let mut report = Report::default();
        report
            .receive(0x5e8, &[0x81, 0xe1, 0x03, 0, 0x6f, 0, 0, 0])
            .unwrap();
        report
            .receive(0x5e8, &[0x81, 0, 0, 0, 0xff, 0, 0, 0])
            .unwrap();
        assert_eq!(
            report.finish().unwrap(),
            vec![Dtc {
                code: "U2103".into(),
                failure_type: 0,
                status: 0x6f
            }]
        );
        let mut empty = Report::default();
        empty
            .receive(0x5e8, &[0x81, 0, 0, 0, 0xff, 0, 0, 0])
            .unwrap();
        assert!(empty.finish().unwrap().is_empty());
    }
}
