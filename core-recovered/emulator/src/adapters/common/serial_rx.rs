// SPDX-License-Identifier: MPL-2.0
//! Bounded serial receive handoff. No adapter protocol or frame interpretation.
//!
//! A full queue stops the stream with an explicit error: dropping a chunk and
//! continuing could turn an incomplete diagnostic reply into a false result.
//! Reads return after a fixed byte budget even if the producer never goes idle.
use std::{
    io::{self, Read},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};

pub(crate) const CHUNK_BYTES: usize = 512;
pub(crate) const QUEUE_CHUNKS: usize = 128;
pub(crate) const READ_CHUNKS: usize = 32;

#[derive(Clone)]
struct Failure {
    kind: io::ErrorKind,
    message: String,
}
type SharedFailure = Arc<Mutex<Option<Failure>>>;

pub(crate) struct ByteSender {
    sender: SyncSender<Vec<u8>>,
    failure: SharedFailure,
}
pub(crate) struct ByteReceiver {
    receiver: Receiver<Vec<u8>>,
    failure: SharedFailure,
}

pub(crate) fn channel() -> (ByteSender, ByteReceiver) {
    let (sender, receiver) = mpsc::sync_channel(QUEUE_CHUNKS);
    let failure = Arc::new(Mutex::new(None));
    (
        ByteSender {
            sender,
            failure: failure.clone(),
        },
        ByteReceiver { receiver, failure },
    )
}

impl ByteSender {
    fn fail(&self, error: io::Error) {
        if let Ok(mut failure) = self.failure.lock() {
            if failure.is_none() {
                *failure = Some(Failure {
                    kind: error.kind(),
                    message: error.to_string(),
                });
            }
        }
    }

    fn push(&self, bytes: Vec<u8>) -> bool {
        if self
            .failure
            .lock()
            .map_or(true, |failure| failure.is_some())
        {
            return false;
        }
        if bytes.len() > CHUNK_BYTES {
            self.fail(io::Error::new(
                io::ErrorKind::InvalidData,
                "serial receive chunk exceeds limit",
            ));
            return false;
        }
        match self.sender.try_send(bytes) {
            Ok(()) => true,
            Err(TrySendError::Full(bytes)) => {
                self.fail(io::Error::new(io::ErrorKind::InvalidData, format!(
                    "serial receive buffer overflow: capacity={} bytes, rejected={} bytes; reopen transport",
                    QUEUE_CHUNKS * CHUNK_BYTES, bytes.len()
                )));
                false
            }
            Err(TrySendError::Disconnected(_)) => false,
        }
    }
}

impl ByteReceiver {
    fn check_failure(&self) -> io::Result<()> {
        let failure = self
            .failure
            .lock()
            .map_err(|_| io::Error::other("serial receive failure state unavailable"))?;
        match failure.as_ref() {
            Some(failure) => Err(io::Error::new(failure.kind, failure.message.clone())),
            None => Ok(()),
        }
    }

    pub(crate) fn read(&self) -> io::Result<Vec<u8>> {
        self.check_failure()?;
        let mut out = Vec::new();
        for index in 0..READ_CHUNKS {
            let chunk = match self.receiver.try_recv() {
                Ok(chunk) => chunk,
                Err(TryRecvError::Empty) if index == 0 => {
                    match self.receiver.recv_timeout(Duration::from_millis(5)) {
                        Ok(chunk) => chunk,
                        Err(mpsc::RecvTimeoutError::Timeout) => break,
                        Err(mpsc::RecvTimeoutError::Disconnected) => {
                            self.check_failure()?;
                            return Err(io::Error::new(
                                io::ErrorKind::BrokenPipe,
                                "serial reader stopped",
                            ));
                        }
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.check_failure()?;
                    return Err(io::Error::new(
                        io::ErrorKind::BrokenPipe,
                        "serial reader stopped",
                    ));
                }
            };
            out.extend_from_slice(&chunk);
        }
        // An overflow/error observed during this read invalidates the whole
        // batch. The failure remains sticky even after queued bytes are drained.
        self.check_failure()?;
        Ok(out)
    }
}

pub(crate) fn receive_loop(mut reader: impl Read, sender: ByteSender, stop: &AtomicBool) {
    let mut buffer = [0; CHUNK_BYTES];
    while !stop.load(Ordering::Acquire) {
        match reader.read(&mut buffer) {
            // Windows COM timeout returns zero successfully; it is not EOF.
            Ok(0) => thread::sleep(Duration::from_millis(1)),
            Ok(n) => {
                if !sender.push(buffer[..n].to_vec()) {
                    break;
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::TimedOut
                        | io::ErrorKind::Interrupted
                        | io::ErrorKind::WouldBlock
                ) =>
            {
                thread::sleep(Duration::from_millis(1));
            }
            Err(error) => {
                sender.fail(error);
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_reads_preserve_all_bytes_in_order() {
        let (sender, receiver) = channel();
        let wire: Vec<u8> = (0..CHUNK_BYTES * QUEUE_CHUNKS).map(|i| i as u8).collect();
        for chunk in wire.chunks(CHUNK_BYTES) {
            assert!(sender.push(chunk.to_vec()));
        }
        let mut actual = Vec::new();
        for _ in 0..QUEUE_CHUNKS / READ_CHUNKS {
            let batch = receiver.read().unwrap();
            assert_eq!(batch.len(), READ_CHUNKS * CHUNK_BYTES);
            actual.extend(batch);
        }
        assert_eq!(actual, wire);
        assert!(receiver.read().unwrap().is_empty());
    }

    #[test]
    fn a_stalled_consumer_causes_visible_terminal_overflow() {
        let (sender, receiver) = channel();
        for _ in 0..QUEUE_CHUNKS {
            assert!(sender.push(vec![7; CHUNK_BYTES]));
        }
        assert!(!sender.push(vec![9; CHUNK_BYTES]));
        let error = receiver.read().unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(error
            .to_string()
            .contains("capacity=65536 bytes, rejected=512 bytes"));
        assert!(receiver.read().is_err());
    }

    #[test]
    fn producer_never_blocks_when_the_consumer_stops() {
        struct Busy;
        impl Read for Busy {
            fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
                out.fill(1);
                Ok(out.len())
            }
        }
        let (sender, receiver) = channel();
        let (finished, completion) = mpsc::channel();
        let worker = thread::spawn(move || {
            receive_loop(Busy, sender, &AtomicBool::new(false));
            finished.send(()).unwrap();
        });
        completion
            .recv_timeout(Duration::from_secs(1))
            .expect("producer blocked on a full queue");
        worker.join().unwrap();
        assert!(receiver
            .read()
            .unwrap_err()
            .to_string()
            .contains("overflow"));
    }

    #[test]
    fn idle_reads_preserve_later_data_and_hard_errors_stay_visible() {
        let (steps, commands) = mpsc::channel::<io::Result<Vec<u8>>>();
        struct Fake(Receiver<io::Result<Vec<u8>>>);
        impl Read for Fake {
            fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
                let data = self.0.recv().unwrap()?;
                out[..data.len()].copy_from_slice(&data);
                Ok(data.len())
            }
        }
        let (sender, receiver) = channel();
        let worker =
            thread::spawn(move || receive_loop(Fake(commands), sender, &AtomicBool::new(false)));
        steps.send(Ok(vec![])).unwrap();
        steps.send(Ok(vec![1, 2, 3])).unwrap();
        // An idle poll is allowed while the producer is being scheduled.
        let started = std::time::Instant::now();
        let bytes = loop {
            let bytes = receiver.read().unwrap();
            if !bytes.is_empty() {
                break bytes;
            }
            assert!(
                started.elapsed() < Duration::from_secs(1),
                "producer did not deliver data"
            );
        };
        assert_eq!(bytes, vec![1, 2, 3]);
        steps
            .send(Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "device disconnected",
            )))
            .unwrap();
        worker.join().unwrap();
        for _ in 0..2 {
            let error = receiver.read().unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
            assert_eq!(error.to_string(), "device disconnected");
        }
    }

    #[test]
    fn oversized_chunks_and_disconnected_consumers_stop_the_producer() {
        let (sender, receiver) = channel();
        assert!(!sender.push(vec![0; CHUNK_BYTES + 1]));
        assert!(receiver.read().is_err());
        let (sender, receiver) = channel();
        drop(receiver);
        assert!(!sender.push(vec![0; CHUNK_BYTES]));
    }
}
