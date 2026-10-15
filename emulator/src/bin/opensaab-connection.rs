// SPDX-License-Identifier: MPL-2.0
//! Serial native-host binding for the same startup/protocol sessions as iOS.
//! This executable performs no Bluetooth or HTTP I/O and never accepts a VIN in argv.
use serde_json::{json, Value};
use std::io::{self, BufRead, Read, Write};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::args_os()
        .nth(1)
        .ok_or("Missing app data directory")?;
    let mut connection = tech2_emu::connection_startup::Session::new(root.into());
    let mut protocol = tech2_emu::adapters::vlinker::Session::startup().owned();
    let stdin = io::stdin();
    let mut input = stdin.lock();
    let mut output = io::stdout().lock();
    loop {
        let mut bytes = Vec::new();
        let count = (&mut input).take(300_001).read_until(b'\n', &mut bytes)?;
        if count == 0 {
            break;
        }
        if count > 300_000 || bytes.last() != Some(&b'\n') {
            return Err("Oversized or incomplete event".into());
        }
        let event: Value = serde_json::from_slice(&bytes)?;
        let result = match event["scope"].as_str() {
            Some("manifest") => opensaab_vlinker_workflow::manifest(),
            Some("connection") => connection.event(&event),
            Some("vlinker") if event["op"] == "configure" => match event["mode"].as_str() {
                Some("startup") => {
                    protocol = tech2_emu::adapters::vlinker::Session::startup().owned();
                    json!({"state":"idle"})
                }
                Some("vin") | Some("identity") => {
                    protocol = tech2_emu::adapters::vlinker::Session::new(event["mode"] == "vin").owned();
                    json!({"state":"idle"})
                }
                Some("release") => {
                    protocol = tech2_emu::adapters::vlinker::Session::new(false);
                    json!({"state":"idle"})
                }
                Some("ecm_info") => {
                    protocol = tech2_emu::adapters::vlinker::Session::probe(false).owned();
                    json!({"state":"idle"})
                }
                Some("ecm_dtc") => {
                    protocol = tech2_emu::adapters::vlinker::Session::probe(true).owned();
                    json!({"state":"idle"})
                }
                _ => json!({"state":"failed","error":"Unsupported probe mode"}),
            },
            Some("vlinker") => protocol.event(&event),
            Some("reports") => {
                let mut request = event.clone(); request["schema"] = json!(1);
                request["root"] = json!(connection.root_path());
                match opensaab_vlinker_workflow::reports::dispatch(&request) {
                    Ok(value) => value, Err(error) => json!({"error":error}),
                }
            }
            _ => json!({"state":"failed","error":"Unknown effect scope"}),
        };
        writeln!(output, "{result}")?;
        output.flush()?;
    }
    Ok(())
}
