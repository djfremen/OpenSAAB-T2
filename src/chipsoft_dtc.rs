// SPDX-License-Identifier: MPL-2.0
//! Bounded T8 DTC read on Chipsoft HS-CAN only. No clearing or security access.
use crate::{
    chipsoft_channel::{self as channel, Client},
    nano_channel::UsbTransport,
    t8_dtc::{Dtc, Report},
};
use std::time::{Duration, Instant};

fn send<T: UsbTransport>(client: &mut Client<T>, data: [u8; 8]) -> Result<(), String> {
    use crate::candi_cpu::{CanElectricalState, CanFrame, CanTransmission};
    println!("HOST_DTC_TX 7E0 {:02X?}", data);
    client.checked(&channel::transmit(
        0,
        &CanTransmission {
            ticket: 1,
            frame: CanFrame {
                id: 0x7e0,
                extended: false,
                rtr: false,
                dlc: 8,
                data: data.to_vec(),
            },
            btr0: 0xc1,
            btr1: 0x36,
            electrical: CanElectricalState::StandardCan,
        },
    )?)?;
    Ok(())
}
fn poll<T: UsbTransport>(client: &mut Client<T>) -> Result<Vec<channel::RawCan>, String> {
    let reply = client.exchange(&channel::command(0x10, channel::words(&[5, 10])))?;
    if reply.status == 0x85 && reply.payload.is_empty() {
        return Ok(vec![]);
    }
    let frames = channel::decode_read(&reply)?;
    for f in &frames {
        println!(
            "DTC_RX protocol={} id={:03X} flags={:X} data={:02X?}",
            f.protocol, f.id, f.flags, f.data
        );
    }
    Ok(frames)
}
fn wait_ack<T: UsbTransport>(
    client: &mut Client<T>,
    request: u8,
    reply: u8,
    timeout: Duration,
) -> Result<(), String> {
    let until = Instant::now() + timeout;
    while Instant::now() < until {
        for f in poll(client)? {
            if f.protocol != 5 || f.flags != 0 || f.id != 0x7e8 {
                continue;
            }
            if f.data.starts_with(&[1, reply]) {
                return Ok(());
            }
            if f.data.starts_with(&[3, 0x7f, request]) && f.data.get(3) != Some(&0x78) {
                return Err(format!(
                    "Session request {request:02X} rejected: {:02X?}",
                    f.data
                ));
            }
        }
    }
    Err(format!("No verified session acknowledgement {reply:02X}"))
}
pub fn read<T: UsbTransport>(client: &mut Client<T>) -> Result<Vec<Dtc>, String> {
    read_with_timeout(client, Duration::from_secs(3), Duration::from_secs(10))
}
fn read_with_timeout<T: UsbTransport>(
    client: &mut Client<T>,
    ack_timeout: Duration,
    report_timeout: Duration,
) -> Result<Vec<Dtc>, String> {
    // Once start is attempted, always attempt the explicit stop, including errors.
    let result = (|| {
        send(client, [2, 0x10, 2, 0, 0, 0, 0, 0])?;
        wait_ack(client, 0x10, 0x50, ack_timeout)?;
        client.checked(&channel::command(0x12, channel::words(&[5])))?;
        send(client, [3, 0xa9, 0x81, 0x12, 0, 0, 0, 0])?;
        let mut report = Report::default();
        let until = Instant::now() + report_timeout;
        while Instant::now() < until && !report.is_complete() {
            for f in poll(client)? {
                if f.protocol == 5 && f.flags == 0 && !report.is_complete() {
                    report.receive(f.id, &f.data)?;
                }
            }
        }
        report.finish()
    })();
    let stopped = send(client, [1, 0x20, 0, 0, 0, 0, 0, 0])
        .and_then(|_| wait_ack(client, 0x20, 0x60, ack_timeout));
    match (result, stopped) {
        (Ok(records), Ok(())) => Ok(records),
        (Err(e), Ok(())) => Err(e),
        (result, Err(e)) => Err(format!(
            "{}; session cleanup failed: {e}",
            result.err().unwrap_or_else(|| "Report received".into())
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chipsoft::{Decoder, Frame};
    use std::collections::VecDeque;
    struct Adapter {
        replies: VecDeque<Vec<u8>>,
        sent: Vec<Vec<u8>>,
        report: Vec<(u32, Vec<u8>)>,
        stop_ok: bool,
    }
    fn raw(records: &[(u32, Vec<u8>)]) -> Frame {
        let mut b = vec![];
        for (id, data) in records {
            b.extend(channel::words(&[5, 0]));
            b.extend(((data.len() + 4) as u16).to_le_bytes());
            b.extend([0; 6]);
            b.extend(id.to_be_bytes());
            b.extend(data);
        }
        let mut p = (records.len() as u16).to_le_bytes().to_vec();
        p.extend((b.len() as u16).to_le_bytes());
        p.extend(b);
        channel::command(0x10, p)
    }
    impl UsbTransport for Adapter {
        fn write(&mut self, wire: &[u8]) -> Result<(), String> {
            let mut frames = vec![];
            Decoder::default().feed(wire, |f| frames.push(f)).unwrap();
            let f = &frames[0];
            if f.opcode == 15 {
                let data = f.payload[24..].to_vec();
                self.sent.push(data.clone());
                self.replies
                    .push_back(channel::command(15, vec![]).encode().unwrap());
                let records = match data[1] {
                    0x10 => vec![(0x7e8, vec![1, 0x50])],
                    0xa9 => self.report.clone(),
                    0x20 if self.stop_ok => vec![(0x7e8, vec![1, 0x60])],
                    _ => vec![],
                };
                self.replies.push_back(raw(&records).encode().unwrap());
            } else if f.opcode == 0x12 {
                self.replies
                    .push_back(channel::command(f.opcode, vec![]).encode().unwrap());
            } else {
                assert_eq!(f.opcode, 0x10);
                if self.replies.is_empty() {
                    self.replies.push_back(raw(&[]).encode().unwrap());
                }
            }
            Ok(())
        }
        fn read(&mut self) -> Result<Vec<u8>, String> {
            Ok(self.replies.pop_front().unwrap_or_default())
        }
    }
    fn attempt(
        report: Vec<(u32, Vec<u8>)>,
        stop_ok: bool,
    ) -> (Result<Vec<Dtc>, String>, Vec<Vec<u8>>) {
        let mut c = Client::new(Adapter {
            replies: VecDeque::new(),
            sent: vec![],
            report,
            stop_ok,
        });
        let result = read_with_timeout(&mut c, Duration::from_millis(2), Duration::from_millis(2));
        (result, c.transport.sent)
    }
    #[test]
    fn current_history_records_require_end_and_session_cleanup() {
        let (result, sent) = attempt(
            vec![
                (0x7e8, vec![3, 0x7f, 0xa9, 0x78]),
                (0x5e8, vec![0x81, 1, 7, 0, 0x12]),
                (0x5e8, vec![0x81, 0, 0, 0, 0xff]),
            ],
            true,
        );
        let records = result.unwrap();
        assert_eq!(records[0].code, "P0107");
        assert_eq!(records[0].status, 0x12);
        assert_eq!(
            sent,
            vec![
                vec![2, 0x10, 2, 0, 0, 0, 0, 0],
                vec![3, 0xa9, 0x81, 0x12, 0, 0, 0, 0],
                vec![1, 0x20, 0, 0, 0, 0, 0, 0]
            ]
        );
    }
    #[test]
    fn silence_rejection_or_missing_end_are_errors_and_still_stop() {
        for frames in [
            vec![],
            vec![(0x5e8, vec![0x81, 1, 7, 0, 2])],
            vec![(0x7e8, vec![3, 0x7f, 0xa9, 0x31])],
            vec![(0x123, vec![0x81, 0, 0, 0, 0xff])],
        ] {
            let (result, sent) = attempt(frames, true);
            assert!(result.is_err());
            assert_eq!(sent.last().unwrap(), &vec![1, 0x20, 0, 0, 0, 0, 0, 0]);
        }
    }
    #[test]
    fn verified_empty_is_distinct_from_cleanup_failure() {
        let end = vec![(0x5e8, vec![0x81, 0, 0, 0, 0xff])];
        assert!(attempt(end.clone(), true).0.unwrap().is_empty());
        assert!(attempt(end, false)
            .0
            .unwrap_err()
            .contains("cleanup failed"));
    }
}
