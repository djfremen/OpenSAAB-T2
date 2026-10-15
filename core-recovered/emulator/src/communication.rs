// SPDX-License-Identifier: MPL-2.0
//! Read-only observations of the supported Saab guest's internal request contract.
//! These are queue messages, not decoded vehicle packets or external transfers.
use crate::bus::Tech2Bus;
use m68k::CpuCore;

fn long(bus: &Tech2Bus, address: u32) -> Option<u32> {
    Some(u32::from_be_bytes(
        bus.peek_bytes(address, 4)?.try_into().ok()?,
    ))
}

fn value(v: Option<u32>) -> String {
    v.map(|v| format!("0x{v:08x}"))
        .unwrap_or_else(|| "unavailable".into())
}

pub fn observe(cpu: &CpuCore, bus: &mut Tech2Bus) {
    if !bus.trace.enabled() || cpu.stopped != 0 {
        return;
    }
    if bus.candi_link.is_some()
        && matches!(
            cpu.pc,
            0x1d6ae0 | 0x1d8156 | 0x1d7036 | 0x1d73e6 | 0x1d68c6 | 0x1d68da | 0x1d694a
        )
    {
        bus.trace_event("candi_guest_link_call", &format!("pc={:#x} flags={:02x?} param={:02x?} enable={:02x?} pending={:02x?} stack={:02x?} external_tx=false", cpu.pc, bus.peek_bytes(0x1fbc10, 4), &bus.sim[0xff0..0xffa], &bus.sim[0xe08..0xe0c], &bus.sim[0xe20..0xe22], bus.peek_bytes(cpu.sp(), 12)));
    }
    if bus.candi_link.is_some()
        && cpu.pc == 0x1d70b8
        && bus.peek_bytes(0x1d7036, 4) == Some(&[0x48, 0xe7, 0x01, 0x04])
    {
        bus.trace_event(
            "candi_device_info_return",
            &format!(
                "d0={:#x} d7={:#x} descriptor={:02x?} payload={:02x?} scope=candi-not-ecu",
                cpu.dar[0],
                cpu.dar[7],
                bus.peek_bytes(cpu.sp() + 30, 12),
                bus.peek_bytes(cpu.sp(), 30)
            ),
        );
    }
    observe_os(cpu, bus);
    observe_consumer(cpu, bus);
    observe_timer_wait(cpu, bus);
    if cpu.pc == 0x1d9ade
        && bus.peek_bytes(cpu.pc, 8) == Some(&[0x48, 0xe7, 0x07, 0x00, 0x3a, 0x2f, 0x00, 0x10])
    {
        let timer = bus.peek_bytes(cpu.sp() + 4, 2)
            .map(|v| u16::from_be_bytes([v[0], v[1]]));
        bus.trace_event("tpu_timer_arm", &format!(
            "timer={timer:?} countdown_units={} caller={} tpumcr={:02x?} external_tx=false",
            value(long(bus, cpu.sp() + 6)), value(long(bus, cpu.sp())), &bus.sim[0xe00..0xe02]
        ));
    }
    let pc = cpu.pc;
    let kind = match pc {
        0x1dbb96 => "guest_request_contract",
        0x1dbbfc => "guest_queue_submit",
        0x1dbc0e => "guest_queue_submit_return",
        0x1dbca2 => "guest_queue_receive_return",
        0x1dbd1e => "guest_wait_timeout",
        0x1dbd3e => "guest_request_complete",
        _ => return,
    };
    // Check the wrapper's frame layout and its return convention before using
    // firmware-specific offsets. Other firmware remains raw/unclassified.
    if bus.peek_bytes(0x1dbb96, 12)
        != Some(&[
            0x48, 0xe7, 0x0f, 0x0c, 0x4f, 0xef, 0xff, 0xe8, 0x2a, 0x6f, 0x00, 0x34,
        ])
        || bus.peek_bytes(0x1dbd3e, 14)
            != Some(&[
                0x30, 0x06, 0x4f, 0xef, 0x00, 0x18, 0x4c, 0xdf, 0x30, 0xf0, 0x4e, 0x74, 0x00, 0x08,
            ])
    {
        return;
    }
    let entry_sp = if pc == 0x1dbb96 {
        cpu.sp()
    } else {
        cpu.sp().wrapping_add(48)
    };
    let descriptor = long(bus, entry_sp.wrapping_add(4));
    let request_key = descriptor.and_then(|p| long(bus, p.wrapping_add(2)));
    let argument = long(bus, entry_sp.wrapping_add(8));
    let caller = long(bus, entry_sp);
    let mut detail = format!(
        "entry_sp={entry_sp:#010x} caller={} descriptor={} request_key={} argument={} external_tx=false scope=guest-internal",
        value(caller), value(descriptor), value(request_key), value(argument)
    );
    match pc {
        0x1dbb96 => {
            let tag = descriptor.and_then(|p| bus.peek_bytes(p, 1)).map(|b| b[0]);
            detail.push_str(&format!(" descriptor_tag={tag:?} expected_tag=2"));
        }
        0x1dbbfc | 0x1dbc0e | 0x1dbca2 => {
            let receive = pc == 0x1dbca2;
            let valid = !receive || cpu.dar[0] == 0;
            let handle = long(bus, if receive { 0x100d3c } else { 0x100d38 });
            detail.push_str(&format!(" queue={} envelope_valid={valid}", value(handle)));
            if pc != 0x1dbbfc {
                detail.push_str(&format!(" helper_d0={:#010x}", cpu.dar[0]));
            }
            // A failed receive leaves stale stack bytes: never label those as
            // a response. Offsets are relative to the wrapper's local frame.
            if valid {
                let words = (0..4)
                    .map(|i| value(long(bus, cpu.sp().wrapping_add(8 + i * 4))))
                    .collect::<Vec<_>>()
                    .join(",");
                detail.push_str(&format!(" envelope_words=[{words}]"));
            }
        }
        0x1dbd1e => {
            detail.push_str(" next_status_low16=0x0001 cause=receive-status-1-or-tick-deadline")
        }
        0x1dbd3e => {
            let result = descriptor.and_then(|p| long(bus, p.wrapping_add(0x18)));
            detail.push_str(&format!(" status_low16={:#06x} descriptor_result={} interpretation=internal-completion-not-ecu-success", cpu.dar[6] as u16, value(result)));
        }
        _ => unreachable!(),
    }
    bus.trace_event(kind, &detail);
}

fn observe_timer_wait(cpu: &CpuCore, bus: &mut Tech2Bus) {
    if cpu.pc != 0x1d9c2e
        || bus.peek_bytes(cpu.pc, 6) != Some(&[0x61, 0xff, 0xff, 0xe3, 0x85, 0xe6])
    {
        return;
    }
    let task = long(bus, cpu.sp());
    let timers = bus.peek_bytes(0x1fc8f0, 140).map(|v| format!("{v:02x?}"));
    bus.trace_event("guest_timer_suspend", &format!(
        "task={} hardware_flags={:#06x} tpu_registers={:02x?} timer_records={} external_tx=false scope=guest-hardware",
        value(task), bus.ram_word(0x1fbc10), &bus.sim[0xe00..0xe22], timers.unwrap_or_default()
    ));
}

fn observe_consumer(cpu: &CpuCore, bus: &mut Tech2Bus) {
    let (name, signature): (&str, &[u8]) = match cpu.pc {
        0x1ee5a6 => (
            "communication_init",
            &[
                0x48, 0xe7, 0x3f, 0x3e, 0x4f, 0xef, 0xff, 0xee, 0x42, 0x43, 0x61, 0x00,
            ],
        ),
        0x1eed42 => (
            "communication_init_result",
            &[
                0x30, 0x03, 0x4f, 0xef, 0x00, 0x12, 0x4c, 0xdf, 0x7c, 0xfc, 0x4e, 0x74,
            ],
        ),
        0x1f168c => (
            "scrx_consumer",
            &[
                0x48, 0xe7, 0x1f, 0x1c, 0x4f, 0xef, 0xff, 0xe4, 0x99, 0xcc, 0x42, 0x46,
            ],
        ),
        0x1f17a8 => (
            "scrx_case_7",
            &[
                0x3f, 0x3c, 0x00, 0x01, 0x20, 0x4f, 0x2f, 0x28, 0x00, 0x16, 0x61, 0x00,
            ],
        ),
        0x1efd74 => (
            "control_7_handler",
            &[
                0x48, 0xe7, 0x3f, 0x3e, 0x59, 0x8f, 0x28, 0x2f, 0x00, 0x34, 0x36, 0x2f,
            ],
        ),
        0x1da464 => (
            "vci_link_handler",
            &[
                0x48, 0xe7, 0x3f, 0x0c, 0x2a, 0x6f, 0x00, 0x24, 0x7a, 0x02, 0x3c, 0x05,
            ],
        ),
        _ => return,
    };
    if bus.peek_bytes(cpu.pc, signature.len()) != Some(signature) {
        return;
    }
    bus.trace_stack("guest_communication_path", cpu.pc, cpu.sp());
    bus.trace_event(
        "guest_communication_boundary",
        &format!(
            "function={name} d0={:#010x} d3={:#010x} scrx={} sctx={} consumer_scrx={} consumer_sctx={} vci_queue={} external_tx=false scope=guest-internal",
            cpu.dar[0], cpu.dar[3],
            value(long(bus, 0x100d38)), value(long(bus, 0x100d3c)),
            value(long(bus, 0x1fd8f0)), value(long(bus, 0x1fd8ec)),
            value(long(bus, 0x1fd8f4)),
        ),
    );
}

/// Observe the actual pSOS wrappers, before TRAP #11 and after its return.
/// Register values are raw ABI evidence; a queue operation is not adapter TX.
fn observe_os(cpu: &CpuCore, bus: &mut Tech2Bus) {
    let (trap, service, name) = match cpu.pc {
        0x1218a | 0x1218c => (0x1218a, 0x01, "task_create"),
        0x121a8 | 0x121aa => (0x121a8, 0x02, "task_identify"),
        0x121d8 | 0x121da => (0x121d8, 0x03, "task_start"),
        0x12220 | 0x12222 => (0x12220, 0x06, "task_suspend"),
        0x12230 | 0x12232 => (0x12230, 0x07, "task_resume"),
        0x124e6 | 0x124e8 => (0x124e6, 0x24, "queue_create"),
        0x12506 | 0x12508 => (0x12506, 0x25, "queue_identify"),
        0x1253a | 0x1253c => (0x1253a, 0x27, "queue_send"),
        0x1255a | 0x1255c => (0x1255a, 0x28, "queue_urgent"),
        0x1257a | 0x1257c => (0x1257a, 0x29, "queue_broadcast"),
        0x125a0 | 0x125a2 => (0x125a0, 0x2a, "queue_receive"),
        _ => return,
    };
    // MOVEQ #service,D0; TRAP #11. Unsupported firmware stays unclassified.
    if bus.peek_bytes(trap - 2, 4) != Some(&[0x70, service, 0x4e, 0x4b]) {
        return;
    }
    let phase = if cpu.pc == trap { "enter" } else { "return" };
    let registers = cpu.dar[..15]
        .iter()
        .map(|v| format!("0x{v:08x}"))
        .collect::<Vec<_>>()
        .join(",");
    let caller = cpu.dar[14].checked_add(4).and_then(|p| long(bus, p));
    bus.trace_event(
        "guest_os_call",
        &format!(
            "service={name} number=0x{service:02x} phase={phase} sp={:#010x} caller={} registers=[{registers}] scope=guest-os external_tx=false",
            cpu.sp(), value(caller)
        ),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bus::ExecutionMode;
    #[test]
    fn contract_reads_never_poll_keypad_or_wrap_invalid_memory() {
        let bus = Tech2Bus::new(vec![0; 0x40000], vec![], ExecutionMode::Fidelity);
        assert_eq!(long(&bus, 0x600500), None);
        assert_eq!(long(&bus, u32::MAX), None);
        assert_eq!(long(&bus, 0x100000), Some(0));
    }

    #[test]
    fn os_observation_requires_signature_and_preserves_registers_and_memory() {
        use m68k::AddressBus;
        let mut flash = vec![0; 0x40000];
        flash[0x12538..0x1253c].copy_from_slice(&[0x70, 0x27, 0x4e, 0x4b]);
        let mut bus = Tech2Bus::new(flash, vec![], ExecutionMode::Fidelity);
        let path =
            std::env::temp_dir().join(format!("tech2-os-observer-{}.jsonl", std::process::id()));
        bus.trace = crate::trace::Trace::open(&path, false).unwrap();
        let mut cpu = CpuCore::new();
        cpu.pc = 0x1253a;
        cpu.set_sp(0x150000);
        cpu.dar[14] = 0x150000;
        cpu.dar[1] = 0x80000;
        cpu.dar[2..6].copy_from_slice(&[0xb0000, 7, 0xa10, 0]);
        bus.write_long(0x150004, 0x1dbc0c);
        let registers = cpu.dar;
        let memory = bus.peek_bytes(0x100000, 0x100000).unwrap().to_vec();
        bus.current_pc = cpu.pc;
        observe(&cpu, &mut bus);
        assert_eq!(cpu.dar, registers);
        assert_eq!(bus.peek_bytes(0x100000, 0x100000).unwrap(), memory);
        cpu.pc = 0x1253c;
        cpu.dar[0] = 0;
        bus.current_pc = cpu.pc;
        observe(&cpu, &mut bus);
        // A different service's address without the expected ROM signature
        // must not produce a guessed semantic event.
        cpu.pc = 0x125a0;
        observe(&cpu, &mut bus);
        bus.trace.check().unwrap();
        let output = std::fs::read_to_string(&path).unwrap();
        let events: Vec<_> = output.lines().collect();
        assert_eq!(events.len(), 2);
        assert!(events[0].contains("service=queue_send number=0x27 phase=enter"));
        assert!(events[0].contains("caller=0x001dbc0c"));
        assert!(events[0].contains("0x000b0000,0x00000007,0x00000a10,0x00000000"));
        assert!(events[1].contains("phase=return"));
        assert!(events.iter().all(|line| line.contains("external_tx=false")));
        drop(bus);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn receive_observation_requires_signature_and_never_reports_stale_data() {
        use m68k::AddressBus;
        let mut bus = Tech2Bus::new(vec![0; 0x40000], vec![], ExecutionMode::Fidelity);
        let path =
            std::env::temp_dir().join(format!("tech2-contract-{}.jsonl", std::process::id()));
        bus.trace = crate::trace::Trace::open(&path, false).unwrap();
        let mut cpu = CpuCore::new();
        cpu.pc = 0x1dbca2;
        cpu.set_sp(0x150000);
        bus.current_pc = cpu.pc;
        // No matching firmware: these addresses must not acquire semantic labels.
        observe(&cpu, &mut bus);
        for (addr, bytes) in [
            (
                0x1dbb96,
                &[
                    0x48, 0xe7, 0x0f, 0x0c, 0x4f, 0xef, 0xff, 0xe8, 0x2a, 0x6f, 0, 0x34,
                ][..],
            ),
            (
                0x1dbd3e,
                &[
                    0x30, 6, 0x4f, 0xef, 0, 0x18, 0x4c, 0xdf, 0x30, 0xf0, 0x4e, 0x74, 0, 8,
                ][..],
            ),
        ] {
            for (i, b) in bytes.iter().enumerate() {
                bus.write_byte(addr + i as u32, *b);
            }
        }
        bus.write_long(0x150034, 0x160000); // descriptor in the original caller arguments
        bus.write_long(0x160002, 0x12345678);
        bus.write_long(0x150008, 0xdeadbeef); // stale receive buffer on failed receive
        cpu.dar[0] = 1;
        observe(&cpu, &mut bus);
        cpu.dar[0] = 0;
        observe(&cpu, &mut bus);
        bus.trace.check().unwrap();
        let output = std::fs::read_to_string(&path).unwrap();
        let events: Vec<_> = output.lines().collect();
        assert_eq!(events.len(), 2);
        assert!(events[0].contains("entry_sp=0x00150030"));
        assert!(events[0].contains("request_key=0x12345678"));
        assert!(events[0].contains("envelope_valid=false"));
        assert!(!events[0].contains("deadbeef"));
        assert!(events[1].contains("envelope_valid=true"));
        assert!(events[1].contains("0xdeadbeef"));
        drop(bus);
        std::fs::remove_file(path).unwrap();
    }
}
