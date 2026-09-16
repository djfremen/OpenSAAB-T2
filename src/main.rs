// SPDX-License-Identifier: MPL-2.0
//! Tech2 Hardware & Software Emulator Entry Point.

mod artifacts;
mod bus;
mod candi;
mod communication;
mod config;
#[cfg(feature = "gui")]
pub mod controls;
#[cfg(feature = "gui")]
mod ecm_info;
mod event_console;
#[cfg(feature = "gui")]
mod gui;
#[cfg(feature = "gui")]
mod gui_console;
mod harness;
mod interactive;
mod lcd;
mod load_binary;
pub mod logger;
mod options;
mod performance;
mod recovery;
mod replay;
mod ssa_flash;
mod tpu;
mod trace;

use artifacts::Outcome;
use bus::{ExecutionMode, Tech2Bus};
use config::Tech2WinConfig;
#[cfg(feature = "gui")]
use gui::GuiRunner;
use load_binary::{load_boot, load_nao, load_opsys, restore_rom_modules};
use m68k::{AddressBus, CpuCore, CpuType, StepResult};
use options::Options;
use std::env;
use std::path::Path;

/// Opt-in, read-only POST call tracing.  This logs the ROM's screen-print
/// helper calls and their caller addresses without changing guest execution.
static POST_TRACE: std::sync::LazyLock<bool> =
    std::sync::LazyLock::new(|| crate::options::env_flag("POST_TRACE"));

/// Opt-in, read-only trace for the card-to-flash bootstrap handoff.  The
/// addresses are code boundaries observed in the supplied firmware, rather
/// than host-injected entry points.  Keeping this behind an environment flag
/// makes it useful for diagnosis without changing the fidelity execution path.
static BOOT_TRACE: std::sync::LazyLock<bool> =
    std::sync::LazyLock::new(|| crate::options::env_flag("BOOT_TRACE"));
static BOOT_TRACE_COUNT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
const BOOT_TRACE_LIMIT: usize = 160;

pub fn service_interrupt(cpu: &mut CpuCore, bus: &mut Tech2Bus, level: u8) {
    let old_sr = cpu.get_sr();
    cpu.t1_flag = 0;
    cpu.t0_flag = 0;
    cpu.set_s_flag(4); // SFLAG_SET = 4
    cpu.int_mask = ((level as u32) & 7) << 8;

    let stacked_pc = cpu.pc;
    let response = bus.interrupt_acknowledge(level);
    let vector = if response == 0xFFFF_FFFF {
        (24 + (level as u32)) & 0xFF
    } else {
        response & 0xFF
    };

    let vec_word = (vector as u16) << 2;
    cpu.push_16(bus, vec_word);
    cpu.push_32(bus, stacked_pc);
    cpu.push_16(bus, old_sr);

    cpu.jump_vector(bus, vector);
    cpu.stopped = 0;
}

pub fn step_guest(cpu: &mut CpuCore, bus: &mut Tech2Bus, insns: u64) -> StepResult {
    let pc = cpu.pc;
    bus.current_pc = pc;
    bus.current_insns = insns;
    bus.recent_pcs[bus.steps_recorded as usize & 63] = pc;
    bus.steps_recorded = bus.steps_recorded.saturating_add(1);
    if bus.at_assertion_halt(pc) {
        let screen = bus
            .screen_text()
            .lines()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" | ");
        let reason = format!("guest assertion at {pc:#010x}: {screen}");
        bus.trace_event("guest_assertion", &reason);
        bus.failure_reason = Some(reason);
        // Stop at the guest's intentional infinite loop; never continue into
        // the stack-unwind/RTD sequence or replace the assertion with success.
        return StepResult::Stopped;
    }
    if bus.trace.enabled() && cpu.stopped == 0 {
        // Address labels are observations from the supplied Saab image, not a
        // general protocol decoder. Stack snapshots never read device registers.
        if matches!(
            pc,
            0x001d_bb96
                | 0x0001_219a
                | 0x0011_219a
                | 0x0001_2524
                | 0x0011_2524
                | 0x0001_258a
                | 0x0011_258a
        ) {
            bus.trace_stack("guest_dispatch_entry", pc, cpu.sp());
            if pc == 0x001d_bb96 {
                bus.trace_screen();
                let pointer = bus
                    .peek_bytes(cpu.sp().wrapping_add(4), 4)
                    .map(|b| u32::from_be_bytes(b.try_into().unwrap()));
                bus.trace_memory("guest_request", pointer, 32);
            } else if matches!(pc, 0x0001_2524 | 0x0011_2524) {
                let pointer = bus
                    .peek_bytes(cpu.sp().wrapping_add(8), 4)
                    .map(|b| u32::from_be_bytes(b.try_into().unwrap()));
                bus.trace_memory("guest_message_arg2", pointer, 16);
            }
        }
    }
    if bus.trace.enabled() && cpu.stopped == 0 && pc == 0x001d_bd48 {
        // RTD #8 at the supplied image's request-wrapper epilogue.
        bus.trace_event(
            "guest_request_return",
            &format!(
                "status_low16={:#06x} d0={:#010x} return_pc={:02x?} interpretation=unclassified",
                cpu.dar[0] as u16,
                cpu.dar[0],
                bus.peek_bytes(cpu.sp(), 4)
            ),
        );
    }
    communication::observe(cpu, bus);
    let research_harness = bus.execution_mode.is_research_harness();

    if *POST_TRACE && pc == 0x0000_0B5C {
        let sp = cpu.sp();
        println!(
            "POST_TRACE insns={insns} caller={:#010x} args={:#010x} {:#010x} {:#010x}",
            bus.read_long(sp),
            bus.read_long(sp + 4),
            bus.read_long(sp + 8),
            bus.read_long(sp + 12),
        );
    }
    if *POST_TRACE && pc == 0x0000_497A {
        let sp = cpu.sp();
        println!(
            "POST_RESULT insns={insns} D0={:#010x} D1={:#010x} D7={:#010x} \
stack={:#010x} {:#010x} {:#010x} {:#010x}",
            cpu.dar[0],
            cpu.dar[1],
            cpu.dar[7],
            bus.read_long(sp),
            bus.read_long(sp + 4),
            bus.read_long(sp + 8),
            bus.read_long(sp + 12),
        );
    }
    if *BOOT_TRACE
        && matches!(
            pc,
            0x0000_9018
                | 0x0000_9076
                | 0x0000_907C
                | 0x0001_63B2
                | 0x0001_640A
                | 0x0001_7424
                | 0x0001_7482
                | 0x0001_748E
        )
    {
        let n = BOOT_TRACE_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if n < BOOT_TRACE_LIMIT {
            let sp = cpu.sp();
            println!(
                "BOOT_TRACE #{n:03} insns={insns} pc={pc:#010x} op={:#06x} \
SP={sp:#010x} SR={:#06x} D0={:#010x} D1={:#010x} A0={:#010x} A1={:#010x} \
stack={:#010x} {:#010x} {:#010x} {:#010x} {:#010x}",
                bus.read_word(pc),
                cpu.get_sr(),
                cpu.dar[0],
                cpu.dar[1],
                cpu.dar[8],
                cpu.dar[9],
                bus.read_long(sp),
                bus.read_long(sp + 4),
                bus.read_long(sp + 8),
                bus.read_long(sp + 12),
                bus.read_long(sp + 16),
            );
        } else if n == BOOT_TRACE_LIMIT {
            println!("BOOT_TRACE: limit ({BOOT_TRACE_LIMIT}) reached");
        }
    }

    let level = bus.poll_interrupt(insns);
    // A hardware HALT cannot be resumed by an interrupt. STOP can, but the
    // core's single-step API does not poll interrupts while stopped.
    if cpu.stopped & m68k::core::execute::STOP_LEVEL_HALT != 0 {
        return StepResult::Stopped;
    }
    let mask = (cpu.int_mask >> 8) as u8;
    if cpu.stopped == m68k::core::execute::STOP_LEVEL_STOP && cpu.s_flag != 0 {
        if level > 0 && (level == 7 || level > mask) {
            service_interrupt(cpu, bus, level);
            cpu.set_irq(0);
            return StepResult::Ok { cycles: 44 };
        }
        return StepResult::Ok { cycles: 4 };
    }
    cpu.set_irq(level);

    // Legacy PC-specific return stub, retained only for experiments after a
    // real card-model failure has been isolated.
    if research_harness && (pc == 0x0001_7828 || pc == 0x0011_7828) {
        bus.trace_stack("research_return_stub", pc, cpu.sp());
        cpu.set_d(0, 1);
        let ret = bus.read_long(cpu.sp());
        cpu.set_sp(cpu.sp() + 8);
        cpu.pc = ret;
    }

    let res = match cpu.step(bus) {
        StepResult::TrapInstruction { trap_num } => {
            cpu.take_trap_exception(bus, trap_num);
            StepResult::Ok { cycles: 34 }
        }
        StepResult::Stopped if cpu.stopped == m68k::core::execute::STOP_LEVEL_STOP => {
            StepResult::Ok { cycles: 4 }
        }
        res => res,
    };

    if bus.trace.enabled() && bus.trace.calls && matches!(res, StepResult::Ok { .. }) {
        let opcode = cpu.ir as u16;
        if opcode & 0xffc0 == 0x4e80 || opcode & 0xff00 == 0x6100 {
            bus.trace.event(
                insns,
                cpu.ppc,
                "subroutine_call",
                &format!(
                    "opcode={opcode:04x} post_step_pc={:#010x} sp={:#010x} stack_top_after_step={:02x?}",
                    cpu.pc,
                    cpu.sp(),
                    bus.peek_bytes(cpu.sp(), 4)
                ),
            );
        } else if matches!(opcode, 0x4e75 | 0x4e74 | 0x4e77) {
            bus.trace.event(
                insns,
                cpu.ppc,
                "subroutine_return",
                &format!(
                    "opcode={opcode:04x} post_step_pc={:#010x} sp={:#010x} d0={:#010x}",
                    cpu.pc,
                    cpu.sp(),
                    cpu.dar[0]
                ),
            );
        }
    }
    let before_hle_pc = cpu.pc;
    if bus.trace.enabled()
        && research_harness
        && bus.research_in_app
        && matches!(
            cpu.pc,
            0x0001_6744 | 0x0011_6744 | 0x0001_2b26 | 0x0011_2b26 | 0x0001_8c48 | 0x0011_8c48
        )
    {
        bus.trace_stack("research_dispatch_stub", cpu.pc, cpu.sp());
    }

    if research_harness && pc == 0x001B_E4F0 {
        bus.research_in_app = true;
        // Clear remaining BSS from $001FF2EC to top of RAM ($00200000)
        for a in (0x001F_F2EC..0x0020_0000).step_by(4) {
            bus.write_long(a, 0);
        }
        // Driver dummy jump table placed at $0038_0000 in ERAM.
        // Callers do JSR disp(A1) (e.g. JSR 12(A1)), expecting executable code at each slot.
        // Each 4-byte slot is CLR.L D0; RTS (0x4280 0x4E75).
        for slot in 0..128 {
            bus.write_word(0x0038_0000 + slot * 4, 0x4280); // CLR.L D0
            bus.write_word(0x0038_0000 + slot * 4 + 2, 0x4E75); // RTS
        }
        bus.write_long(0x001F_F568, 0x0038_0000);
    }
    let in_app = research_harness && bus.research_in_app;

    // 0. If guest calls $D94 via JSR (a no-op callback initialized to the default vector),
    // return cleanly (RTS) instead of letting RTE pop a non-existent exception frame.
    if in_app && cpu.pc == 0x0000_0D94 {
        let ret = bus.read_long(cpu.sp());
        cpu.set_sp(cpu.sp() + 4);
        cpu.set_d(0, 0);
        cpu.pc = ret;
    }

    // 1. $1CD0CA and $1CD0D8 call a cursor/screen position helper expecting RTD #4.
    // In this EPROM version, $1113C is not an entry point; emulate RTD #4.
    if in_app && (cpu.pc == 0x0001_113C || cpu.pc == 0x0001_113A) {
        let ret = bus.read_long(cpu.sp());
        cpu.set_sp(cpu.sp() + 8);
        cpu.pc = ret;
    }

    // 2. Application calls into opsys (< $0004_0000) redirect to opsys in RAM ($0010_0000).
    // This maps $0000_FB32 -> $0010_FB32 (syscall #0x20 stub), $0001_2B26 -> $0011_2B26, etc.
    if in_app && pc >= 0x0014_0000 && cpu.pc < 0x0004_0000 {
        let mut target = cpu.pc;
        if target == 0x0001_2B28 {
            target = 0x0001_2B26;
        }
        cpu.pc = target.wrapping_add(0x0010_0000);
    }

    // 3. opsys $16744 is a driver/message dispatch helper.
    // Writes output pointers: *(arg2) = b1, *(arg3) = b2, *(arg4) = 0.
    if in_app && (cpu.pc == 0x0011_6744 || cpu.pc == 0x0001_6744) {
        let p1 = bus.read_long(cpu.sp() + 8);
        let p2 = bus.read_long(cpu.sp() + 12);
        let p3 = bus.read_long(cpu.sp() + 16);
        let mut b1 = bus.research_struct_buf;
        if b1 >= 0x0036_0000 {
            bus.research_struct_buf = 0x0030_0000;
            b1 = 0x0030_0000;
        }
        let b2 = b1 + 0x100;
        bus.research_struct_buf = b2 + 0x100;
        for a in (b1..b1 + 0x100).step_by(4) {
            bus.write_long(a, 0);
        }
        for a in (b2..b2 + 0x100).step_by(4) {
            bus.write_long(a, 0);
        }
        if (0x0010_0000..0x0040_0000).contains(&p1) {
            bus.write_long(p1, b1);
        }
        if (0x0010_0000..0x0040_0000).contains(&p2) {
            bus.write_long(p2, b2);
        }
        if (0x0010_0000..0x0040_0000).contains(&p3) {
            bus.write_long(p3, 0);
        }
        let ret = bus.read_long(cpu.sp());
        cpu.set_sp(cpu.sp() + 4);
        cpu.set_d(0, 0);
        cpu.pc = ret;
    }

    // 4. opsys $144F8 is a node deallocator: (node, list_head).
    // If node is invalid (e.g. -1 or null), clear the list head and return.
    if in_app && (cpu.pc == 0x0011_44F8 || cpu.pc == 0x0001_44F8) {
        let node = bus.read_long(cpu.sp() + 4);
        let list_head = bus.read_long(cpu.sp() + 8);
        if node == 0 || node == 0xFFFF_FFFF || !(0x0010_0000..0x0040_0000).contains(&node) {
            if (0x0010_0000..0x0040_0000).contains(&list_head) {
                bus.write_long(list_head + 0x50, 0);
            }
            let ret = bus.read_long(cpu.sp());
            cpu.set_sp(cpu.sp() + 4);
            cpu.set_d(0, 0);
            cpu.pc = ret;
        }
    }

    // 5. opsys $175EC is clear_list(list_head): empties list at 0x50(list_head).
    if in_app && (cpu.pc == 0x0011_75EC || cpu.pc == 0x0001_75EC) {
        let list = bus.read_long(cpu.sp() + 4);
        if (0x0010_0000..0x0040_0000).contains(&list) {
            bus.write_long(list + 0x50, 0);
        }
        let ret = bus.read_long(cpu.sp());
        cpu.set_sp(cpu.sp() + 4);
        cpu.set_d(0, 0);
        cpu.pc = ret;
    }

    // 6. opsys $16524 is task_switch(tcb, status).
    // If tcb has invalid PC or SP, do not switch to an invalid task; return to caller.
    if in_app && (cpu.pc == 0x0011_6524 || cpu.pc == 0x0001_6524) {
        let tcb = bus.read_long(cpu.sp() + 4);
        let target_sp = if (0x0010_0000..0x0040_0000).contains(&tcb) {
            bus.read_long(tcb + 4)
        } else {
            0
        };
        let target_pc = if (0x0010_0000..0x0040_0000).contains(&tcb) {
            bus.read_long(tcb)
        } else {
            0
        };
        if target_pc < 0x0010_0000 || !(0x0010_0000..0x0040_0000).contains(&target_sp) {
            let ret = bus.read_long(cpu.sp());
            cpu.set_sp(cpu.sp() + 4);
            cpu.set_d(0, 0);
            cpu.pc = ret;
        }
    }

    // Communication helpers must execute their guest control flow. The old
    // "abort" hook at $1F799E intercepted a normal BNE, overwrote status, and
    // falsely reported recovery. The old "mock vehicle" hook at $1F1018
    // actually intercepted a buffer-length summation routine. Neither is a
    // valid vehicle transport or cancellation implementation.

    // 7. opsys $12B26 is channel/object open helper.
    // If called from application, set *handle = 1 and return D0 = 0 (success).
    if in_app && (cpu.pc == 0x0011_2B26 || cpu.pc == 0x0001_2B26) {
        let handle_ptr = bus.read_long(cpu.sp() + 12);
        if (0x0010_0000..0x0040_0000).contains(&handle_ptr) {
            bus.write_long(handle_ptr, 1);
        }
        let ret = bus.read_long(cpu.sp());
        cpu.set_sp(cpu.sp() + 4);
        cpu.set_d(0, 0);
        cpu.pc = ret;
    }

    // 8. opsys $14456 is find_object(table, id).
    if in_app && (cpu.pc == 0x0011_4456 || cpu.pc == 0x0001_4456) {
        let table = bus.read_long(cpu.sp() + 4);
        let id = bus.read_long(cpu.sp() + 8);
        let bucket = id & 0x1F;
        let bucket_addr = table.wrapping_add(bucket * 4);
        let mut curr = if (0x0010_0000..0x0040_0000).contains(&bucket_addr) {
            bus.read_long(bucket_addr.wrapping_add(0x20))
        } else {
            0
        };
        let mut found = 0;
        let mut steps = 0;
        while (0x0010_0000..0x0040_0000).contains(&curr) && steps < 64 {
            steps += 1;
            let node_id = bus.read_long(curr.wrapping_add(4));
            if node_id == id {
                found = curr;
                break;
            }
            curr = bus.read_long(curr.wrapping_add(0x0C));
        }
        let ret = bus.read_long(cpu.sp());
        cpu.set_sp(cpu.sp() + 4);
        cpu.set_d(0, found);
        cpu.pc = ret;
    }

    // 8b. opsys $18C48 is buffer/descriptor setup. Return D0 = 0.
    if in_app && (cpu.pc == 0x0011_8C48 || cpu.pc == 0x0001_8C48) {
        let ret = bus.read_long(cpu.sp());
        cpu.set_sp(cpu.sp() + 4);
        cpu.set_d(0, 0);
        cpu.pc = ret;
    }

    // 10. opsys $1240C is linked list traversal: A2 = A2->next.
    // If next pointer is invalid / unmapped, terminate list traversal (A2 = NULL).
    if in_app && (cpu.pc == 0x0011_240C || cpu.pc == 0x0001_240C) {
        let a2 = cpu.dar[10];
        let next = if (0x0010_0000..0x0040_0000).contains(&a2) {
            bus.read_long(a2.wrapping_add(8))
        } else {
            0
        };
        if !(0x0010_0000..0x0040_0000).contains(&next) {
            cpu.dar[10] = 0;
            cpu.set_d(0, 0);
            cpu.pc = if cpu.pc >= 0x0010_0000 {
                0x0011_2414
            } else {
                0x0001_2414
            };
        }
    }

    // 11. opsys $169BE is compiler runtime __divsi3(D0, D1).
    // Safely handles division by zero without trapping.
    if in_app && (cpu.pc == 0x0011_69BE || cpu.pc == 0x0001_69BE) {
        let d1 = cpu.dar[1] as i32;
        let d0 = cpu.dar[0] as i32;
        let res = if d1 != 0 { d0.wrapping_div(d1) } else { 0 };
        let ret = bus.read_long(cpu.sp());
        cpu.set_sp(cpu.sp() + 4);
        cpu.set_d(0, res as u32);
        cpu.pc = ret;
    }

    // 12. opsys $16A5A is compiler runtime __modsi3(D0, D1).
    // Safely handles modulo by zero without trapping.
    if in_app && (cpu.pc == 0x0011_6A5A || cpu.pc == 0x0001_6A5A) {
        let d1 = cpu.dar[1] as i32;
        let d0 = cpu.dar[0] as i32;
        let res = if d1 != 0 { d0.wrapping_rem(d1) } else { 0 };
        let ret = bus.read_long(cpu.sp());
        cpu.set_sp(cpu.sp() + 4);
        cpu.set_d(0, res as u32);
        cpu.pc = ret;
    }

    // 13. opsys $10146 is directory path building loop.
    // Guard against circular parent references in directory tables (max 8 levels).
    if in_app && (cpu.pc == 0x0011_0146 || cpu.pc == 0x0001_0146) {
        let depth = bus.research_path_depth;
        bus.research_path_depth += 1;
        if depth >= 8 {
            bus.research_path_depth = 0;
            cpu.pc = if cpu.pc >= 0x0010_0000 {
                0x0011_017C
            } else {
                0x0001_017C
            };
        }
    }

    // 14. opsys $1663E is bzero(dst, len). Accelerate in Rust.
    if in_app && (cpu.pc == 0x0011_663E || cpu.pc == 0x0001_663E) {
        let dst = bus.read_long(cpu.sp() + 4);
        let len = bus.read_long(cpu.sp() + 8);
        if len > 0 && len <= 0x0010_0000 && (0x0010_0000..0x0040_0000).contains(&dst) {
            for a in dst..dst.saturating_add(len) {
                bus.write_byte(a, 0);
            }
        }
        let ret = bus.read_long(cpu.sp());
        cpu.set_sp(cpu.sp() + 4);
        cpu.set_d(0, 0);
        cpu.pc = ret;
    }

    // Guard bcopy: if src is invalid unmapped pointer (e.g. 0xFFFFFF70 because A6 is 0),
    // do not corrupt pSOS global structures!
    if in_app && (pc == 0x0011_653A || pc == 0x0001_653A) {
        let src = bus.read_long(cpu.sp() + 4);
        let dst = bus.read_long(cpu.sp() + 8);
        let len = bus.read_long(cpu.sp() + 12);
        if len <= 0x0010_0000
            && !(0x0010_0000..0x0040_0000).contains(&src)
            && (0x0010_0000..0x0040_0000).contains(&dst)
        {
            for a in dst..dst.saturating_add(len) {
                bus.write_byte(a, 0);
            }
            let ret = bus.read_long(cpu.sp());
            cpu.set_sp(cpu.sp() + 4);
            cpu.set_d(0, 0);
            cpu.pc = ret;
        }
    }

    // Fix pSOS TCB allocation table pointer in 12(A5)
    if in_app && (pc == 0x0011_257C || pc == 0x0001_257C) {
        let a5 = cpu.dar[13];
        if (0x0010_0000..0x0040_0000).contains(&a5) {
            let tcb_table = bus.read_long(a5 + 12);
            if !(0x0010_0000..0x0040_0000).contains(&tcb_table) {
                println!("FIXING 12(A5): was {tcb_table:#010x} -> setting to 0x001D0000");
                bus.write_long(a5 + 12, 0x001D_0000);
            }
        }
    }
    // 15. opsys $1072C is BCS $106CA after CMP.L D5, D3.
    // If D5 is invalid (> 128, e.g. uninitialized stack garbage), skip runaway loop to $1072E.
    if in_app && (pc == 0x0011_072C || pc == 0x0001_072C) && cpu.dar[5] > 128 {
        cpu.pc = if pc >= 0x0010_0000 {
            0x0011_072E
        } else {
            0x0001_072E
        };
    }

    // 16. opsys $11814 is extent search loop in file block lookup.
    // If 4(A2) == 0 (end of extents), exit loop cleanly to $1183C.
    if in_app && (cpu.pc == 0x0011_1814 || cpu.pc == 0x0001_1814) {
        let a2 = cpu.dar[10];
        let extent_len = if (0x0010_0000..0x0040_0000).contains(&a2) {
            bus.read_long(a2.wrapping_add(4))
        } else {
            0
        };
        if extent_len == 0 {
            cpu.set_d(0, 1);
            cpu.pc = if cpu.pc >= 0x0010_0000 {
                0x0011_183C
            } else {
                0x0001_183C
            };
        }
    }

    // 17. opsys $10496 is BCS $1046C after CMP.L D4, D3 in directory search.
    // If D4 == 0xFFFFFFFF (-1, empty directory), skip to $10498 (return 0).
    if in_app
        && (pc == 0x0011_0496 || pc == 0x0001_0496)
        && (cpu.dar[4] == 0xFFFF_FFFF || (cpu.dar[4] as i32) < 0)
    {
        cpu.set_d(0, 0);
        cpu.pc = if pc >= 0x0010_0000 {
            0x0011_0498
        } else {
            0x0001_0498
        };
    }

    if bus.trace.enabled() && cpu.pc != before_hle_pc {
        bus.trace_event("research_redirect", &format!("from={before_hle_pc:#010x} to={:#010x} d0={:#010x} backend=host-hle external_tx=false", cpu.pc, cpu.dar[0]));
    }
    res
}

#[derive(Debug, Clone)]
pub struct HeadlessRunResult {
    pub final_insns: u64,
    pub stop_reason: String,
    pub splash_ready: bool,
    pub outcome: Outcome,
    pub last_verified: String,
}

pub struct RunSettings<'a> {
    pub limit_insns: u64,
    pub sample_hotspots: bool,
    pub interactive: bool,
    pub android_live: bool,
    pub harness_target: Option<options::HarnessTarget>,
    pub output_dir: &'a Path,
    pub replay: &'a [replay::ReplayKey],
}

/// Headless execution loop running the guest CPU up to `limit_insns`.
///
/// Stops early if the guest splash is verified in VRAM, guest bootstrap fails,
/// or CPU halts/stops.
pub fn run_headless(
    cpu: &mut CpuCore,
    bus: &mut Tech2Bus,
    opsys: &[u8],
    mut insns: u64,
    settings: RunSettings<'_>,
    pc_counts: &mut std::collections::HashMap<u32, u64>,
) -> HeadlessRunResult {
    let limit_insns = settings.limit_insns;
    let mut interactive = settings
        .interactive
        .then(|| interactive::Session::new(settings.output_dir));
    let mut harness = settings
        .harness_target
        .map(|target| harness::Harness::new(target, insns));
    let mut outcome = Outcome::Incomplete;
    let mut stop = String::from("instruction budget exhausted before requested milestone");
    let mut splash_ready = false;
    let mut ring: [u32; 64] = [0; 64];
    let mut ring_i: usize = 0;
    let mut fault_reported = false;
    let mut replay_index = 0;
    let mut next_live_frame = std::time::Instant::now();
    let mut live_screen = String::new();
    let mut live_frame = artifacts::LiveFrame::default();
    let mut next_native_key = std::time::Instant::now();
    let security_ssa_before = (settings.harness_target
        == Some(options::HarnessTarget::SecurityLink1367))
    .then(|| bus.card.get(0xfe0000..0xfe0000 + 714).map(<[u8]>::to_vec))
    .flatten();

    let trace_from: u64 = env::var("TRACE_FROM")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(u64::MAX);
    let trace_to: u64 = env::var("TRACE_TO")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    while insns < limit_insns {
        if insns.is_multiple_of(50_000) {
            if settings.android_live && std::time::Instant::now() >= next_live_frame {
                next_live_frame = std::time::Instant::now() + std::time::Duration::from_millis(100);
                let capture = (|| -> std::io::Result<()> {
                    live_frame.publish(bus, settings.output_dir)?;
                    let screen = bus.screen_text();
                    if screen != live_screen {
                        std::fs::write(settings.output_dir.join("screen.txt"), &screen)?;
                        println!(
                            "LCD: {}",
                            screen.split_whitespace().collect::<Vec<_>>().join(" ")
                        );
                        live_screen = screen;
                    }
                    Ok(())
                })();
                if let Err(e) = capture {
                    outcome = Outcome::OutputFailure;
                    stop = format!("Live LCD capture: {e}");
                    break;
                }
                if settings.output_dir.join("native-stop").is_file() {
                    stop = "Native Android session stopped by operator".into();
                    break;
                }
            }
            if let Some(session) = &mut interactive {
                match session.poll(cpu, bus) {
                    Ok(interactive::Poll::Continue) => {}
                    Ok(interactive::Poll::Paused) => {
                        std::thread::sleep(std::time::Duration::from_millis(20));
                        continue;
                    }
                    Ok(interactive::Poll::Stop) => {
                        stop = "interactive session stopped by operator or deadline".into();
                        break;
                    }
                    Err(error) => {
                        stop = format!("interactive session: {error}");
                        outcome = Outcome::OutputFailure;
                        break;
                    }
                }
            }
        }
        let pc = cpu.pc;
        bus.current_pc = pc;
        bus.current_insns = insns;
        while settings
            .replay
            .get(replay_index)
            .is_some_and(|key| key.insns == insns)
        {
            let key = settings.replay[replay_index];
            if key.code == bus::KEY_EXIT {
                bus.exit_key();
            } else {
                bus.press_key(key.code);
            }
            replay_index += 1;
        }

        if insns >= trace_from && insns <= trace_to {
            let op = bus.read_word(pc);
            let (decoded, _) = m68k::dasm::disassemble(pc, op, CpuType::M68EC020);
            println!(
                "TRACE insns={} pc={:#010x} op={:#06x} {decoded} SP={:#010x} D0={:#010x} D1={:#010x} D7={:#010x} A0={:#010x} A1={:#010x} A5={:#010x} A7={:#010x}",
                insns, pc, op, cpu.sp(), cpu.dar[0], cpu.dar[1], cpu.dar[7], cpu.dar[8], cpu.dar[9], cpu.dar[13], cpu.dar[15]
            );
        }

        ring[ring_i % 64] = pc;
        ring_i += 1;

        if pc == 0x0000_0D94 && !fault_reported {
            fault_reported = true;
            let n = ring_i.min(64);
            let start = ring_i - n;
            let mut recent = Vec::with_capacity(n);
            print!("FIRST FAULT -> $D94 at insns={insns}; preceding PCs:");
            for k in start..ring_i {
                let p = ring[k % 64];
                recent.push(p);
                print!(" {:#08x}", p);
            }
            println!();
            println!(
                "  IMASK={:#x} SP={:#010x} A0={:#010x} A1={:#010x} D0={:#010x}",
                cpu.int_mask,
                cpu.sp(),
                cpu.dar[8],
                cpu.dar[9],
                cpu.dar[0]
            );
            let d_regs: [u32; 8] = cpu.dar[0..8].try_into().unwrap();
            let a_regs: [u32; 8] = cpu.dar[8..16].try_into().unwrap();
            crate::logger::record_crash_report(
                "CPU",
                insns,
                "Vector $D94 Unhandled Exception",
                pc,
                cpu.sp(),
                &d_regs,
                &a_regs,
                &recent,
            );
        }

        match step_guest(cpu, bus, insns) {
            StepResult::Ok { .. } => {}
            res => {
                let n = ring_i.min(64);
                let start = ring_i - n;
                let mut recent = Vec::with_capacity(n);
                for k in start..ring_i {
                    recent.push(ring[k % 64]);
                }
                let d_regs: [u32; 8] = cpu.dar[0..8].try_into().unwrap();
                let a_regs: [u32; 8] = cpu.dar[8..16].try_into().unwrap();
                crate::logger::record_crash_report(
                    "CPU",
                    insns,
                    &bus.failure_reason
                        .clone()
                        .unwrap_or_else(|| format!("CPU Step Stopped: {res:?}")),
                    pc,
                    cpu.sp(),
                    &d_regs,
                    &a_regs,
                    &recent,
                );
                println!("STEP STOPPED: insns={insns} PC={pc:#010x} res={res:?}");
                print!("  Preceding PCs:");
                for k in 0..64 {
                    print!(" {:#08x}", ring[(ring_i + k) % 64]);
                }
                println!();
                println!("  SP={:#010x} bank={:#x}", cpu.sp(), bus.bank);
                stop = bus
                    .failure_reason
                    .clone()
                    .unwrap_or_else(|| format!("stopped PC={pc:#010x} res={res:?}"));
                outcome = Outcome::GuestFailure;
                break;
            }
        }

        // $161C0..$163B0 is an intentional blank hole in the supplied flash,
        // not executable firmware.  Record the exact control-flow source if
        // the guest enters it; this distinguishes a missing chip-select model
        // from a bad function pointer or relocation.
        if *BOOT_TRACE
            && (0x0001_61C0..0x0001_63B2).contains(&cpu.pc)
            && !(0x0001_61C0..0x0001_63B2).contains(&pc)
        {
            println!(
                "BOOT_TRACE hole-entry insns={insns} from={pc:#010x} op={:#06x} to={:#010x} SP={:#010x} stack={:#010x} {:#010x} {:#010x}",
                bus.read_word(pc),
                cpu.pc,
                cpu.sp(),
                bus.read_long(cpu.sp()),
                bus.read_long(cpu.sp() + 4),
                bus.read_long(cpu.sp() + 8),
            );
        }

        insns += 1;
        bus.current_pc = pc;

        // Sample PC hotspots every 1000 insns for spin-loop detection
        if settings.sample_hotspots && insns.is_multiple_of(1000) {
            *pc_counts.entry(pc).or_insert(0) += 1;
        }

        // The research harness used to inject the downloaded operating system
        // after the RAM test.  A fidelity run records the same handoff point
        // but leaves image loading to guest-visible hardware.
        if pc == 0x000008ec && !bus.opsys_restored {
            if bus.execution_mode.is_research_harness() {
                println!("POST RAM TEST FINISHED: restoring research opsys RAM...");
                if !opsys.is_empty() {
                    bus.load_opsys_ram(opsys);
                }
            } else {
                println!("POST RAM TEST FINISHED: fidelity mode leaves opsys RAM untouched");
            }
            bus.opsys_restored = true;
        }

        // Scan at a frame-sized cadence.
        if !settings.interactive && insns.is_multiple_of(100_000) && insns >= 6_500_000 {
            println!(
                "HEARTBEAT insns={} PC={:#010x} SP={:#010x}",
                insns,
                cpu.pc,
                cpu.sp()
            );
        }
        if insns.is_multiple_of(50_000) {
            bus.current_insns = insns;
            bus.current_pc = cpu.pc;
            bus.trace_screen();
            if matches!(
                settings.harness_target,
                Some(
                    options::HarnessTarget::DtcLink1367
                        | options::HarnessTarget::EngineData1367
                        | options::HarnessTarget::NativeManual
                )
            ) && std::time::Instant::now() >= next_native_key
            {
                next_native_key = std::time::Instant::now() + std::time::Duration::from_millis(20);
                let path = settings.output_dir.join("native-key.txt");
                match std::fs::read_to_string(&path) {
                    Ok(command) => {
                        let command = command.trim();
                        if command == "stop" {
                            let _ = std::fs::remove_file(&path);
                            stop = "native DTC operator stopped the session".into();
                            break;
                        }
                        let code = command
                            .strip_prefix("0x")
                            .and_then(|hex| u8::from_str_radix(hex, 16).ok())
                            .filter(|code| *code <= 31);
                        let Some(code) = code else {
                            stop = "invalid native-key.txt; expected one 0x00..0x1f encoder code or stop".into();
                            outcome = Outcome::OutputFailure;
                            break;
                        };
                        if let Err(error) = std::fs::remove_file(&path) {
                            stop = format!("consume native key: {error}");
                            outcome = Outcome::OutputFailure;
                            break;
                        }
                        bus.trace_event("native_operator_key", &format!("encoder={code:#04x}"));
                        if code == bus::KEY_EXIT {
                            bus.exit_key();
                        } else {
                            bus.press_key(code);
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => {
                        stop = format!("read native key: {error}");
                        outcome = Outcome::OutputFailure;
                        break;
                    }
                }
            }
            if insns.is_multiple_of(1_000_000) && bus.trace.enabled() {
                bus.trace_event(
                    "progress",
                    &format!(
                        "sp={:#010x} stopped={} pit_ticks={} keys_queued={} key_phases_consumed={}",
                        cpu.sp(),
                        cpu.stopped,
                        bus.pit_ticks,
                        bus.key_events,
                        bus.key_consumed
                    ),
                );
            }
            if let Err(e) = bus.trace.check() {
                stop = format!("trace persistence: {e}");
                outcome = Outcome::OutputFailure;
                break;
            }
            if bus
                .candi_link
                .as_ref()
                .is_some_and(|link| link.is_stopped())
            {
                stop = "native CANdi stopped; see preceding transport/CPU trace".into();
                outcome = Outcome::Incomplete;
                break;
            }
            if bus.guest_boot_failed() {
                stop = String::from("guest reported processor halt or pSOS bootstrap failure");
                outcome = Outcome::GuestFailure;
                break;
            }
            // Keep the guest's explicit connection error on screen. Continuing
            // its unsupported-link cleanup can walk an uninitialized list and
            // overwrite the useful failure with an address-error halt.
            if !settings.interactive && bus.execution_mode.is_research_harness() {
                if let Some(reason) = recovery::guest_link_error(&bus.screen_text()) {
                    bus.trace_event("guest_link_unavailable", reason);
                    stop = reason.into();
                    outcome = Outcome::Incomplete;
                    break;
                }
            }
            if bus.guest_splash_reached() {
                splash_ready = true;
                if !settings.interactive && harness.is_none() && settings.replay.is_empty() {
                    stop = String::from("guest splash verified");
                    outcome = Outcome::Success;
                    break;
                }
            }
            if let Some(harness) = &mut harness {
                match harness.observe(&bus.screen_text(), insns) {
                    Err(reason) => {
                        stop = reason;
                        break;
                    }
                    Ok(Some(event)) => {
                        let path =
                            settings
                                .output_dir
                                .join(if event.capture == "native_dtc_screen" {
                                    format!("native-dtc-{insns}.ppm")
                                } else {
                                    format!("{}.ppm", event.capture)
                                });
                        if let Err(error) = artifacts::save_screen(bus, &path) {
                            stop = format!("capture {}: {error}", path.display());
                            outcome = Outcome::OutputFailure;
                            break;
                        }
                        if event.capture == "native_dtc_screen" {
                            if let Err(error) = std::fs::write(
                                settings.output_dir.join("native-dtc-screen.txt"),
                                bus.screen_text(),
                            ) {
                                stop = format!("native DTC screen text: {error}");
                                outcome = Outcome::OutputFailure;
                                break;
                            }
                        }
                        if let Some(before) = &security_ssa_before {
                            let evidence_dir = settings
                                .output_dir
                                .join(format!("security-{insns}-{}", event.capture));
                            let saved = std::fs::create_dir(&evidence_dir)
                                .and_then(|()| {
                                    artifacts::save_security_snapshot(bus, before, &evidence_dir)
                                })
                                .and_then(|()| {
                                    std::fs::write(
                                        evidence_dir.join("screen.txt"),
                                        bus.screen_text(),
                                    )
                                });
                            if let Err(error) = saved {
                                stop = format!(
                                    "security milestone capture {}: {error}",
                                    evidence_dir.display()
                                );
                                outcome = Outcome::OutputFailure;
                                break;
                            }
                        }
                        crate::log_info!(
                            "HARNESS",
                            insns,
                            "Verified {}; captured {}",
                            event.capture,
                            path.display()
                        );
                        if event.exit {
                            bus.exit_key();
                        }
                        if let Some(key) = event.key {
                            bus.press_key(key);
                        }
                        if event.complete {
                            stop = format!(
                                "research harness {:?} verified",
                                settings.harness_target.unwrap()
                            );
                            outcome = Outcome::Success;
                            break;
                        }
                    }
                    Ok(None) => {}
                }
            }
        }
    }
    if outcome == Outcome::Incomplete {
        if let Some(harness) = &harness {
            stop = format!(
                "{stop}; waiting for {}; last verified: {}",
                harness.waiting_for(),
                harness.last_verified
            );
        }
    }

    HeadlessRunResult {
        final_insns: insns,
        stop_reason: stop,
        splash_ready,
        outcome,
        last_verified: harness
            .map(|h| h.last_verified.to_string())
            .unwrap_or_else(|| if splash_ready { "splash" } else { "none" }.into()),
    }
}

fn main() -> std::process::ExitCode {
    let _performance = performance::start();
    let mut attempt = 0;
    loop {
        match run(attempt) {
            Ok((_, true)) => {
                attempt += 1;
            }
            Ok((code, false)) => return std::process::ExitCode::from(code),
            Err((code, error)) => {
                eprintln!("ERROR: {error}");
                #[cfg(feature = "gui")]
                if desktop_requested() {
                    let message = format!("Cannot start scanner. {error} Check the image paths or place the required files in the launch folder. Restart to retry, or quit.");
                    match gui::startup_recovery(&message) {
                        Ok(recovery::Action::Restart) => {
                            attempt += 1;
                            continue;
                        }
                        Ok(_) => {}
                        Err(e) => eprintln!("Recovery window: {}", e.message),
                    }
                }
                return std::process::ExitCode::from(code);
            }
        }
    }
}

#[cfg(feature = "gui")]
fn desktop_requested() -> bool {
    let Some(args) = env::args_os()
        .skip(1)
        .map(|a| a.into_string().ok())
        .collect::<Option<Vec<_>>>()
    else {
        return false;
    };
    Options::parse(args, |key| env::var(key).ok()).is_ok_and(|opts| !opts.headless && !opts.help)
}

fn run(attempt: u64) -> Result<(u8, bool), (u8, String)> {
    let args: Vec<String> = env::args_os()
        .skip(1)
        .map(|a| {
            a.into_string()
                .map_err(|_| (2, "arguments must be valid UTF-8".into()))
        })
        .collect::<Result<_, _>>()?;
    // Help remains usable even when the environment contains bad settings.
    if args.iter().any(|a| a == "--help" || a == "-help") {
        println!("{}", options::HELP);
        return Ok((0, false));
    }
    let mut opts = Options::parse(args, |key| env::var(key).ok()).map_err(|e| (2, e))?;
    if opts.help {
        println!("{}", options::HELP);
        return Ok((0, false));
    }
    if let Some(module) = opts.ibus_dtc {
        tech2_emu::ibus_dtc::run(
            tech2_emu::vcx::Connection {
                target: opts.vcx_ssh.clone().expect("validated SSH target"),
                control_path: opts.vcx_control.clone(),
                ecu_profile: tech2_emu::vcx::EcuProfile::IbusBcm1367,
            },
            module,
            &opts.output,
        )
        .map_err(|e| (1, e))?;
        return Ok((0, false));
    }
    if attempt > 0 {
        // Keep the failed/cancelled run's report and trace. Never reset an old
        // directory merely because the user chooses Restart more than once.
        let base = opts.output.clone();
        let mut id = attempt;
        loop {
            let path = base.join(format!("restart-{id:04}"));
            if !path.exists() {
                opts.output = path;
                break;
            }
            id += 1;
        }
    }
    let replay_keys = opts
        .replay
        .as_deref()
        .map(replay::load)
        .transpose()
        .map_err(|e| (2, e))?
        .unwrap_or_default();
    if replay_keys
        .last()
        .is_some_and(|key| key.insns >= opts.max_insns)
    {
        return Err((
            2,
            "--max-insns must extend beyond the last replay key".into(),
        ));
    }
    let test_harness = opts.test_harness;
    let headless = opts.headless;
    let execution_mode = if opts.research {
        ExecutionMode::ResearchHarness
    } else {
        ExecutionMode::Fidelity
    };
    let mode = if opts.research {
        "research-harness"
    } else {
        "fidelity"
    };
    if opts.fast_boot {
        env::set_var("TECH2_FAKE_POST", "1");
    }
    println!(
        "MODE: {mode}{}",
        if opts.research {
            " (guest patches/HLE; not native boot evidence)"
        } else {
            ""
        }
    );

    let conf_path = opts.config.clone().or_else(|| {
        Path::new("tech2win.conf")
            .exists()
            .then(|| "tech2win.conf".into())
    });
    let cfg = if let Some(path) = &conf_path {
        Tech2WinConfig::load(path).map_err(|e| (2, e))?
    } else {
        Tech2WinConfig::default()
    };
    let (boot_path, boot) = load_boot(opts.boot.as_deref()).map_err(|e| (2, e))?;
    let configured_card = conf_path
        .as_ref()
        .filter(|_| cfg.card_path_explicit)
        .map(|_| cfg.card_path.as_path());
    let (card_path, card) =
        load_nao(opts.card.as_deref().or(configured_card)).map_err(|e| (2, e))?;
    cfg.print_banner();
    println!(
        "boot {} {} bytes vec SSP={:#010x} PC={:#010x}",
        boot_path.display(),
        boot.len(),
        load_binary::be_long(&boot, 0),
        load_binary::be_long(&boot, 4)
    );
    // Native firmware runs do not need a separate host-loaded download image.
    let (opsys_path, opsys) = if opts.research || opts.opsys.is_some() {
        load_opsys(opts.opsys.as_deref()).map_err(|e| (2, e))?
    } else {
        (std::path::PathBuf::new(), Vec::new())
    };
    // Refuse to overwrite an input image/configuration, including symlink
    // aliases, with this run's artifacts.
    if opts.candi && opts.candi_firmware.is_none() {
        opts.candi_firmware = Some(candi::worker::default_firmware_path());
    }
    let inputs: Vec<_> = [
        Some(&boot_path),
        Some(&card_path),
        Some(&opsys_path),
        conf_path.as_ref(),
        opts.replay.as_ref(),
        opts.candi_firmware.as_ref(),
    ]
    .into_iter()
    .flatten()
    .filter_map(|p| std::fs::canonicalize(p).ok())
    .collect();
    for name in [
        "report.json",
        "crash.json",
        "tech2.log",
        "trace.jsonl",
        "lcd.ppm",
        "console.ppm",
        "ssa-card-before.bin",
        "ssa-card-after.bin",
        "security-guest-ram.bin",
        "security-guest-eram.bin",
        "native-security-snapshot.json",
        "stage1_splash.ppm",
        "stage2_main_menu.ppm",
        "stage3_model_year.ppm",
        "stage4_vehicle_platform.ppm",
        "stage5_diagnostics.ppm",
        "stage6_engine.ppm",
        "stage7_checking_key.ppm",
        "stage7_recovered_menu.ppm",
        "stage8_back_to_platform.ppm",
    ] {
        let path = opts.output.join(name);
        if std::fs::canonicalize(&path).is_ok_and(|p| inputs.contains(&p)) {
            return Err((
                4,
                format!(
                    "output {} would overwrite an input; choose another --output-dir",
                    path.display()
                ),
            ));
        }
    }
    std::fs::create_dir_all(&opts.output).map_err(|e| {
        (
            4,
            format!("output directory {}: {e}", opts.output.display()),
        )
    })?;
    // Remove an old verdict before attempting this run's outputs, so a failure
    // cannot leave a previous success report looking current.
    let report_path = opts.output.join("report.json");
    match std::fs::remove_file(&report_path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err((4, format!("{}: {e}", report_path.display()))),
    }
    let crash_path = opts.output.join("crash.json");
    match std::fs::remove_file(&crash_path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err((4, format!("{}: {e}", crash_path.display()))),
    }
    logger::init_file(&opts.output.join("tech2.log")).map_err(|e| (4, format!("log file: {e}")))?;
    // Keep alive for the run; Drop joins the worker. Default (flag off) is a no-op.
    let candi_handle = candi::maybe_start(candi::CandiConfig {
        enabled: opts.candi && !opts.candi_native_link,
        firmware_path: opts.candi_firmware.clone(),
    })
    .map_err(|e| (2, e))?;
    if opts.candi {
        println!(
            "CANDI: firmware CPU research enabled; native_uart={} adapter backend selected by --candi-nano-ssh",
            opts.candi_native_link
        );
    }
    let initial_security_ssa =
        if opts.target == options::HarnessTarget::SecurityLink1367 || opts.candi_chipsoft_seeds {
            Some(
                card.get(0xfe0000..0xfe0000 + 714)
                    .ok_or_else(|| {
                        (
                            2,
                            "Security research requires the known SSA card region".into(),
                        )
                    })?
                    .to_vec(),
            )
        } else {
            None
        };
    let mut bus = Tech2Bus::new(boot, card, execution_mode);
    if initial_security_ssa.is_some() {
        bus.ssa_flash = Some(ssa_flash::SsaFlash::default());
    }
    if opts.candi_native_link {
        let path = opts
            .candi_firmware
            .clone()
            .unwrap_or_else(candi::worker::default_firmware_path);
        bus.candi_link = Some(candi::native_link::NativeLink::new(&path).map_err(|e| (2, e))?);
        if let Some(target) = &opts.candi_nano_ssh {
            bus.candi_link
                .as_mut()
                .unwrap()
                .attach_j2534(
                    tech2_emu::vcx::Connection {
                        target: target.clone(),
                        control_path: opts.candi_nano_control.clone(),
                        ecu_profile: tech2_emu::vcx::EcuProfile::default(),
                    },
                    &opts.output,
                    opts.target == options::HarnessTarget::SecurityLink1367,
                    opts.candi_j2534_adapter,
                    opts.candi_seatbelt_audible,
                )
                .map_err(|e| (2, e))?;
        }
    }
    if let Some(token) = &opts.candi_chipsoft_usb_token {
        bus.candi_link
            .as_mut()
            .ok_or_else(|| (2, "Missing native CANdi".into()))?
            .attach_chipsoft_usb(
                token,
                &opts.output,
                opts.candi_chipsoft_symbol_only,
                opts.candi_chipsoft_seeds,
                opts.candi_chipsoft_audible,
            )
            .map_err(|e| (2, e))?;
    }
    if let Some(token) = &opts.candi_nano_usb_token {
        bus.candi_link
            .as_mut()
            .ok_or_else(|| (2, "Missing native CANdi".into()))?
            .attach_nano_usb(
                token,
                &opts.output,
                if opts.candi_nano_clear_dtc {
                    tech2_emu::nano_native::Profile::ClearDtc
                } else {
                    tech2_emu::nano_native::Profile::collection(
                        opts.target == options::HarnessTarget::SecurityLink1367,
                    )
                },
            )
            .map_err(|e| (2, e))?;
    }
    if matches!(
        opts.target,
        options::HarnessTarget::EcmLink1367
            | options::HarnessTarget::SecurityLink1367
            | options::HarnessTarget::DtcLink1367
            | options::HarnessTarget::EngineData1367
            | options::HarnessTarget::NativeManual
    ) {
        if let Some(link) = &mut bus.candi_link {
            link.enable_ignition_monitor(&opts.output);
        }
    }
    if opts.interactive_headless {
        bus.trace.enable_console();
    }
    if opts.verbose || (opts.candi_native_link && !opts.interactive_headless) {
        let path = opts.output.join("trace.jsonl");
        bus.trace = trace::Trace::open(&path, opts.trace_calls)
            .map_err(|e| (4, format!("trace {}: {e}", path.display())))?;
        if !opts.verbose && !opts.trace_calls {
            bus.trace.concise_transport();
            bus.trace_event("trace_policy", "detail=transport; CAN frames, CANdi messages, requests, screens, keys and errors retained; register/UART-byte/guest-internal detail omitted; use --verbose for full trace");
        }
        bus.trace_event("run_start", &format!("schema=1 mode={mode} trace_calls={} vehicle_transport=see-native-bridge-events screen_sample_insns=50000 request_labels=supplied-Saab-image; navigation=most-recent-host-key-not-proof-of-causality", opts.trace_calls));
        bus.trace_event("trace_retention", "32 MiB per segment, 3 previous segments; maximum 128 MiB per run; .1 is newest previous segment, .3 oldest; oversized records explicitly marked omitted");
        println!(
            "TRACE: {} (rotating: 32 MiB x 4 files, newest history in .1)",
            path.display()
        );
    }
    if opts.research {
        bus.load_opsys_ram(&opsys);
    }
    bus.bg_color = cfg.bg_u32();
    bus.fg_color = cfg.fg_u32();

    if execution_mode.is_research_harness() && opsys.len() == 0x37FE8 {
        bus.flash[0x8018..0x40000].copy_from_slice(&opsys);
        println!(
            "RESEARCH reflash: opsys.dwn -> flash $08018..$40000 ({:#x} bytes)",
            opsys.len()
        );
    }

    let native = !crate::options::env_flag("NO_NATIVE");
    if native && execution_mode.is_research_harness() {
        let mods = restore_rom_modules(&mut bus.flash, &opsys);
        let total: usize = mods.iter().map(|m| m.2).sum();
        println!(
            "NATIVE restored {} blanked ROM module(s), {total:#x} bytes total:",
            mods.len()
        );
        for (dst, src, len) in &mods {
            println!(
                "  flash ${dst:05X} len={len:#07x} <- opsys ${src:05X} (delta ${:X}) entry=${:05X}",
                dst - src,
                dst + 0x20
            );
        }
        // Note: 0x0E512 is ROM clear_screen(), NOT a card present check.
        // Overwriting it with RTS disabled screen clearing on all menu transitions.
        if crate::options::env_flag("PATCH_CARD_E512") && bus.flash.len() > 0x0E515 {
            bus.flash[0x0E512] = 0x70; // MOVEQ #1,D0
            bus.flash[0x0E513] = 0x01;
            bus.flash[0x0E514] = 0x4E; // RTS
            bus.flash[0x0E515] = 0x75;
        }
        // The old patch wrote 60 00 over $0D7C2: that is BRA.W, and its displacement is
        // the EXISTING next word ($33FC), so it branched wildly to $10BC0. $0D7C2 is
        // `TST.W D0`, $0D7C4 is `BLE.B +$10` -- force the taken path by turning the
        // conditional into an unconditional branch of the same size.
        if crate::options::env_flag("PATCH_CARDCHK") && bus.flash.len() > 0x0D7C5 {
            bus.flash[0x0D7C4] = 0x60;
        }
    }

    if execution_mode.is_research_harness() && crate::options::env_flag("HARNESS_PLANTS") {
        bus.plant_rtc();
        bus.plant_a5_mmobj();
    }

    let mut cpu = CpuCore::new();
    cpu.set_cpu_type(CpuType::M68EC020);
    if native {
        cpu.sr_mask = 0xA71F;
        println!("NATIVE sr_mask=0xA71F (CPU32: no M bit) on M68EC020 core");
    }
    cpu.reset(&mut bus);

    let max_insns = opts.max_insns;
    let mut pc_counts = std::collections::HashMap::new();
    let limit = if headless {
        max_insns
    } else {
        opts.warmup_insns.min(max_insns)
    };
    println!(
        "BOOT: running up to {limit} instructions (headless={headless}, harness={test_harness})"
    );
    let res = run_headless(
        &mut cpu,
        &mut bus,
        &opsys,
        0,
        RunSettings {
            limit_insns: limit,
            sample_hotspots: !opts.interactive_headless,
            interactive: opts.interactive_headless,
            android_live: opts.candi_nano_usb_token.is_some()
                || opts.candi_chipsoft_usb_token.is_some()
                || options::env_flag("TECH2_ANDROID_LIVE"),
            harness_target: test_harness.then_some(opts.target),
            output_dir: &opts.output,
            replay: &replay_keys,
        },
        &mut pc_counts,
    );
    let mut insns = res.final_insns;
    let mut stop = res.stop_reason;
    let mut outcome = res.outcome;
    let last_verified = res.last_verified;
    let restart = false;
    #[cfg(feature = "gui")]
    let mut restart = restart;
    #[cfg(feature = "gui")]
    if !headless && outcome == Outcome::Success && bus.guest_splash_reached() {
        println!("BOOT: opening TECH2-SCREEN with verified guest splash");
        let mut gui = GuiRunner::new_with_options(cfg.bg_u32(), cfg.fg_u32(), false);
        if let Some(target) = &opts.vcx_ssh {
            gui.set_vcx(
                tech2_emu::vcx::Connection {
                    target: target.clone(),
                    control_path: opts.vcx_control.clone(),
                    ecu_profile: opts.vcx_ecu,
                },
                opts.output.clone(),
                opts.ecm_information,
            );
        }
        if let Some(handle) = &candi_handle {
            gui.set_candi_worker(handle.running_flag());
        }
        match gui.run(&mut cpu, &mut bus, &mut insns) {
            Ok(()) => stop = "GUI closed normally".into(),
            Err(e) => {
                restart = e.restart;
                outcome = e.outcome;
                stop = e.message;
            }
        }
    }

    #[cfg(feature = "gui")]
    if !headless && !res.splash_ready {
        restart = gui::startup_recovery(&format!("Scanner program did not start. {stop} Check the selected images, then restart or quit."))
            .map_err(|e| (4, e.message))? == recovery::Action::Restart;
    }

    // ---- keypad self-test -------------------------------------------------
    // Inject a synthetic key event and let the guest run on, then report whether
    // the guest's *own* reader consumed it.  This is the only way to tell a
    // working keypad path from one that merely stores a byte somewhere.
    if crate::options::env_flag("TECH2_KEYTEST") && outcome == Outcome::Success {
        let code = std::env::var("TECH2_KEYTEST_CODE")
            .ok()
            .and_then(|v| u8::from_str_radix(v.trim().trim_start_matches("0x"), 16).ok())
            .unwrap_or(bus::KEY_DOWN);
        let idx_before = bus.read_word(bus::KEY_MENU_IDX);
        let latch_before = bus.read_byte(bus::KEY_LATCH);
        let reads_before = bus.key_reads;
        let consumed_before = bus.key_consumed;
        println!(
            "\nKEYTEST: injecting code {code:#04x}  (menu_idx={idx_before:#06x} latch={latch_before:#04x} sr_mask={} vec25={:#010x})",
            (cpu.int_mask >> 8) & 7,
            bus.read_long(25 * 4)
        );
        bus.press_key(code);
        let budget: u64 = std::env::var("TECH2_KEYTEST_INSNS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(2_000_000);
        for _ in 0..budget {
            let result = step_guest(&mut cpu, &mut bus, insns);
            if !matches!(result, StepResult::Ok { .. }) {
                outcome = Outcome::GuestFailure;
                stop = format!("keytest CPU failure: {result:?}");
                break;
            }
            insns += 1;
        }
        println!(
            "KEYTEST: reads={} (+{}) consumed={} (+{}) pulses={} queued={} irq_pending={}",
            bus.key_reads,
            bus.key_reads - reads_before,
            bus.key_consumed,
            bus.key_consumed - consumed_before,
            bus.key_pulses,
            bus.key_queue.len(),
            bus.key_irq_pending()
        );
        println!(
            "KEYTEST: menu_idx {:#06x} -> {:#06x}   latch {:#04x} -> {:#04x}",
            idx_before,
            bus.read_word(bus::KEY_MENU_IDX),
            latch_before,
            bus.read_byte(bus::KEY_LATCH)
        );
        let tbl = |bus: &mut Tech2Bus, base: u32| -> String {
            (0..0x20)
                .map(|i| format!("{:02x}", bus.read_byte(base + i)))
                .collect::<Vec<_>>()
                .join(" ")
        };
        let up = tbl(&mut bus, bus::KEYTBL_UP);
        let down = tbl(&mut bus, bus::KEYTBL_DOWN);
        println!("KEYTEST: table $100CD0 (key up)   {up}");
        println!("KEYTEST: table $100CEA (key down) {down}");
    }

    println!("\nREPORT insns={insns} stop={stop}");
    if let Some(before) = &initial_security_ssa {
        if let Err(e) = artifacts::save_security_snapshot(&bus, before, &opts.output) {
            outcome = Outcome::OutputFailure;
            stop = format!("{stop}; security memory capture: {e}");
        }
    }
    let capture_path = opts.output.join("lcd.ppm");
    if let Err(e) = artifacts::save_screen(&bus, &capture_path) {
        outcome = Outcome::OutputFailure;
        stop = format!("{stop}; capture {}: {e}", capture_path.display());
    } else {
        println!("screen dump: {} (320x240)", capture_path.display());
    }
    #[cfg(feature = "gui")]
    if bus.candi_link.is_some() {
        bus.trace_event("candi_link_status", &format!("Guest stopped: {stop}"));
        if let Err(error) = artifacts::save_console_screen(&bus, &opts.output.join("console.ppm")) {
            outcome = Outcome::OutputFailure;
            stop = format!("{stop}; console capture: {error}");
        }
    }
    let picr = u16::from_be_bytes([bus.sim[bus::OFF_PICR], bus.sim[bus::OFF_PICR + 1]]);
    let pitr = u16::from_be_bytes([bus.sim[0x0A24], bus.sim[0x0A25]]);
    println!(
        "EPROM CFI: erases={} writes={} sectors={:?} status={} | LCD: cmd_wr={} data_wr={} | PICR={:#06x} PITR={:#06x}",
        bus.eprom_erases,
        bus.eprom_writes,
        bus.eprom_program_sector_bytes,
        bus.eprom_cfi_status,
        bus.lcd.cmd_wr, bus.lcd.data_wr, picr, pitr
    );
    println!("CARD READ PAGES: {:?}", bus.card_rd_pages);
    if let Ok(v) = std::env::var("DUMP_RAM") {
        let mut it = v.split(':');
        let a =
            u32::from_str_radix(it.next().unwrap_or("0").trim_start_matches("0x"), 16).unwrap_or(0);
        let n = it
            .next()
            .and_then(|x| x.parse::<u32>().ok())
            .unwrap_or(48)
            .min(1_048_576);
        let hex: String = (0..n)
            .map(|i| format!("{:02x}", bus.read_byte(a.wrapping_add(i))))
            .collect();
        println!("DUMP_RAM {a:#08x}: {hex}");
    }
    if let Ok(v) = std::env::var("DUMP_FLASH") {
        let mut it = v.split(':');
        let a =
            u32::from_str_radix(it.next().unwrap_or("0").trim_start_matches("0x"), 16).unwrap_or(0);
        let n = it
            .next()
            .and_then(|x| x.parse::<u32>().ok())
            .unwrap_or(48)
            .min(1_048_576);
        let hex: String = (0..n)
            .map(|i| format!("{:02x}", bus.read_byte(a.wrapping_add(i))))
            .collect();
        println!("DUMP_FLASH {a:#08x}: {hex}");
    }

    // Print PC hotspots to diagnose spin loops
    if !pc_counts.is_empty() {
        let mut top: Vec<_> = pc_counts.iter().collect();
        top.sort_by_key(|(_, &count)| std::cmp::Reverse(count));
        println!("PC HOTSPOTS (top 15 most-visited, sampled every 1000 insns):");
        for (pc, count) in top.iter().take(15) {
            println!("  PC={:#010x}  hits={}", pc, count);
        }
        // A hotspot alone does not say what the guest is waiting for.  Dump the
        // words around the hottest PC so the spin loop can actually be read --
        // this is guest RAM loaded from the card, so it exists nowhere on disk.
        let hot = *top[0].0;
        let lo = hot.saturating_sub(0x20);
        println!("SPIN WINDOW around {hot:#010x}:");
        for a in (lo..lo + 0x50).step_by(2) {
            let w = bus.read_word(a);
            let mark = if a == hot { "  <== hottest" } else { "" };
            println!("  {a:#010x}: {w:04x}{mark}");
        }

        println!("WINDOW 0x001C1E00:");
        for a in (0x001C_1E00..0x001C_1E60).step_by(2) {
            println!("  {a:#010x}: {:04x}", bus.read_word(a));
        }
        println!("WINDOW 0x001C2300:");
        for a in (0x001C_2300..0x001C_2360).step_by(2) {
            println!("  {a:#010x}: {:04x}", bus.read_word(a));
        }
    }

    println!(
        "KEYPAD: events={} consumed={} reads={} irq_pending={}",
        bus.key_events,
        bus.key_consumed,
        bus.key_reads,
        bus.key_irq_pending()
    );

    {
        let r = bus.post_results();
        let passed = r.iter().filter(|(_, ok)| *ok).count();
        println!(
            "POST: {}/{} pass{}",
            passed,
            r.len(),
            if r.is_empty() {
                " (no POST text on screen)"
            } else {
                ""
            }
        );
        let fails: Vec<&str> = r
            .iter()
            .filter(|(_, ok)| !*ok)
            .map(|(n, _)| n.as_str())
            .collect();
        if !fails.is_empty() {
            println!("POST FAIL: {}", fails.join(" "));
        }
    }
    // Verdict. Set-bit count alone false-positives: a crashed run that sprays VRAM
    // scores as high as a real screen. Require readable text as well.
    let total_gfx: u32 = bus.lcd.vram.iter().map(|b| b.count_ones()).sum();
    let text = bus.screen_text();
    let runs: Vec<String> = text
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| l.len() >= 4)
        .collect();
    println!("VRAM set_bits={total_gfx} text_runs={}", runs.len());
    for w in runs.iter().take(14) {
        println!("  SCREEN: {w}");
    }
    if outcome == Outcome::GuestFailure {
        let q = artifacts::json_string;
        let stack = bus
            .peek_bytes(cpu.sp(), 128)
            .map(|v| format!("{v:?}"))
            .unwrap_or_else(|| "null".into());
        let snapshot = format!("{{\n  \"reason\": {},\n  \"mode\": {},\n  \"instructions\": {},\n  \"pc\": {},\n  \"sp\": {},\n  \"sr\": {},\n  \"d_registers\": {:?},\n  \"a_registers\": {:?},\n  \"recent_pcs\": {:?},\n  \"stack_bytes\": {},\n  \"screen\": {}\n}}\n",
            q(&stop), q(mode), insns, cpu.pc, cpu.sp(), cpu.get_sr(), &cpu.dar[..8], &cpu.dar[8..], bus.recent_pc_history(), stack, q(&text));
        if let Err(e) = std::fs::write(&crash_path, snapshot) {
            outcome = Outcome::OutputFailure;
            stop = format!("{stop}; crash snapshot: {e}");
        }
    }
    let candi_snapshot = candi_handle
        .and_then(|handle| handle.stop())
        .or_else(|| bus.candi_link.as_ref().map(|link| link.snapshot()));
    if let Err(e) = logger::check_file() {
        outcome = Outcome::OutputFailure;
        stop = format!("{stop}; log persistence: {e}");
    }
    bus.current_insns = insns;
    bus.current_pc = cpu.pc;
    bus.trace_screen();
    bus.trace_event(
        "run_end",
        &format!("status={} reason={stop}", outcome.name()),
    );
    if let Err(e) = bus.trace.check() {
        outcome = Outcome::OutputFailure;
        stop = format!("{stop}; trace persistence: {e}");
    }
    println!("VERDICT: {} — {}", outcome.name(), stop);
    let q = artifacts::json_string;
    let candi_report = candi_snapshot.map(|s| format!(
        "{{\"profile\":\"msi-addressed-download\",\"native_uart\":{},\"attempted\":{},\"completed\":{},\"cycles\":{},\"pc\":{},\"reason\":{},\"serial_rx_bytes\":{},\"serial_rx_breaks\":{},\"serial_tx_bytes\":{},\"adapter_tx_confirmations\":{},\"external_tx\":{}}}",
        bus.candi_link.is_some(), s.attempted, s.completed, s.cycles, s.pc,
        q(&s.reason.as_ref().map(ToString::to_string).unwrap_or_default()),
        s.serial_rx_count, s.serial_break_count, s.serial_tx_count,
        bus.candi_link.as_ref().map_or(0, |link| link.live_confirmations()),
        bus.candi_link.as_ref().is_some_and(|link| link.live_confirmations() > 0)
    )).unwrap_or_else(|| "null".into());
    let report = format!("{{\n  \"candi\": {candi_report},\n  \"status\": {},\n  \"exit_code\": {},\n  \"mode\": {},\n  \"instructions\": {},\n  \"pc\": {},\n  \"reason\": {},\n  \"last_verified\": {},\n  \"boot_image\": {},\n  \"card_image\": {},\n  \"opsys_image\": {},\n  \"host_cancelled_operations\": {}\n}}\n",
        q(outcome.name()), outcome.code(), q(mode), insns, cpu.pc, q(&stop), q(&last_verified),
        q(&boot_path.to_string_lossy()), q(&card_path.to_string_lossy()), q(&opsys_path.to_string_lossy()), bus.host_cancelled_operations);
    std::fs::write(&report_path, report)
        .map_err(|e| (4, format!("report {}: {e}", report_path.display())))?;
    Ok((outcome.code(), restart))
}

#[cfg(test)]
mod runtime_tests {
    use super::*;

    fn machine() -> (CpuCore, Tech2Bus) {
        let mut boot = vec![0; 0x40000];
        boot[..4].copy_from_slice(&0x0010_1000u32.to_be_bytes());
        boot[4..8].copy_from_slice(&0x0010_0800u32.to_be_bytes());
        boot[0x108..0x10c].copy_from_slice(&0x0010_0900u32.to_be_bytes());
        let mut bus = Tech2Bus::new(boot, vec![0; 0x100000], ExecutionMode::Fidelity);
        bus.write_word(0x0010_0800, 0x60fe); // BRA to self
        bus.write_word(0x0010_0900, 0x4e73); // RTE
        bus.sim[bus::OFF_PICR] = 6;
        bus.sim[bus::OFF_PICR + 1] = 0x42;
        bus.sim[0x0a25] = 1;
        let mut cpu = CpuCore::new();
        cpu.set_cpu_type(CpuType::M68EC020);
        cpu.sr_mask = 0xa71f;
        cpu.reset(&mut bus);
        cpu.set_sr(0x2000);
        (cpu, bus)
    }

    #[cfg(feature = "gui")]
    #[test]
    fn menu_checkpoint_restores_execution_and_rebases_timer_without_replaying_input() {
        let (mut cpu, mut bus) = machine();
        for insns in 0..100 {
            assert!(matches!(
                step_guest(&mut cpu, &mut bus, insns),
                StepResult::Ok { .. }
            ));
        }
        bus.current_insns = 100;
        let saved_pc = cpu.pc;
        let saved_sr = cpu.get_sr();
        let saved_regs = cpu.dar;
        let saved = recovery::Checkpoint::capture(&cpu, &bus).unwrap();
        bus.press_key(0x10);
        bus.ram[0x123] = 0xaa;
        bus.card[5] = 0xbb;
        bus.lcd.vram[12] = 0xcc;
        cpu.dar[0] = 0xdeadbeef;
        bus.current_insns = 1_000_000;
        saved.restore(&mut cpu, &mut bus);
        assert_eq!(
            (cpu.pc, cpu.get_sr(), cpu.dar),
            (saved_pc, saved_sr, saved_regs)
        );
        assert_eq!((bus.ram[0x123], bus.card[5], bus.lcd.vram[12]), (0, 0, 0));
        assert_eq!(bus.current_insns, 1_000_000);
        assert!(bus.key_queue.is_empty());
        assert!(bus.key_active.is_none());
        assert_eq!(bus.key_events, 1);
        assert_eq!(bus.host_cancelled_operations, 1);
        assert_eq!(bus.poll_interrupt(1_009_899), 0);
        assert_eq!(bus.poll_interrupt(1_009_900), 6);
        assert!(matches!(
            step_guest(&mut cpu, &mut bus, 1_009_900),
            StepResult::Ok { .. }
        ));
        assert!(bus.failure_reason.is_none());
    }

    #[test]
    #[ignore = "requires local proprietary firmware; run with --release"]
    fn firmware_year_highlight_follows_down_and_up() {
        let (_, boot) = load_boot(None).unwrap();
        let (_, card) = load_nao(None).unwrap();
        let (_, opsys) = load_opsys(None).unwrap();
        let mut bus = Tech2Bus::new(boot, card, ExecutionMode::ResearchHarness);
        bus.load_opsys_ram(&opsys);
        bus.flash[0x8018..0x40000].copy_from_slice(&opsys);
        restore_rom_modules(&mut bus.flash, &opsys);
        let mut cpu = CpuCore::new();
        cpu.set_cpu_type(CpuType::M68EC020);
        cpu.sr_mask = 0xa71f;
        cpu.reset(&mut bus);
        let mut harness = harness::Harness::new(options::HarnessTarget::Menus, 0);
        let mut year_at = None;
        let mut moved_down = false;
        for insns in 0..80_000_000 {
            assert!(matches!(
                step_guest(&mut cpu, &mut bus, insns),
                StepResult::Ok { .. }
            ));
            if insns % 50_000 != 0 {
                continue;
            }
            if let Some(at) = year_at {
                if insns < at + 1_000_000 {
                    continue;
                }
                let text = bus.screen_text();
                if !moved_down {
                    assert!(text.contains("2 / 15"), "{text}");
                    assert_eq!(bus.highlighted_text().as_deref(), Some("(B) 2011"));
                    assert!(!bus.lcd.pixel(280, 67));
                    assert!(bus.lcd.pixel(280, 82));
                    bus.press_key(bus::KEY_UP);
                    moved_down = true;
                    year_at = Some(insns);
                } else {
                    assert!(text.contains("1 / 15"), "{text}");
                    assert_eq!(bus.highlighted_text().as_deref(), Some("(C) 2012"));
                    assert!(bus.lcd.pixel(280, 67));
                    assert!(!bus.lcd.pixel(280, 82));
                    return;
                }
            } else if let Some(event) = harness.observe(&bus.screen_text(), insns).unwrap() {
                if event.capture == "stage3_model_year" {
                    assert!(bus.lcd.pixel(280, 67));
                    assert!(!bus.lcd.pixel(280, 82));
                    bus.press_key(bus::KEY_DOWN);
                    year_at = Some(insns);
                } else if let Some(key) = event.key {
                    bus.press_key(key);
                }
            }
        }
        panic!("did not verify year selection highlight");
    }

    #[cfg(feature = "gui")]
    #[test]
    #[ignore = "requires local proprietary firmware; run with --release"]
    fn firmware_menu_checkpoint_returns_from_working_and_accepts_exit() {
        let (_, boot) = load_boot(None).unwrap();
        let (_, card) = load_nao(None).unwrap();
        let (_, opsys) = load_opsys(None).unwrap();
        let mut bus = Tech2Bus::new(boot, card, ExecutionMode::ResearchHarness);
        bus.load_opsys_ram(&opsys);
        bus.flash[0x8018..0x40000].copy_from_slice(&opsys);
        restore_rom_modules(&mut bus.flash, &opsys);
        let mut cpu = CpuCore::new();
        cpu.set_cpu_type(CpuType::M68EC020);
        cpu.sr_mask = 0xa71f;
        cpu.reset(&mut bus);
        let mut harness = harness::Harness::new(options::HarnessTarget::Recovery, 0);
        let mut saved: Option<recovery::Checkpoint> = None;
        let mut previous_screen = String::new();
        let mut restored_at = None;
        for insns in 0..80_000_000 {
            assert!(
                matches!(step_guest(&mut cpu, &mut bus, insns), StepResult::Ok { .. }),
                "{}",
                bus.screen_text()
            );
            if insns % 50_000 != 0 {
                continue;
            }
            if let Some(at) = restored_at {
                if insns > at + 200_000 && bus.screen_text().contains("Diagnostics") {
                    assert!(!bus.screen_text().contains("Working"));
                    return;
                }
                assert!(insns < at + 2_000_000, "restored menu did not accept Exit");
            } else if recovery::unavailable_reason(&bus.screen_text()).is_some() {
                saved
                    .take()
                    .expect("checkpoint before Engine selection")
                    .restore(&mut cpu, &mut bus);
                assert_eq!(bus.screen_text(), previous_screen);
                // Let the restored idle loop run, then send a real guest Exit.
                bus.exit_key();
                restored_at = Some(insns);
            } else if let Some(event) = harness.observe(&bus.screen_text(), insns).unwrap() {
                if event.capture == "stage6_engine" {
                    assert!(recovery::can_checkpoint(&bus));
                    previous_screen = bus.screen_text();
                    saved = Some(recovery::Checkpoint::capture(&cpu, &bus).unwrap());
                }
                if let Some(key) = event.key {
                    bus.press_key(key);
                }
            }
        }
        panic!("did not verify menu recovery");
    }

    #[test]
    fn research_assertion_preserves_halt_stack_cleanup_and_failure_context() {
        let (mut cpu, mut bus) = machine();
        bus.execution_mode = ExecutionMode::ResearchHarness;
        let epilogue = [0x60, 0xfe, 0x4f, 0xef, 0x00, 0x20];
        bus.flash[0x10cbe..0x10cc4].copy_from_slice(&epilogue);
        assert_eq!(bus.read_word(0x10cbe), 0x60fe);
        assert_eq!(bus.read_word(0x10cc0), 0x4fef);
        cpu.pc = 0x10cbe;
        let sp = cpu.sp();
        assert!(matches!(
            step_guest(&mut cpu, &mut bus, 50),
            StepResult::Stopped
        ));
        assert_eq!(cpu.pc, 0x10cbe);
        assert_eq!(cpu.sp(), sp);
        assert!(bus
            .failure_reason
            .as_deref()
            .unwrap()
            .contains("guest assertion"));
        assert_eq!(bus.recent_pc_history(), vec![0x10cbe]);
        assert_eq!(bus.peek_bytes(0x10cbe, 6), Some(epilogue.as_slice()));
        bus.flash[0x10cbe] = 0x4e;
        assert!(!bus.at_assertion_halt(0x10cbe));
    }

    #[test]
    fn stop_waits_for_due_tick_and_returns_to_original_stack() {
        let (mut cpu, mut bus) = machine();
        cpu.stop(0x2000);
        let pc = cpu.pc;
        let sp = cpu.sp();
        step_guest(&mut cpu, &mut bus, 0);
        step_guest(&mut cpu, &mut bus, 9999);
        assert_eq!(cpu.pc, pc);
        assert_eq!(bus.pit_ticks, 0);
        assert_eq!(cpu.stopped, 1);
        step_guest(&mut cpu, &mut bus, 10000);
        assert_eq!(cpu.pc, 0x0010_0900);
        assert_eq!(cpu.sp(), sp - 8);
        assert_eq!(bus.pit_ticks, 1);
        assert!(!bus.pit_pending);
        assert_eq!(cpu.int_level, 0);
        assert!(matches!(
            step_guest(&mut cpu, &mut bus, 10001),
            StepResult::Ok { .. }
        ));
        assert_eq!(cpu.pc, pc);
        assert_eq!(cpu.sp(), sp);
        step_guest(&mut cpu, &mut bus, 10002);
        assert_eq!(
            bus.pit_ticks, 1,
            "a serviced tick must not be delivered twice"
        );
    }

    #[test]
    fn masked_tick_is_retained_and_halt_is_not_woken() {
        let (mut cpu, mut bus) = machine();
        cpu.stop(0x2700);
        step_guest(&mut cpu, &mut bus, 0);
        step_guest(&mut cpu, &mut bus, 10000);
        assert!(bus.pit_pending);
        assert_eq!(bus.pit_ticks, 0);
        cpu.set_sr(0x2000);
        step_guest(&mut cpu, &mut bus, 10001);
        assert_eq!(bus.pit_ticks, 1);
        cpu.halt();
        assert!(matches!(
            step_guest(&mut cpu, &mut bus, 20000),
            StepResult::Stopped
        ));
        assert_ne!(cpu.stopped & 2, 0);
    }

    #[test]
    fn communication_status_branch_is_not_replaced_with_fake_cancellation() {
        let (mut cpu, mut bus) = machine();
        bus.execution_mode = ExecutionMode::ResearchHarness;
        bus.sim[0x0a25] = 0;
        // Actual guest instructions at the old, incorrectly labelled abort hook.
        bus.write_word(0x001f_799a, 0x0c47); // CMPI.W #1,D7
        bus.write_word(0x001f_799c, 1);
        bus.write_word(0x001f_799e, 0x6620); // BNE to $1F79C0
        cpu.pc = 0x001f_799a;
        cpu.dar[7] = 0;
        bus.write_word(cpu.sp() + 26, 0x1234);
        step_guest(&mut cpu, &mut bus, 0);
        assert_eq!(cpu.pc, 0x001f_799e);
        assert_eq!(cpu.dar[7], 0);
        assert_eq!(bus.read_word(cpu.sp() + 26), 0x1234);
        step_guest(&mut cpu, &mut bus, 1);
        assert_eq!(cpu.pc, 0x001f_79c0);
    }

    #[test]
    fn headless_budget_fault_and_success_have_different_outcomes() {
        for (opcode, budget, splash, expected) in [
            (0x60fe, 1, false, Outcome::Incomplete),
            (0xffff, 100, false, Outcome::GuestFailure),
            (0x60fe, 50_000, true, Outcome::Success),
        ] {
            let (mut cpu, mut bus) = machine();
            bus.sim[0x0a25] = 0;
            bus.write_word(cpu.pc, opcode);
            if splash {
                for (row, text) in [
                    (0, "Press [ENTER]"),
                    (1, "Software Version"),
                    (2, "North American Operations"),
                ] {
                    bus.lcd.vram[row * 40..row * 40 + text.len()].copy_from_slice(text.as_bytes());
                }
            }
            let result = run_headless(
                &mut cpu,
                &mut bus,
                &[],
                0,
                RunSettings {
                    limit_insns: budget,
                    sample_hotspots: false,
                    interactive: false,
                    android_live: false,
                    harness_target: None,
                    output_dir: Path::new("."),
                    replay: &[],
                },
                &mut std::collections::HashMap::new(),
            );
            assert_eq!(result.outcome, expected);
        }
    }
}
