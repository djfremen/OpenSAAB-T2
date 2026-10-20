// SPDX-License-Identifier: MPL-2.0
#[cfg(unix)]
fn main() {
    use serde_json::{json, Value};
    use std::{
        io::{self, BufRead, Write},
        sync::{
            atomic::{AtomicBool, Ordering},
            mpsc, Arc, Mutex,
        },
    };
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 2 || args[0] != "--bridge" || !std::path::Path::new(&args[1]).is_absolute() {
        eprintln!("Usage: opensaab-mcp --bridge /absolute/private/MCP/directory");
        std::process::exit(2);
    }
    let cancelled = Arc::new(AtomicBool::new(false));
    let active = Arc::new(Mutex::new(Value::Null));
    let (tx, rx) = mpsc::sync_channel(16);
    let reader_cancel = cancelled.clone();
    let reader_active = active.clone();
    std::thread::spawn(move || {
        let mut input = io::stdin().lock();
        loop {
            let mut bytes = Vec::new();
            // Bound a line while reading, rather than allocating an unbounded JSON message.
            let mut oversized = false;
            loop {
                let chunk = match input.fill_buf() {
                    Ok(v) => v,
                    Err(_) => return,
                };
                if chunk.is_empty() {
                    if bytes.is_empty() {
                        return;
                    }
                    break;
                }
                let end = chunk.iter().position(|b| *b == b'\n').map(|n| n + 1);
                let count = end.unwrap_or(chunk.len());
                if bytes.len() + count <= 65536 {
                    bytes.extend_from_slice(&chunk[..count]);
                } else {
                    oversized = true;
                }
                input.consume(count);
                if end.is_some() {
                    break;
                }
            }
            let value = if oversized {
                Err("Request exceeds 64 KiB".to_owned())
            } else {
                serde_json::from_slice::<Value>(&bytes).map_err(|_| "Parse error".into())
            };
            if let Ok(request) = &value {
                if request["method"] == "notifications/cancelled" {
                    if request["params"]["requestId"] == *reader_active.lock().unwrap() {
                        reader_cancel.store(true, Ordering::Relaxed);
                    }
                    continue;
                }
            }
            if tx.send(value).is_err() {
                return;
            }
        }
    });
    let exe = std::env::current_exe().unwrap();
    let version = exe
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.join("Resources/build.json"))
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .and_then(|v| v["version"].as_str().map(str::to_owned))
        .unwrap_or("development".into());
    let bridge = tech2_emu::mcp::Bridge::new(args[1].clone().into(), cancelled.clone());
    let mut server = tech2_emu::mcp::Server::new(bridge, version);
    let mut out = io::stdout().lock();
    for message in rx {
        cancelled.store(false, Ordering::Relaxed);
        let response = match message {
            Ok(request) => {
                *active.lock().unwrap() = request.get("id").cloned().unwrap_or(Value::Null);
                server.dispatch(request)
            }
            Err(error) => {
                Some(json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":error}}))
            }
        };
        *active.lock().unwrap() = Value::Null;
        if let Some(response) = response {
            if writeln!(out, "{response}")
                .and_then(|_| out.flush())
                .is_err()
            {
                break;
            }
        }
    }
}
#[cfg(not(unix))]
fn main() {
    eprintln!("This MCP native binding is currently a macOS preview.");
    std::process::exit(2);
}
