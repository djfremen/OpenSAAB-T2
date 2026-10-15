// SPDX-License-Identifier: MPL-2.0
//! Explicit simulator bridge. One authenticated, fresh read; no arbitrary CAN API.
use super::*;
use serde_json::{json, Value};
use std::{net::{TcpListener, Shutdown}, path::Path, sync::{Arc, atomic::{AtomicBool, Ordering}}};
fn packet(reader: &mut BufReader<TcpStream>) -> Result<Value, String> {
    let s = line(reader)?;
    if s.len() > 4096 { return Err("Bridge request exceeded limit".into()); }
    serde_json::from_str(&s).map_err(|_| "Invalid bridge request".into())
}
fn validated_start(v: &Value, token: &str) -> Result<String, String> {
    let id = v["attempt_id"].as_str().unwrap_or("");
    if v["schema"] != 1 || v["op"] != "read_vin" || v["adapter"] != "vcx_nano"
        || v["token"] != token || id.len() != 36
        || !id.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-') {
        return Err("Bridge authentication, adapter or attempt identity rejected".into());
    }
    Ok(id.into())
}
pub fn serve(args: &[String]) -> Result<(), String> {
    if args.len() != 3 { return Err("Usage: --ios-bridge-once EXPLICIT_CDC_PORT PRIVATE_TOKEN_FILE NEW_RESULT_PATH".into()); }
    let token = std::fs::read_to_string(&args[1]).map_err(|e| e.to_string())?;
    let token = token.trim();
    if token.len()!=32 || !token.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("Bridge requires a private 128-bit session token".into());
    }
    if Path::new(&args[2]).exists() || Path::new(&format!("{}.frames.jsonl",args[2])).exists() {
        return Err("Bridge result paths must be new".into());
    }
    let listener = TcpListener::bind(("127.0.0.1",8768)).map_err(|e| e.to_string())?;
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    println!("IOS_BRIDGE_READY schema=1 adapter=vcx_nano endpoint=loopback:8768 single_attempt=true cold_ready=false");
    let deadline = Instant::now()+Duration::from_secs(90);
    let mut stream = loop {
        match listener.accept() {
            Ok((stream,_)) => break stream,
            Err(e) if e.kind()==std::io::ErrorKind::WouldBlock && Instant::now()<deadline => std::thread::sleep(Duration::from_millis(20)),
            Err(e) => return Err(format!("Bridge accept unavailable or expired: {e}")),
        }
    };
    drop(listener);
    // macOS accept inherits O_NONBLOCK from the polling listener. Restore
    // blocking mode before the cancellation reader; idle is not cancellation.
    stream.set_nonblocking(false).map_err(|e|e.to_string())?;
    stream.set_read_timeout(Some(Duration::from_secs(5))).map_err(|e| e.to_string())?;
    stream.set_write_timeout(Some(Duration::from_secs(2))).map_err(|e| e.to_string())?;
    let mut reader=BufReader::new(stream.try_clone().map_err(|e|e.to_string())?);
    let attempt=validated_start(&packet(&mut reader)?,token)?;
    stream.write_all(b"{\"type\":\"started\",\"schema\":1,\"adapter\":\"vcx_nano\"}\n").map_err(|e|e.to_string())?;
    reader.get_mut().set_read_timeout(Some(Duration::from_secs(45))).map_err(|e|e.to_string())?;
    let cancelled=Arc::new(AtomicBool::new(false));let cancel=cancelled.clone();
    // EOF, invalid follow-up or Stop cancels; none starts a second request.
    let monitor=std::thread::spawn(move || {let result=packet(&mut reader);println!("IOS_BRIDGE_TERMINATION {}",if result.is_ok(){"explicit-follow-up"}else{"socket-close-or-deadline"});cancel.store(true,Ordering::SeqCst);});
    let result=run_probe(vec!["--port".into(),args[0].clone(),args[2].clone(),"--hs-vin-check".into()],Some(cancelled));
    let mut report:Value=std::fs::read(&args[2]).ok().and_then(|b|serde_json::from_slice(&b).ok())
        .unwrap_or_else(||json!({"status":"failed","usb_closed":false,"channel_cleanup":false,"error":result.err()}));
    report["status"]=json!(if report["status"]=="hs_vin_received" {"vin_received"}else{"failed"});
    report["attempt_id"]=json!(attempt);
    report["request_origin"]=json!("host-startup-discovery");
    report["host_vehicle_request_attempts"]=json!(if report["vehicle_tx"].as_u64().unwrap_or(0)>0 {1}else{0});
    report["host_vehicle_requests_written"]=report["host_vehicle_request_attempts"].clone();
    report["automatic_replay"]=json!(false);report["cold_ready"]=json!(false);
    report["result"]=json!({"adapter_model":"vcx_nano","adapter_label":"VXDIAG Nano · USB bridge","profile_id":"nano-macos-native-hscan","product":report["hardware"],"chip_firmware":report["firmware"],"vin":report["vin"]});
    let reply=json!({"schema":1,"type":"result","attempt_id":attempt,"report":report}).to_string()+"\n";
    std::fs::write(format!("{}.bridge.json",args[2]),reply.as_bytes()).map_err(|e|e.to_string())?;
    let sent=stream.write_all(reply.as_bytes()).map_err(|e|e.to_string());
    let _=stream.shutdown(Shutdown::Both);let _=monitor.join();sent
}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn macos_idle_client_is_not_a_cancel_signal() {
        let listener=TcpListener::bind(("127.0.0.1",0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let address=listener.local_addr().unwrap();
        let (release_tx,release_rx)=std::sync::mpsc::channel();
        let peer=std::thread::spawn(move || {
            let mut client=TcpStream::connect(address).unwrap();
            client.write_all(b"{\"schema\":1}\n").unwrap();
            release_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        });
        let end=Instant::now()+Duration::from_secs(2);
        let (mut socket,_)=loop {
            match listener.accept() {Ok(v)=>break v,Err(e) if e.kind()==std::io::ErrorKind::WouldBlock && Instant::now()<end=>std::thread::sleep(Duration::from_millis(2)),other=>panic!("{other:?}")}
        };
        socket.set_nonblocking(false).unwrap();
        socket.set_read_timeout(Some(Duration::from_millis(80))).unwrap();
        let mut reader=BufReader::new(socket);
        assert_eq!(packet(&mut reader).unwrap()["schema"],1);
        let start=Instant::now();assert!(packet(&mut reader).is_err());
        assert!(start.elapsed()>=Duration::from_millis(50),"idle must wait for its read deadline, rather than cancel immediately");
        release_tx.send(()).unwrap();peer.join().unwrap();
    }
    #[test] fn rejects_wrong_token_family_and_replayed_identity() {
        let token="a".repeat(32);let mut p=json!({"schema":1,"op":"read_vin","adapter":"vcx_nano","token":token,"attempt_id":"12345678-1234-1234-1234-123456789abc"});
        assert!(validated_start(&p,&token).is_ok());
        for (field,value) in [("token",json!("b".repeat(32))),("adapter",json!("chipsoft")),("op",json!("raw_can")),("attempt_id",json!("saved")),("schema",json!(2))] {
            let saved=p[field].clone();p[field]=value;assert!(validated_start(&p,&token).is_err());p[field]=saved;
        }
    }
}
