// SPDX-License-Identifier: MPL-2.0
//! Shared guest stepping used by CLI and embedded hosts.
use crate::{bus::Tech2Bus, candi, communication};
use m68k::{AddressBus, CpuCore, StepResult};

/// Opt-in, read-only POST call tracing.  This logs the ROM's screen-print
/// helper calls and their caller addresses without changing guest execution.
static POST_TRACE: std::sync::LazyLock<bool> =
    std::sync::LazyLock::new(|| crate::options::env_flag("POST_TRACE"));

/// Opt-in, read-only trace for the card-to-flash bootstrap handoff.  The
/// addresses are code boundaries observed in the supplied firmware, rather
/// than host-injected entry points.  Keeping this behind an environment flag
/// makes it useful for diagnosis without changing the fidelity execution path.
pub(crate) static BOOT_TRACE: std::sync::LazyLock<bool> =
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
    if bus.failure_reason.is_some() { return StepResult::Stopped; }
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
    let mut kernel=std::mem::take(&mut bus.psos);
    kernel.before_instruction(cpu,bus);
    bus.psos=kernel;
    if bus.failure_reason.is_some(){return StepResult::Stopped;}
    candi::demand::guest_dependency(cpu, bus);
    if bus.failure_reason.is_some() { return StepResult::Stopped; }
    let pc = cpu.pc;
    bus.current_pc = pc;
    communication::observe(cpu, bus);
    if let Some(mut display)=bus.firmware_display.take() {
        display.before_instruction(cpu.pc,&mut cpu.dar,bus);
        bus.firmware_display=Some(display);
    }

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
            let mut files=std::mem::take(&mut bus.psos_files);
            // Seed only a valid create/open, at its current FAT snapshot. An
            // invalid filename/stack must not freeze an earlier capture view.
            let can_seed=trap_num==11 && files.enabled && !files.initialized && matches!(cpu.dar[0],0x208|0x20d) && cpu.sp().checked_add(4).is_some_and(|args| {
                use tech2_emu::recovered_memory::GuestMemory;
                if bus.bytes(args,12).is_none() {return false;}
                let name_pointer=if cpu.dar[0]==0x208 {bus.u32(args)} else {bus.u32(args+4)};
                name_pointer.is_some_and(|p|tech2_emu::psos_files::PsosFiles::guest_filename(bus,p).is_some())
                    && (cpu.dar[0]!=0x20d || bus.u32(args).is_some_and(|p|bus.ram_mut(p,4).is_some()))
            });
            let seed_failure=if trap_num==11 && files.enabled && !files.initialized && matches!(cpu.dar[0],0x208|0x20d) && can_seed {
                match bus.capture_storage.as_ref().ok_or_else(||"Capture snapshot is unavailable".to_string()).and_then(tech2_emu::psos_files::PsosFiles::seed) {
                    Ok(seeded)=>{let calls=files.calls;files=seeded;files.calls=calls;None},
                    Err(error)=>{files.calls+=1;files.last_error=error;Some(if cpu.dar[0]==0x208 {0x2011}else{0x2002})},
                }
            } else {None};
            let handled=seed_failure.or_else(||if trap_num==11 {files.dispatch(cpu.dar[0],cpu.sp(),bus)} else {None});
            bus.psos_files=files;
            let mut kernel=std::mem::take(&mut bus.psos);
            let kernel_handled=kernel.trap(trap_num,handled,cpu,bus);
            bus.psos=kernel;
            if !kernel_handled {if let Some(code)=handled {cpu.set_d(0,code);} else {cpu.take_trap_exception(bus, trap_num);}}

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
