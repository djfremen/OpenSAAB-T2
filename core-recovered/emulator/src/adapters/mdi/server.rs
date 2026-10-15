// SPDX-License-Identifier: MPL-2.0
//! Internal bounded x86 raw worker. No host diagnostic requests in bridge mode.
use super::{
    native::Native,
    raw::{self, Session},
};
use std::{
    io::{BufRead, BufReader, Read},
    net::TcpListener,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

fn run(session: &mut Session<Native>, port: Option<u16>) -> Result<[u64; 3], String> {
    let listener = port
        .map(|p| TcpListener::bind(("127.0.0.1", p)))
        .transpose()
        .map_err(|e| e.to_string())?;
    session.start(port.is_some())?;
    let (sender, commands) = mpsc::sync_channel::<String>(16);
    if let Some(listener) = listener {
        thread::spawn(move || {
            if let Ok((stream, _)) = listener.accept() {
                let _ = stream.set_read_timeout(Some(Duration::from_secs(6)));
                let mut reader = BufReader::new(stream);
                loop {
                    let mut line = String::new();
                    match reader.by_ref().take(257).read_line(&mut line) {
                        Ok(n) if n > 0 && n <= 256 => {
                            if sender.try_send(line.trim().into()).is_err() {
                                break;
                            }
                        }
                        _ => break,
                    }
                }
            }
        });
    }
    println!("BRIDGE READY version=1 raw=true generated_requests=0");
    let started = Instant::now();
    let mut heartbeat = started;
    let mut counts = [0u64; 3];
    let limit = if port.is_some() {
        Duration::from_secs(300)
    } else {
        Duration::from_secs(8)
    };
    loop {
        if started.elapsed() >= limit {
            if port.is_some() {
                return Err("MDI raw session deadline".into());
            }
            break;
        }
        if port.is_some() {
            loop {
                match commands.try_recv() {
                    Ok(line) => {
                        heartbeat = Instant::now();
                        match line.as_str() {
                            "PING" => {}
                            "QUIT" => return Ok(counts),
                            _ => {
                                let (c, t) = session.transmit(&line)?;
                                println!("BRIDGE ACCEPT {c} {t}");
                            }
                        }
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        return Err("MDI command socket disconnected".into())
                    }
                }
            }
            if heartbeat.elapsed() > Duration::from_secs(5) {
                return Err("MDI host heartbeat expired".into());
            }
        }
        for c in [0, 2] {
            for _ in 0..32 {
                let Some(m) = session.receive(c)? else {
                    break;
                };
                counts[c] += 1;
                println!(
                    "BRIDGE RX {c} {} {:08X} {} {}",
                    m.protocol,
                    m.rx_status,
                    m.timestamp,
                    raw::hex(&m.data[..m.data_size as usize])
                );
            if counts.iter().sum::<u64>() > 300000 {
                    return Err("MDI raw receive record limit".into());
                }
            }
        }
        thread::sleep(Duration::from_millis(1));
    }
    Ok(counts)
}
pub fn main(port: Option<u16>) -> u8 {
    // Even a stuck native DLL is bounded. Hard exit means cleanup is unconfirmed.
    thread::spawn(|| {
        thread::sleep(Duration::from_secs(315));
        std::process::exit(124);
    });
    let mut s = match Native::new() {
        Ok(d) => Session::new(d),
        Err(e) => {
            println!("BRIDGE ERROR {e}");
            return 1;
        }
    };
    let result = run(&mut s, port);
    let clean = s.close();
    match &result {
        Ok(counts) => println!(
            "BRIDGE CALL RX_COUNTS hs={} sw={} wake_notices={}",
            counts[0], counts[2], s.wake_notices
        ),
        Err(e) => println!("BRIDGE ERROR {e}"),
    }
    println!("BRIDGE CLEAN success={clean}");
    if result.is_ok() && clean {
        println!("BRIDGE EXIT 0");
        0
    } else {
        println!("BRIDGE EXIT 1");
        1
    }
}
pub fn probe() -> u8 {
    thread::spawn(|| {
        thread::sleep(Duration::from_secs(25));
        std::process::exit(124);
    });
    let mut s = match Native::new() {
        Ok(d) => Session::new(d),
        Err(e) => {
            println!("BRIDGE ERROR {e}");
            return 1;
        }
    };
    let result = s.start(false).and_then(|_| s.probe_identity());
    let clean = s.close();
    match &result {
        Ok(payload) => println!(
            "MDI_IBUS_IDENTITY={}",
            serde_json::json!({"positive_payload_hex":raw::hex(payload),"origin":"physical","raw_protocol":raw::SW_PROTOCOL,"host_experiment":true,"request_count":1,"cleanup_ok":clean,"cold_start_qualified":false})
        ),
        Err(e) => println!("BRIDGE ERROR {e}"),
    }
    println!("BRIDGE CLEAN success={clean}");
    if result.is_ok() && clean {
        0
    } else {
        1
    }
}
