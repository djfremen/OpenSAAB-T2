// SPDX-License-Identifier: MPL-2.0
//! Per-run, buffered observation trace. No guest reads or transport side effects.
use crate::artifacts::json_string as q;
use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};

const TRACE_SEGMENT_BYTES: u64 = 32 * 1024 * 1024;
const TRACE_BACKUPS: usize = 3;
const MAX_RECORD_BYTES: usize = 64 * 1024;

// Observation-only detail: retain actual CAN frames, complete CANdi messages,
// requests, navigation, errors and timeouts in the normal transport trace.
// --verbose / --trace-calls restores these register and guest-internal events.
fn low_level_detail(kind: &str, detail: &str) -> bool {
    match kind {
        "guest_os_call"
        | "guest_dispatch_entry"
        | "guest_message_arg2"
        | "guest_communication_boundary"
        | "guest_communication_path"
        | "guest_timer_suspend" => true,
        "cs6_write" => detail.contains("backend=raw-register-write"),
        "candi_uart_timer" => detail.starts_with("host_service="),
        "candi_native_link" => [
            "CAN register write ",
            "CAN transceiver GPIO write ",
            "UART write ",
            "UART receive read ",
            "Tech2->CANdi bytes=",
            "CANdi->Tech2 events=",
        ]
        .iter()
        .any(|prefix| detail.starts_with(prefix)),
        _ => false,
    }
}

#[derive(Default)]
pub struct Trace {
    writer: Option<BufWriter<File>>,
    error: Option<String>,
    path: Option<PathBuf>,
    segment_bytes: u64,
    segment_limit: u64,
    pub calls: bool,
    pub navigation: u64,
    sequence: u64,
    last_screen: Option<String>,
    last_navigation_state: Option<(u16, u8)>,
    live_console: bool,
    concise: bool,
    last_console_screen: String,
    last_highlight: Option<String>,
}
impl Trace {
    pub fn open(path: &Path, calls: bool) -> io::Result<Self> {
        Self::open_with_limit(path, calls, TRACE_SEGMENT_BYTES)
    }
    fn open_with_limit(path: &Path, calls: bool, segment_limit: u64) -> io::Result<Self> {
        assert!(segment_limit >= 512);
        Ok(Self {
            writer: Some(BufWriter::new(File::create(path)?)),
            path: Some(path.to_owned()),
            segment_limit,
            calls,
            ..Self::default()
        })
    }
    pub fn concise_transport(&mut self) {
        self.concise = true;
    }
    pub fn enabled(&self) -> bool {
        (self.writer.is_some() || self.live_console) && self.error.is_none()
    }
    pub fn enable_console(&mut self) {
        self.live_console = true;
        self.last_screen = None;
        self.last_console_screen.clear();
        self.last_highlight = None;
    }
    pub fn event(&mut self, insns: u64, pc: u32, kind: &str, detail: &str) {
        if !self.enabled() || (self.concise && low_level_detail(kind, detail)) {
            return;
        }
        if let Some((level, subsystem, message)) = crate::event_console::summarize(kind, detail) {
            if let Ok(mut logger) = crate::logger::get_logger().lock() {
                logger.log(level, subsystem, insns, &message);
            }
        }
        if self.writer.is_none() {
            return;
        }
        self.sequence += 1;
        let mut record = format!(
            "{{\"seq\":{},\"insns\":{},\"pc\":\"0x{:08x}\",\"navigation\":{},\"kind\":{},\"detail\":{}}}\n",
            self.sequence, insns, pc, self.navigation, q(kind), q(detail));
        let record_limit = if self.path.is_some() {
            MAX_RECORD_BYTES.min(self.segment_limit as usize)
        } else {
            MAX_RECORD_BYTES
        };
        if record.len() > record_limit {
            // Keep valid JSON and mark the gap; never silently clip a CAN payload.
            record = format!(
                "{{\"seq\":{},\"insns\":{},\"pc\":\"0x{:08x}\",\"navigation\":{},\"kind\":\"trace_record_omitted\",\"detail\":\"record exceeded {} bytes; original encoded length={}\"}}\n",
                self.sequence, insns, pc, self.navigation, record_limit, record.len());
        }
        if let Err(e) = self.write_record(record.as_bytes()) {
            self.error = Some(e.to_string());
        }
    }
    fn write_record(&mut self, record: &[u8]) -> io::Result<()> {
        if self.path.is_some() && self.segment_bytes + record.len() as u64 > self.segment_limit {
            self.rotate()?;
        }
        if let Some(writer) = &mut self.writer {
            writer.write_all(record)?;
            self.segment_bytes += record.len() as u64;
        }
        Ok(())
    }
    fn rotated_path(path: &Path, index: usize) -> PathBuf {
        let mut name = path.as_os_str().to_owned();
        name.push(format!(".{index}"));
        PathBuf::from(name)
    }
    fn rotate(&mut self) -> io::Result<()> {
        let path = self.path.as_ref().expect("rotation requires a file path");
        if let Some(mut writer) = self.writer.take() {
            writer.flush()?;
        }
        // Close before renaming for Windows as well as macOS/Android.
        match std::fs::remove_file(Self::rotated_path(path, TRACE_BACKUPS)) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        for index in (1..TRACE_BACKUPS).rev() {
            match std::fs::rename(
                Self::rotated_path(path, index),
                Self::rotated_path(path, index + 1),
            ) {
                Ok(()) => {}
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(e),
            }
        }
        std::fs::rename(path, Self::rotated_path(path, 1))?;
        self.writer = Some(BufWriter::new(File::create(path)?));
        self.segment_bytes = 0;
        // Rotation is normal retention, not a reason to interrupt diagnostics.
        Ok(())
    }
    pub fn screen(&mut self, insns: u64, pc: u32, screen: String) {
        if self.enabled() && self.last_screen.as_ref() != Some(&screen) {
            let summary = crate::event_console::screen_summary(&screen);
            if !summary.is_empty() && summary != self.last_console_screen {
                crate::log_info!("SCREEN", insns, "{}", summary);
                self.last_console_screen = summary;
            }
            self.event(insns, pc, "screen", &screen);
            self.last_screen = Some(screen);
        }
    }
    pub fn highlight(&mut self, insns: u64, pc: u32, selected: Option<String>) {
        if self.enabled() && self.last_highlight != selected {
            if let Some(text) = &selected {
                self.event(insns, pc, "menu_highlight", text);
            }
            self.last_highlight = selected;
        }
    }
    pub fn navigation_state(&mut self, insns: u64, pc: u32, menu: u16, latch: u8) {
        if self.enabled() && self.last_navigation_state != Some((menu, latch)) {
            self.event(
                insns,
                pc,
                "navigation_state",
                &format!("menu_index={menu} key_latch={latch:#04x} source=guest-ram"),
            );
            self.last_navigation_state = Some((menu, latch));
        }
    }
    pub fn check(&mut self) -> Result<(), String> {
        if self.error.is_none() {
            if let Some(writer) = &mut self.writer {
                if let Err(e) = writer.flush() {
                    self.error = Some(e.to_string());
                }
            }
        }
        match &self.error {
            Some(e) => Err(e.clone()),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rolling_trace_is_bounded_and_retains_ordered_complete_records() {
        let dir = std::env::temp_dir().join(format!("tech2-trace-rotation-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("trace.jsonl");
        let mut trace = Trace::open_with_limit(&path, false, 512).unwrap();
        for i in 0..200 {
            trace.event(i, 0, "test", &format!("event {i}: {}", "x".repeat(70)));
        }
        trace.check().unwrap();
        assert!(trace.enabled());
        assert_eq!(trace.sequence, 200);
        let mut sequences = Vec::new();
        let mut bytes = 0;
        for i in (0..=TRACE_BACKUPS).rev() {
            let segment = if i == 0 {
                path.clone()
            } else {
                Trace::rotated_path(&path, i)
            };
            let data = std::fs::read_to_string(&segment).unwrap();
            assert!(data.len() <= 512);
            assert!(data.ends_with('\n'));
            bytes += data.len();
            for line in data.lines() {
                let json: serde_json::Value = serde_json::from_str(line).unwrap();
                sequences.push(json["seq"].as_u64().unwrap());
            }
        }
        assert!(bytes <= 4 * 512);
        assert!(sequences[0] > 1); // old history has been pruned
        assert_eq!(sequences.last(), Some(&200));
        assert!(sequences.windows(2).all(|pair| pair[1] == pair[0] + 1));
        assert!(!Trace::rotated_path(&path, 4).exists());
        drop(trace);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn oversized_trace_record_is_explicitly_omitted_without_exceeding_cap() {
        let path = std::env::temp_dir().join(format!(
            "tech2-trace-oversized-{}.jsonl",
            std::process::id()
        ));
        let mut trace = Trace::open_with_limit(&path, false, 512).unwrap();
        trace.event(1, 0, "large-detail", &"\0".repeat(600));
        trace.check().unwrap();
        let data = std::fs::read_to_string(&path).unwrap();
        assert!(data.len() <= 512);
        let json: serde_json::Value = serde_json::from_str(&data).unwrap();
        assert_eq!(json["kind"], "trace_record_omitted");
        assert!(trace.enabled());
        drop(trace);
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn failed_rotation_is_reported_and_does_not_resume_writing() {
        let dir =
            std::env::temp_dir().join(format!("tech2-trace-rotation-error-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("trace.jsonl");
        std::fs::create_dir(Trace::rotated_path(&path, TRACE_BACKUPS)).unwrap();
        let mut trace = Trace::open_with_limit(&path, false, 512).unwrap();
        for i in 0..10 {
            trace.event(i, 0, "test", &"x".repeat(100));
        }
        let error = trace.check().unwrap_err();
        assert!(!trace.enabled());
        assert!(std::fs::metadata(&path).unwrap().len() <= 512);
        trace.event(11, 0, "test", "must not resume");
        assert_eq!(trace.check().unwrap_err(), error);
        drop(trace);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn concise_trace_preserves_protocol_and_failure_evidence() {
        let path = std::env::temp_dir().join(format!("tech2-trace-{}.jsonl", std::process::id()));
        let mut trace = Trace::open(&path, false).unwrap();
        trace.concise_transport();
        for (kind, detail) in [
            ("candi_native_link", "CAN register write address=0x1"),
            ("candi_native_link", "UART receive read event=Some(00)"),
            ("candi_native_link", "Tech2->CANdi bytes=[2e,d2]"),
            ("candi_uart_timer", "host_service=0xff"),
            ("cs6_write", "backend=raw-register-write"),
            ("guest_message_arg2", "argument dump"),
        ] {
            trace.event(1, 0, kind, detail);
        }
        assert_eq!(trace.sequence, 0);
        let essential = [
            (
                "candi_native_link",
                "NANO RX controller=2 id=541 data=[81,B9]",
            ),
            ("candi_native_link", "CAN TX intent id=241"),
            ("candi_native_link", "FRAME Tech2 -> CANdi checksum=ok"),
            ("candi_native_link", "FRAME CANdi -> Tech2 checksum=bad"),
            ("candi_native_link", "controller error overflow"),
            ("candi_uart_timer", "expired channel=5"),
            ("candi_uart_timer", "unexpected future error"),
            ("guest_request", "request=read-dtc"),
            ("native_operator_key", "encoder=0x0c"),
            ("screen", "Vehicle Identification"),
        ];
        for (kind, detail) in essential {
            trace.event(2, 0, kind, detail);
        }
        assert_eq!(trace.sequence, essential.len() as u64);
        trace.check().unwrap();
        let output = std::fs::read_to_string(&path).unwrap();
        for (_, detail) in essential {
            assert!(output.contains(detail));
        }
        assert!(!output.contains("register write"));
        trace.concise = false;
        trace.event(
            3,
            0,
            "candi_native_link",
            "CAN register write verbose restored",
        );
        trace.check().unwrap();
        assert!(std::fs::read_to_string(&path)
            .unwrap()
            .contains("verbose restored"));
        drop(trace);
        std::fs::remove_file(path).unwrap();
    }
    #[cfg(feature = "gui")]
    #[test]
    fn live_console_observes_without_requiring_verbose_file_output() {
        let mut trace = Trace::default();
        assert!(!trace.enabled());
        trace.enable_console();
        assert!(trace.enabled());
        trace.event(
            1,
            0x1dbbfc,
            "guest_queue_submit",
            "queue=0x8 envelope_words=[1,2,3,4]",
        );
        assert!(trace.writer.is_none());
        assert!(trace.check().is_ok());
    }
    #[test]
    fn buffered_write_failure_is_sticky_and_observable() {
        // A read-only file descriptor permits buffering but rejects flush.
        let mut trace = Trace {
            writer: Some(BufWriter::new(File::open(file!()).unwrap())),
            ..Trace::default()
        };
        trace.event(1, 0x500, "test", "buffered");
        let error = trace.check().unwrap_err();
        assert!(!trace.enabled());
        trace.event(2, 0x502, "test", "cannot hide the error");
        assert_eq!(trace.check().unwrap_err(), error);
    }
}
