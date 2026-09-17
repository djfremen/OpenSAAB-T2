// SPDX-License-Identifier: MPL-2.0
//! Private offline boot experiment. No saved state, firmware edits, or adapter.
//! Translate a verified RAM-only scan loop; advance STOP only to the next event.
use crate::bus::{Tech2Bus, ERAM_BASE, ERAM_SIZE, RAM_BASE, RAM_SIZE};
use m68k::{AddressBus, CpuCore};

const START: u32 = 0x1784c;
const LOOP_PCS: [u32; 15] = [
    0x1784c, 0x1784e, 0x17852, 0x17856, 0x17868, 0x1786a, 0x1786c, 0x1786e, 0x17870, 0x17872,
    0x17874, 0x17876, 0x1787a, 0x1787c, 0x17882,
];
const CODE: &[u8] = &[
    0x42, 0x46, 0x1c, 0x29, 0x00, 0x02, 0x0c, 0x41, 0x00, 0x22, 0x66, 0x10, 0x42, 0x41, 0x12, 0x29,
    0x00, 0x06, 0x0c, 0x01, 0x00, 0x01, 0x66, 0x20, 0x70, 0x01, 0x60, 0x1e, 0x70, 0x02, 0xd0, 0x46,
    0xd0, 0x40, 0x48, 0xc0, 0xd3, 0xc0, 0x42, 0x41, 0x12, 0x11, 0x0c, 0x41, 0x00, 0xff, 0x67, 0x08,
    0xb3, 0xfc, 0x00, 0x40, 0x00, 0x00, 0x65, 0xc8,
];

fn record(bus: &mut Tech2Bus, insns: u64, count: u64, pcs: &[u32]) {
    for n in count.saturating_sub(64)..count {
        bus.recent_pcs[(bus.steps_recorded + n) as usize & 63] = pcs[n as usize % pcs.len()];
    }
    bus.steps_recorded = bus.steps_recorded.saturating_add(count);
    bus.current_insns = insns + count - 1;
    bus.current_pc = pcs[(count - 1) as usize % pcs.len()];
}

#[inline]
pub fn candidate(cpu: &CpuCore) -> bool {
    cpu.stopped != 0
        || matches!(
            cpu.pc,
            START | 0x7ae | 0x7c2 | 0x7dc | 0x810 | 0xcf76 | 0xd9f2
        )
}

pub fn advance(cpu: &mut CpuCore, bus: &mut Tech2Bus, insns: u64, bound: u64) -> u64 {
    if !candidate(cpu) {
        return 0;
    }
    if bus.candi_link.is_some()
        || bus.trace.enabled()
        || !bus.execution_mode.is_research_harness()
        || cpu.t0_flag != 0
        || cpu.t1_flag != 0
    {
        return 0;
    }
    let level = bus.poll_interrupt(insns);
    if level != 0 && (level == 7 || level > (cpu.int_mask >> 8) as u8) {
        return 0;
    }
    let deadline = match bus.load_test_deadline() {
        Some(deadline) => deadline,
        None if cpu.stopped == 0 => bound, // no scheduled event; ordinary RAM/ROM only
        None => return 0,
    };
    let end = deadline.min(bound.saturating_sub(1));
    let available = end.saturating_sub(insns);
    if available == 0 {
        return 0;
    }
    if cpu.stopped == m68k::core::execute::STOP_LEVEL_STOP && cpu.s_flag != 0 {
        record(bus, insns, available, &[cpu.pc]);
        bus.poll_interrupt(end - 1);
        return available;
    }
    if cpu.stopped == 0 {
        let n = advance_memory_loop(cpu, bus, available);
        if n != 0 {
            let pcs: &[u32] = match cpu.pc {
                0x7ae => &[0x7ae, 0x7b0, 0x7b2, 0x7b4],
                0x7c2 => &[0x7c2, 0x7c4, 0x7c8, 0x7ca, 0x7cc],
                0x7dc => &[0x7dc, 0x7e2],
                0x810 => &[0x810, 0x812, 0x816],
                0xcf76 => &[0xcf76, 0xcf78, 0xcf7a],
                0xd9f2 => &[0xd9f2, 0xd9f4],
                _ => unreachable!(),
            };
            record(bus, insns, n, pcs);
            bus.poll_interrupt(insns + n - 1);
            return n;
        }
    }
    if cpu.stopped != 0 || cpu.pc != START || bus.peek_bytes(START, CODE.len()) != Some(CODE) {
        return 0;
    }
    // Reserve one normal iteration at the end to restore precise core bookkeeping.
    let max = (available / 15).saturating_sub(1);
    if max == 0 {
        return 0;
    }
    let mut iterations = 0;
    while iterations < max {
        let address = cpu.dar[9];
        if cpu.dar[1] as u16 == 0x22
            || !(ERAM_BASE + 0x202..ERAM_BASE + ERAM_SIZE - 2).contains(&address)
        {
            break;
        }
        let len = bus.eram[(address - ERAM_BASE + 2) as usize];
        let stride = 4 + u32::from(len) * 2;
        let next = address + stride;
        if next >= ERAM_BASE + ERAM_SIZE {
            break;
        }
        let tag = bus.eram[(next - ERAM_BASE) as usize];
        if tag == 0xff {
            break;
        }
        // Exact register results of the 15-instruction taken-backedge path.
        // Both reads are plain external RAM (never the research ATA/COR window).
        debug_assert_eq!(bus.read_byte(address + 2), len);
        debug_assert_eq!(bus.read_byte(next), tag);
        cpu.dar[6] = (cpu.dar[6] & 0xffff0000) | u32::from(len);
        cpu.dar[0] = stride;
        cpu.dar[9] = next;
        cpu.dar[1] = (cpu.dar[1] & 0xffff0000) | u32::from(tag);
        cpu.set_ccr(0x09); // CMPA next,$400000: N/C; ADD.W never sets X here.
        iterations += 1;
    }
    if iterations == 0 {
        return 0;
    }
    let count = iterations * 15;
    record(bus, insns, count, &LOOP_PCS);
    bus.poll_interrupt(insns + count - 1);
    // The memory loop has no device access. Original execution handles the
    // final iteration, all exits, instruction boundaries and any interrupt.
    cpu.jump(START);
    count
}

fn cmp_flags(lhs: u32, rhs: u32, x: u8) -> u8 {
    let diff = lhs.wrapping_sub(rhs);
    let v = ((lhs ^ rhs) & (lhs ^ diff) & 0x80000000) != 0;
    x | (u8::from(diff >> 31 != 0) << 3)
        | (u8::from(diff == 0) << 2)
        | (u8::from(v) << 1)
        | u8::from(lhs < rhs)
}

// Translations retain all RAM writes and reads, condition codes and instruction
// clock ticks. A timer/observation boundary and each exit return to the core.
fn advance_memory_loop(cpu: &mut CpuCore, bus: &mut Tech2Bus, available: u64) -> u64 {
    let mut n = 0;
    match cpu.pc {
        0x7ae
            if bus.peek_bytes(0x7ae, 8)
                == Some(&[0x22, 0xc4, 0x58, 0x84, 0xb8, 0x80, 0x6f, 0xf8]) =>
        {
            let max = (available / 4).saturating_sub(1);
            while n < max {
                let addr = cpu.dar[9];
                let next = cpu.dar[4].wrapping_add(4);
                if !(RAM_BASE..=RAM_BASE + RAM_SIZE - 4).contains(&addr)
                    || next as i32 > cpu.dar[0] as i32
                {
                    break;
                }
                bus.write_long(addr, cpu.dar[4]);
                cpu.dar[9] = addr + 4;
                let carry = cpu.dar[4] > u32::MAX - 4;
                cpu.dar[4] = next;
                cpu.set_ccr(cmp_flags(next, cpu.dar[0], u8::from(carry) << 4));
                n += 1;
            }
            n * 4
        }
        0x7c2
            if bus.peek_bytes(0x7c2, 12)
                == Some(&[
                    0xb3, 0xd1, 0x66, 0, 0x01, 0x1c, 0x58, 0x89, 0xb2, 0x89, 0x66, 0xf4,
                ]) =>
        {
            let max = (available / 5).saturating_sub(1);
            while n < max {
                let addr = cpu.dar[9];
                if !(RAM_BASE..=RAM_BASE + RAM_SIZE - 4).contains(&addr)
                    || cpu.dar[1] == addr + 4
                    || bus.read_long(addr) != addr
                {
                    break;
                }
                cpu.dar[9] = addr + 4;
                cpu.set_ccr(cmp_flags(cpu.dar[1], addr + 4, cpu.get_ccr() & 0x10));
                n += 1;
            }
            n * 5
        }
        0x7dc
            if bus.peek_bytes(0x7dc, 10)
                == Some(&[0x22, 0xfc, 0xaa, 0xaa, 0xaa, 0xaa, 0x51, 0xc8, 0xff, 0xf8]) =>
        {
            let max = (available / 2).saturating_sub(1);
            while n < max {
                let addr = cpu.dar[9];
                if !(RAM_BASE..=RAM_BASE + RAM_SIZE - 4).contains(&addr) || cpu.dar[0] as u16 == 0 {
                    break;
                }
                bus.write_long(addr, 0xaaaaaaaa);
                cpu.dar[9] = addr + 4;
                cpu.dar[0] =
                    (cpu.dar[0] & 0xffff0000) | u32::from((cpu.dar[0] as u16).wrapping_sub(1));
                cpu.set_ccr((cpu.get_ccr() & 0x10) | 8);
                n += 1;
            }
            n * 2
        }
        0x810
            if bus.peek_bytes(0x810, 10)
                == Some(&[0xb2, 0x99, 0x66, 0, 0, 0xce, 0x51, 0xc8, 0xff, 0xf8]) =>
        {
            let max = (available / 3).saturating_sub(1);
            while n < max {
                let addr = cpu.dar[9];
                if !(RAM_BASE..=RAM_BASE + RAM_SIZE - 4).contains(&addr)
                    || cpu.dar[0] as u16 == 0
                    || bus.read_long(addr) != cpu.dar[1]
                {
                    break;
                }
                cpu.dar[9] = addr + 4;
                cpu.dar[0] =
                    (cpu.dar[0] & 0xffff0000) | u32::from((cpu.dar[0] as u16).wrapping_sub(1));
                cpu.set_ccr((cpu.get_ccr() & 0x10) | 4);
                n += 1;
            }
            n * 3
        }
        0xcf76 if bus.peek_bytes(0xcf76, 6) == Some(&[0x16, 0xdc, 0xb9, 0xcd, 0x65, 0xfa]) => {
            let max = (available / 3).saturating_sub(1);
            while n < max {
                let src = cpu.dar[12];
                let dst = cpu.dar[11];
                if !(src < 0x40000
                    || (RAM_BASE..RAM_BASE + RAM_SIZE).contains(&src)
                    || (0x200000..0x300000).contains(&src))
                    || !(RAM_BASE..RAM_BASE + RAM_SIZE).contains(&dst)
                    || src + 1 >= cpu.dar[13]
                {
                    break;
                }
                let value = bus.read_byte(src);
                bus.write_byte(dst, value);
                cpu.dar[12] = src + 1;
                cpu.dar[11] = dst + 1;
                cpu.set_ccr(cmp_flags(src + 1, cpu.dar[13], cpu.get_ccr() & 0x10));
                n += 1;
            }
            n * 3
        }
        0xd9f2 if bus.peek_bytes(0xd9f2, 4) == Some(&[0x4a, 0x19, 0x66, 0xfc]) => {
            let max = (available / 2).saturating_sub(1);
            while n < max {
                let addr = cpu.dar[9];
                // Only backing ROM/RAM, never a device register with read effects.
                if !(addr < 0x40000 || (RAM_BASE..RAM_BASE + RAM_SIZE).contains(&addr)) {
                    break;
                }
                let value = bus.read_byte(addr);
                if value == 0 {
                    break;
                }
                cpu.dar[9] = addr + 1;
                cpu.set_ccr((cpu.get_ccr() & 0x10) | if value & 0x80 != 0 { 8 } else { 0 });
                n += 1;
            }
            n * 2
        }
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bus::{ExecutionMode, OFF_PICR, OFF_PITR};
    use m68k::CpuType;
    fn machine() -> (CpuCore, Tech2Bus) {
        let mut bus = Tech2Bus::new(
            vec![0; 0x40000],
            vec![0; 0x100000],
            ExecutionMode::ResearchHarness,
        );
        bus.flash[START as usize..START as usize + CODE.len()].copy_from_slice(CODE);
        bus.sim[OFF_PITR + 1] = 1;
        bus.sim[OFF_PICR] = 4;
        bus.poll_interrupt(0);
        let mut cpu = CpuCore::new();
        cpu.set_cpu_type(CpuType::M68EC020);
        cpu.set_sr(0x2700);
        cpu.pc = START;
        cpu.dar[9] = ERAM_BASE + 0x204;
        cpu.dar[1] = 0x12340000;
        cpu.dar[6] = 0xabcd1234;
        (cpu, bus)
    }
    #[test]
    fn ram_scan_matches_interpreter_across_lengths_and_flags() {
        for seed in 0..12u32 {
            let (mut slow, mut sb) = machine();
            for (i, b) in sb.eram.iter_mut().enumerate() {
                *b = ((i as u32 * 17 + seed * 71) % 251) as u8;
            }
            slow.dar[1] = (slow.dar[1] & 0xffff0000) | u32::from(sb.eram[0x204]);
            let mut fast: CpuCore =
                serde_json::from_value(serde_json::to_value(&slow).unwrap()).unwrap();
            let mut fb = sb.recovery_snapshot();
            let n = advance(&mut fast, &mut fb, 1, 700);
            assert!(n > 0);
            for i in 1..=n {
                assert!(matches!(
                    crate::step_guest(&mut slow, &mut sb, i),
                    m68k::StepResult::Ok { .. }
                ));
            }
            assert_eq!(fast.pc, slow.pc);
            assert_eq!(fast.dar, slow.dar, "seed {seed}");
            assert_eq!(fast.get_sr(), slow.get_sr());
            assert_eq!(fb.eram, sb.eram);
            assert_eq!(fb.sim, sb.sim);
            for i in n + 1..n + 101 {
                assert!(matches!(
                    crate::step_guest(&mut fast, &mut fb, i),
                    m68k::StepResult::Ok { .. }
                ));
                assert!(matches!(
                    crate::step_guest(&mut slow, &mut sb, i),
                    m68k::StepResult::Ok { .. }
                ));
                assert_eq!(fast.pc, slow.pc);
                assert_eq!(fast.dar, slow.dar);
                assert_eq!(fast.get_sr(), slow.get_sr());
            }
        }
    }
    #[test]
    fn memory_loops_match_core_with_real_memory_effects() {
        for pc in [0x7ae, 0xd9f2] {
            for seed in 0..16u32 {
                let (mut slow, mut sb) = machine();
                let code: &[u8] = if pc == 0x7ae {
                    &[0x22, 0xc4, 0x58, 0x84, 0xb8, 0x80, 0x6f, 0xf8]
                } else {
                    &[0x4a, 0x19, 0x66, 0xfc]
                };
                sb.flash[pc..pc + code.len()].copy_from_slice(code);
                slow.pc = pc as u32;
                slow.dar[9] = RAM_BASE + 0x2000;
                slow.dar[4] = RAM_BASE + seed * 4;
                slow.dar[0] = RAM_BASE + RAM_SIZE - 1;
                slow.set_ccr(seed as u8 * 2);
                sb.ram.fill(((seed * 17) % 255 + 1) as u8);
                let mut fast: CpuCore =
                    serde_json::from_value(serde_json::to_value(&slow).unwrap()).unwrap();
                let mut fb = sb.recovery_snapshot();
                let n = advance(&mut fast, &mut fb, 1, 700);
                assert!(n > 0, "pc {pc:x}");
                for i in 1..=n {
                    crate::step_guest(&mut slow, &mut sb, i);
                }
                assert_eq!(fast.pc, slow.pc);
                assert_eq!(fast.dar, slow.dar);
                assert_eq!(fast.get_sr(), slow.get_sr(), "pc {pc:x} seed {seed}");
                assert_eq!(fb.ram, sb.ram);
                for i in n + 1..n + 101 {
                    crate::step_guest(&mut slow, &mut sb, i);
                    crate::step_guest(&mut fast, &mut fb, i);
                    assert_eq!(fast.pc, slow.pc);
                    assert_eq!(fast.dar, slow.dar);
                    assert_eq!(fast.get_sr(), slow.get_sr());
                }
                assert_eq!(fb.ram, sb.ram);
            }
        }
    }
    #[test]
    fn remaining_memory_loops_match_core_including_following_instructions() {
        let loops: &[(usize, &[u8])] = &[
            (
                0x7c2,
                &[
                    0xb3, 0xd1, 0x66, 0, 0x01, 0x1c, 0x58, 0x89, 0xb2, 0x89, 0x66, 0xf4,
                ],
            ),
            (
                0x7dc,
                &[0x22, 0xfc, 0xaa, 0xaa, 0xaa, 0xaa, 0x51, 0xc8, 0xff, 0xf8],
            ),
            (
                0x810,
                &[0xb2, 0x99, 0x66, 0, 0, 0xce, 0x51, 0xc8, 0xff, 0xf8],
            ),
            (0xcf76, &[0x16, 0xdc, 0xb9, 0xcd, 0x65, 0xfa]),
        ];
        for &(pc, code) in loops {
            for x in [0, 0x10] {
                let (mut slow, mut sb) = machine();
                sb.flash[pc..pc + code.len()].copy_from_slice(code);
                slow.pc = pc as u32;
                slow.set_ccr(x);
                slow.dar[0] = 0xabcdffff;
                slow.dar[9] = RAM_BASE + 0x2000;
                slow.dar[1] = if pc == 0x7c2 {
                    RAM_BASE + RAM_SIZE
                } else {
                    0xaaaaaaaa
                };
                slow.dar[12] = RAM_BASE + 0x4000;
                slow.dar[13] = RAM_BASE + 0x5000;
                slow.dar[11] = RAM_BASE + 0x6000;
                for i in (0..RAM_SIZE as usize).step_by(4) {
                    sb.ram[i..i + 4].copy_from_slice(
                        &(if pc == 0x7c2 {
                            RAM_BASE + i as u32
                        } else {
                            0xaaaaaaaa
                        })
                        .to_be_bytes(),
                    );
                }
                let mut fast: CpuCore =
                    serde_json::from_value(serde_json::to_value(&slow).unwrap()).unwrap();
                let mut fb = sb.recovery_snapshot();
                let n = advance(&mut fast, &mut fb, 1, 700);
                assert!(n > 0, "{pc:x}");
                for i in 1..=n {
                    crate::step_guest(&mut slow, &mut sb, i);
                }
                assert_eq!(fast.pc, slow.pc, "{pc:x}");
                assert_eq!(fast.dar, slow.dar, "{pc:x}");
                assert_eq!(fast.get_sr(), slow.get_sr(), "{pc:x}");
                assert_eq!(fb.ram, sb.ram, "{pc:x}");
                for i in n + 1..n + 101 {
                    crate::step_guest(&mut slow, &mut sb, i);
                    crate::step_guest(&mut fast, &mut fb, i);
                    assert_eq!(fast.pc, slow.pc, "{pc:x}");
                    assert_eq!(fast.dar, slow.dar, "{pc:x}");
                    assert_eq!(fast.get_sr(), slow.get_sr(), "{pc:x}");
                }
                assert_eq!(fb.ram, sb.ram);
            }
        }
    }
    #[test]
    fn card_copy_retains_bytes_and_bus_read_counters() {
        for bank in [0, 3] {
            let (mut slow, mut sb) = machine();
            sb.flash[0xcf76..0xcf7c].copy_from_slice(&[0x16, 0xdc, 0xb9, 0xcd, 0x65, 0xfa]);
            sb.bank = bank;
            for (i, b) in sb.card.iter_mut().enumerate() {
                *b = (i % 251) as u8;
            }
            slow.pc = 0xcf76;
            slow.dar[12] = 0x200100;
            slow.dar[13] = 0x200300;
            slow.dar[11] = RAM_BASE + 0x8000;
            let mut fast: CpuCore =
                serde_json::from_value(serde_json::to_value(&slow).unwrap()).unwrap();
            let mut fb = sb.recovery_snapshot();
            let n = advance(&mut fast, &mut fb, 1, 700);
            assert!(n > 0);
            for i in 1..=n {
                crate::step_guest(&mut slow, &mut sb, i);
            }
            assert_eq!(fast.dar, slow.dar);
            assert_eq!(fast.pc, slow.pc);
            assert_eq!(fast.get_sr(), slow.get_sr());
            assert_eq!(fb.ram, sb.ram);
            assert_eq!(fb.card_rd_pages, sb.card_rd_pages);
            assert_eq!(fb.attr_reads, sb.attr_reads);
        }
    }

    #[test]
    fn scan_declines_device_window_exit_tuple_and_changed_code() {
        for case in 0..4 {
            let (mut cpu, mut bus) = machine();
            match case {
                0 => cpu.dar[9] = ERAM_BASE,
                1 => cpu.dar[1] = 0x22,
                2 => bus.flash[START as usize] = 0,
                _ => bus.eram[0x208] = 0xff,
            }
            assert_eq!(advance(&mut cpu, &mut bus, 1, 700), 0);
        }
    }
    #[test]
    fn stopped_cpu_stops_at_deadline_without_inventing_interrupt() {
        let (mut cpu, mut bus) = machine();
        cpu.stopped = m68k::core::execute::STOP_LEVEL_STOP;
        cpu.set_sr(0x2000);
        let n = advance(&mut cpu, &mut bus, 1, 50000);
        assert_eq!(n, 9999);
        assert!(!bus.pit_pending);
        assert_eq!(bus.poll_interrupt(10000), 4);
        assert!(bus.pit_pending);
    }
}
