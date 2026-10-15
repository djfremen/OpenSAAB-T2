// SPDX-License-Identifier: MPL-2.0
//! Native macOS/Windows adapter identification. Never opens a vehicle channel.

#[cfg(target_os = "macos")]
static CANCEL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[cfg(target_os = "macos")]
extern "C" fn cancel_probe(_: libc::c_int) {
    CANCEL.store(true, std::sync::atomic::Ordering::Relaxed);
}

#[cfg(target_os = "macos")]
struct InterruptGuard(libc::sigaction);

#[cfg(target_os = "macos")]
impl InterruptGuard {
    fn install() -> std::io::Result<Self> {
        // SAFETY: initialized signal mask and extern C handler which only sets
        // a lock-free atomic flag. old receives the previous signal action.
        unsafe {
            let mut action: libc::sigaction = std::mem::zeroed();
            let mut old: libc::sigaction = std::mem::zeroed();
            libc::sigemptyset(&mut action.sa_mask);
            action.sa_sigaction = cancel_probe as *const () as usize;
            if libc::sigaction(libc::SIGINT, &action, &mut old) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(Self(old))
        }
    }
}

#[cfg(target_os = "macos")]
impl Drop for InterruptGuard {
    fn drop(&mut self) {
        // SAFETY: restore the action returned by the successful install call.
        unsafe {
            libc::sigaction(libc::SIGINT, &self.0, std::ptr::null_mut());
        }
    }
}

#[cfg(target_os = "macos")]
fn run() -> Result<(), Box<dyn std::error::Error>> {
    use std::time::{Duration, Instant};
    use tech2_emu::{chipsoft_probe, chipsoft_serial};

    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.as_slice() == ["--help"] || args.as_slice() == ["-h"] {
        println!("Usage: chipsoft-probe --list\n       chipsoft-probe --port /dev/cu.usbmodemNAME [--timeout-ms 2000]\n\nLists USB CDC candidates without opening them. Explicit --port sends only\nGET_INFO (01 00 00 00 00 00 00 00), with a 1..30000 ms deadline.\nNo vehicle channel, OBD-II request or ECU write. Ctrl-C exits.\nCandidate names do not establish device identity; the reply must identify Chipsoft Pro.");
        return Ok(());
    }
    if args.is_empty() || args.as_slice() == ["--list"] {
        let candidates = chipsoft_serial::candidates()?;
        if candidates.is_empty() {
            println!(
                "No USB CDC callout ports found. Connect the Chipsoft by USB and run --list again."
            );
        }
        for path in candidates {
            println!("{}  USB CDC candidate; identity unverified", path.display());
        }
        return Ok(());
    }
    if !((args.len() == 2 || args.len() == 4 && args[2] == "--timeout-ms") && args[0] == "--port") {
        return Err(
            "Usage: chipsoft-probe --list | --port /dev/cu.usbmodemNAME [--timeout-ms 2000]".into(),
        );
    }
    let timeout_ms = if args.len() == 4 {
        args[3].parse::<u64>()?
    } else {
        2000
    };
    if !(1..=30000).contains(&timeout_ms) {
        return Err("timeout must be 1..30000 milliseconds".into());
    }
    let start = Instant::now();
    eprintln!(
        "PROBE_START port={} deadline_ms={timeout_ms} operation=GET_INFO vehicle_channel=closed",
        args[1]
    );
    let interrupt = InterruptGuard::install()?;
    let mut port = chipsoft_serial::Port::open(std::path::Path::new(&args[1]))?;
    let opened_at = start.elapsed();
    let remaining = Duration::from_millis(timeout_ms)
        .checked_sub(opened_at)
        .filter(|budget| !budget.is_zero())
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "identification deadline expired during serial open",
            )
        })?;
    // Collect a bounded log in memory so a blocked stdout consumer cannot hold
    // the transport worker beyond its deadline. Print only after port cleanup.
    let mut events = vec![format!("{:8.3}ms SERIAL_OPEN raw=true nonblocking=true baud=inherited explicit_dtr_rts_assertion=false stale_queues=purged", opened_at.as_secs_f64() * 1000.0)];
    let mut dropped = 0u64;
    let result = chipsoft_probe::identify(&mut port, remaining, &CANCEL, |event| {
        if events.len() >= 256 {
            dropped += 1;
            return;
        }
        let detail = match event {
            chipsoft_probe::Event::SerialWrite(bytes) => {
                format!("SERIAL_TX accepted_by_os=true bytes={}", hex(bytes))
            }
            chipsoft_probe::Event::SerialRead(bytes) => {
                format!("SERIAL_RX bytes={}", hex(bytes))
            }
            chipsoft_probe::Event::Reply(frame) => format!(
                "ADAPTER_REPLY opcode={:04X} status={:04X} length={}",
                frame.opcode,
                frame.status,
                frame.payload.len()
            ),
        };
        events.push(format!(
            "{:8.3}ms {detail}",
            start.elapsed().as_secs_f64() * 1000.0
        ));
    });
    drop(port);
    drop(interrupt);
    for event in events {
        eprintln!("{event}");
    }
    if dropped != 0 {
        eprintln!("LOG_TRUNCATED dropped_events={dropped}");
    }
    eprintln!(
        "SERIAL_CLOSED elapsed_ms={:.3}",
        start.elapsed().as_secs_f64() * 1000.0
    );
    let identity = result?;
    println!("Identified: {identity}\nAdapter identification only; vehicle communication has not been tested.");
    Ok(())
}

#[cfg(target_os = "macos")]
fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(target_os = "windows")]
fn run() -> Result<(), Box<dyn std::error::Error>> {
    use std::io::{self, Read, Write};
    use std::sync::atomic::AtomicBool;
    use std::time::Duration;
    use tech2_emu::{chipsoft_probe, chipsoft_serial, adapters::common::usb::UsbTransport};
    // Adapt the existing Windows CDC binding to the shared GET_INFO-only probe.
    // It never opens a CAN channel or reads the VIN during firmware resume.
    struct Stream { port: chipsoft_serial::Port, pending: Vec<u8> }
    impl Read for Stream {
        fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
            if out.is_empty() { return Ok(0); }
            if self.pending.is_empty() { self.pending = UsbTransport::read(&mut self.port).map_err(io::Error::other)?; }
            if self.pending.is_empty() { return Err(io::ErrorKind::WouldBlock.into()); }
            let n=out.len().min(self.pending.len());out[..n].copy_from_slice(&self.pending[..n]);self.pending.drain(..n);Ok(n)
        }
    }
    impl Write for Stream {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> { UsbTransport::write(&mut self.port,bytes).map_err(io::Error::other)?;Ok(bytes.len()) }
        fn flush(&mut self) -> io::Result<()> { Ok(()) }
    }
    let args: Vec<_>=std::env::args().skip(1).collect();
    if args.as_slice()==["--help"] || args.as_slice()==["-h"] || args.as_slice()==["--list"] {
        println!("Usage: chipsoft-probe --port COMn [--timeout-ms 2000]\nWindows CDC GET_INFO only. Select the Chipsoft COM port shown by the app. No vehicle channel or VIN read.");return Ok(());
    }
    if !((args.len()==2 || args.len()==4 && args[2]=="--timeout-ms") && args[0]=="--port") { return Err("Use --port COMn [--timeout-ms 2000]".into()); }
    let timeout=if args.len()==4 {args[3].parse::<u64>()?}else{2000};
    if !(1..=30000).contains(&timeout) {return Err("timeout must be 1..30000 milliseconds".into());}
    let mut stream=Stream{port:chipsoft_serial::Port::open(std::path::Path::new(&args[1]))?,pending:Vec::new()};
    let result=chipsoft_probe::identify(&mut stream,Duration::from_millis(timeout),&AtomicBool::new(false), |_|{});
    drop(stream);
    println!("Identified: {}\nAdapter identification only; vehicle communication has not been tested.",result?);
    Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn run() -> Result<(), Box<dyn std::error::Error>> {
    Err("chipsoft-probe currently supports native macOS and Windows USB CDC ports only".into())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("PROBE_FAILED {error}");
        std::process::exit(1);
    }
}
