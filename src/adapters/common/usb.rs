// SPDX-License-Identifier: MPL-2.0
//! Bounded Android socket-to-USB byte transport. No adapter opcodes or CAN interpretation.
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpStream,
};

pub trait UsbTransport {
    fn write(&mut self, bytes: &[u8]) -> Result<(), String>;
    /// A bounded read; empty bytes mean no data yet.
    fn read(&mut self) -> Result<Vec<u8>, String>;
}

pub struct SocketUsb(pub BufReader<TcpStream>);
pub(crate) fn hex(b: &[u8]) -> String {
    b.iter().map(|v| format!("{v:02X}")).collect()
}
impl SocketUsb {
    pub fn command(&mut self, cmd: &str) -> Result<String, String> {
        self.0
            .get_mut()
            .write_all(format!("{cmd}\n").as_bytes())
            .map_err(|e| e.to_string())?;
        let mut line = String::new();
        let n = self
            .0
            .by_ref()
            .take(32775)
            .read_line(&mut line)
            .map_err(|e| e.to_string())?;
        if n == 0 || n >= 32775 || !line.ends_with('\n') {
            return Err("USB controller closed/line limit".into());
        }
        Ok(line.trim_end().into())
    }
}
impl UsbTransport for SocketUsb {
    fn write(&mut self, bytes: &[u8]) -> Result<(), String> {
        if self.command(&format!("TX {}", hex(bytes)))? != "TXOK" {
            return Err("USB write not accepted in full".into());
        }
        Ok(())
    }
    fn read(&mut self) -> Result<Vec<u8>, String> {
        let reply = self.command("READ")?;
        if reply == "EMPTY" {
            return Ok(Vec::new());
        }
        let s = reply.strip_prefix("RX ").ok_or("Invalid USB response")?;
        if s.len() > 32768 || s.len() % 2 != 0 || !s.is_ascii() {
            return Err("Invalid USB hex length".into());
        }
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|_| "Invalid USB hex".into()))
            .collect()
    }
}
