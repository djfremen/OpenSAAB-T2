// SPDX-License-Identifier: MPL-2.0
//! Opt-in CANDi firmware CPU worker. The Tech2 UART and CAN backend remain unwired.

use super::link::{CandiEndpoint, CandiLink, Tech2Endpoint};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use tech2_emu::candi_cpu::{Firmware, Machine, Snapshot, APPLICATION_ENTRY, DEFAULT_BUDGET};

#[derive(Clone, Debug, Default)]
pub struct CandiConfig {
    pub enabled: bool,
    pub firmware_path: Option<PathBuf>,
}

pub fn default_firmware_path() -> PathBuf {
    if std::path::Path::new("candi.bin").is_file() {
        PathBuf::from("candi.bin")
    } else {
        PathBuf::from("dumps/candi/candi.bin")
    }
}

#[derive(Debug)]
#[allow(dead_code)] // Tech2 UART endpoint is not wired yet.
pub struct CandiHandle {
    stop: Arc<AtomicBool>,
    running: Arc<AtomicBool>,
    join: Option<JoinHandle<Snapshot>>,
    pub tech2: Tech2Endpoint,
    pub firmware_path: PathBuf,
}

impl CandiHandle {
    #[cfg(feature = "gui")]
    pub fn running_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.running)
    }

    #[cfg(test)]
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }

    pub fn stop(mut self) -> Option<Snapshot> {
        self.finish()
    }

    fn finish(&mut self) -> Option<Snapshot> {
        self.stop.store(true, Ordering::Release);
        let result = self.join.take().and_then(|join| match join.join() {
            Ok(snapshot) => Some(snapshot),
            Err(_) => {
                crate::log_error!("CANDI", 0, "CANDi CPU worker panicked; external_tx=false");
                None
            }
        });
        self.running.store(false, Ordering::Release);
        result
    }
}

impl Drop for CandiHandle {
    fn drop(&mut self) {
        self.finish();
    }
}

/// The default is local-only. An explicit path never falls back to another ROM.
/// Loading and validation happen before thread creation, so input errors reach
/// the host's normal startup recovery instead of leaving a dead worker badge.
pub fn maybe_start(config: CandiConfig) -> Result<Option<CandiHandle>, String> {
    if !config.enabled {
        return Ok(None);
    }
    let firmware_path = config.firmware_path.unwrap_or_else(default_firmware_path);
    let machine = Machine::new(Firmware::load(&firmware_path)?);
    let (tech2, endpoint) = CandiLink::new().split();
    let stop = Arc::new(AtomicBool::new(false));
    let running = Arc::new(AtomicBool::new(true));
    let stop_thread = Arc::clone(&stop);
    let running_thread = Arc::clone(&running);
    crate::log_info!("CANDI", 0,
        "ROM loaded path={} bytes=65544 cpu=M68EC020/CPU32 profile=msi-addressed-download entry={:#010x} budget={} UART=unwired external_tx=false",
        firmware_path.display(), APPLICATION_ENTRY, DEFAULT_BUDGET);
    let join = thread::Builder::new()
        .name("candi-worker".into())
        .spawn(move || worker_main(machine, stop_thread, running_thread, endpoint))
        .map_err(|e| format!("start CANDi worker: {e}"))?;
    Ok(Some(CandiHandle {
        stop,
        running,
        join: Some(join),
        tech2,
        firmware_path,
    }))
}

// Clear the running indicator even if a CPU/core bug unwinds this thread.
struct RunningGuard(Arc<AtomicBool>);
impl Drop for RunningGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

fn worker_main(
    mut machine: Machine,
    stop: Arc<AtomicBool>,
    running: Arc<AtomicBool>,
    endpoint: CandiEndpoint,
) -> Snapshot {
    let _guard = RunningGuard(running);
    loop {
        if stop.load(Ordering::Acquire) {
            machine.cancel();
            break;
        }
        if !machine.step(DEFAULT_BUDGET) {
            break;
        }
        // A finite instruction batch bounds cancellation latency and mailbox work.
        if machine.attempted() % 256 == 0 {
            match endpoint.try_recv_from_tech2() {
                Ok(Some(frame)) => crate::log_warn!(
                    "CANDI",
                    0,
                    "Mailbox input len={} rejected: firmware UART unwired; not vehicle TX",
                    frame.bytes.len()
                ),
                Ok(None) => {}
                Err(_) => {
                    machine.cancel();
                    break;
                }
            }
            thread::yield_now();
        }
    }
    let snapshot = machine.snapshot();
    for access in &snapshot.peripheral_writes {
        crate::log_info!("CANDI", snapshot.attempted,
            "Peripheral write address={:#010x} width={} value={:#010x} model=startup-registers-only external_tx=false",
            access.address, access.width, access.value);
    }
    for instruction in &snapshot.recent {
        crate::log_info!(
            "CANDI",
            snapshot.attempted,
            "CPU recent pc={:#010x} opcode={:#06x}",
            instruction.pc,
            instruction.opcode
        );
    }
    crate::log_warn!("CANDI", snapshot.attempted,
        "CPU stopped attempted={} completed={} cycles={} pc={:#010x} sp={:#010x} vbr={:#010x} peripheral_writes={} reason={} external_tx=false",
        snapshot.attempted, snapshot.completed, snapshot.cycles, snapshot.pc,
        snapshot.sp, snapshot.vbr, snapshot.peripheral_write_count, snapshot.reason.as_ref().map(ToString::to_string).unwrap_or_default());
    snapshot
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_config_starts_nothing() {
        assert!(maybe_start(CandiConfig::default()).unwrap().is_none());
    }

    #[test]
    fn missing_explicit_rom_returns_error_without_fallback() {
        let result = maybe_start(CandiConfig {
            enabled: true,
            firmware_path: Some(PathBuf::from("/nonexistent-candi-test/firmware.bin")),
        });
        assert!(result.unwrap_err().contains("CANDi firmware"));
    }

    #[test]
    fn cpu_worker_finishes_and_clears_running_status() {
        // Synthetic profile/loop, no proprietary ROM required by the tests.
        let path =
            std::env::temp_dir().join(format!("tech2-candi-worker-{}.bin", std::process::id()));
        let mut rom = vec![0; 0xa800];
        rom[4..8].copy_from_slice(&APPLICATION_ENTRY.to_be_bytes());
        rom[0x40..0x42].copy_from_slice(&[0x60, 0xfe]); // BRA to self.
        rom[0x48..0x59].copy_from_slice(b"CANdi Application");
        rom[0x72..0x78].copy_from_slice(&[0x4e, 0xf9, 0, 1, 0x8c, 0x94]);
        rom[0x8c94..0x8c98].copy_from_slice(&[0x60, 0, 0, 0x32]);
        rom[0x8c9c..0x8caa].copy_from_slice(b"CANdi RES V0.1");
        rom[0x8cc8..0x8cd2]
            .copy_from_slice(&[0x46, 0xfc, 0x27, 0, 0x2e, 0x7c, 0, 0x13, 0xff, 0xfc]);
        let mut bytes = vec![0; 8];
        bytes.extend_from_slice(&rom[..512]);
        for (index, block) in rom[512..].chunks(512).enumerate() {
            bytes.extend_from_slice(&(0x10200u32 + index as u32 * 512).to_le_bytes());
            bytes.extend_from_slice(&(block.len() as u32).to_le_bytes());
            bytes.extend_from_slice(block);
        }
        bytes.resize(65544, 0);
        std::fs::write(&path, bytes).unwrap();
        let handle = maybe_start(CandiConfig {
            enabled: true,
            firmware_path: Some(path.clone()),
        })
        .unwrap()
        .unwrap();
        std::fs::remove_file(path).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while handle.is_running() && std::time::Instant::now() < deadline {
            thread::yield_now();
        }
        let finished = !handle.is_running();
        let snapshot = handle.stop().unwrap();
        assert!(
            finished,
            "worker did not honor its finite instruction budget"
        );
        assert_eq!(snapshot.completed, DEFAULT_BUDGET);
        assert_eq!(
            snapshot.reason,
            Some(tech2_emu::candi_cpu::StopReason::Budget)
        );
    }
}
