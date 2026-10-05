//! Synthetic oracle and simulated transport checks only; never opens a device.
use super::*;
use crate::adapters::vcx_nano::{
    channel::{control, Client, UsbTransport},
    protocol::Decoder,
};
use std::collections::VecDeque;
fn unhex(s: &str) -> Vec<u8> {
    s.as_bytes()
        .chunks_exact(2)
        .map(|c| u8::from_str_radix(std::str::from_utf8(c).unwrap(), 16).unwrap())
        .collect()
}
const TIME: Trailer = Trailer {
    filetime: 123456789,
    ticks: 0x10203040,
};
#[test]
fn exact_independent_c_python_all_156_cases() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/cipher-all-lengths.json")).unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 156);
    for case in cases {
        let plain = unhex(case["plain"].as_str().unwrap());
        let key: [u8; 16] = unhex(case["key"].as_str().unwrap()).try_into().unwrap();
        let expected = unhex(case["cipher"].as_str().unwrap());
        let mut data = plain.clone();
        crypt(&mut data, &key, false).unwrap();
        assert_eq!(data, expected);
        crypt(&mut data, &key, true).unwrap();
        assert_eq!(data, plain);
    }
}
#[test]
fn independent_sealed_query_record_selector() {
    let f: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/interoperability.json")).unwrap();
    let key: [u8; 16] = unhex(f["key_hex"].as_str().unwrap()).try_into().unwrap();
    for case in f["cases"].as_array().unwrap() {
        let plain = unhex(case["plaintext_hex"].as_str().unwrap());
        assert_eq!(
            seal(&plain, &key, TIME).unwrap(),
            unhex(case["sealed_hex"].as_str().unwrap())
        );
        match case["name"].as_str().unwrap() {
            "query16" => assert_eq!(plain, query()),
            "selector144" => assert_eq!(plain, selector()),
            "record16" => assert_eq!(
                open_record(&unhex(case["sealed_hex"].as_str().unwrap()), &key)
                    .unwrap()
                    .as_slice(),
                plain
            ),
            _ => panic!(),
        };
    }
}
#[test]
fn independent_dh_symmetry_and_fresh_phases() {
    let f: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/interoperability.json")).unwrap();
    let a = f["DH_synthetic"]["private_a"].as_u64().unwrap();
    let b = f["DH_synthetic"]["private_b"].as_u64().unwrap();
    let da = Dh::generate(&mut || Ok(a), None).unwrap();
    let db = Dh::generate(&mut || Ok(b), None).unwrap();
    assert_eq!(
        da.public.to_le_bytes().as_slice(),
        unhex(f["DH_synthetic"]["public_a_LE_hex"].as_str().unwrap())
    );
    assert_eq!(
        db.public.to_le_bytes().as_slice(),
        unhex(f["DH_synthetic"]["public_b_LE_hex"].as_str().unwrap())
    );
    let ka = da.key(&[9; 8], db.public).unwrap();
    assert_eq!(ka, db.key(&[9; 8], da.public).unwrap());
    assert_eq!(
        &ka[8..],
        unhex(f["DH_synthetic"]["shared_LE_hex"].as_str().unwrap())
    );
    let dc = Dh::generate(&mut || Ok(a + 1), Some(&da)).unwrap();
    assert_ne!(ka, dc.key(&[9; 8], pow_mod(5, b + 1)).unwrap());
}
#[test]
fn degenerate_public_rng_and_repeated_host_rejected() {
    let dh = Dh::generate(&mut || Ok(4328719365), None).unwrap();
    for peer in [0, 1, PRIME - 1, PRIME] {
        assert!(dh.key(&[0; 8], peer).is_err());
    }
    for random in [0, 1, PRIME - 1, PRIME] {
        assert!(Dh::generate(&mut || Ok(random), None).is_err());
    }
    assert!(Dh::generate(&mut || Ok(4328719365), Some(&dh)).is_err());
    assert!(Dh::generate(&mut || Err("rng unavailable".into()), None).is_err());
}
#[test]
fn marker_token_and_cipher_bounds_rejected() {
    let key = [7; 16];
    let mut record = query();
    record[8..12].copy_from_slice(&PASSTHRU_ID.to_le_bytes());
    assert!(open_record(&seal(&record, &key, TIME).unwrap(), &key).is_ok());
    for index in [0] {
        let mut bad = record;
        bad[index] ^= 1;
        assert!(open_record(&seal(&bad, &key, TIME).unwrap(), &key).is_err());
    }
    let mut wrong = record.to_vec();
    wrong.extend([0; 16]);
    crypt(&mut wrong, &key, false).unwrap();
    assert!(open_record(&wrong, &key).is_err());
    for len in [0, 4, 7, 9, 164] {
        assert!(crypt(&mut vec![0; len], &key, false).is_err());
    }
    assert!(open_record(&[0; 31], &key).is_err());
}
struct Device {
    writes: usize,
    reads: VecDeque<Vec<u8>>,
    keys: Vec<[u8; 16]>,
    bad_id: bool,
    repeat_peer: bool,
    status_failure: bool,
    unsupported_firmware: bool,
    final_partial_reply: bool,
}
impl Device {
    fn new() -> Self {
        Self {
            writes: 0,
            reads: VecDeque::new(),
            keys: vec![],
            bad_id: false,
            repeat_peer: false,
            status_failure: false,
            unsupported_firmware: false,
            final_partial_reply: false,
        }
    }
}
impl UsbTransport for Device {
    fn write(&mut self, bytes: &[u8]) -> Result<(), String> {
        let mut decoder = Decoder::default();
        let frames = decoder.feed(bytes)?;
        decoder.finish()?;
        let frame = &frames[0];
        let opcode = frame.header[2];
        let mut result = vec![0];
        match self.writes {
            0 => {
                assert_eq!(opcode, 0x8c);
                let mut info = vec![0; 64];
                info[16..24].copy_from_slice(&[9, 8, 7, 6, 5, 4, 3, 2]);
                info[28..38].copy_from_slice(b"VCX-NANO\0\0");
                info[52..56].copy_from_slice(if self.unsupported_firmware {
                    &[3, 4, 9, 1]
                } else {
                    &[2, 4, 9, 1]
                });
                result.extend(info);
            }
            1 | 3 => {
                assert_eq!(opcode, 0xa0);
                let host = u64::from_le_bytes(frame.payload.as_slice().try_into().unwrap());
                let private = if self.writes == 1 || self.repeat_peer {
                    21575960585
                } else {
                    21575960586
                };
                let device = Dh::generate(&mut || Ok(private), None)?;
                self.keys.push(device.key(&[9, 8, 7, 6, 5, 4, 3, 2], host)?);
                result.extend(device.public.to_le_bytes());
            }
            2 | 5 => {
                assert_eq!(opcode, if self.writes == 2 { 0x84 } else { 0xa2 });
                let key = self.keys.last().unwrap();
                let mut query_bytes = frame.payload.clone();
                crypt(&mut query_bytes, key, true)?;
                assert_eq!(&query_bytes[..16], &query());
                assert_eq!(&query_bytes[16..20], &0xa567a567u32.to_le_bytes());
                let mut record = query();
                record[8..12].copy_from_slice(
                    &(if self.bad_id {
                        PASSTHRU_ID ^ 1
                    } else {
                        PASSTHRU_ID
                    })
                    .to_le_bytes(),
                );
                result.extend(seal(&record, key, TIME)?);
            }
            4 => {
                assert_eq!(opcode, 0xa1);
                let mut plain = frame.payload.clone();
                crypt(&mut plain, self.keys.last().unwrap(), true)?;
                assert_eq!(&plain[..144], &selector());
            }
            _ => panic!("extra transaction"),
        }
        if self.status_failure && self.writes == 1 {
            result[0] = 0xfe;
        }
        let reply = control(0, opcode, &result).encode()?;
        let mid = reply.len() / 2;
        self.reads.push_back(reply[..mid].to_vec());
        let mut last = reply[mid..].to_vec();
        if self.final_partial_reply && self.writes == 5 {
            last.extend([0xbb, 0x80]);
        }
        self.reads.push_back(last);
        self.writes += 1;
        Ok(())
    }
    fn read(&mut self) -> Result<Vec<u8>, String> {
        self.reads
            .pop_front()
            .ok_or("Synthetic device exhausted".into())
    }
}
fn simulated(device: Device) -> (Result<(), String>, Progress, Client<InitOnly<Device>>) {
    let mut client = Client::new(InitOnly {
        inner: device,
        completed_writes: 0,
    });
    let mut r = VecDeque::from([4328719365, 4328719366]);
    let mut p = Progress::default();
    let result = initialize(
        &mut client,
        &mut || r.pop_front().ok_or("rng exhausted".into()),
        &mut || Ok(TIME),
        &mut p,
    );
    (result, p, client)
}
#[test]
fn two_phase_scripted_device_uses_distinct_keys_and_fragmented_replies() {
    let (r, p, mut c) = simulated(Device::new());
    assert!(r.is_ok(), "{r:?}");
    assert!(p.query_record_verified && p.installed_record_verified);
    assert_eq!(p.fresh_dh_exchanges, 2);
    let t = c.transport_mut();
    assert_eq!(t.completed_writes, 6);
    assert_ne!(t.inner.keys[0], t.inner.keys[1]);
}
#[test]
fn different_record_metadata_accepted_in_both_phases_with_fixed_selector() {
    let mut d = Device::new();
    d.bad_id = true;
    let (r, p, mut c) = simulated(d);
    assert!(r.is_ok());
    assert!(p.query_record_verified && p.installed_record_verified);
    assert_eq!(c.transport_mut().completed_writes, 6);
}
#[test]
fn record_metadata_is_opaque_and_never_changes_literal_selector() {
    let key = [7; 16];
    for metadata in [[0u8; 8], [255u8; 8], [1, 2, 3, 4, 5, 6, 7, 8]] {
        let mut record = query();
        record[8..16].copy_from_slice(&metadata);
        assert_eq!(
            open_record(&seal(&record, &key, TIME).unwrap(), &key).unwrap(),
            record
        );
        assert_eq!(&selector()[0x84..0x88], &PASSTHRU_ID.to_le_bytes());
        assert_eq!(&selector()[..0x84], &[0; 0x84]);
    }
}
#[test]
fn reused_device_public_stops_before_a1() {
    let mut d = Device::new();
    d.repeat_peer = true;
    let (r, p, mut c) = simulated(d);
    assert!(r.is_err());
    assert!(p.query_record_verified && !p.installed_record_verified);
    assert_eq!(c.transport_mut().completed_writes, 4);
}
#[test]
fn nonzero_status_stops_startup() {
    let mut d = Device::new();
    d.status_failure = true;
    let (r, p, mut c) = simulated(d);
    assert!(r.is_err());
    assert_eq!(p.fresh_dh_exchanges, 0);
    assert_eq!(c.transport_mut().completed_writes, 2);
}
#[test]
fn unsupported_firmware_stops_after_identity_before_entropy_or_dh() {
    let mut device = Device::new();
    device.unsupported_firmware = true;
    let mut client = Client::new(InitOnly {
        inner: device,
        completed_writes: 0,
    });
    let mut progress = Progress::default();
    let result = initialize(
        &mut client,
        &mut || panic!("Firmware failure must precede entropy"),
        &mut || panic!("Firmware failure must precede trailer time"),
        &mut progress,
    );
    assert!(result.is_err());
    assert_eq!(client.transport_mut().completed_writes, 1);
    assert_eq!(progress.fresh_dh_exchanges, 0);
    assert!(!progress.query_record_verified && !progress.installed_record_verified);
}
#[test]
fn tx_gate_blocks_channel_can_reset_firmware_and_wrong_order_before_write() {
    for op in [0, 0x40, 0x41, 0x8d, 0x84, 0xa1, 0xa2, 0xb0] {
        let mut t = InitOnly {
            inner: Device::new(),
            completed_writes: 0,
        };
        assert!(t.write(&control(0, op, &[]).encode().unwrap()).is_err());
        assert_eq!(t.inner.writes, 0);
    }
    let mut t = InitOnly {
        inner: Device::new(),
        completed_writes: 0,
    };
    let combined = [
        control(0, 0x8c, &[]).encode().unwrap(),
        control(0, 0x8c, &[]).encode().unwrap(),
    ]
    .concat();
    assert!(t.write(&combined).is_err());
    assert_eq!(t.inner.writes, 0);
}

#[test]
fn production_entry_reuses_actual_identity_and_observes_five_auth_replies() {
    let mut client = Client::new(InitOnly {
        inner: Device::new(),
        completed_writes: 0,
    });
    let identity_reply = client
        .exchange(&control(0, 0x8c, &[]), &mut |_| Ok(()))
        .unwrap();
    let mut values = VecDeque::from([4328719365, 4328719366]);
    let mut progress = Progress::default();
    let mut seen = Vec::new();
    initialize_from_identity(
        &mut client,
        &identity_reply,
        &mut || values.pop_front().ok_or("RNG exhausted".into()),
        &mut || Ok(TIME),
        &mut progress,
        &mut |f| {
            seen.push(f.header[2]);
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(seen, [0xa0, 0x84, 0xa0, 0xa1, 0xa2]);
    assert_eq!(client.transport_mut().completed_writes, 6);
    assert!(progress.installed_record_verified && progress.query_record_verified);
    assert_eq!(progress.fresh_dh_exchanges, 2);
}

#[test]
fn supplied_bad_identity_never_draws_entropy_or_writes_auth() {
    let mut client = Client::new(InitOnly {
        inner: Device::new(),
        completed_writes: 0,
    });
    let good = client
        .exchange(&control(0, 0x8c, &[]), &mut |_| Ok(()))
        .unwrap();
    for (index, replacement) in [(29, b'X'), (53, 3), (0, 0xfe)] {
        let mut bad = good.clone();
        bad.payload[index] = replacement;
        let mut progress = Progress {
            query_record_verified: true,
            installed_record_verified: true,
            fresh_dh_exchanges: 2,
        };
        assert!(initialize_from_identity(
            &mut client,
            &bad,
            &mut || panic!("Bad identity must not draw entropy"),
            &mut || panic!("Bad identity must not draw time"),
            &mut progress,
            &mut |_| panic!("Bad identity must not transfer auth")
        )
        .is_err());
        assert_eq!(client.transport_mut().completed_writes, 1);
        assert!(!progress.query_record_verified && !progress.installed_record_verified);
        assert_eq!(progress.fresh_dh_exchanges, 0);
    }
}

#[test]
fn partial_suffix_after_valid_final_record_does_not_qualify_startup() {
    let mut device = Device::new();
    device.final_partial_reply = true;
    let (result, progress, mut client) = simulated(device);
    assert!(result.unwrap_err().contains("Truncated Nano frame"));
    assert!(progress.query_record_verified && !progress.installed_record_verified);
    assert_eq!(progress.fresh_dh_exchanges, 2);
    assert_eq!(client.transport_mut().completed_writes, 6);
}
