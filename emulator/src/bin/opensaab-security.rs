// SPDX-License-Identifier: MPL-2.0
//! One bounded schema-1 JSON request on stdin; never takes credentials or VIN in argv.
use std::io::{self, Read};
fn main() {
    let mut input = Vec::new();
    let result = (|| -> Result<serde_json::Value, String> {
        io::stdin()
            .take(2 * 1024 * 1024 + 1)
            .read_to_end(&mut input)
            .map_err(|_| "Cannot read request")?;
        if input.len() > 2 * 1024 * 1024 {
            return Err("Request too large".into());
        }
        let v = serde_json::from_slice(&input).map_err(|_| "Invalid request JSON")?;
        tech2_emu::security::dispatch(&v)
    })();
    match result {
        Ok(v) => println!("{}", serde_json::json!({"ok":true,"result":v})),
        Err(e) => {
            println!("{}", serde_json::json!({"ok":false,"error":e}));
            std::process::exit(1);
        }
    }
}
