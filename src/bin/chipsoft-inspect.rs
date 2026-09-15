// SPDX-License-Identifier: MPL-2.0
//! Offline USBPcap validator. This executable never opens a serial port.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{self, BufReader, BufWriter, Read, Write};
use tech2_emu::chipsoft::{opcode_name, Decoder};

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

#[derive(Clone, Copy)]
struct Endian(bool);
impl Endian {
    fn u16(self, b: &[u8]) -> u16 {
        if self.0 {
            u16::from_le_bytes([b[0], b[1]])
        } else {
            u16::from_be_bytes([b[0], b[1]])
        }
    }
    fn u32(self, b: &[u8]) -> u32 {
        let a = [b[0], b[1], b[2], b[3]];
        if self.0 {
            u32::from_le_bytes(a)
        } else {
            u32::from_be_bytes(a)
        }
    }
}

/// Clean EOF is allowed only between records, never in the middle of a header.
fn record_header(reader: &mut impl Read) -> io::Result<Option<[u8; 16]>> {
    let mut bytes = [0; 16];
    loop {
        match reader.read(&mut bytes[..1]) {
            Ok(0) => return Ok(None),
            Ok(_) => break,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
    reader.read_exact(&mut bytes[1..])?;
    Ok(Some(bytes))
}

fn inspect(
    reader: &mut impl Read,
    output: &mut impl Write,
    bus: u16,
    device: u16,
    verbose: bool,
) -> io::Result<()> {
    let mut header = [0; 24];
    reader.read_exact(&mut header)?;
    let (endian, nanos) = match &header[..4] {
        [0xd4, 0xc3, 0xb2, 0xa1] => (Endian(true), false),
        [0xa1, 0xb2, 0xc3, 0xd4] => (Endian(false), false),
        [0x4d, 0x3c, 0xb2, 0xa1] => (Endian(true), true),
        [0xa1, 0xb2, 0x3c, 0x4d] => (Endian(false), true),
        _ => return Err(invalid("expected classic pcap (pcapng is not supported)")),
    };
    if endian.u16(&header[4..]) != 2
        || endian.u16(&header[6..]) != 4
        || endian.u32(&header[20..]) != 249
    {
        return Err(invalid("expected pcap 2.4 with USBPcap link type 249"));
    }
    let mut decoders = [Decoder::default(), Decoder::default()];
    let mut counts = BTreeMap::<(usize, u16, u16), u64>::new();
    let mut record = 0u64;
    let mut transfers = 0u64;
    let mut usb_errors = 0u64;
    writeln!(
        output,
        "source=recorded-usb live_tx=false bus={bus} device={device} endpoints=01/81"
    )?;
    while let Some(h) = record_header(reader)? {
        record += 1;
        let size = endian.u32(&h[8..]) as usize;
        if size > 1024 * 1024 || size > endian.u32(&h[12..]) as usize {
            return Err(invalid(format!(
                "record {record}: invalid/oversized record ({size})"
            )));
        }
        let mut bytes = vec![0; size];
        reader.read_exact(&mut bytes)?;
        if bytes.len() < 27 {
            return Err(invalid(format!("record {record}: short USBPcap header")));
        }
        // USBPcap pseudoheaders stay little-endian, independently of pcap headers.
        let le = Endian(true);
        let header_len = usize::from(le.u16(&bytes));
        if header_len < 27 || header_len > size {
            return Err(invalid(format!(
                "record {record}: invalid USB header length"
            )));
        }
        if le.u16(&bytes[17..]) != bus || le.u16(&bytes[19..]) != device || bytes[22] != 3 {
            continue;
        }
        let direction = match bytes[21] {
            0x01 => 0,
            0x81 => 1,
            ep => {
                return Err(invalid(format!(
                    "record {record}: unexpected bulk endpoint {ep:02X}"
                )))
            }
        };
        let len = le.u32(&bytes[23..]) as usize;
        if len > size - header_len {
            return Err(invalid(format!("record {record}: truncated USB transfer")));
        }
        let usb_status = le.u32(&bytes[10..]);
        if usb_status != 0 {
            // A zero-data failed/cancelled read at a frame boundary contributes
            // no stream bytes. Report it without pretending the USB succeeded.
            // Never join partial replies across a transport failure.
            if len != 0 || decoders.iter().any(|decoder| decoder.finish().is_err()) {
                return Err(invalid(format!(
                    "record {record}: failed USB transfer status={usb_status:08X} with data or an incomplete frame"
                )));
            }
            usb_errors += 1;
            writeln!(output, "record={record} USB_ERROR status={usb_status:08X} endpoint={:02X} len=0 at_frame_boundary=true", bytes[21])?;
            continue;
        }
        if len == 0 {
            continue;
        }
        transfers += 1;
        let mut write_result = Ok(());
        decoders[direction].feed(&bytes[header_len..header_len + len], |frame| {
            *counts.entry((direction, frame.opcode, frame.status)).or_default() += 1;
            if verbose && write_result.is_ok() {
                write_result = (|| {
                    let seconds = endian.u32(&h);
                    let fraction = u64::from(endian.u32(&h[4..])) * if nanos { 1 } else { 1000 };
                    write!(output, "record={record} completed_at={seconds}.{fraction:09} RECORDED_{} op={:04X} {} status={:04X} len={} checksum=ok wire=",
                        ["OUT", "IN"][direction], frame.opcode, opcode_name(frame.opcode), frame.status, frame.payload.len())?;
                    for b in frame.encode().map_err(|e| invalid(e.to_string()))? { write!(output, "{b:02X} ")?; }
                    writeln!(output)
                })();
            }
        }).map_err(|e| invalid(format!("record {record}, endpoint {:02X}: {e}", bytes[21])))?;
        write_result?;
    }
    for (direction, decoder) in decoders.iter().enumerate() {
        decoder
            .finish()
            .map_err(|e| invalid(format!("EOF {}: {e}", ["OUT", "IN"][direction])))?;
    }
    if transfers == 0 {
        return Err(invalid("no matching bulk data; check bus/device selection"));
    }
    for ((direction, opcode, status), count) in &counts {
        writeln!(
            output,
            "RECORDED_{} op={opcode:04X} {:<16} status={status:04X} frames={count}",
            ["OUT", "IN"][*direction],
            opcode_name(*opcode)
        )?;
    }
    let frames: u64 = counts.values().sum();
    let device_errors: u64 = counts
        .iter()
        .filter(|((direction, _, status), _)| *direction == 1 && *status != 0)
        .map(|(_, count)| count)
        .sum();
    writeln!(output, "Validated {frames} frames from {transfers} USB transfers; {device_errors} nonzero device replies; no checksum failures or partial frames.")?;
    writeln!(
        output,
        "USB transfers with nonzero status and no data: {usb_errors}."
    )?;
    writeln!(output, "Validation covers framing only. Device status meanings, channel setup, and ECU success require separate interpretation.")
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.as_slice() == ["--help"] || args.as_slice() == ["-h"] {
        println!("Usage: chipsoft-inspect CAPTURE.pcap BUS DEVICE [--verbose]\nOffline only. Select the Chipsoft USB bus/device in the capture.\n--verbose logs every reassembled request/reply, including status and exact bytes.");
        return Ok(());
    }
    if !(args.len() == 3 || args.len() == 4 && args[3] == "--verbose") {
        return Err("Usage: chipsoft-inspect CAPTURE.pcap BUS DEVICE [--verbose]".into());
    }
    let mut reader = BufReader::new(File::open(&args[0])?);
    let mut output = BufWriter::new(io::stdout().lock());
    inspect(
        &mut reader,
        &mut output,
        args[1].parse()?,
        args[2].parse()?,
        args.len() == 4,
    )?;
    output.flush()?;
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("chipsoft-inspect: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn capture(chunks: &[&[u8]], big_endian: bool) -> Vec<u8> {
        let mut out = Vec::new();
        let u16_bytes = |v: u16| {
            if big_endian {
                v.to_be_bytes()
            } else {
                v.to_le_bytes()
            }
        };
        let u32_bytes = |v: u32| {
            if big_endian {
                v.to_be_bytes()
            } else {
                v.to_le_bytes()
            }
        };
        out.extend(u32_bytes(0xa1b2c3d4));
        out.extend(u16_bytes(2));
        out.extend(u16_bytes(4));
        for v in [0, 0, 65535, 249] {
            out.extend(u32_bytes(v));
        }
        for chunk in chunks {
            for v in [1, 2, 27 + chunk.len() as u32, 27 + chunk.len() as u32] {
                out.extend(u32_bytes(v));
            }
            let mut usb = [0u8; 27];
            usb[..2].copy_from_slice(&27u16.to_le_bytes());
            usb[16] = 1;
            usb[17..19].copy_from_slice(&1u16.to_le_bytes());
            usb[19..21].copy_from_slice(&10u16.to_le_bytes());
            usb[21] = 0x81;
            usb[22] = 3;
            usb[23..].copy_from_slice(&(chunk.len() as u32).to_le_bytes());
            out.extend(usb);
            out.extend_from_slice(chunk);
        }
        out
    }

    #[test]
    fn usb_fragmentation_and_pcap_endianness() {
        for big in [false, true] {
            let data = capture(&[&[0x10], &[0, 0, 0, 0x85, 0, 0, 0]], big);
            let mut output = Vec::new();
            inspect(&mut &data[..], &mut output, 1, 10, true).unwrap();
            let text = String::from_utf8(output).unwrap();
            assert!(text.contains("status=0085"));
            assert!(
                text.contains("Validated 1 frames from 2 USB transfers; 1 nonzero device replies")
            );
        }
    }

    #[test]
    fn rejects_truncation_wrong_device_and_corruption() {
        let full = capture(&[&[4, 0, 0, 0, 0, 0, 0, 0]], false);
        assert!(inspect(&mut &full[..], &mut Vec::new(), 1, 11, false).is_err());
        for len in 0..full.len() {
            assert!(inspect(&mut &full[..len], &mut Vec::new(), 1, 10, false).is_err());
        }
        let partial = capture(&[&[4]], false);
        assert!(inspect(&mut &partial[..], &mut Vec::new(), 1, 10, false).is_err());
        let corrupt = capture(&[&[4, 0, 0, 0, 0, 0, 1, 0]], false);
        assert!(inspect(&mut &corrupt[..], &mut Vec::new(), 1, 10, false).is_err());
    }

    #[test]
    fn empty_usb_failure_is_reported_but_cannot_bridge_partial_frames() {
        let mut empty = capture(&[&[]], false);
        // First record USB header starts at 24 + 16.
        empty[50..54].copy_from_slice(&0xc0010000u32.to_le_bytes());
        let good = capture(&[&[4, 0, 0, 0, 0, 0, 0, 0]], false);
        empty.extend_from_slice(&good[24..]);
        let mut output = Vec::new();
        inspect(&mut &empty[..], &mut output, 1, 10, false).unwrap();
        assert!(String::from_utf8(output)
            .unwrap()
            .contains("USB_ERROR status=C0010000"));

        let mut partial = capture(&[&[4]], false);
        partial.extend_from_slice(&empty[24..]);
        let error = inspect(&mut &partial[..], &mut Vec::new(), 1, 10, false).unwrap_err();
        assert!(error.to_string().contains("incomplete frame"));
    }
}
