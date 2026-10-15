// SPDX-License-Identifier: MPL-2.0
//! Private recording is observational: a full or failed recorder cannot stop transport.
use std::io::Write;

pub struct BoundedCapture<W: Write> {
    output: W,
    limit: u64,
    written: u64,
    omitted: u64,
    write_error: Option<String>,
}
impl<W: Write> BoundedCapture<W> {
    pub fn new(output: W, limit: u64) -> Self {
        Self {
            output,
            limit,
            written: 0,
            omitted: 0,
            write_error: None,
        }
    }
    pub fn record(&mut self, activity: &serde_json::Value) {
        if activity["operations"].as_array().is_some_and(Vec::is_empty)
            && activity["events"].as_array().is_some_and(Vec::is_empty)
        {
            return;
        }
        let bytes = match serde_json::to_vec(activity) {
            Ok(bytes) => bytes,
            Err(_) => {
                self.omitted += 1;
                return;
            }
        };
        let length = bytes.len() as u64 + 1;
        if self.write_error.is_some() || length > self.limit.saturating_sub(self.written) {
            self.omitted += 1;
            return;
        }
        if let Err(error) = self
            .output
            .write_all(&bytes)
            .and_then(|_| self.output.write_all(b"\n"))
        {
            // No path, diagnostic payload or exception text in the summary.
            self.write_error = Some(format!("{:?}", error.kind()));
            self.omitted += 1;
        } else {
            self.written += length;
        }
    }
    pub fn report(&self) -> serde_json::Value {
        serde_json::json!({"byte_limit":self.limit,"bytes_written":self.written,
            "omitted_records":self.omitted,"truncated":self.omitted>0,"write_error_kind":self.write_error,
            "retention":"initial nonempty activity; cleanup and transport outcome recorded separately"})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn exhaustion_never_becomes_a_transport_error_or_exceeds_budget() {
        let event = json!({"operations":[{"ticket":1}],"events":[{"received":true}]});
        let size = serde_json::to_vec(&event).unwrap().len() as u64 + 1;
        let mut capture = BoundedCapture::new(Vec::new(), size * 2);
        for _ in 0..100 {
            capture.record(&event);
        }
        assert_eq!(capture.output.len() as u64, size * 2);
        assert_eq!(capture.report()["omitted_records"], 98);
        assert_eq!(capture.report()["truncated"], true);
        assert!(capture.report()["write_error_kind"].is_null());
        for line in capture
            .output
            .split(|b| *b == b'\n')
            .filter(|line| !line.is_empty())
        {
            assert_eq!(
                serde_json::from_slice::<serde_json::Value>(line).unwrap(),
                event
            );
        }
    }
    #[test]
    fn idle_polling_does_not_consume_capture_budget() {
        let mut capture = BoundedCapture::new(Vec::new(), 128);
        for _ in 0..10_000 {
            capture.record(&json!({"operations":[],"events":[]}));
        }
        assert_eq!(capture.report()["bytes_written"], 0);
        assert_eq!(capture.report()["omitted_records"], 0);
        capture.record(&json!({"operations":["close"],"events":[]}));
        assert!(!capture.output.is_empty());
    }
    #[test]
    fn disk_failure_is_reported_without_becoming_transport_failure() {
        struct Broken;
        impl Write for Broken {
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
                Err(std::io::ErrorKind::StorageFull.into())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let mut capture = BoundedCapture::new(Broken, 1024);
        for _ in 0..4 {
            capture.record(&json!({"error":"private diagnostic payload"}));
        }
        assert_eq!(capture.report()["write_error_kind"], "StorageFull");
        assert_eq!(capture.report()["omitted_records"], 4);
        assert!(!capture.report().to_string().contains("private"));
    }
}
