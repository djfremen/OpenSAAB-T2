// SPDX-License-Identifier: MPL-2.0
//! Standalone, hardware-free CANDi CPU research run.
use std::io::{self, Write};
use std::path::Path;
use tech2_emu::candi_cpu::{Firmware, Machine, StopReason, APPLICATION_ENTRY, DEFAULT_BUDGET};

fn run() -> Result<u8, (u8, String)> {
    let mut args: Vec<_> = std::env::args_os().skip(1).collect();
    let device_info = args.last().is_some_and(|arg| arg == "--device-info-probe");
    if device_info {
        args.pop();
    }
    if args.len() == 1 && args[0] == "--help" {
        println!("Usage: candi-run FIRMWARE [INSTRUCTION_BUDGET] [--device-info-probe]\nHardware-free application-entry experiment. Default budget: {DEFAULT_BUDGET}.\nExit: 2 input error, 3 incomplete (device/budget/STOP), 4 output error.");
        return Ok(0);
    }
    if args.is_empty() || args.len() > 2 {
        return Err((
            2,
            "Usage: candi-run FIRMWARE [INSTRUCTION_BUDGET] [--device-info-probe]".into(),
        ));
    }
    let budget = match args.get(1) {
        None => {
            if device_info {
                2_000_000
            } else {
                DEFAULT_BUDGET
            }
        }
        Some(n) => n
            .to_str()
            .and_then(|s| s.parse::<u64>().ok())
            .filter(|n| *n > 0)
            .ok_or((2, "instruction budget must be a positive integer".into()))?,
    };
    if device_info && budget <= 100_000 {
        return Err((
            2,
            "device info probe needs a budget above 100000 for startup and response".into(),
        ));
    }
    let firmware = Firmware::load(Path::new(&args[0])).map_err(|e| (2, e))?;
    let mut machine = Machine::new(firmware);
    if device_info {
        while machine.attempted() < 100_000 && machine.step(budget) {}
        // ICD DeviceControl / ReadInfo plus additive checksum: a local request.
        machine
            .receive_serial_frame(&[0x90, 0x03, 0x6d])
            .map_err(|e| (3, e))?;
    }
    while machine.step(budget) {}
    let s = machine.snapshot();
    let mut out = io::BufWriter::new(io::stdout().lock());
    let result = (|| -> io::Result<()> {
        writeln!(out, "CANDI profile=msi-addressed-download cpu=M68EC020/CPU32 entry={APPLICATION_ENTRY:#010x} external_tx=false")?;
        writeln!(out, "LINK origin=local-virtual-wire device_info_probe={device_info} rx_bytes={} rx_breaks={} tx={:02x?} ignored_rom_writes={} external_tx=false", s.serial_rx_count, s.serial_break_count, s.serial_tx, s.ignored_rom_write_count)?;
        for a in &s.peripheral_writes {
            writeln!(
                out,
                "WRITE address={:#010x} width={} value={:#010x} model=startup-registers-only external_tx=false",
                a.address, a.width, a.value
            )?;
        }
        for i in &s.recent {
            writeln!(out, "RECENT pc={:#010x} opcode={:#06x}", i.pc, i.opcode)?;
        }
        writeln!(out, "STOP attempted={} completed={} cycles={} pc={:#010x} sp={:#010x} vbr={:#010x} peripheral_writes={} reason={}", s.attempted, s.completed, s.cycles, s.pc, s.sp, s.vbr, s.peripheral_write_count, s.reason.as_ref().map(ToString::to_string).unwrap_or_default())?;
        out.flush()
    })();
    result.map_err(|e| (4, format!("CANDi output: {e}")))?;
    // Reaching an unsupported device, instruction or budget is never boot success.
    debug_assert!(matches!(
        s.reason,
        Some(StopReason::UnsupportedAccess(_) | StopReason::Cpu(_) | StopReason::Budget)
    ));
    Ok(3)
}

fn main() -> std::process::ExitCode {
    match run() {
        Ok(code) => code.into(),
        Err((code, message)) => {
            eprintln!("{message}");
            code.into()
        }
    }
}
