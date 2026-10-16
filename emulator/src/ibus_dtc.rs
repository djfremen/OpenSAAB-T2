// SPDX-License-Identifier: MPL-2.0
//! VIN-guarded body DTC reads through the Windows bridge. Host diagnostic origin.
//! The decoder is independent of the Windows DLL; this is not guest CANdi traffic.
use crate::{
    t8_dtc::{Dtc, Report},
    vcx::{run_logged_command, Connection},
};
use std::{
    fs,
    path::Path,
    sync::{atomic::AtomicBool, mpsc},
    time::Duration,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Module {
    Bcm,
    Cim,
}
impl Module {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s.to_ascii_lowercase().as_str() {
            "bcm" => Ok(Self::Bcm),
            "cim" => Ok(Self::Cim),
            _ => Err("DTC module must be bcm or cim".into()),
        }
    }
    pub fn name(self) -> &'static str {
        if self == Self::Bcm {
            "BCM"
        } else {
            "CIM"
        }
    }
    fn tx(self) -> u32 {
        if self == Self::Bcm {
            0x242
        } else {
            0x241
        }
    }
}

/// Reconstruct the report from raw RX, including VIN/name guards and report end.
pub fn parse_log(log: &str, module: Module) -> Result<Vec<Dtc>, String> {
    if log.len() > 65536 {
        return Err("DTC log exceeds 64 KiB".into());
    }
    for required in [
        "PassThruConnect SW_ISO15765_PS baud=33333 rc=0x00000000",
        "PassThruConnect SW_CAN_PS baud=33333 rc=0x00000000",
        "PassThruStopMsgFilter raw rc=0x00000000",
        "PassThruStopMsgFilter iso rc=0x00000000",
        "PassThruDisconnect raw rc=0x00000000",
        "PassThruDisconnect iso rc=0x00000000",
        "PassThruClose rc=0x00000000",
    ] {
        if !log.lines().any(|l| l.ends_with(required)) {
            return Err(format!("Missing {required}"));
        }
    }
    for required in ["PROBE_RESULT=0", "HELPER_EXIT=0"] {
        if !log.lines().any(|l| l.trim() == required) {
            return Err(format!("Missing {required}"));
        }
    }
    let mut report = Report::default();
    let (mut stage, mut vin, mut name, mut normal) = (0, false, false, false);
    let expected = ["1A-90", "1A-97", "20", "A9-81-12"];
    for line in log.lines() {
        if let Some((_, request)) = line.split_once("REQUEST module=") {
            let want = format!(
                "{} CAN={:03X} payload={}",
                module.name(),
                module.tx(),
                expected.get(stage).unwrap_or(&"INVALID")
            );
            if request != want || stage == 1 && !vin || stage == 2 && !name || stage == 3 && !normal
            {
                return Err(
                    "Unexpected DTC request order, target, or unverified identity/session".into(),
                );
            }
            stage += 1;
        }
        let Some((_, rx)) = line.split_once("RX status=0x") else {
            continue;
        };
        let status =
            u32::from_str_radix(rx.split_whitespace().next().ok_or("Missing RX status")?, 16)
                .map_err(|_| "Invalid status")?;
        // Ignore TX indications and incomplete ISO messages; accept J2534 addressing flags.
        if status & !0x70000 != 0 {
            continue;
        }
        let protocol: u32 = rx
            .split_once("protocol=")
            .ok_or("Missing protocol")?
            .1
            .split_whitespace()
            .next()
            .ok_or("Missing protocol value")?
            .parse()
            .map_err(|_| "Invalid protocol")?;
        let hex = rx.split_once(" bytes=").ok_or("Missing RX bytes")?.1;
        let bytes = hex
            .trim()
            .split('-')
            .map(|v| u8::from_str_radix(v, 16))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| "Invalid RX hex")?;
        if bytes.len() < 5 {
            return Err("Truncated receive message".into());
        }
        let id = u32::from_be_bytes(bytes[..4].try_into().unwrap());
        let data = &bytes[4..];
        if protocol == 0x8007 && id == module.tx() + 0x400 {
            if data.starts_with(&[0x7f]) {
                if data.len() != 3 || data[2] != 0x78 {
                    return Err(format!("Module rejected diagnostic request: {hex}"));
                }
                continue;
            }
            if stage == 1 && data.starts_with(&[0x5a, 0x90]) {
                if &data[2..] != b"YS3FH46U681000002" {
                    return Err("VIN guard failed".into());
                }
                vin = true;
            }
            if stage == 2 && data.starts_with(&[0x5a, 0x97]) {
                if String::from_utf8_lossy(&data[2..]).trim() != module.name() {
                    return Err("Module guard failed".into());
                }
                name = true;
            }
            if stage == 3 && data == [0x60] {
                normal = true;
            }
        }
        if stage == 4 && protocol == 0x8008 {
            report.receive_for_ids(id, data, module.tx() + 0x400, module.tx() + 0x300)?;
        }
    }
    if stage != 4 || !vin || !name || !normal {
        return Err("Missing verified identity or DTC request".into());
    }
    report.finish()
}

/// Explicit headless host operation in tech2-emu; preserves all transmitted/received bytes.
pub fn run(connection: Connection, module: Module, directory: &Path) -> Result<(), String> {
    connection.validate()?;
    fs::create_dir(directory).map_err(|e| format!("Create fresh DTC directory: {e}"))?;
    // Only an enum-selected module is interpolated, never arbitrary input.
    let script = format!("$env:VCX_DTC_MODULE='{}'; & ([scriptblock]::Create((Get-Content \"$env:USERPROFILE\\invoke_vcx_t8_vin.ps1\" -Raw))) -ProbePath \"$env:USERPROFILE\\vcx_ibus_1367_dtc.ps1\" -Profile Identity -TimeoutSeconds 25", module.name());
    let command = connection.script_command(&script)?;
    println!(
        "DTC_ORIGIN=host-diagnostic native_guest_transport=false MODULE={}",
        module.name()
    );
    let (tx, rx) = mpsc::channel();
    let path = std::thread::scope(|scope| {
        let printer = scope.spawn(move || {
            for line in rx {
                println!("{line}");
            }
        });
        let result = run_logged_command(
            command,
            directory,
            &AtomicBool::new(false),
            Duration::from_secs(35),
            &tx,
        );
        drop(tx);
        let _ = printer.join();
        result
    })?;
    let codes = parse_log(
        &fs::read_to_string(path).map_err(|e| e.to_string())?,
        module,
    )?;
    let records = codes
        .iter()
        .map(|d| {
            format!(
                "{{\"code\":\"{}\",\"failure_type\":\"{:02X}\",\"status\":\"{:02X}\"}}",
                d.code, d.failure_type, d.status
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let json = format!("{{\n  \"origin\":\"host-diagnostic\",\n  \"native_guest_transport\":false,\n  \"vin\":\"YS3FH46U681000002\",\n  \"module\":\"{}\",\n  \"complete\":true,\n  \"cleared\":false,\n  \"records\":[{records}]\n}}\n",module.name());
    fs::write(directory.join("dtc-result.json"), json).map_err(|e| e.to_string())?;
    println!(
        "VERIFIED {} DTC count={} complete=true cleared=false",
        module.name(),
        codes.len()
    );
    for d in codes {
        println!(
            "{} {}-{:02X} status={:02X}",
            module.name(),
            d.code,
            d.failure_type,
            d.status
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(m: Module) -> String {
        let mut s = String::new();
        for l in [
            "PassThruConnect SW_ISO15765_PS baud=33333",
            "PassThruConnect SW_CAN_PS baud=33333",
            "PassThruStopMsgFilter raw",
            "PassThruStopMsgFilter iso",
            "PassThruDisconnect raw",
            "PassThruDisconnect iso",
            "PassThruClose",
        ] {
            s += &format!("{l} rc=0x00000000\n");
        }
        for (request, data) in [
            ("1A-90", [&[0x5a, 0x90][..], b"YS3FH46U681000002"].concat()),
            ("1A-97", [&[0x5a, 0x97][..], m.name().as_bytes()].concat()),
            ("20", vec![0x60]),
        ] {
            s += &format!(
                "REQUEST module={} CAN={:03X} payload={request}\n",
                m.name(),
                m.tx()
            );
            let bytes = [m.tx().wrapping_add(0x400).to_be_bytes().as_slice(), &data].concat();
            s += &format!(
                "RX status=0x00000000 protocol=32775 timestamp_us=1 bytes={}\n",
                bytes
                    .iter()
                    .map(|b| format!("{b:02X}"))
                    .collect::<Vec<_>>()
                    .join("-")
            );
        }
        s += &format!(
            "REQUEST module={} CAN={:03X} payload=A9-81-12\n",
            m.name(),
            m.tx()
        );
        for data in ["81-E1-03-00-6F", "81-00-00-00-FF"] {
            s += &format!(
                "RX status=0x00000000 protocol=32776 timestamp_us=2 bytes=00-00-05-{:02X}-{data}\n",
                m.tx() & 255
            );
        }
        s + "PROBE_RESULT=0\nHELPER_EXIT=0\n"
    }
    #[test]
    fn verifies_both_addresses_and_preserves_codes() {
        for m in [Module::Bcm, Module::Cim] {
            let s = fixture(m);
            assert_eq!(
                parse_log(&s, m).unwrap(),
                vec![Dtc {
                    code: "U2103".into(),
                    failure_type: 0,
                    status: 0x6f
                }]
            );
            for bad in [
                s.replace("81-00-00-00-FF", "82-00-00-00-FF"),
                s.replace("00-00-05-", "00-00-04-"),
                s.replace("A9-81-12", "04"),
                s.replace("status=0x00000000", "status=0x00000009"),
                s.replace("HELPER_EXIT=0", "HELPER_EXIT=1"),
                s.replace("PassThruClose rc=0x00000000", "PassThruClose rc=0x00000001"),
                s.replace("30-30-32", "30-30-33"),
            ] {
                assert!(parse_log(&bad, m).is_err(), "{bad}");
            }
        }
    }
    #[test]
    fn only_end_marker_proves_an_empty_report() {
        let s = fixture(Module::Cim);
        let empty = s
            .lines()
            .filter(|l| !l.contains("81-E1-03"))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(parse_log(&empty, Module::Cim).unwrap().is_empty());
        assert!(parse_log(&empty.replace("81-00-00-00-FF", "81-00-00"), Module::Cim).is_err());
        let rejected = empty.replace("REQUEST module=CIM CAN=241 payload=A9-81-12", "REQUEST module=CIM CAN=241 payload=A9-81-12\nRX status=0x00000000 protocol=32775 timestamp_us=1 bytes=00-00-06-41-7F-A9-31");
        assert!(parse_log(&rejected, Module::Cim).is_err());
    }
}
