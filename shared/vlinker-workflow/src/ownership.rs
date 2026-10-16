// SPDX-License-Identifier: MPL-2.0
//! One owner spanning startup, probe, cleanup and confirmed native transport release.
use std::sync::{atomic::{AtomicU64, Ordering}, Mutex};
static OWNER: Mutex<Option<(String, usize)>> = Mutex::new(None);
static SEQUENCE: AtomicU64 = AtomicU64::new(0);
pub fn active() -> bool { OWNER.lock().unwrap_or_else(|e| e.into_inner()).is_some() }
pub fn unique() -> String {
    format!("{}-{}-{}", std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed))
}
pub struct Lease { owner: String }
impl Lease {
    pub fn acquire(owner: &str) -> Option<Self> {
        if owner.is_empty() || owner.len() > 100 { return None; }
        let mut current = OWNER.lock().unwrap_or_else(|e| e.into_inner());
        match current.as_mut() {
            None => *current = Some((owner.to_owned(), 1)),
            Some((id, refs)) if id == owner => *refs += 1,
            _ => return None,
        }
        Some(Self { owner: owner.to_owned() })
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        let mut current = OWNER.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((id, refs)) = current.as_mut() {
            if id == &self.owner {
                *refs -= 1;
                if *refs == 0 { *current = None; }
            }
        }
    }
}
#[no_mangle]
pub extern "C" fn opensaab_vlinker_busy() -> i32 { i32::from(active()) }
