// SPDX-License-Identifier: MPL-2.0
//! Thin bounded-file binding for the exact shared card-derived prompt parser.
use std::{fs,io::{Read,Write},path::Path};
const CARD_SIZE:usize=32*1024*1024;
fn unique()->String{format!("{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos())}
fn read(path:&Path,limit:usize)->Result<Vec<u8>,String>{
 let meta=fs::symlink_metadata(path).map_err(|e|e.to_string())?;
 if !meta.is_file()||meta.len()>limit as u64{return Err("Unexpected prompt input file".into());}
 let mut bytes=Vec::new();fs::File::open(path).map_err(|e|e.to_string())?.take(limit as u64+1).read_to_end(&mut bytes).map_err(|e|e.to_string())?;
 if bytes.len()>limit{return Err("Prompt file grew beyond bound".into());}Ok(bytes)
}
fn write_json(path:&Path,value:&serde_json::Value)->Result<(),String>{
 let mut options=fs::OpenOptions::new();options.write(true).create_new(true);
 #[cfg(unix)]{use std::os::unix::fs::OpenOptionsExt;options.mode(0o600);}
 let mut file=options.open(path).map_err(|e|e.to_string())?;file.write_all(value.to_string().as_bytes()).map_err(|e|e.to_string())?;file.sync_all().map_err(|e|e.to_string())
}
#[path = "../shared/security-prompt/prompt.rs"]
pub mod prompt;
