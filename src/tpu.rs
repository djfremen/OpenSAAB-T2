// SPDX-License-Identifier: MPL-2.0
//! Tech2's downloaded TPU countdown function (channels 3 and 7).
//!
//! This is a model of the observed Tech2 timer personality, not a TPU
//! microinstruction interpreter. The caller enables it only for the supported
//! research firmware. Offline execution uses a deterministic instruction clock;
//! a live adapter uses monotonic time so CPU throughput cannot stretch ECU timers.
use std::time::Duration;

const QUANTUM: u64 = 10_000;
const SYSTEM_HZ: u128 = 16_777_216;
// OEM emulator.exe 0x41e0e3 schedules the next function-15 pass 0x1062
// TCR1 ticks ahead. With the guest's TPUMCR=0x1ccc, TCR1 is system clock / 4.
const COUNTDOWN_TCR1_TICKS: u128 = 0x1062;
const TICR: usize = 0xe08;
const CIER: usize = 0xe0a;
const CISR: usize = 0xe20;

fn word(sim: &[u8; 0x1000], off: usize) -> u16 {
    u16::from_be_bytes([sim[off], sim[off + 1]])
}
fn put(sim: &mut [u8; 0x1000], off: usize, value: u16) {
    sim[off..off + 2].copy_from_slice(&value.to_be_bytes());
}

#[derive(Clone, Default)]
pub struct Countdown {
    next: Option<u64>,
    wall_last: Option<Duration>,
    wall_remainder: u128,
    wall_divider: u16,
    uart: UartTimeout,
}

/// Observed function 13 on CANdi UART channel 5, not a general TPU model.
/// OEM host 0x41e130 loads a 32-bit TCR1 interval, resets on idle edges,
/// and sets CISR on expiry. Host service 1 reloads the interval.
#[derive(Clone, Default)]
struct UartTimeout {
    service: bool,
    remaining: Option<u64>,
    last: u64,
    idle: bool,
}

impl Countdown {
    #[cfg(feature = "load-test")]
    pub fn next_instruction_deadline(&self) -> Option<u64> {
        self.next
    }
    pub fn rebase(&mut self, captured: u64, now: u64) {
        self.next = self
            .next
            .map(|at| now.saturating_add(at.saturating_sub(captured)));
        self.uart.last = now;
        // A checkpoint must never count host time spent outside its session.
        self.wall_last = None;
        self.wall_remainder = 0;
    }

    pub fn uart_host_service(&mut self, value: u8) {
        self.uart.service |= (value >> 2) & 3 == 1;
    }

    /// Instruction-clock approximation: one TCR1 tick per guest instruction.
    /// Only enabled with the virtual native CANdi link and supported guest.
    pub fn advance_uart(&mut self, sim: &mut [u8; 0x1000], now: u64, idle: bool) -> bool {
        let elapsed = now.saturating_sub(self.uart.last);
        self.uart.last = now;
        if word(sim, 0xe00) & 0x8000 != 0 {
            return false; // TPU STOP freezes the remaining interval.
        }
        if (word(sim, 0xe10) >> 4) & 15 != 13 || (word(sim, 0xe1e) >> 10) & 3 == 0 {
            self.uart.remaining = None;
            return false;
        }
        if self.uart.service || (idle && !self.uart.idle) {
            let interval = (u64::from(word(sim, 0xf50)) << 16) | u64::from(word(sim, 0xf52));
            self.uart.remaining = Some(interval.max(1));
            self.uart.service = false;
            put(sim, 0xf54, u16::from(idle));
            put(sim, 0xf56, word(sim, 0xf50));
            put(sim, 0xf58, word(sim, 0xf52) & 0x8000);
        } else if idle {
            if let Some(remaining) = self.uart.remaining.as_mut() {
                *remaining = remaining.saturating_sub(elapsed);
                put(sim, 0xf56, (*remaining >> 16) as u16);
                if *remaining == 0 {
                    self.uart.remaining = None;
                    put(sim, 0xf54, 1);
                    put(sim, CISR, word(sim, CISR) | 0x20);
                    self.uart.idle = idle;
                    return true;
                }
            }
        }
        self.uart.idle = idle;
        false
    }

    /// Returns the channels whose counters reached zero on this update.
    pub fn advance(&mut self, sim: &mut [u8; 0x1000], now: u64) -> u16 {
        if self.wall_last.take().is_some() {
            self.next = None;
            self.wall_remainder = 0;
        }
        let next = *self.next.get_or_insert(now.saturating_add(QUANTUM));
        if now < next {
            return 0;
        }
        let elapsed = (now - next) / QUANTUM + 1;
        self.next = Some(now.saturating_add(QUANTUM - (now - next) % QUANTUM));
        Self::decrement(sim, elapsed)
    }

    /// Live adapter clock. The caller supplies monotonic elapsed time, sampled
    /// with the cooperative CANdi poll; no host-generated Tester Present is used.
    pub fn advance_wall(&mut self, sim: &mut [u8; 0x1000], now: Duration) -> u16 {
        let mcr = word(sim, 0xe00);
        // OEM 0x41fec0 decodes TCR1P and PSCK from TPUMCR this way.
        let divider = (if mcr & 0x40 != 0 { 4 } else { 32 }) << ((mcr >> 13) & 3);
        let previous = self.wall_last.replace(now);
        self.next = None;
        if previous.is_none() || now < previous.unwrap() || self.wall_divider != divider {
            self.wall_divider = divider;
            self.wall_remainder = 0;
            return 0;
        }
        if mcr & 0x8000 != 0 {
            return 0; // Freeze counts; discard elapsed STOP time.
        }
        let numerator = (now - previous.unwrap()).as_nanos() * SYSTEM_HZ + self.wall_remainder;
        let denominator = 1_000_000_000 * u128::from(divider) * COUNTDOWN_TCR1_TICKS;
        let elapsed = numerator / denominator;
        self.wall_remainder = numerator % denominator;
        Self::decrement(sim, elapsed.min(u128::from(u64::MAX)) as u64)
    }

    fn decrement(sim: &mut [u8; 0x1000], elapsed: u64) -> u16 {
        // STOP halts TPU execution. Counts remain available for restart.
        if word(sim, 0xe00) & 0x8000 != 0 {
            return 0;
        }
        let mut expired = 0;
        for channel in [3, 7] {
            let function = (word(sim, 0xe12 - (channel / 4) * 2) >> ((channel % 4) * 4)) & 15;
            let priority = (word(sim, 0xe1e) >> (channel * 2)) & 3;
            let sequence = (word(sim, 0xe16) >> (channel * 2)) & 3;
            if function != 15 || priority == 0 || sequence != 2 {
                continue;
            }
            let base = 0xf00 + channel * 16;
            put(sim, base, word(sim, base).wrapping_add(elapsed as u16));
            for index in 1..=5 {
                let off = 0xf00 + channel * 16 + index * 2;
                let before = word(sim, off);
                let after = u64::from(before).saturating_sub(elapsed) as u16;
                if before != 0 {
                    put(sim, off, after);
                    if after == 0 {
                        expired |= 1 << channel;
                    }
                }
            }
        }
        put(sim, CISR, word(sim, CISR) | expired);
        expired
    }
}

/// Pending status survives IACK; the guest ISR clears CISR explicitly.
pub fn interrupt(sim: &[u8; 0x1000]) -> Option<(u8, u32)> {
    let pending = word(sim, CIER) & word(sim, CISR);
    let level = ((word(sim, TICR) >> 8) & 7) as u8;
    if pending == 0 || level == 0 {
        return None;
    }
    let channel = 15 - pending.leading_zeros();
    Some((level, u32::from(word(sim, TICR) & 0xf0) | channel))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn configured() -> [u8; 0x1000] {
        let mut s = [0; 0x1000];
        put(&mut s, 0xe12, 0xf000);
        put(&mut s, 0xe10, 0xf000);
        put(&mut s, 0xe1e, 0x4040);
        put(&mut s, 0xe16, 0x8080);
        put(&mut s, TICR, 0x0450);
        put(&mut s, CIER, 0x88);
        s
    }

    #[test]
    fn expiry_survives_masking_and_requires_guest_acknowledgement() {
        let mut s = configured();
        let mut t = Countdown::default();
        put(&mut s, 0xf32, 2);
        put(&mut s, CIER, 0);
        t.advance(&mut s, 0);
        assert_eq!(t.advance(&mut s, QUANTUM), 0);
        assert_eq!(word(&s, 0xf32), 1);
        assert_eq!(t.advance(&mut s, QUANTUM * 2), 8);
        assert_eq!(interrupt(&s), None);
        put(&mut s, CIER, 8);
        assert_eq!(interrupt(&s), Some((4, 0x53)));
        assert_eq!(interrupt(&s), Some((4, 0x53)));
        put(&mut s, CISR, 0);
        assert_eq!(t.advance(&mut s, QUANTUM * 3), 0);
        assert_eq!(interrupt(&s), None);
    }

    #[test]
    fn independent_groups_no_underflow_and_elapsed_time_is_not_lost() {
        let mut s = configured();
        let mut t = Countdown::default();
        put(&mut s, 0xf32, 4);
        put(&mut s, 0xf3a, 2);
        put(&mut s, 0xf72, 5);
        put(&mut s, 0xff2, 123); // Other TPU personality is untouched.
        t.advance(&mut s, 0);
        assert_eq!(t.advance(&mut s, QUANTUM * 3 + 1), 8);
        assert_eq!(word(&s, 0xf32), 1);
        assert_eq!(word(&s, 0xf3a), 0);
        assert_eq!(word(&s, 0xf72), 2);
        assert_eq!(word(&s, 0xff2), 123);
        assert_eq!(t.advance(&mut s, QUANTUM * 5), 0x88);
    }

    #[test]
    fn stopped_disabled_and_other_functions_do_not_run() {
        for (off, value) in [(0xe00, 0x8000), (0xe12, 0), (0xe1e, 0), (0xe16, 0)] {
            let mut s = configured();
            let mut t = Countdown::default();
            put(&mut s, off, value);
            put(&mut s, 0xf32, 1);
            t.advance(&mut s, 0);
            assert_eq!(t.advance(&mut s, QUANTUM * 10), 0);
            assert_eq!(word(&s, 0xf32), 1);
        }
    }

    #[test]
    fn restored_checkpoint_keeps_remaining_delay() {
        let mut s = configured();
        let mut t = Countdown::default();
        put(&mut s, 0xf32, 1);
        t.advance(&mut s, 0);
        t.rebase(500, 1_000_000);
        assert_eq!(t.advance(&mut s, 1_009_499), 0);
        assert_eq!(t.advance(&mut s, 1_009_500), 8);
    }

    #[test]
    fn live_2500_tick_timer_expires_in_2_5_seconds_independent_of_poll_rate() {
        for poll_ms in [1, 7, 100, 2500] {
            let mut s = configured();
            put(&mut s, 0xe00, 0x1ccc);
            put(&mut s, 0xf32, 2500);
            put(&mut s, CIER, 0); // Hardware time still advances with IRQ masked.
            let mut t = Countdown::default();
            t.advance_wall(&mut s, Duration::ZERO);
            for ms in (poll_ms..2500).step_by(poll_ms as usize) {
                assert_eq!(t.advance_wall(&mut s, Duration::from_millis(ms)), 0);
            }
            assert_eq!(t.advance_wall(&mut s, Duration::from_millis(2500)), 8);
            assert_eq!(word(&s, 0xf32), 0);
            assert_eq!(interrupt(&s), None);
            put(&mut s, CIER, 8);
            assert_eq!(interrupt(&s), Some((4, 0x53)));
            put(&mut s, CISR, 0);
            assert_eq!(t.advance_wall(&mut s, Duration::from_secs(20)), 0);
        }
    }

    #[test]
    fn live_clock_preserves_stop_counts_and_restarts_clock_domains_without_catchup() {
        let mut s = configured();
        put(&mut s, 0xe00, 0x1ccc);
        put(&mut s, 0xf32, 10);
        let mut t = Countdown::default();
        t.advance(&mut s, 123_000_000);
        assert_eq!(t.advance_wall(&mut s, Duration::from_secs(100)), 0);
        t.advance_wall(&mut s, Duration::from_millis(100_004));
        assert_eq!(word(&s, 0xf32), 6);
        put(&mut s, 0xe00, 0x9ccc);
        t.advance_wall(&mut s, Duration::from_secs(110));
        assert_eq!(word(&s, 0xf32), 6);
        put(&mut s, 0xe00, 0x1ccc);
        assert_eq!(t.advance_wall(&mut s, Duration::from_millis(110_006)), 8);
        put(&mut s, 0xf32, 1);
        t.rebase(123_000_000, 0);
        assert_eq!(t.advance_wall(&mut s, Duration::from_secs(500)), 0);
        assert_eq!(word(&s, 0xf32), 1);
        assert_eq!(t.advance(&mut s, 2_000_000), 0);
        assert_eq!(t.advance(&mut s, 2_010_000), 8);
    }

    #[test]
    fn live_tcr1_prescaler_and_fractional_ticks_follow_original_host() {
        let mut s = configured();
        let mut t = Countdown::default();
        // PSCK clear selects /32: approximately 8 ms per countdown tick.
        put(&mut s, 0xe00, 0x1c8c);
        put(&mut s, 0xf72, 1);
        t.advance_wall(&mut s, Duration::ZERO);
        assert_eq!(t.advance_wall(&mut s, Duration::from_millis(7)), 0);
        assert_eq!(t.advance_wall(&mut s, Duration::from_millis(8)), 0x80);
        // Prescaler switch begins a fresh phase and keeps the programmed count.
        put(&mut s, 0xe00, 0x3ccc); // /8, approximately 2 ms.
        put(&mut s, 0xf72, 1);
        t.advance_wall(&mut s, Duration::from_millis(9));
        assert_eq!(t.advance_wall(&mut s, Duration::from_micros(10_000)), 0);
        assert_eq!(t.advance_wall(&mut s, Duration::from_micros(10_500)), 0);
        assert_eq!(t.advance_wall(&mut s, Duration::from_micros(11_000)), 0x80);
    }

    #[test]
    fn uart_timeout_reloads_on_service_and_receive_idle_then_expires_once() {
        let mut s = configured();
        let mut t = Countdown::default();
        put(&mut s, 0xe10, 0x00d0);
        put(&mut s, 0xe1e, 0x0400);
        put(&mut s, 0xf52, 100);
        put(&mut s, CIER, 0);
        t.uart_host_service(4);
        assert!(!t.advance_uart(&mut s, 0, true));
        assert!(!t.advance_uart(&mut s, 90, true));
        assert!(!t.advance_uart(&mut s, 95, false));
        assert!(!t.advance_uart(&mut s, 96, true)); // Receive completion restarts interval.
        assert!(!t.advance_uart(&mut s, 195, true));
        assert!(t.advance_uart(&mut s, 196, true));
        assert_eq!(interrupt(&s), None);
        put(&mut s, CIER, 0x20);
        assert_eq!(interrupt(&s), Some((4, 0x55)));
        put(&mut s, CISR, 0);
        assert!(!t.advance_uart(&mut s, 1_000, true));
        t.uart_host_service(4);
        assert!(!t.advance_uart(&mut s, 1_001, true));
        assert!(t.advance_uart(&mut s, 1_101, true));
    }

    #[test]
    fn uart_timeout_preserves_wide_interval_stop_and_checkpoint_delay() {
        let mut s = configured();
        let mut t = Countdown::default();
        put(&mut s, 0xe10, 0x00d0);
        put(&mut s, 0xe1e, 0x0400);
        put(&mut s, 0xf50, 1);
        put(&mut s, 0xf52, 10);
        t.uart_host_service(4);
        assert!(!t.advance_uart(&mut s, 0, true));
        assert!(!t.advance_uart(&mut s, 65_536, true));
        put(&mut s, 0xe00, 0x8000);
        assert!(!t.advance_uart(&mut s, 100_000, true));
        put(&mut s, 0xe00, 0);
        t.rebase(100_000, 1_000_000);
        assert!(!t.advance_uart(&mut s, 1_000_009, true));
        assert!(t.advance_uart(&mut s, 1_000_010, true));
        put(&mut s, CISR, 0);
        put(&mut s, 0xe10, 0); // Another function must not generate a timeout.
        t.uart_host_service(4);
        assert!(!t.advance_uart(&mut s, 2_000_000, true));
        assert_eq!(word(&s, CISR), 0);
    }
}
