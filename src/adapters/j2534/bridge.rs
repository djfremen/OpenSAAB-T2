// SPDX-License-Identifier: MPL-2.0
//! Persistent raw Windows J2534 backend (Nano or Chipsoft Pro). All TX originates in native CANdi; no diagnostic client.
use crate::{
    candi_cpu::{CanFrame, CanTransmission},
    vcx::Connection,
};
use std::{
    fs::{File, OpenOptions},
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    path::Path,
    process::{Child, Command, Stdio},
    sync::mpsc::{self, Receiver, SyncSender},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

pub use crate::can_adapter::{realtime_delay, Event};
#[cfg(test)]
use crate::candi_cpu::CanElectricalState;

struct Pending {
    tx: CanTransmission,
    accepted: bool,
    deadline: Instant,
}
pub struct Bridge {
    child: Child,
    forward_cleanup: Option<Command>,
    input: Option<SyncSender<String>>,
    output: Receiver<String>,
    reader: Option<JoinHandle<()>>,
    writer: Option<JoinHandle<()>>,
    log: File,
    pending: [Option<Pending>; 3],
    last_ticket: [u64; 3],
    heartbeat: Instant,
    ready: bool,
    closed: bool,
    pub confirmations: u64,
}
fn hex(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        "-".into()
    } else {
        bytes.iter().map(|v| format!("{v:02X}")).collect()
    }
}
fn bytes(s: &str) -> Result<Vec<u8>, String> {
    if s.len() > 24 || s.len() % 2 != 0 || !s.is_ascii() {
        return Err("Invalid raw frame hex".into());
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|_| "Invalid hex".into()))
        .collect()
}
/// A specific experimental mapping: native channel 0's wake helper selects low
/// GPIO bits 10; capture-backed J2534 operation is SW_CAN_HV_TX, never HS CAN.
pub fn encode(controller: usize, tx: &CanTransmission) -> Result<String, String> {
    let flags = crate::can_adapter::electrical_route(controller, tx)?.j2534_flags;
    Ok(format!(
        "TX {controller} {} {flags:04X} {:03X} {}",
        tx.ticket,
        tx.frame.id,
        hex(&tx.frame.data)
    ))
}
fn confirmed_ticket(pending: Option<&Pending>, frame: &CanFrame) -> Result<u64, String> {
    let pending = pending.ok_or("Unmatched TX indication")?;
    if !pending.accepted || pending.tx.frame != *frame {
        return Err("Mismatched TX indication".into());
    }
    Ok(pending.tx.ticket)
}
impl Bridge {
    pub fn start(
        connection: Connection,
        directory: &Path,
        seed_reads: bool,
    ) -> Result<Self, String> {
        Self::start_with_adapter(
            connection,
            directory,
            seed_reads,
            crate::vcx::J2534Adapter::Nano,
            false,
        )
    }
    pub fn start_with_adapter(
        connection: Connection,
        directory: &Path,
        seed_reads: bool,
        adapter: crate::vcx::J2534Adapter,
        seatbelt_audible: bool,
    ) -> Result<Self, String> {
        let reservation = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
        let address = reservation.local_addr().map_err(|e| e.to_string())?;
        drop(reservation);
        let mut command = connection.native_bridge_command(
            address.port(),
            seed_reads,
            adapter,
            seatbelt_audible,
        )?;
        let log = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(directory.join("native-nano.log"))
            .map_err(|e| e.to_string())?;
        let err = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(directory.join("native-nano-stderr.log"))
            .map_err(|e| e.to_string())?;
        let mut child = command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(err)
            .spawn()
            .map_err(|e| e.to_string())?;
        let stdout = child.stdout.take().unwrap();
        let (out_tx, output) = mpsc::sync_channel(4096);
        let reader = thread::spawn(move || {
            let mut stream = BufReader::new(stdout);
            let mut total = 0usize;
            loop {
                let mut line = Vec::new();
                match stream.by_ref().take(1025).read_until(b'\n', &mut line) {
                    Ok(0) => break,
                    Ok(n) => {
                        total += n;
                        if n > 1024 || total > 16 * 1024 * 1024 {
                            let _ = out_tx.try_send("BRIDGE ERROR output limit".into());
                            break;
                        }
                        if out_tx
                            .try_send(String::from_utf8_lossy(&line).trim().to_string())
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });
        let mut this = Self {
            child,
            forward_cleanup: connection.native_forward_cleanup(address.port()),
            input: None,
            output,
            reader: Some(reader),
            writer: None,
            log,
            pending: std::array::from_fn(|_| None),
            last_ticket: [0; 3],
            heartbeat: Instant::now(),
            ready: false,
            closed: false,
            confirmations: 0,
        };
        let deadline = Instant::now() + Duration::from_secs(10);
        while !this.ready {
            this.poll()?;
            if Instant::now() >= deadline {
                return Err("Native J2534 bridge startup timeout".into());
            }
            thread::sleep(Duration::from_millis(5));
        }
        let mut socket = TcpStream::connect_timeout(&address, Duration::from_secs(1))
            .map_err(|e| format!("Native bridge command socket: {e}"))?;
        socket
            .set_write_timeout(Some(Duration::from_secs(1)))
            .map_err(|e| e.to_string())?;
        socket.set_nodelay(true).map_err(|e| e.to_string())?;
        let (input, commands) = mpsc::sync_channel::<String>(16);
        let writer = thread::spawn(move || {
            for command in commands {
                // One complete line per write: separate text/newline writes
                // can encounter delayed ACK/Nagle on the SSH forwarding hop.
                let mut wire = command.into_bytes();
                wire.push(b'\n');
                if socket.write_all(&wire).is_err() {
                    break;
                }
            }
        });
        this.input = Some(input);
        this.writer = Some(writer);
        this.send("PING".into())?;
        Ok(this)
    }
    fn send(&mut self, line: String) -> Result<(), String> {
        writeln!(self.log, "HOST {line}").map_err(|e| e.to_string())?;
        self.input
            .as_ref()
            .ok_or("Bridge closed")?
            .try_send(line)
            .map_err(|e| format!("Bridge command queue: {e}"))
    }
    pub fn transmit(&mut self, controller: usize, tx: CanTransmission) -> Result<(), String> {
        let line = encode(controller, &tx)?;
        if self.closed
            || !self.ready
            || self.pending[controller].is_some()
            || tx.ticket <= self.last_ticket[controller]
        {
            return Err("Unavailable bridge or stale/pending TX".into());
        }
        self.send(line)?;
        self.last_ticket[controller] = tx.ticket;
        self.pending[controller] = Some(Pending {
            tx,
            accepted: false,
            deadline: Instant::now() + Duration::from_secs(2),
        });
        Ok(())
    }
    pub fn poll(&mut self) -> Result<Vec<Event>, String> {
        if self.closed {
            return Err("Native J2534 bridge closed".into());
        }
        let mut events = Vec::new();
        // Bounded batch, while letting native IRQs drain between polls.
        for _ in 0..64 {
            let line = match self.output.try_recv() {
                Ok(line) => line,
                Err(mpsc::TryRecvError::Empty) => break,
                Err(_) => return Err("Native J2534 bridge output disconnected".into()),
            };
            writeln!(self.log, "{line}").map_err(|e| e.to_string())?;
            let Some(line) = line.strip_prefix("BRIDGE ") else {
                continue;
            };
            let p: Vec<_> = line.split_whitespace().collect();
            match p.first().copied() {
                Some("READY")
                    if line == "READY version=1 raw=true generated_requests=0" && !self.ready =>
                {
                    self.ready = true
                }
                Some("ERROR" | "EXIT") => return Err(format!("Native J2534 {line}")),
                Some("ACCEPT") if p.len() == 3 => {
                    let c: usize = p[1].parse().map_err(|_| "Bad ACCEPT controller")?;
                    let ticket: u64 = p[2].parse().map_err(|_| "Bad ACCEPT ticket")?;
                    let pending = self
                        .pending
                        .get_mut(c)
                        .and_then(Option::as_mut)
                        .ok_or("Unmatched ACCEPT")?;
                    if pending.tx.ticket != ticket || pending.accepted {
                        return Err("Stale/duplicate ACCEPT".into());
                    }
                    pending.accepted = true;
                }
                Some("RX") if p.len() == 6 => {
                    let c: usize = p[1].parse().map_err(|_| "Bad RX controller")?;
                    let protocol: u32 = p[2].parse().map_err(|_| "Bad protocol")?;
                    let status = u32::from_str_radix(p[3], 16).map_err(|_| "Bad RX status")?;
                    if !matches!((c, protocol), (0, 5) | (2, 0x8008)) {
                        return Err("Mismatched raw bus/protocol".into());
                    }
                    let data = bytes(p[5])?;
                    if data.len() < 4 {
                        return Err("Truncated CAN ID".into());
                    }
                    let frame = CanFrame {
                        id: u32::from_be_bytes(data[..4].try_into().unwrap()),
                        extended: false,
                        rtr: false,
                        dlc: (data.len() - 4) as u8,
                        data: data[4..].to_vec(),
                    };
                    frame.validate()?;
                    if status == 0 {
                        events.push(Event::Received(c, frame));
                    } else if status == 1 {
                        let ticket = confirmed_ticket(self.pending[c].as_ref(), &frame)?;
                        self.pending[c] = None;
                        self.confirmations += 1;
                        events.push(Event::Completed {
                            controller: c,
                            ticket,
                            source: crate::can_adapter::CompletionSource::J2534SoftwareLoopback,
                        });
                    } else {
                        return Err(format!("Unsupported raw RX status {status:X}"));
                    }
                    // Spread driver batches across CPU quanta so native IRQ
                    // handlers can consume the controller FIFO between frames.
                    break;
                }
                Some("CALL" | "TX" | "CLEAN" | "ADAPTER" | "IDENTITY") => {}
                _ => return Err(format!("Malformed bridge record: {line}")),
            }
        }
        if self
            .pending
            .iter()
            .flatten()
            .any(|p| Instant::now() >= p.deadline)
        {
            return Err("Native CAN TX confirmation timeout; outcome unknown".into());
        }
        if let Some(status) = self.child.try_wait().map_err(|e| e.to_string())? {
            return Err(format!("Native J2534 SSH exited: {status}"));
        }
        if self.ready && self.input.is_some() && self.heartbeat.elapsed() >= Duration::from_secs(1)
        {
            self.send("PING".into())?;
            self.heartbeat = Instant::now();
        }
        self.log.flush().map_err(|e| e.to_string())?;
        Ok(events)
    }
    pub fn close(&mut self) {
        if self.closed {
            return;
        }
        self.closed = true;
        if let Some(input) = self.input.take() {
            let _ = input.try_send("QUIT".into());
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            for line in self.output.try_iter() {
                let _ = writeln!(self.log, "{line}");
            }
            if self.child.try_wait().ok().flatten().is_some() {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(t) = self.writer.take() {
            let _ = t.join();
        }
        if let Some(t) = self.reader.take() {
            let _ = t.join();
        }
        for line in self.output.try_iter() {
            let _ = writeln!(self.log, "{line}");
        }
        let _ = self.log.flush();
        if let Some(mut command) = self.forward_cleanup.take() {
            if let Ok(mut child) = command
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
            {
                let deadline = Instant::now() + Duration::from_secs(1);
                while Instant::now() < deadline && child.try_wait().ok().flatten().is_none() {
                    thread::sleep(Duration::from_millis(10));
                }
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }
}
impl Drop for Bridge {
    fn drop(&mut self) {
        self.close();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn live_clock_bounds_lead_without_sleeping_when_behind() {
        assert_eq!(
            realtime_delay(16_777_216, Duration::from_secs(1)),
            Duration::ZERO
        );
        assert_eq!(
            realtime_delay(16_777_216, Duration::from_millis(998)),
            Duration::from_millis(2)
        );
        assert_eq!(
            realtime_delay(16_777_216, Duration::from_millis(999)),
            Duration::from_millis(1)
        );
        assert_eq!(
            realtime_delay(16_777_216, Duration::from_secs(2)),
            Duration::ZERO
        );
        assert_eq!(
            realtime_delay(u64::MAX, Duration::ZERO),
            Duration::from_millis(2)
        );
    }
    #[test]
    fn only_an_accepted_matching_frame_can_complete_a_ticket() {
        let frame = CanFrame {
            id: 0x241,
            extended: false,
            rtr: false,
            dlc: 3,
            data: vec![2, 0x1a, 0x90],
        };
        let mut pending = Pending {
            tx: CanTransmission {
                ticket: 7,
                frame: frame.clone(),
                btr0: 0xdd,
                btr1: 0x36,
                electrical: CanElectricalState::Unspecified,
            },
            accepted: false,
            deadline: Instant::now() + Duration::from_secs(1),
        };
        assert!(confirmed_ticket(None, &frame).is_err());
        assert!(confirmed_ticket(Some(&pending), &frame).is_err());
        pending.accepted = true;
        assert_eq!(confirmed_ticket(Some(&pending), &frame).unwrap(), 7);
        let mut wrong = frame.clone();
        wrong.id = 0x242;
        assert!(confirmed_ticket(Some(&pending), &wrong).is_err());
        wrong = frame.clone();
        wrong.data[2] = 0x97;
        assert!(confirmed_ticket(Some(&pending), &wrong).is_err());
    }
    #[test]
    fn electrical_route_is_not_inferred_from_id() {
        let mut tx = CanTransmission {
            ticket: 1,
            frame: CanFrame {
                id: 0x100,
                extended: false,
                rtr: false,
                dlc: 0,
                data: vec![],
            },
            btr0: 0xdd,
            btr1: 0x36,
            electrical: CanElectricalState::SingleWireGpio {
                latch: 0x26,
                assignment: 0,
                direction: 0x7f,
            },
        };
        assert_eq!(encode(2, &tx).unwrap(), "TX 2 1 0400 100 -");
        assert!(encode(0, &tx).is_err());
        assert!(encode(1, &tx).is_err());
        tx.electrical = CanElectricalState::Unspecified;
        assert!(encode(2, &tx).is_err());
        tx.electrical = CanElectricalState::SingleWireGpio {
            latch: 0x26,
            assignment: 0,
            direction: 1,
        };
        assert!(encode(2, &tx).is_err());
        tx.electrical = CanElectricalState::SingleWireGpio {
            latch: 0x27,
            assignment: 0,
            direction: 0x7f,
        };
        assert_eq!(encode(2, &tx).unwrap(), "TX 2 1 0000 100 -");
    }
}

impl crate::can_adapter::Backend for Bridge {
    fn poll(&mut self) -> Result<Vec<Event>, String> {
        Bridge::poll(self)
    }
    fn transmit(&mut self, controller: usize, tx: CanTransmission) -> Result<(), String> {
        Bridge::transmit(self, controller, tx)
    }
    fn confirmations(&self) -> u64 {
        self.confirmations
    }
    fn close(&mut self) {
        Bridge::close(self);
    }
}
