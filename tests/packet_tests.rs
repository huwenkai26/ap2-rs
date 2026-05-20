use bytes::Bytes;

use iap2_rs::packet::{ControlByte, Iap2Packet, PacketType};

// ──────────────────────────────────────────────────────────
// Packet 序列化 / 反序列化
// ──────────────────────────────────────────────────────────

#[test]
fn detect_packet_roundtrip() {
    let pkt = Iap2Packet::detect();
    let wire = pkt.to_bytes();
    assert_eq!(wire.as_ref(), &[0xFF, 0x5A, 0x00, 0x06, 0xEE, 0x10]);

    let parsed = Iap2Packet::from_bytes(&wire).unwrap();
    assert_eq!(parsed.control.packet_type, PacketType::Detect);
    assert_eq!(parsed.seq, 0x10);
}

#[test]
fn syn_packet_serialize() {
    let pkt = Iap2Packet::syn(0x9D);
    let wire = pkt.to_bytes();

    // header: FF 5A + 2-byte len + control + seq + ack + session + checksum
    assert_eq!(wire[0], 0xFF);
    assert_eq!(wire[1], 0x5A);
    assert_eq!(wire[4], 0x80); // SYN control byte
    assert_eq!(wire[5], 0x9D); // seq
    assert_eq!(wire[6], 0x00); // ack
}

#[test]
fn syn_packet_roundtrip() {
    let pkt = Iap2Packet::syn(0x9D);
    let wire = pkt.to_bytes();
    let parsed = Iap2Packet::from_bytes(&wire).unwrap();

    assert_eq!(parsed.control.packet_type, PacketType::Syn);
    assert_eq!(parsed.seq, 0x9D);
    assert_eq!(parsed.ack, 0x00);
    assert!(!parsed.payload.is_empty()); // SYN carries negotiation payload
}

#[test]
fn ack_packet_roundtrip() {
    let pkt = Iap2Packet::ack(0x02, 0x01);
    let wire = pkt.to_bytes();
    let parsed = Iap2Packet::from_bytes(&wire).unwrap();

    assert_eq!(parsed.control.packet_type, PacketType::Ack);
    assert_eq!(parsed.seq, 0x02);
    assert_eq!(parsed.ack, 0x01);
    assert!(parsed.payload.is_empty());
}

#[test]
fn data_packet_with_session_roundtrip() {
    let payload = Bytes::from_static(&[0x40, 0x40, 0x00, 0x08, 0x1D, 0x01, 0xAA, 0xBB]);
    let pkt = Iap2Packet::data(0x03, 0x02, 0x0A, payload.clone());
    let wire = pkt.to_bytes();
    let parsed = Iap2Packet::from_bytes(&wire).unwrap();

    assert_eq!(parsed.seq, 0x03);
    assert_eq!(parsed.ack, 0x02);
    assert_eq!(parsed.session_id, Some(0x0A));
    assert_eq!(parsed.payload, payload);
}

#[test]
fn header_checksum_validation() {
    let pkt = Iap2Packet::ack(0x10, 0x05);
    let wire = pkt.to_bytes();
    // header checksum = wire[0..9] sum should be 0
    let sum: u8 = wire[0..9].iter().fold(0u8, |a, b| a.wrapping_add(*b));
    assert_eq!(sum, 0, "header checksum verification failed");
}

#[test]
fn payload_checksum_validation() {
    let payload = Bytes::from_static(&[0xDE, 0xAD, 0xBE, 0xEF]);
    let pkt = Iap2Packet::data(0x01, 0x00, 0x0A, payload);
    let wire = pkt.to_bytes();

    // payload + checksum byte should sum to 0
    let payload_start = 9; // PACKET_HEADER_SIZE
    let payload_bytes = &wire[payload_start..];
    let sum: u8 = payload_bytes.iter().fold(0u8, |a, b| a.wrapping_add(*b));
    assert_eq!(sum, 0, "payload checksum verification failed");
}

#[test]
fn reject_truncated_packet() {
    let result = Iap2Packet::from_bytes(&[0xFF, 0x5A]);
    assert!(result.is_err());
}

#[test]
fn reject_wrong_sync_bytes() {
    let result = Iap2Packet::from_bytes(&[0x00, 0x00, 0x00, 0x09, 0x40, 0x01, 0x00, 0x00, 0x00]);
    assert!(result.is_err());
}

#[test]
fn reject_bad_header_checksum() {
    let mut wire = Iap2Packet::ack(0x01, 0x00).to_bytes().to_vec();
    // corrupt the header checksum byte
    wire[8] = wire[8].wrapping_add(1);
    let result = Iap2Packet::from_bytes(&wire);
    assert!(result.is_err());
}

#[test]
fn control_byte_encoding() {
    let cb = ControlByte::new(PacketType::Syn);
    assert_eq!(cb.to_byte(), 0x80);

    let mut cb_session = ControlByte::new(PacketType::Ack);
    cb_session.has_session = true;
    assert_eq!(cb_session.to_byte(), 0x48); // 0x40 | 0x08

    let parsed = ControlByte::from_byte(0xC0).unwrap();
    assert_eq!(parsed.packet_type, PacketType::SynAck);
    assert!(!parsed.has_session);
}

#[test]
fn packet_type_from_byte_all_variants() {
    assert_eq!(PacketType::try_from(0x80).unwrap(), PacketType::Syn);
    assert_eq!(PacketType::try_from(0xC0).unwrap(), PacketType::SynAck);
    assert_eq!(PacketType::try_from(0x40).unwrap(), PacketType::Ack);
    assert_eq!(PacketType::try_from(0x00).unwrap(), PacketType::Data);
    assert_eq!(PacketType::try_from(0x60).unwrap(), PacketType::Eak);
    assert_eq!(PacketType::try_from(0xA0).unwrap(), PacketType::Rst);
    assert_eq!(PacketType::try_from(0xEE).unwrap(), PacketType::Detect);
}
