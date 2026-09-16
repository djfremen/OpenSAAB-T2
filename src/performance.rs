// SPDX-License-Identifier: MPL-2.0
//! Opt-in, bounded host resource samples. Never reads guest memory or vehicle data.
use serde_json::{json, Value};
use std::{
    collections::VecDeque,
    fs,
    path::PathBuf,
    sync::mpsc,
    thread,
    time::{Duration, Instant, SystemTime},
};

const INTERVAL_MS: u64 = 5000;
const MAX_SAMPLES: usize = 12;

pub struct Recorder {
    stop: mpsc::Sender<()>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Drop for Recorder {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

pub fn start() -> Option<Recorder> {
    let path = PathBuf::from(std::env::var_os("OPENSAAB_PERFORMANCE_DIR")?);
    record(path)
}

fn record(path: PathBuf) -> Option<Recorder> {
    if !path.is_dir() {
        return None;
    }
    let (stop, receive) = mpsc::channel();
    let worker = thread::Builder::new()
        .name("host-resources".into())
        .spawn(move || {
            let start = Instant::now();
            let mut history = VecDeque::new();
            let mut first_frame = None;
            let mut count = 0u64;
            let mut complete = false;
            loop {
                let elapsed = start.elapsed().as_millis() as u64;
                let mut sample = snapshot(elapsed);
                let live = path.join("live.ppm");
                if let Ok(meta) = fs::metadata(live) {
                    if meta.len() == 230415 {
                        first_frame.get_or_insert(elapsed);
                        if let Ok(modified) = meta.modified() {
                            if let Ok(age) = SystemTime::now().duration_since(modified) {
                                sample["frame_age_ms"] = json!(age.as_millis() as u64);
                            }
                        }
                    }
                }
                history.push_back(sample);
                if history.len() > MAX_SAMPLES {
                    history.pop_front();
                }
                count += 1;
                let mut report = json!({"performance_schema":1,"sample_interval_ms":INTERVAL_MS,
                "sample_count":count,"complete":complete,"samples":history});
                if let Some(ms) = first_frame {
                    report["first_frame_observed_ms"] = json!(ms);
                }
                // Atomic replacement leaves the previous bounded sample available after a kill.
                let tmp = path.join("host-performance.json.tmp");
                if let Ok(bytes) = serde_json::to_vec(&report) {
                    if fs::write(&tmp, bytes).is_ok() {
                        let _ = fs::rename(tmp, path.join("host-performance.json"));
                    }
                }
                if complete {
                    break;
                }
                if !matches!(
                    receive.recv_timeout(Duration::from_millis(INTERVAL_MS)),
                    Err(mpsc::RecvTimeoutError::Timeout)
                ) {
                    complete = true;
                }
            }
        })
        .ok()?;
    Some(Recorder {
        stop,
        worker: Some(worker),
    })
}

fn kib_value(text: &str, name: &str) -> Option<u64> {
    text.lines().find_map(|line| {
        let tail = line.strip_prefix(name)?;
        let mut fields = tail.split_whitespace();
        let value = fields.next()?.parse().ok()?;
        if fields.next()? != "kB" {
            return None;
        }
        Some(value)
    })
}

fn snapshot(elapsed: u64) -> Value {
    let mut result = json!({"elapsed_ms":elapsed});
    // RUSAGE_SELF includes all threads of this native process; Java UI is separate.
    #[cfg(unix)]
    unsafe {
        let mut usage: libc::rusage = std::mem::zeroed();
        if libc::getrusage(libc::RUSAGE_SELF, &mut usage) == 0 {
            let cpu_ms = (usage.ru_utime.tv_sec as i64 + usage.ru_stime.tv_sec as i64) * 1000
                + (usage.ru_utime.tv_usec as i64 + usage.ru_stime.tv_usec as i64) / 1000;
            result["cpu_ms"] = json!(cpu_ms.max(0));
            let peak = usage.ru_maxrss as u64;
            result["peak_rss_kib"] = json!(if cfg!(target_os = "macos") {
                peak / 1024
            } else {
                peak
            });
            result["minor_faults"] = json!(usage.ru_minflt.max(0));
            result["major_faults"] = json!(usage.ru_majflt.max(0));
        }
    }
    if let Ok(status) = fs::read_to_string("/proc/self/status") {
        if let Some(n) = kib_value(&status, "VmRSS:") {
            result["rss_kib"] = json!(n);
        }
    }
    if let Ok(memory) = fs::read_to_string("/proc/meminfo") {
        for (source, key) in [
            ("MemTotal:", "ram_total_kib"),
            ("MemAvailable:", "ram_available_kib"),
            ("SwapFree:", "swap_free_kib"),
        ] {
            if let Some(n) = kib_value(&memory, source) {
                result[key] = json!(n);
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recorder_finishes_without_waiting_for_interval_and_writes_bounded_data() {
        let directory = std::env::temp_dir().join(format!(
            "opensaab-perf-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&directory).unwrap();
        let start = Instant::now();
        drop(record(directory.clone()).unwrap());
        assert!(start.elapsed() < Duration::from_secs(3));
        let value: Value =
            serde_json::from_slice(&fs::read(directory.join("host-performance.json")).unwrap())
                .unwrap();
        assert_eq!(value["complete"], true);
        assert!(value["samples"].as_array().unwrap().len() <= MAX_SAMPLES);
        assert!(value.get("first_frame_observed_ms").is_none());
        assert!(!value.to_string().contains(directory.to_str().unwrap()));
        fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn memory_parser_never_copies_text_or_assumes_missing_zero() {
        assert_eq!(
            kib_value("VmRSS:\t123 kB\nName: secret", "VmRSS:"),
            Some(123)
        );
        assert_eq!(kib_value("VmRSS: secret kB", "VmRSS:"), None);
        assert_eq!(kib_value("VmRSS: 123 bytes", "VmRSS:"), None);
        assert_eq!(kib_value("MemFree: 10 kB", "MemAvailable:"), None);
    }
    #[test]
    fn native_sample_contains_only_nonnegative_numbers() {
        let sample = snapshot(25);
        assert_eq!(sample["elapsed_ms"], 25);
        assert!(sample
            .as_object()
            .unwrap()
            .values()
            .all(|v| v.as_u64().is_some()));
        assert!(sample.get("cpu_ms").is_some());
    }
}
