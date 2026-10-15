// SPDX-License-Identifier: MPL-2.0
//! Host ECM Information page. Guest state is paused, never given synthetic replies.
use crate::lcd::{draw_text_to_buffer as text, fill_rect_stride as fill};
use std::path::PathBuf;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tech2_emu::vcx::{Connection, VinJob, VinResult};

pub struct EcmInfo {
    connection: Connection,
    output: PathBuf,
    job: Option<VinJob>,
    pub visible: bool,
    result: Option<VinResult>,
    error: Option<String>,
    error_at: Option<Instant>,
    started: Instant,
}

pub fn available(screen: &str) -> bool {
    screen.contains("Diagnostics") && screen.contains("Engine") && !screen.contains("Working")
}

impl EcmInfo {
    pub fn new(connection: Connection, output: PathBuf) -> Self {
        Self {
            connection,
            output,
            job: None,
            visible: false,
            result: None,
            error: None,
            error_at: None,
            started: Instant::now(),
        }
    }

    pub fn open(&mut self, insns: u64) {
        self.open_profile(insns, true);
    }

    pub fn open_profile(&mut self, insns: u64, identity: bool) {
        self.visible = true;
        if self.job.is_some() {
            return;
        }
        self.result = None;
        self.error = None;
        self.error_at = None;
        self.started = Instant::now();
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let directory = self
            .output
            .join(format!("vcx-{stamp}-{}", std::process::id()));
        crate::log_info!("J2534", insns, "Host module information requested: protocol={} baud={}; planned TX {:03X}: 1A 90; awaiting helper evidence", self.connection.ecu_profile.protocol(), self.connection.ecu_profile.baud(), self.connection.ecu_profile.request_id());
        match VinJob::start_profile(self.connection.clone(), directory, identity) {
            Ok(job) => self.job = Some(job),
            Err(error) => self.fail(error, insns),
        }
    }

    fn fail(&mut self, error: String, insns: u64) {
        crate::log_error!("J2534", insns, "Host VIN read failed: {error}");
        self.error = Some(error);
        self.error_at = Some(Instant::now());
    }

    pub fn poll(&mut self, insns: u64) {
        let result = self.job.as_mut().and_then(|job| {
            let result = job.poll();
            for line in job.take_logs() {
                crate::log_info!("J2534", insns, "{line}");
            }
            result
        });
        if let Some(result) = result {
            self.job = None;
            match result {
                Ok(result) => {
                    crate::log_info!(
                        "J2534",
                        insns,
                        "LIVE ECU VIN={} origin=host-diagnostic; guest reply injection=false",
                        result.vin
                    );
                    self.result = Some(result);
                    if let Err(error) = self.save() {
                        self.fail(
                            format!("VIN read succeeded, but page capture failed: {error}"),
                            insns,
                        );
                    }
                }
                Err(error) => self.fail(error, insns),
            }
        }
        if self.error_at.is_some_and(|at| at.elapsed().as_secs() >= 5) {
            self.visible = false;
        }
    }

    pub fn close(&mut self) {
        // Keep the bounded worker alive to close its native channel normally.
        // Reopening while it runs reuses the same job; it cannot double-send.
        self.visible = false;
    }

    pub fn draw(&self, pixels: &mut [u32]) {
        let white = 0x00ffffff;
        let muted = 0x00b7d2ee;
        pixels.fill(0x000000aa);
        fill(pixels, 320, 0, 0, 320, 28, 0x00000066);
        let ibus = self.connection.ecu_profile == tech2_emu::vcx::EcuProfile::IbusBcm1367;
        text(
            pixels,
            320,
            if ibus {
                "DIAGNOSTICS / BCM INFORMATION"
            } else {
                "DIAGNOSTICS / ECM INFORMATION"
            },
            8,
            10,
            white,
        );
        text(
            pixels,
            320,
            &format!(
                "{} - live ECU connection",
                self.connection.ecu_profile.name()
            ),
            8,
            39,
            white,
        );
        text(
            pixels,
            320,
            if ibus {
                "VCX Nano / pin 1 / 33.3 kbit/s"
            } else {
                "VCX Nano / CAN 500 kbit/s"
            },
            8,
            55,
            muted,
        );
        if let Some(error) = &self.error {
            text(
                pixels,
                320,
                "READ FAILED - no current VIN",
                8,
                82,
                0x00ffcc66,
            );
            let chars: Vec<char> = error.chars().collect();
            for (row, chunk) in chars.chunks(48).take(8).enumerate() {
                text(
                    pixels,
                    320,
                    &chunk.iter().collect::<String>(),
                    8,
                    103 + row * 10,
                    white,
                );
            }
            let seconds = 5 - self
                .error_at
                .map(|at| at.elapsed().as_secs().min(5))
                .unwrap_or(0);
            text(
                pixels,
                320,
                &format!("Returning to Diagnostics in {seconds}s"),
                8,
                196,
                muted,
            );
        } else if let Some(result) = &self.result {
            let value = |pid| {
                result
                    .identities
                    .iter()
                    .find(|v| v.pid == pid)
                    .map(|v| v.value.as_str())
                    .unwrap_or("Not read")
            };
            let mut fields = vec![
                format!("VIN: {}", result.vin),
                format!("Software: {}", value(0x08)),
                format!("Codefile: {}", value(0x73)),
                format!("ECU SW ID: {}", value(0x95)),
                format!("Calibration: {}", value(0x74)),
                format!(
                    "Adapter FW: {}",
                    result
                        .adapter
                        .as_ref()
                        .and_then(|v| v.firmware.split_whitespace().next())
                        .unwrap_or("Unavailable")
                ),
                result
                    .adapter
                    .as_ref()
                    .map(|v| format!("DLL: {} API: {}", v.dll, v.api))
                    .unwrap_or_default(),
            ];
            if self.connection.ecu_profile == tech2_emu::vcx::EcuProfile::Me96Vehicle1367 {
                fields.splice(
                    1..5,
                    [
                        format!("ECU: {}", value(0x97)),
                        format!("ID 9A: {}", value(0x9a)),
                        format!("ID C1: {}", value(0xc1)),
                        format!("ID C2: {}", value(0xc2)),
                        format!("ID C3: {}", value(0xc3)),
                        format!("ID CB: {}", value(0xcb)),
                    ],
                );
            }
            if ibus {
                fields.splice(
                    1..5,
                    [
                        format!("Module: {}", value(0x97)),
                        format!("ID 9A: {}", value(0x9a)),
                        format!("ID C1: {}", value(0xc1)),
                        "TX 242 / RX 642 / read only".into(),
                    ],
                );
            }
            for (row, field) in fields.iter().enumerate() {
                // Keep externally supplied text inside the scanner viewport.
                let displayed: String = field.chars().take(49).collect();
                text(
                    pixels,
                    320,
                    &displayed,
                    8,
                    78 + row * 13,
                    if row == 0 { white } else { muted },
                );
            }
            text(pixels, 320, "R / I: refresh identification", 8, 199, white);
        } else {
            text(
                pixels,
                320,
                "Reading module identification...",
                8,
                101,
                white,
            );
            text(
                pixels,
                320,
                &format!(
                    "Elapsed: {}s / limit: 35s",
                    self.started.elapsed().as_secs()
                ),
                8,
                123,
                muted,
            );
            text(pixels, 320, "Esc returns immediately", 8, 150, white);
        }
        fill(pixels, 320, 0, 216, 320, 24, 0x00000066);
        text(
            pixels,
            320,
            "Esc: Back | Host diagnostic via J2534",
            8,
            225,
            white,
        );
    }

    fn save(&self) -> std::io::Result<()> {
        let result = self.result.as_ref().unwrap();
        let record = serde_json::json!({
            "ecu_profile": self.connection.ecu_profile.name(),
            "status": "success", "origin": "host-diagnostic", "guest_transport": "pending",
            "identities": result.identities.iter().map(|v| serde_json::json!({
                "pid": format!("{:02X}", v.pid), "raw": v.bytes, "value": v.value
            })).collect::<Vec<_>>(),
            "adapter_version": result.adapter.as_ref().map(|v| serde_json::json!({
                "firmware": v.firmware, "dll": v.dll, "api": v.api
            })),
            "vin": result.vin, "response_id": format!("{:03X}", self.connection.ecu_profile.response_id()), "response_bytes": result.response,
            "protocol": self.connection.ecu_profile.protocol(), "baud": self.connection.ecu_profile.baud(), "log_path": result.log_path,
        });
        std::fs::write(
            result.log_path.with_file_name("ecm-result.json"),
            serde_json::to_vec_pretty(&record)?,
        )?;
        let mut pixels = vec![0; 320 * 240];
        self.draw(&mut pixels);
        let mut bytes = b"P6\n320 240\n255\n".to_vec();
        for pixel in pixels {
            bytes.extend_from_slice(&[(pixel >> 16) as u8, (pixel >> 8) as u8, pixel as u8]);
        }
        let path = self
            .result
            .as_ref()
            .unwrap()
            .log_path
            .with_file_name("ecm-information.ppm");
        std::fs::write(path, bytes)
    }
}

pub fn draw_entry(pixels: &mut [u32]) {
    fill(pixels, 320, 0, 216, 320, 24, 0x001a3d2a);
    text(
        pixels,
        320,
        "F9: ECM Information - live VCX",
        8,
        225,
        0x00ffffff,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn live_entry_is_only_on_idle_diagnostics() {
        assert!(available(
            "Diagnostics\n(4) 2004 Saab 9-3 Sport (9440)\nF0: Engine"
        ));
        assert!(!available("Diagnostics Engine Working"));
        assert!(!available("Main Menu F0: Diagnostics"));
    }
    #[test]
    fn failed_refresh_clears_old_vin_and_returns_after_deadline() {
        let mut page = EcmInfo::new(
            Connection {
                target: "-invalid".into(),
                control_path: None,
                ecu_profile: Default::default(),
            },
            PathBuf::new(),
        );
        page.result = Some(VinResult {
            vin: "YS3FD49Y041000000".into(),
            response: vec![],
            log_path: PathBuf::new(),
            identities: Vec::new(),
            adapter: None,
        });
        page.open(0); // Rejected before starting SSH or touching any adapter.
        assert!(page.visible);
        assert!(page.result.is_none());
        assert!(page.error.is_some());
        assert!(page.job.is_none());
        page.error_at = Some(Instant::now() - std::time::Duration::from_secs(6));
        page.poll(0);
        assert!(!page.visible);
    }
}
