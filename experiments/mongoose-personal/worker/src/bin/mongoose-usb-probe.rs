// SPDX-License-Identifier: MPL-2.0
//! Android Mongoose beta worker. The Android app owns USB effects.
use std::{
    fs::OpenOptions,
    io::{BufReader, Write},
    net::TcpStream,
    time::Duration,
};
use tech2_emu::adapters::{
    common::usb::SocketUsb,
    mongoose::live::{self, Mode},
};
fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 4 || args[1].len() != 32 || !args[1].bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("Expected port, session token, new report and test mode".into());
    }
    let port: u16 = args[0].parse().map_err(|_| "Invalid local port")?;
    if port < 1024 {
        return Err("Invalid local port".into());
    }
    let mode = match args[3].as_str() {
        "startup" => Mode::Startup,
        "hs_channel" => Mode::HighSpeedChannel,
        "vin" => Mode::Vin,
        "swcan" => Mode::SingleWireRead,
        "swcan_wake" => Mode::SingleWireWakeRead,
        _ => return Err("Unknown Mongoose beta test".into()),
    };
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&args[2]).map_err(|e| e.to_string())?;
    let stream = TcpStream::connect_timeout(
        &std::net::SocketAddr::from(([127, 0, 0, 1], port)),
        Duration::from_secs(2),
    )
    .map_err(|e| e.to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|e| e.to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .map_err(|e| e.to_string())?;
    stream.set_nodelay(true).map_err(|e| e.to_string())?;
    let mut io = SocketUsb(BufReader::new(stream));
    if io.command(&format!("HELLO {}", args[1]))? != "READY mongoose" {
        return Err("Wrong Android transport/session".into());
    }
    let result = live::run(&mut io, mode);
    file.write_all(serde_json::to_string_pretty(&result).unwrap().as_bytes())
        .map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    if result["status"] != "passed" {
        return Err("Mongoose test failed; see the private result report".into());
    }
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
