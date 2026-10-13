// SPDX-License-Identifier: MPL-2.0
//! Fresh classic-MDI identity announcements observed on the selected USB carrier.
//! No key, management owner, USB descriptor serial or saved identity is used.
use smoltcp::wire::{IpAddress, IpProtocol, Ipv4Address, Ipv4Packet, UdpPacket};

pub fn announced_serial(frame: &[u8]) -> Option<u32> {
    // The dedicated USB link carries the observed multicast group, not a scan
    // of Android's LAN. IPv4 and UDP framing/checksums precede identity parsing.
    if frame.len() < 42 || frame[..6] != [1, 0, 0x5e, 1, 1, 1] || frame[12..14] != [8, 0] {
        return None;
    }
    let ip = Ipv4Packet::new_checked(&frame[14..]).ok()?;
    if ip.version() != 4
        || !ip.verify_checksum()
        || ip.more_frags()
        || ip.frag_offset() != 0
        || ip.next_header() != IpProtocol::Udp
        || ip.src_addr() != Ipv4Address::new(192, 168, 171, 2)
        || ip.dst_addr() != Ipv4Address::new(225, 1, 1, 1)
    {
        return None;
    }
    let udp = UdpPacket::new_checked(ip.payload()).ok()?;
    if udp.dst_port() != 8194
        || udp.src_port() == 0
        || usize::from(udp.len()) != ip.payload().len()
        || !udp.verify_checksum(
            &IpAddress::Ipv4(ip.src_addr()),
            &IpAddress::Ipv4(ip.dst_addr()),
        )
    {
        return None;
    }
    serial(udp.payload())
}

fn serial(body: &[u8]) -> Option<u32> {
    // Short advertisements and advertisements with bounded ownership metadata
    // share record opcode 0x86d, version byte 7 and the unaligned LE serial.
    if !(40..=256).contains(&body.len())
        || u32::from_le_bytes(body[..4].try_into().ok()?) as usize != body.len()
        || u32::from_le_bytes(body[4..8].try_into().ok()?) != 0x86d
        || body[8] != 7
    {
        return None;
    }
    let serial = u32::from_le_bytes(body[9..13].try_into().ok()?);
    (serial != 0).then_some(serial)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn advertisement(size: usize) -> Vec<u8> {
        let mut b = vec![0; size];
        b[..4].copy_from_slice(&(size as u32).to_le_bytes());
        b[4..8].copy_from_slice(&0x86du32.to_le_bytes());
        b[8] = 7;
        b[9..13].copy_from_slice(&0xf1234567u32.to_le_bytes());
        b
    }
    fn frame() -> Vec<u8> {
        let mut f = vec![0; 82];
        f[..6].copy_from_slice(&[1, 0, 0x5e, 1, 1, 1]);
        f[12..14].copy_from_slice(&[8, 0]);
        let mut ip = Ipv4Packet::new_unchecked(&mut f[14..34]);
        ip.set_version(4);
        ip.set_header_len(20);
        ip.set_total_len(68);
        ip.set_hop_limit(1);
        ip.set_next_header(IpProtocol::Udp);
        ip.set_src_addr(Ipv4Address::new(192, 168, 171, 2));
        ip.set_dst_addr(Ipv4Address::new(225, 1, 1, 1));
        ip.fill_checksum();
        let mut udp = UdpPacket::new_unchecked(&mut f[34..]);
        udp.set_src_port(33333);
        udp.set_dst_port(8194);
        udp.set_len(48);
        udp.payload_mut().copy_from_slice(&advertisement(40));
        udp.fill_checksum(
            &IpAddress::v4(192, 168, 171, 2),
            &IpAddress::v4(225, 1, 1, 1),
        );
        f
    }
    #[test]
    fn validates_selected_usb_network_and_checksums() {
        let f = frame();
        assert_eq!(announced_serial(&f), Some(0xf1234567));
        for at in [0, 12, 24, 29, 33, 36, 52] {
            let mut bad = f.clone();
            bad[at] ^= 1;
            assert_eq!(announced_serial(&bad), None);
        }
        for n in 0..f.len() {
            assert_eq!(announced_serial(&f[..n]), None);
        }
        // A correctly checksummed announcement from another source is rejected.
        let mut other = f;
        let mut ip = Ipv4Packet::new_unchecked(&mut other[14..34]);
        ip.set_src_addr(Ipv4Address::new(192, 168, 171, 3));
        ip.fill_checksum();
        assert_eq!(announced_serial(&other), None);
    }
    #[test]
    fn short_and_owned_announcements_have_same_unsigned_identity() {
        for n in [40, 97, 98, 99] {
            assert_eq!(serial(&advertisement(n)), Some(0xf1234567));
        }
    }
    #[test]
    fn rejects_truncated_wrong_version_opcode_length_and_zero_serial() {
        let b = advertisement(40);
        for n in 0..40 {
            assert_eq!(serial(&b[..n]), None);
        }
        for at in [0, 4, 8] {
            let mut bad = b.clone();
            bad[at] ^= 1;
            assert_eq!(serial(&bad), None);
        }
        let mut bad = b;
        bad[9..13].fill(0);
        assert_eq!(serial(&bad), None);
        assert_eq!(serial(&advertisement(257)), None);
    }
}
