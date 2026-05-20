use iap2_rs::link::Iap2Link;
use iap2_rs::packet::{Iap2Packet, PacketType};
use iap2_rs::transport::fake::FakeTransport;
use iap2_rs::types::LinkConfig;

/// 工具函数：构造一个 SYN-ACK 回复包
fn make_syn_ack(peer_seq: u8, ack_to: u8) -> Vec<u8> {
    let mut pkt = Iap2Packet::new(
        iap2_rs::packet::ControlByte::new(PacketType::SynAck),
        peer_seq,
        ack_to,
    );
    // SYN-ACK carries negotiation payload (same as SYN)
    pkt.payload = bytes::Bytes::from_static(&[
        0x01, 0x05, 0x10, 0x00, 0x04, 0x0B, 0x00, 0x17, 0x03, 0x03, 0x01, 0x01, 0x02, 0x0A,
        0x00, 0x01, 0x0B, 0x02, 0x01,
    ]);
    pkt.to_bytes().to_vec()
}

#[tokio::test]
async fn link_negotiate_detect_syn_ack() {
    let config = LinkConfig {
        max_retries: 3,
        timeout_ms: 200,
        detect_to_syn_delay_ms: 10,
        syn_retry_delay_ms: 10,
        iap1_probe_timeout_ms: 10,
        initial_seq: 0x9D,
    };

    // 准备 iPhone 响应序列：
    // 1. iAP1 probe response (可以是任意 6 字节)
    // 2. SYN-ACK: ack our SYN seq (0x9D)
    let iap1_resp = vec![0xFF, 0x55, 0x02, 0x00, 0xEE, 0x10];
    let syn_ack = make_syn_ack(0x01, 0x9D);

    let (mut transport, state) = FakeTransport::with_rx_data(vec![iap1_resp, syn_ack]);

    let mut link = Iap2Link::new(config);
    let result = link.negotiate(&mut transport).await;
    assert!(result.is_ok(), "negotiate failed: {:?}", result.err());
    assert!(link.is_established());

    // 验证发送日志
    let sent = state.lock().unwrap().tx_log.clone();
    assert!(sent.len() >= 3, "expected at least 3 sends (iAP1 probe, detect, SYN, ACK), got {}", sent.len());

    // 第一个发送应该是 iAP1 probe (detect marker)
    assert_eq!(&sent[0][0..2], &[0xFF, 0x55]);

    // 第二个发送应该是 DETECT packet
    let detect = Iap2Packet::from_bytes(&sent[1]).unwrap();
    assert_eq!(detect.control.packet_type, PacketType::Detect);

    // 第三个发送应该是 SYN
    let syn = Iap2Packet::from_bytes(&sent[2]).unwrap();
    assert_eq!(syn.control.packet_type, PacketType::Syn);
    assert_eq!(syn.seq, 0x9D);

    // 第四个发送应该是 ACK (完成握手)
    let ack = Iap2Packet::from_bytes(&sent[3]).unwrap();
    assert_eq!(ack.control.packet_type, PacketType::Ack);
}

#[tokio::test]
async fn link_negotiate_retries_on_timeout() {
    let config = LinkConfig {
        max_retries: 2,
        timeout_ms: 50,
        detect_to_syn_delay_ms: 5,
        syn_retry_delay_ms: 5,
        iap1_probe_timeout_ms: 5,
        initial_seq: 0x10,
    };

    // 不提供 SYN-ACK 响应，应该超时并重试
    let iap1_resp = vec![0xFF, 0x55, 0x02, 0x00, 0xEE, 0x10];
    let (mut transport, _state) = FakeTransport::with_rx_data(vec![iap1_resp]);

    let mut link = Iap2Link::new(config);
    let result = link.negotiate(&mut transport).await;
    assert!(result.is_err(), "should fail after retries exhausted");
}

#[tokio::test]
async fn link_send_and_receive_data() {
    let config = LinkConfig::default();
    let mut link = Iap2Link::new(config);

    // 手动建立 link（跳过 negotiate）
    // 通过反射设置 state
    link.force_established(0x9E, 0x01);

    // 准备 iPhone 发来的 data packet
    let payload = bytes::Bytes::from_static(&[0x40, 0x40, 0x00, 0x06, 0x1D, 0x00]);
    let iphone_pkt = Iap2Packet::data(0x02, 0x9E, 0x0A, payload.clone());
    let wire = iphone_pkt.to_bytes().to_vec();

    let (mut transport, state) = FakeTransport::with_rx_data(vec![wire]);

    let received = link.receive_data(&mut transport).await.unwrap();
    assert_eq!(received.session_id, Some(0x0A));
    assert_eq!(received.payload, payload);

    // 验证我们发回了 ACK
    let sent = state.lock().unwrap().tx_log.clone();
    assert!(!sent.is_empty(), "should have sent ACK");
    let ack = Iap2Packet::from_bytes(&sent[0]).unwrap();
    assert_eq!(ack.control.packet_type, PacketType::Ack);
}
