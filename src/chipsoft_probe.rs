// SPDX-License-Identifier: MPL-2.0
//! Bounded, cancellable identification only. No vehicle channel is opened.

use crate::chipsoft::{Decoder, Frame};
use std::io::{self, Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

pub const MAX_TIMEOUT: Duration = Duration::from_secs(30);

/// Events describe actual host I/O. TX means bytes accepted by the serial
/// driver, not confirmed USB delivery or vehicle transmission.
#[derive(Debug)]
pub enum Event<'a> {
    SerialWrite(&'a [u8]),
    SerialRead(&'a [u8]),
    Reply(&'a Frame),
}

fn check_wait(cancel: &AtomicBool, deadline: Instant) -> io::Result<()> {
    if cancel.load(Ordering::Relaxed) {
        Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "Chipsoft probe cancelled",
        ))
    } else if Instant::now() >= deadline {
        Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "Chipsoft identification deadline expired",
        ))
    } else {
        Ok(())
    }
}

fn pause(deadline: Instant) {
    std::thread::sleep(
        deadline
            .saturating_duration_since(Instant::now())
            .min(Duration::from_millis(2)),
    );
}

/// `port` MUST provide nonblocking Read/Write, and `log` must not block.
/// This worker API has one total wall-clock budget, including partial writes
/// and reads. Each probe needs a fresh, exclusively owned, purged port; a
/// timeout or corrupt reply must not be followed by another in-flight request.
pub fn identify(
    port: &mut (impl Read + Write),
    timeout: Duration,
    cancel: &AtomicBool,
    mut log: impl FnMut(Event<'_>),
) -> io::Result<String> {
    if timeout.is_zero() || timeout > MAX_TIMEOUT {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "probe timeout must be greater than zero and at most 30 seconds",
        ));
    }
    let deadline = Instant::now() + timeout;
    // GET_INFO only. Do not open/arm a vehicle channel to identify hardware.
    let request = [1, 0, 0, 0, 0, 0, 0, 0];
    let mut sent = 0;
    while sent < request.len() {
        check_wait(cancel, deadline)?;
        match port.write(&request[sent..]) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "serial port accepted no probe bytes",
                ))
            }
            Ok(n) => {
                log(Event::SerialWrite(&request[sent..sent + n]));
                sent += n;
            }
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) =>
            {
                pause(deadline)
            }
            Err(e) => return Err(e),
        }
    }
    let mut decoder = Decoder::default();
    let mut bytes = [0; 512];
    loop {
        if let Err(error) = check_wait(cancel, deadline) {
            let detail = match decoder.finish() {
                Ok(()) => "no complete identification reply".to_owned(),
                Err(e) => e.to_string(),
            };
            return Err(io::Error::new(error.kind(), format!("{error}; {detail}")));
        }
        match port.read(&mut bytes) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "serial device closed/disconnected during identification",
                ))
            }
            Ok(n) => {
                log(Event::SerialRead(&bytes[..n]));
                let mut replies = Vec::new();
                decoder
                    .feed(&bytes[..n], |reply| replies.push(reply))
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
                if replies.is_empty() {
                    continue;
                }
                // An unexpected opcode, trailing partial frame or multiple
                // replies indicates stale/foreign traffic. Never pick a convenient
                // matching frame and claim identification succeeded.
                for reply in &replies {
                    log(Event::Reply(reply));
                }
                decoder
                    .finish()
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
                if replies.len() != 1 || replies[0].opcode != 1 {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "unexpected traffic during GET_INFO; close the port and inspect the log",
                    ));
                }
                let reply = &replies[0];
                if reply.status != 0 {
                    return Err(io::Error::other(format!(
                        "GET_INFO device status={:04X}",
                        reply.status
                    )));
                }
                let info = std::str::from_utf8(&reply.payload).map_err(|_| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        "GET_INFO reply is not ASCII firmware information",
                    )
                })?;
                if !info.starts_with("CHIPSOFT J2534 Pro v. ")
                    || !info.bytes().all(|b| (0x20..=0x7e).contains(&b))
                {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "device did not identify as a supported Chipsoft J2534 Pro",
                    ));
                }
                check_wait(cancel, deadline)?;
                return Ok(info.to_owned());
            }
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) =>
            {
                pause(deadline)
            }
            Err(e) => return Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    struct FakePort {
        reads: VecDeque<Vec<u8>>,
        written: Vec<u8>,
        blocked_write: bool,
    }
    impl Read for FakePort {
        fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
            if let Some(data) = self.reads.pop_front() {
                out[..data.len()].copy_from_slice(&data);
                Ok(data.len())
            } else {
                Err(io::ErrorKind::WouldBlock.into())
            }
        }
    }
    impl Write for FakePort {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.blocked_write {
                return Err(io::ErrorKind::WouldBlock.into());
            }
            let n = bytes.len().min(3);
            self.written.extend_from_slice(&bytes[..n]);
            Ok(n)
        }
        fn flush(&mut self) -> io::Result<()> {
            panic!("must not use unbounded serial drain")
        }
    }
    fn port(chunks: Vec<Vec<u8>>) -> FakePort {
        FakePort {
            reads: chunks.into(),
            written: Vec::new(),
            blocked_write: false,
        }
    }
    fn info() -> Vec<u8> {
        b"\x01\x00\x1b\x00\x00\x00\xc1\x06CHIPSOFT J2534 Pro v. 1.5.2".to_vec()
    }

    #[test]
    fn fragmented_capture_and_partial_writes_are_logged_exactly() {
        let wire = info();
        let mut p = port(vec![wire[..1].to_vec(), wire[1..].to_vec()]);
        let mut tx = Vec::new();
        let mut rx = Vec::new();
        let result = identify(
            &mut p,
            Duration::from_secs(1),
            &AtomicBool::new(false),
            |e| match e {
                Event::SerialWrite(bytes) => tx.extend_from_slice(bytes),
                Event::SerialRead(bytes) => rx.extend_from_slice(bytes),
                Event::Reply(_) => {}
            },
        )
        .unwrap();
        assert_eq!(result, "CHIPSOFT J2534 Pro v. 1.5.2");
        assert_eq!(tx, [1, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(tx, p.written);
        assert_eq!(rx, wire);
    }

    #[test]
    fn stalled_write_and_partial_read_share_bounded_deadline() {
        for blocked_write in [false, true] {
            let mut p = port(vec![vec![1]]);
            p.blocked_write = blocked_write;
            let start = Instant::now();
            let error = identify(
                &mut p,
                Duration::from_millis(15),
                &AtomicBool::new(false),
                |_| {},
            )
            .unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::TimedOut);
            assert!(start.elapsed() < Duration::from_secs(1));
            if !blocked_write {
                assert!(error.to_string().contains("incomplete frame"));
            }
        }
    }

    #[test]
    fn cancellation_prevents_or_stops_io() {
        let cancel = AtomicBool::new(true);
        let mut p = port(vec![]);
        assert_eq!(
            identify(&mut p, Duration::from_secs(1), &cancel, |_| {})
                .unwrap_err()
                .kind(),
            io::ErrorKind::Interrupted
        );
        assert!(p.written.is_empty());
        cancel.store(false, Ordering::Relaxed);
        assert_eq!(
            identify(&mut p, Duration::from_secs(1), &cancel, |_| cancel
                .store(true, Ordering::Relaxed))
            .unwrap_err()
            .kind(),
            io::ErrorKind::Interrupted
        );
        assert_eq!(p.written.len(), 3);
    }

    #[test]
    fn rejects_error_corruption_wrong_device_and_stale_traffic() {
        let mut corrupt = info();
        corrupt[8] ^= 1;
        let mut trailing = info();
        trailing.push(1);
        for wire in [
            vec![1, 0, 0, 0, 0x85, 0, 0, 0],
            vec![4, 0, 0, 0, 0, 0, 0, 0],
            Frame {
                opcode: 1,
                status: 0,
                payload: b"OTHER DEVICE".to_vec(),
            }
            .encode()
            .unwrap(),
            corrupt,
            trailing,
            [info(), info()].concat(),
        ] {
            assert!(identify(
                &mut port(vec![wire]),
                Duration::from_secs(1),
                &AtomicBool::new(false),
                |_| {}
            )
            .is_err());
        }
    }

    #[test]
    fn invalid_budget_and_disconnect_fail() {
        let mut p = port(vec![vec![]]);
        for timeout in [Duration::ZERO, Duration::from_secs(31)] {
            assert_eq!(
                identify(&mut p, timeout, &AtomicBool::new(false), |_| {})
                    .unwrap_err()
                    .kind(),
                io::ErrorKind::InvalidInput
            );
            assert!(p.written.is_empty());
        }
        assert_eq!(
            identify(
                &mut p,
                Duration::from_secs(1),
                &AtomicBool::new(false),
                |_| {}
            )
            .unwrap_err()
            .kind(),
            io::ErrorKind::UnexpectedEof
        );
    }
}
