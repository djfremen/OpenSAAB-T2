// SPDX-License-Identifier: MPL-2.0
use opensaab_adapter_translation::{
    gm::Report,
    host::{Action, Config, FlowControl, Limits, Protocol, ResultData, Session, State},
    isotp::{Receiver, Step},
    profile::{
        Capabilities, CommandFamily, FlowControlOwner, Mode, Representation, SessionProfile,
    },
    Bus, Observation, Origin, Packet, RawFrame,
};
fn caps(family: CommandFamily) -> Capabilities {
    Capabilities {
        family,
        buses: vec![Bus::HsCan, Bus::SwCan],
        raw_receive: true,
        raw_transmit: true,
        assembled_receive: true,
        adapter_flow_control: true,
        simultaneous_buses: true,
    }
}
fn config(bus: Bus, id: u32, service: u8) -> Config {
    Config {
        session_id: 1,
        profile: SessionProfile {
            bus,
            mode: Mode::HostIsoTp,
            representation: Representation::RawCan,
            flow_control: FlowControlOwner::Host,
        },
        request_id: id - 8,
        reply_id: id,
        extended: false,
        protocol: Protocol::IsoTp {
            service,
            positive_prefix: vec![service + 0x40],
        },
        limits: Limits::default(),
        host_flow_control: FlowControl {
            block_size: 0,
            st_min: 0x10,
        },
    }
}
fn rx(session_id: u64, at_us: u64, bus: Bus, id: u32, data: &[u8]) -> Observation {
    Observation {
        session_id,
        at_us,
        device_timestamp: Some(999),
        origin: Origin::BehaviorModel,
        packet: Packet::Raw(RawFrame::standard(bus, id, data.to_vec()).unwrap()),
    }
}
fn gm() -> Config {
    let mut c = config(Bus::SwCan, 0x642, 0xa9);
    c.request_id = 0x242;
    c.profile.mode = Mode::HostGmReport;
    c.profile.flow_control = FlowControlOwner::None;
    c.protocol = Protocol::GmA9Report { report_id: 0x542 };
    c
}
#[test]
fn one_contract_has_identical_results_for_every_command_family() {
    // These are qualified synthetic profiles, not retail hardware claims.
    for family in [
        CommandFamily::Chipsoft,
        CommandFamily::Nano,
        CommandFamily::J2534,
        CommandFamily::Elm,
        CommandFamily::Stn,
        CommandFamily::Mic,
        CommandFamily::BlueDriver,
        CommandFamily::Virtual,
    ] {
        let mut s = Session::new(gm(), &caps(family), 0).unwrap();
        assert_eq!(
            s.feed(&rx(
                1,
                1,
                Bus::SwCan,
                0x542,
                &[0x81, 0xe1, 3, 0, 0x6f, 0, 0, 0]
            ))
            .unwrap(),
            Action::Progress
        );
        let result = s
            .feed(&rx(
                1,
                2,
                Bus::SwCan,
                0x542,
                &[0x81, 0, 0, 0, 0xff, 0, 0, 0],
            ))
            .unwrap();
        let Action::Complete(ResultData::GmReport(records)) = result else {
            panic!()
        };
        assert_eq!(
            (&records[0].code, records[0].failure_type, records[0].status),
            (&"U2103".to_string(), 0, 0x6f)
        );
        assert_eq!(s.state(), State::Complete);
    }
}
#[test]
fn complete_empty_missing_end_invalid_end_and_failure_remain_distinct() {
    let c = caps(CommandFamily::Mic);
    let mut empty = Session::new(gm(), &c, 0).unwrap();
    assert_eq!(
        empty
            .feed(&rx(1, 1, Bus::SwCan, 0x542, &[0x81, 0, 0, 0, 0xff]))
            .unwrap(),
        Action::Complete(ResultData::GmReport(vec![]))
    );
    let mut missing = Session::new(gm(), &c, 0).unwrap();
    missing
        .feed(&rx(1, 1, Bus::SwCan, 0x542, &[0x81, 0xe1, 3, 0, 0x6f]))
        .unwrap();
    assert!(missing.tick(1_000_001).is_err());
    assert_eq!(missing.state(), State::Failed);
    let mut invalid = Session::new(gm(), &c, 0).unwrap();
    assert!(invalid
        .feed(&rx(1, 1, Bus::SwCan, 0x542, &[0x81, 0, 0, 0, 0]))
        .is_err());
    assert!(invalid
        .feed(&rx(1, 2, Bus::SwCan, 0x542, &[0x81, 0, 0, 0, 0xff]))
        .is_err());
    let mut report = Report::default();
    assert!(report.receive(0x5e8, &[0x81, 0, 0, 0, 0]).is_err());
    assert!(report.finish().is_err());
}
#[test]
fn gm_records_do_not_enter_isotp_and_wrong_bus_id_session_are_ignored() {
    let mut s = Session::new(gm(), &caps(CommandFamily::Chipsoft), 0).unwrap();
    for packet in [
        rx(1, 1, Bus::HsCan, 0x542, &[0x81, 0, 0, 0, 0xff]),
        rx(1, 2, Bus::SwCan, 0x543, &[0x81, 0, 0, 0, 0xff]),
        rx(2, 3, Bus::SwCan, 0x542, &[0x81, 0, 0, 0, 0xff]),
    ] {
        assert_eq!(s.feed(&packet).unwrap(), Action::Ignored);
    }
    assert_eq!(s.state(), State::Active);
    assert!(Receiver::new(512)
        .unwrap()
        .push(&[0x81, 0, 0, 0, 0xff])
        .is_err());
    assert_eq!(
        s.feed(&rx(1, 4, Bus::SwCan, 0x542, &[0x81, 0, 0, 0, 0xff]))
            .unwrap(),
        Action::Complete(ResultData::GmReport(vec![]))
    );
}
#[test]
fn pending_extends_idle_but_absolute_deadline_and_unrelated_traffic_do_not() {
    let mut cfg = gm();
    cfg.limits.idle_us = 10;
    cfg.limits.total_us = 25;
    let mut s = Session::new(cfg, &caps(CommandFamily::Stn), 0).unwrap();
    for at in [9, 18, 24] {
        assert_eq!(
            s.feed(&rx(1, at, Bus::SwCan, 0x642, &[3, 0x7f, 0xa9, 0x78]))
                .unwrap(),
            Action::Pending
        );
    }
    assert!(s.tick(25).is_err());
    let mut cfg = gm();
    cfg.limits.idle_us = 10;
    let mut s = Session::new(cfg, &caps(CommandFamily::Stn), 0).unwrap();
    assert_eq!(
        s.feed(&rx(1, 9, Bus::HsCan, 0x642, &[3, 0x7f, 0xa9, 0x78]))
            .unwrap(),
        Action::Ignored
    );
    assert!(s.tick(10).is_err());
}
#[test]
fn interleaved_ecus_and_equal_ids_on_different_buses_never_merge() {
    let cap = caps(CommandFamily::J2534);
    let mut a = Session::new(config(Bus::HsCan, 0x7e8, 0x1a), &cap, 0).unwrap();
    let mut b = Session::new(config(Bus::SwCan, 0x7e8, 0x1a), &cap, 0).unwrap();
    let observations = [
        rx(1, 1, Bus::HsCan, 0x7e8, &[0x10, 8, 0x5a, 0x90, 1, 2, 3, 4]),
        rx(1, 2, Bus::SwCan, 0x7e8, &[0x10, 8, 0x5a, 0x90, 5, 6, 7, 8]),
        rx(1, 3, Bus::HsCan, 0x7e8, &[0x21, 9, 10, 0, 0, 0, 0, 0]),
        rx(1, 4, Bus::SwCan, 0x7e8, &[0x21, 11, 12, 0, 0, 0, 0, 0]),
    ];
    assert!(matches!(
        a.feed(&observations[0]).unwrap(),
        Action::SendFlowControl(_)
    ));
    assert_eq!(b.feed(&observations[0]).unwrap(), Action::Ignored);
    assert_eq!(a.feed(&observations[1]).unwrap(), Action::Ignored);
    assert!(matches!(
        b.feed(&observations[1]).unwrap(),
        Action::SendFlowControl(_)
    ));
    assert_eq!(
        a.feed(&observations[2]).unwrap(),
        Action::Complete(ResultData::Payload(vec![0x5a, 0x90, 1, 2, 3, 4, 9, 10]))
    );
    assert_eq!(b.feed(&observations[2]).unwrap(), Action::Ignored);
    assert_eq!(
        b.feed(&observations[3]).unwrap(),
        Action::Complete(ResultData::Payload(vec![0x5a, 0x90, 5, 6, 7, 8, 11, 12]))
    );
    let mut a = Session::new(config(Bus::HsCan, 0x7e8, 0x1a), &cap, 0).unwrap();
    assert_eq!(
        a.feed(&rx(1, 1, Bus::HsCan, 0x7e9, &[1, 0x5a])).unwrap(),
        Action::Ignored
    );
}
#[test]
fn sequence_wrap_padding_truncation_and_duplicates_are_bounded() {
    let mut receiver = Receiver::new(200).unwrap();
    assert_eq!(
        receiver.push(&[0x10, 121, 1, 2, 3, 4, 5, 6]).unwrap(),
        Step::FirstFrame
    );
    let mut expected = vec![1, 2, 3, 4, 5, 6];
    for n in 1..=17 {
        let data = [0x20 | (n & 15), n, n, n, n, n, n, n];
        let count = (121 - expected.len()).min(7);
        expected.extend_from_slice(&data[1..1 + count]);
        let result = receiver.push(&data).unwrap();
        if n == 17 {
            assert_eq!(result, Step::Complete(expected.clone()));
        } else {
            assert_eq!(result, Step::ConsecutiveFrame);
        }
    }
    assert!(receiver.push(&[1, 1]).is_err());
    for wrong in [
        vec![0x22, 0, 0, 0, 0, 0, 0, 0],
        vec![0x21, 1],
        vec![0x10, 8, 1, 2, 3, 4, 5, 6],
    ] {
        let mut receiver = Receiver::new(64).unwrap();
        receiver.push(&[0x10, 19, 1, 2, 3, 4, 5, 6]).unwrap();
        assert!(receiver.push(&wrong).is_err());
        assert!(receiver.push(&[0x21, 0, 0, 0, 0, 0, 0, 0]).is_err());
    }
    assert!(Receiver::new(64).unwrap().push(&[0, 0, 0]).is_err());
    assert!(Receiver::new(8)
        .unwrap()
        .push(&[0x10, 9, 1, 2, 3, 4, 5, 6])
        .is_err());
}
#[test]
fn fc_owner_and_block_size_are_explicit_and_adapter_owner_emits_no_fc() {
    let cap = caps(CommandFamily::Mic);
    let first = rx(1, 1, Bus::HsCan, 0x7e8, &[0x10, 20, 0x5a, 0x90, 1, 2, 3, 4]);
    let mut cfg = config(Bus::HsCan, 0x7e8, 0x1a);
    cfg.host_flow_control.block_size = 1;
    let mut s = Session::new(cfg.clone(), &cap, 0).unwrap();
    let Action::SendFlowControl(fc) = s.feed(&first).unwrap() else {
        panic!()
    };
    assert_eq!(
        (fc.id, fc.data),
        (0x7e0, vec![0x30, 1, 0x10, 0, 0, 0, 0, 0])
    );
    assert!(matches!(
        s.feed(&rx(1, 2, Bus::HsCan, 0x7e8, &[0x21, 5, 6, 7, 8, 9, 10, 11]))
            .unwrap(),
        Action::SendFlowControl(_)
    ));
    cfg.profile.flow_control = FlowControlOwner::Adapter;
    let mut s = Session::new(cfg, &cap, 0).unwrap();
    assert_eq!(s.feed(&first).unwrap(), Action::Progress);
}
#[test]
fn assembled_messages_cannot_be_relabelled_raw_or_sent_to_guest() {
    let packet = Packet::IsoTpPayload {
        bus: Bus::HsCan,
        id: 0x7e8,
        extended: false,
        data: vec![0x5a, 0x90, 1],
    };
    assert!(packet.guest_frame().is_err());
    let mut cfg = config(Bus::HsCan, 0x7e8, 0x1a);
    cfg.profile.representation = Representation::IsoTpPayload;
    cfg.profile.flow_control = FlowControlOwner::Adapter;
    let mut s = Session::new(cfg, &caps(CommandFamily::Elm), 0).unwrap();
    assert_eq!(
        s.feed(&Observation {
            session_id: 1,
            at_us: 1,
            device_timestamp: None,
            origin: Origin::Replay,
            packet: packet.clone()
        })
        .unwrap(),
        Action::Complete(ResultData::Payload(vec![0x5a, 0x90, 1]))
    );
    let mut s = Session::new(
        config(Bus::HsCan, 0x7e8, 0x1a),
        &caps(CommandFamily::Elm),
        0,
    )
    .unwrap();
    assert!(s
        .feed(&Observation {
            session_id: 1,
            at_us: 1,
            device_timestamp: None,
            origin: Origin::Replay,
            packet
        })
        .is_err());
}
#[test]
fn cancellation_clock_regression_nrc_and_wrong_service_are_terminal() {
    let cap = caps(CommandFamily::Nano);
    let mut s = Session::new(gm(), &cap, 100).unwrap();
    assert!(s.tick(99).is_err());
    let mut s = Session::new(gm(), &cap, 0).unwrap();
    s.cancel();
    assert_eq!(s.state(), State::Cancelled);
    assert!(s
        .feed(&rx(1, 1, Bus::SwCan, 0x542, &[0x81, 0, 0, 0, 0xff]))
        .is_err());
    for data in [
        vec![3, 0x7f, 0x1a, 0x31],
        vec![3, 0x7f, 0xa9, 0x78],
        vec![2, 0x50, 0x90],
    ] {
        let mut s = Session::new(config(Bus::HsCan, 0x7e8, 0x1a), &cap, 0).unwrap();
        assert!(s.feed(&rx(1, 1, Bus::HsCan, 0x7e8, &data)).is_err());
        assert_eq!(s.state(), State::Failed);
    }
}
#[test]
fn unqualified_bus_representation_and_simultaneous_mode_are_rejected() {
    let mut cap = caps(CommandFamily::Stn);
    cap.buses = vec![Bus::HsCan];
    assert!(Session::new(gm(), &cap, 0).is_err());
    cap = caps(CommandFamily::Mic);
    cap.adapter_flow_control = false;
    let mut p = config(Bus::HsCan, 0x7e8, 0x1a).profile;
    p.flow_control = FlowControlOwner::Adapter;
    assert!(cap.validate(p).is_err());
    cap = caps(CommandFamily::Mic);
    cap.simultaneous_buses = false;
    assert!(cap.validate_concurrent(&[p, gm().profile]).is_err());
    p.mode = Mode::GuestRaw;
    assert!(cap.validate(p).is_err());
    p.flow_control = FlowControlOwner::Guest;
    assert!(cap.validate(p).is_ok());
    let mut cfg = config(Bus::HsCan, 0x7e8, 0x1a);
    cfg.host_flow_control.st_min = 0x80;
    assert!(Session::new(cfg, &cap, 0).is_err());
}
#[test]
fn malformed_raw_frames_are_never_normalized_into_valid_traffic() {
    let mut frame = RawFrame::standard(Bus::HsCan, 0x7e8, vec![1, 2]).unwrap();
    frame.dlc = 8;
    assert!(Packet::Raw(frame).guest_frame().is_err());
    assert!(RawFrame::standard(Bus::HsCan, 0x800, vec![]).is_err());
    assert!(RawFrame::standard(Bus::HsCan, 1, vec![0; 9]).is_err());
}
#[test]
fn report_capacity_and_service_pending_cannot_produce_success_after_error() {
    let mut r = Report::new(1).unwrap();
    r.receive(0x5e8, &[0x81, 1, 7, 0, 0x6f]).unwrap();
    assert!(r.receive(0x5e8, &[0x81, 1, 7, 0, 0x6f]).is_err());
    assert!(r.receive(0x5e8, &[0x81, 0, 0, 0, 0xff]).is_err());
    assert!(r.finish().is_err());
    let mut s = Session::new(
        config(Bus::HsCan, 0x7e8, 0x1a),
        &caps(CommandFamily::Virtual),
        0,
    )
    .unwrap();
    assert_eq!(
        s.feed(&rx(1, 1, Bus::HsCan, 0x7e8, &[3, 0x7f, 0x1a, 0x78]))
            .unwrap(),
        Action::Pending
    );
    assert_eq!(
        s.feed(&rx(1, 2, Bus::HsCan, 0x7e8, &[3, 0x5a, 0x90, 5]))
            .unwrap(),
        Action::Complete(ResultData::Payload(vec![0x5a, 0x90, 5]))
    );
}
