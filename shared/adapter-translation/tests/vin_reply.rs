// SPDX-License-Identifier: MPL-2.0
use opensaab_adapter_translation::isotp::VinReply;

const PENDING: [u8; 8] = [3, 0x7f, 0x1a, 0x78, 0, 0, 0, 0];
const FIRST: [u8; 8] = [0x10, 19, 0x5a, 0x90, b'Y', b'S', b'3', b'F'];
const NEXT: [u8; 8] = [0x21, b'H', b'4', b'6', b'U', b'6', b'8', b'1'];
const LAST: [u8; 8] = [0x22, b'0', b'0', b'0', b'0', b'0', b'1', 0];

#[test]
fn captured_pending_prefix_waits_without_flow_control_then_completes_vin() {
    let mut reply = VinReply::default();
    // Exact service/NRC/frame shape observed on the Intel Nano; VIN synthetic.
    for _ in 0..3 {
        assert!(!reply.feed(&PENDING).unwrap());
        assert_eq!(reply.vin().unwrap(), None);
    }
    assert!(reply.feed(&FIRST).unwrap());
    assert!(!reply.feed(&NEXT).unwrap());
    assert!(!reply.feed(&LAST).unwrap());
    assert_eq!(reply.vin().unwrap().as_deref(), Some("YS3FH46U681000001"));
    assert!(reply.feed(&PENDING).is_err());
}

#[test]
fn wrong_service_nrc_size_and_first_frame_fail_terminally() {
    for invalid in [
        vec![3, 0x7f, 0x22, 0x78, 0, 0, 0, 0],
        vec![3, 0x7f, 0x1a, 0x22, 0, 0, 0, 0],
        vec![3, 0x7f, 0x1a, 0x78],
        vec![4, 0x7f, 0x1a, 0x78, 0, 0, 0, 0],
        vec![0x10, 19, 0x5a, 0x91, b'Y', b'S', b'3', b'F'],
        vec![0x21, 0, 0, 0, 0, 0, 0, 0],
    ] {
        let mut reply = VinReply::default();
        assert!(reply.feed(&invalid).is_err());
        assert!(reply.feed(&PENDING).is_err());
        assert!(reply.feed(&FIRST).is_err());
        assert!(reply.vin().is_err());
    }
}

#[test]
fn pending_cannot_interrupt_segmented_reply_or_recover_a_sequence_error() {
    let mut reply = VinReply::default();
    assert!(reply.feed(&FIRST).unwrap());
    assert!(reply.feed(&PENDING).is_err());
    assert!(reply.feed(&NEXT).is_err());
    assert!(reply.vin().is_err());

    let mut reply = VinReply::default();
    assert!(!reply.feed(&PENDING).unwrap());
    assert!(reply.feed(&FIRST).unwrap());
    assert!(reply.feed(&LAST).is_err());
    assert!(reply.feed(&PENDING).is_err());
    assert!(reply.feed(&NEXT).is_err());
    assert!(reply.vin().is_err());
}
