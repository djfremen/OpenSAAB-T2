// SPDX-License-Identifier: MPL-2.0
//! Regression boundaries from the accepted October 4 normal Nano channel trace.
//! Offline fixtures do not qualify an independent cold hardware initializer.
use tech2_emu::{
    adapters::{chipsoft, j2534, vcx_nano},
    can_adapter::{electrical_route, ElectricalMode},
    candi_cpu::{CanElectricalState, CanFrame, CanTransmission},
};

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02X}")).collect()
}

#[test]
fn nano_setup_matches_capture_and_cleanup_deletes_the_installed_filter() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/nano_channel_sequence_2026_10_04.json"
    ))
    .unwrap();
    assert_eq!(fixture["independent_cold_initializer"], false);
    let setup: Vec<_> = [0, 1]
        .into_iter()
        .flat_map(|channel| vcx_nano::init::setup(channel).unwrap())
        .map(|frame| hex(&frame.encode().unwrap()))
        .collect();
    let shutdown: Vec<_> = [1, 0]
        .into_iter()
        .flat_map(vcx_nano::init::shutdown)
        .map(|frame| hex(&frame.encode().unwrap()))
        .collect();
    assert_eq!(serde_json::json!(setup), fixture["setup"]);
    // The later native cleanup correction deletes slot zero. Keep the older
    // captured zero-width PASS records in the fixture as historical evidence;
    // they no longer describe the corrected teardown operation.
    assert_eq!(shutdown, [
        "BB800048010100000000CABB", "BB80004301C4BB", "BB80004101C2BB",
        "BB800048000100000000C9BB", "BB80004300C3BB", "BB80004100C1BB",
    ]);
    assert_ne!(serde_json::json!(shutdown), fixture["shutdown"]);
    assert!(vcx_nano::init::setup(2).is_err());
    // Existing callers still resolve the relocated initialization functions.
    assert_eq!(
        tech2_emu::nano_channel::setup(1).unwrap(),
        vcx_nano::init::setup(1).unwrap()
    );
    assert_eq!(
        tech2_emu::chipsoft_channel::setup(5).unwrap(),
        chipsoft::init::setup(5).unwrap()
    );
}

fn tx(electrical: CanElectricalState, id: u32, data: &[u8]) -> CanTransmission {
    CanTransmission {
        ticket: 1,
        frame: CanFrame {
            id,
            extended: false,
            rtr: false,
            dlc: data.len() as u8,
            data: data.to_vec(),
        },
        btr0: 0xdd,
        btr1: 0x36,
        electrical,
    }
}

#[test]
fn electrical_intent_is_neutral_and_each_driver_encodes_its_own_wake_flag() {
    let wake = tx(
        CanElectricalState::SingleWireGpio {
            latch: 0x26,
            assignment: 0,
            direction: 0x7f,
        },
        0x100,
        &[],
    );
    assert_eq!(
        electrical_route(2, &wake).unwrap().mode,
        ElectricalMode::SingleWireWake
    );
    let nano = vcx_nano::native::encode(2, &wake, 0).unwrap();
    assert_eq!(&nano.payload[..4], &0x1000_u32.to_be_bytes());
    let chipsoft = chipsoft::channel::transmit(2, &wake).unwrap();
    assert_eq!(&chipsoft.payload[14..18], &0x400_u32.to_le_bytes());
    assert_eq!(
        j2534::bridge::encode(2, &wake).unwrap(),
        "TX 2 1 0400 100 -"
    );
    let data = tx(
        CanElectricalState::SingleWireGpio {
            latch: 3,
            assignment: 0,
            direction: 3,
        },
        0x101,
        &[2, 0x1a, 0x90],
    );
    assert_eq!(
        electrical_route(2, &data).unwrap().mode,
        ElectricalMode::Data
    );
    assert_eq!(
        &vcx_nano::native::encode(2, &data, 0).unwrap().payload[..4],
        &[0; 4]
    );
    assert_eq!(
        &chipsoft::channel::transmit(2, &data).unwrap().payload[14..18],
        &[0; 4]
    );
    assert_eq!(
        j2534::bridge::encode(2, &data).unwrap(),
        "TX 2 1 0000 101 021A90"
    );
    // Identifier alone cannot manufacture a wake or bypass physical-route checks.
    let invalid = tx(
        CanElectricalState::SingleWireGpio {
            latch: 2,
            assignment: 0,
            direction: 1,
        },
        0x100,
        &[],
    );
    for (controller, request) in [(0, &wake), (2, &invalid)] {
        assert!(electrical_route(controller, request).is_err());
        assert!(vcx_nano::native::encode(controller, request, 0).is_err());
        assert!(chipsoft::channel::transmit(controller, request).is_err());
        assert!(j2534::bridge::encode(controller, request).is_err());
    }
}
