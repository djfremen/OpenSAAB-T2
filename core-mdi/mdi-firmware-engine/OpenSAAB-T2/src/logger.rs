// SPDX-License-Identifier: MPL-2.0
//! Centralized Diagnostic Logging & Crash Reporting Engine.
//!
//! Provides ring-buffered, thread-safe logging with severity levels (INFO, WARN, ERROR, CRASH),
//! file persistence to `tech2.log`, headless console streaming, and live rendering for the attached GUI console.

use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum LogLevel {
    Info,
    Warn,
    Error,
    Crash,
}

impl LogLevel {
    pub fn tag(&self) -> &'static str {
        match self {
            LogLevel::Info => "INFO",
            LogLevel::Warn => "WARN",
            LogLevel::Error => "ERROR",
            LogLevel::Crash => "CRASH",
        }
    }

    pub fn ansi_prefix(&self) -> &'static str {
        match self {
            LogLevel::Info => "\x1b[36m",    // Cyan
            LogLevel::Warn => "\x1b[33m",    // Yellow
            LogLevel::Error => "\x1b[31m",   // Red
            LogLevel::Crash => "\x1b[35;1m", // Bold Magenta
        }
    }

    pub fn color_u32(&self) -> u32 {
        match self {
            LogLevel::Info => 0x0058_A6FF,  // Terminal cyan/blue
            LogLevel::Warn => 0x00D2_9922,  // Amber
            LogLevel::Error => 0x00F8_5149, // Coral red
            LogLevel::Crash => 0x00FF_7B72, // Bright red/magenta
        }
    }
}

#[derive(Clone, Debug)]
pub struct LogEntry {
    pub time_ms: u64,
    pub insns: u64,
    pub level: LogLevel,
    pub subsystem: &'static str,
    pub message: String,
}

pub struct Tech2Logger {
    start_time: Instant,
    buffer: VecDeque<LogEntry>,
    link_buffer: VecDeque<LogEntry>,
    max_capacity: usize,
    pub total_errors: u32,
    pub total_warnings: u32,
    pub total_crashes: u32,
    log_file: Option<File>,
    file_error: Option<String>,
    pub print_stdout: bool,
}

impl Tech2Logger {
    pub fn new() -> Self {
        Self {
            start_time: Instant::now(),
            buffer: VecDeque::with_capacity(512),
            link_buffer: VecDeque::with_capacity(128),
            max_capacity: 512,
            total_errors: 0,
            total_warnings: 0,
            total_crashes: 0,
            log_file: None,
            file_error: None,
            print_stdout: true,
        }
    }

    pub fn log(&mut self, level: LogLevel, subsystem: &'static str, insns: u64, msg: &str) {
        let elapsed_ms = self.start_time.elapsed().as_millis() as u64;

        match level {
            LogLevel::Error => self.total_errors = self.total_errors.saturating_add(1),
            LogLevel::Warn => self.total_warnings = self.total_warnings.saturating_add(1),
            LogLevel::Crash => self.total_crashes = self.total_crashes.saturating_add(1),
            LogLevel::Info => {}
        }

        let entry = LogEntry {
            time_ms: elapsed_ms,
            insns,
            level,
            subsystem,
            message: msg.to_string(),
        };

        if self.buffer.len() >= self.max_capacity {
            self.buffer.pop_front();
        }
        if subsystem == "CANDI" {
            if self.link_buffer.len() >= 128 {
                self.link_buffer.pop_front();
            }
            self.link_buffer.push_back(entry.clone());
        }
        self.buffer.push_back(entry);

        // Persistent file write
        if let Some(f) = &mut self.log_file {
            let secs = elapsed_ms as f64 / 1000.0;
            if let Err(error) = writeln!(
                f,
                "[{:8.3}s | {:10}] [{:5}] [{:6}] {}",
                secs,
                insns,
                level.tag(),
                subsystem,
                msg
            ) {
                self.file_error = Some(error.to_string());
            }
        }

        // Terminal output in headless mode or if enabled
        if self.print_stdout {
            let secs = elapsed_ms as f64 / 1000.0;
            println!(
                "{}[{:7.3}s | {:5}] [{:5}] {}\x1b[0m",
                level.ansi_prefix(),
                secs,
                subsystem,
                level.tag(),
                msg
            );
        }
    }

    /// Retrieve formatted text lines for the GUI console view.
    /// Each entry returns (color, line_string). Long lines wrap within max_chars.
    pub fn get_console_lines(&self, max_chars: usize, max_lines: usize) -> Vec<(u32, String)> {
        self.console_lines(max_chars, max_lines, &self.buffer)
    }

    pub fn get_link_console_lines(&self, max_chars: usize, max_lines: usize) -> Vec<(u32, String)> {
        self.console_lines(max_chars, max_lines, &self.link_buffer)
    }

    fn console_lines(
        &self,
        max_chars: usize,
        max_lines: usize,
        entries: &VecDeque<LogEntry>,
    ) -> Vec<(u32, String)> {
        let mut lines = Vec::new();
        if max_chars == 0 || max_lines == 0 {
            return lines;
        }

        for entry in entries.iter().rev() {
            let prefix = format!(
                "[{:02}.{:02}s][{}] ",
                entry.time_ms / 1000,
                (entry.time_ms % 1000) / 10,
                entry.subsystem
            );
            let full_msg = format!("{}{}", prefix, entry.message);
            let color = entry.level.color_u32();

            // Word wrap / chunk into max_chars
            let mut remaining = full_msg.as_str();
            let mut msg_lines = Vec::new();
            while !remaining.is_empty() {
                let Some((boundary, _)) = remaining.char_indices().nth(max_chars) else {
                    msg_lines.push((color, remaining.to_string()));
                    break;
                };
                let split_idx = remaining[..boundary]
                    .rfind(' ')
                    .map(|i| i + 1)
                    .unwrap_or(boundary);
                msg_lines.push((color, remaining[..split_idx].to_string()));
                remaining = &remaining[split_idx..];
            }

            for line in msg_lines.into_iter().rev() {
                lines.push(line);
                if lines.len() >= max_lines {
                    break;
                }
            }
            if lines.len() >= max_lines {
                break;
            }
        }

        lines.reverse();
        lines
    }

    pub fn stats(&self) -> (u32, u32, u32) {
        (self.total_errors, self.total_warnings, self.total_crashes)
    }
}

impl Default for Tech2Logger {
    fn default() -> Self {
        Self::new()
    }
}

pub fn init_file(path: &std::path::Path) -> std::io::Result<()> {
    let file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(path)?;
    let mut logger = get_logger()
        .lock()
        .map_err(|_| std::io::Error::other("logger lock poisoned"))?;
    // A restart starts a fresh console as well as a fresh log file.
    *logger = Tech2Logger::new();
    logger.log_file = Some(file);
    logger.file_error = None;
    Ok(())
}

pub fn check_file() -> std::io::Result<()> {
    let mut logger = get_logger()
        .lock()
        .map_err(|_| std::io::Error::other("logger lock poisoned"))?;
    if let Some(error) = &logger.file_error {
        return Err(std::io::Error::other(error.clone()));
    }
    if let Some(file) = &mut logger.log_file {
        file.flush()?;
    }
    Ok(())
}

static GLOBAL_LOGGER: OnceLock<Mutex<Tech2Logger>> = OnceLock::new();

pub fn get_logger() -> &'static Mutex<Tech2Logger> {
    GLOBAL_LOGGER.get_or_init(|| Mutex::new(Tech2Logger::new()))
}

#[macro_export]
macro_rules! log_info {
    ($subsystem:expr, $insns:expr, $($arg:tt)*) => {
        if let Ok(mut l) = $crate::logger::get_logger().lock() {
            l.log($crate::logger::LogLevel::Info, $subsystem, $insns, &format!($($arg)*));
        }
    };
}

#[macro_export]
macro_rules! log_warn {
    ($subsystem:expr, $insns:expr, $($arg:tt)*) => {
        if let Ok(mut l) = $crate::logger::get_logger().lock() {
            l.log($crate::logger::LogLevel::Warn, $subsystem, $insns, &format!($($arg)*));
        }
    };
}

#[macro_export]
macro_rules! log_error {
    ($subsystem:expr, $insns:expr, $($arg:tt)*) => {
        if let Ok(mut l) = $crate::logger::get_logger().lock() {
            l.log($crate::logger::LogLevel::Error, $subsystem, $insns, &format!($($arg)*));
        }
    };
}

#[macro_export]
macro_rules! log_crash {
    ($subsystem:expr, $insns:expr, $($arg:tt)*) => {
        if let Ok(mut l) = $crate::logger::get_logger().lock() {
            l.log($crate::logger::LogLevel::Crash, $subsystem, $insns, &format!($($arg)*));
        }
    };
}

/// Generate a full diagnostic crash report and record it to the logger.
#[allow(clippy::too_many_arguments)] // Full register snapshot plus ordered execution context.
pub fn record_crash_report(
    subsystem: &'static str,
    insns: u64,
    fault_type: &str,
    pc: u32,
    sp: u32,
    d_regs: &[u32; 8],
    a_regs: &[u32; 8],
    preceding_pcs: &[u32],
) {
    log_crash!(
        subsystem,
        insns,
        "CRASH DETECTED: {} at PC={:#010x} SP={:#010x}",
        fault_type,
        pc,
        sp
    );
    log_crash!(
        subsystem,
        insns,
        "  D0={:#010x} D1={:#010x} D2={:#010x} D3={:#010x}",
        d_regs[0],
        d_regs[1],
        d_regs[2],
        d_regs[3]
    );
    log_crash!(
        subsystem,
        insns,
        "  D4={:#010x} D5={:#010x} D6={:#010x} D7={:#010x}",
        d_regs[4],
        d_regs[5],
        d_regs[6],
        d_regs[7]
    );
    log_crash!(
        subsystem,
        insns,
        "  A0={:#010x} A1={:#010x} A2={:#010x} A3={:#010x}",
        a_regs[0],
        a_regs[1],
        a_regs[2],
        a_regs[3]
    );
    log_crash!(
        subsystem,
        insns,
        "  A4={:#010x} A5={:#010x} A6={:#010x} A7={:#010x}",
        a_regs[4],
        a_regs[5],
        a_regs[6],
        a_regs[7]
    );

    if !preceding_pcs.is_empty() {
        let mut pc_str = String::new();
        for (i, p) in preceding_pcs.iter().take(12).enumerate() {
            if i > 0 {
                pc_str.push_str(" -> ");
            }
            pc_str.push_str(&format!("{:#08x}", p));
        }
        log_crash!(subsystem, insns, "  Trace: {}", pc_str);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_history_survives_queue_noise_and_remains_bounded() {
        let mut logger = Tech2Logger::new();
        logger.print_stdout = false;
        logger.log(LogLevel::Info, "CANDI", 1, "Tech2 -> CANdi 90 03 6D");
        for _ in 0..600 {
            logger.log(LogLevel::Info, "QUEUE", 2, "internal event");
        }
        assert!(logger
            .get_link_console_lines(78, 19)
            .iter()
            .any(|(_, line)| line.contains("90 03 6D")));
        for _ in 0..200 {
            logger.log(LogLevel::Info, "CANDI", 3, "next frame");
        }
        assert_eq!(logger.link_buffer.len(), 128);
    }

    #[test]
    fn test_logger_ring_buffer_and_stats() {
        let mut logger = Tech2Logger::new();
        logger.print_stdout = false; // silence test stdout

        logger.log(LogLevel::Info, "SYS", 100, "System ready");
        logger.log(LogLevel::Warn, "DLC", 200, "DLC timeout warning");
        logger.log(LogLevel::Error, "MEM", 300, "Memory parity error");
        logger.log(LogLevel::Crash, "CPU", 400, "Bus fault $0002");

        let (errs, warns, crashes) = logger.stats();
        assert_eq!(errs, 1);
        assert_eq!(warns, 1);
        assert_eq!(crashes, 1);
        assert_eq!(logger.buffer.len(), 4);
    }

    #[test]
    fn test_console_lines_wrapping() {
        let mut logger = Tech2Logger::new();
        logger.print_stdout = false;

        logger.log(
            LogLevel::Error,
            "TEST",
            1000,
            "A very long error message that exceeds typical screen width and needs wrapping cleanly",
        );

        let lines = logger.get_console_lines(30, 10);
        assert!(lines.len() >= 2);
        for (_color, text) in &lines {
            assert!(text.len() <= 30);
        }
    }

    #[test]
    fn unicode_and_zero_width_wrapping_are_safe() {
        let mut logger = Tech2Logger::new();
        logger.print_stdout = false;
        logger.log(LogLevel::Error, "CPU", 0, "é漢字 🦀 — failure");
        assert!(logger.get_console_lines(0, 10).is_empty());
        assert!(logger.get_console_lines(10, 0).is_empty());
        for width in 1..12 {
            let lines = logger.get_console_lines(width, 100);
            assert!(!lines.is_empty());
            assert!(lines.iter().all(|(_, text)| text.chars().count() <= width));
        }
    }

    #[test]
    fn test_record_crash_report() {
        let d_regs = [
            0x1111, 0x2222, 0x3333, 0x4444, 0x5555, 0x6666, 0x7777, 0x8888,
        ];
        let a_regs = [
            0xA000, 0xA100, 0xA200, 0xA300, 0xA400, 0xA500, 0xA600, 0xA700,
        ];
        let pcs = [0x1000, 0x1004, 0x1008];

        record_crash_report(
            "TEST_CPU",
            500_000,
            "Divide by Zero",
            0x1008,
            0x1FFE00,
            &d_regs,
            &a_regs,
            &pcs,
        );

        let logger = get_logger().lock().unwrap();
        assert!(logger.total_crashes >= 1);
    }
}
