use std::net::{SocketAddr, UdpSocket};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::util::test_util::ADDRESS_SPEND;
use bingle_core::dtls::{NetworkMux, UdpNetworkMux};

// Global test guard to serialize tests in this module (Rust tests run in parallel by default)
static TEST_GUARD: OnceLock<Mutex<()>> = OnceLock::new();

// Global recorders for handler invocations
static DTLS_RECORDS: OnceLock<Mutex<Vec<(SocketAddr, Vec<u8>)>>> = OnceLock::new();
static STUN_RECORDS: OnceLock<Mutex<Vec<(SocketAddr, Vec<u8>)>>> = OnceLock::new();
static TURN_RECORDS: OnceLock<Mutex<Vec<(SocketAddr, Vec<u8>)>>> = OnceLock::new();

fn dtls_handler(
    _src: &dyn NetworkMux,
    from: &bingle_core::api::bingle_api::NetworkEndpoint,
    data: &[u8],
) {
    let m = DTLS_RECORDS.get_or_init(|| Mutex::new(Vec::new()));
    let addr = from
        .inet_socket_address()
        .expect("DTLS handler expected direct endpoint");
    m.lock().unwrap().push((addr, data.to_vec()));
}

fn stun_handler(_src: &dyn NetworkMux, from: &SocketAddr, data: &[u8]) {
    let m = STUN_RECORDS.get_or_init(|| Mutex::new(Vec::new()));
    m.lock().unwrap().push((*from, data.to_vec()));
}

fn turn_handler(_src: &dyn NetworkMux, from: &SocketAddr, data: &[u8]) {
    let m = TURN_RECORDS.get_or_init(|| Mutex::new(Vec::new()));
    m.lock().unwrap().push((*from, data.to_vec()));
}

fn clear_all_records() {
    if let Some(m) = DTLS_RECORDS.get() {
        m.lock().unwrap().clear();
    }
    if let Some(m) = STUN_RECORDS.get() {
        m.lock().unwrap().clear();
    }
    if let Some(m) = TURN_RECORDS.get() {
        m.lock().unwrap().clear();
    }
}

fn wait_for_records<F: Fn() -> bool>(timeout_ms: u64, predicate: F) -> bool {
    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(timeout_ms) {
        if predicate() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    predicate()
}

#[ntest::timeout(30_000)]
#[test]
#[cfg(not(target_os = "ios"))]
pub fn dispatches_stun_dtls_turn() {
    let _g = TEST_GUARD.get_or_init(|| Mutex::new(())).lock().unwrap();
    clear_all_records();

    // Bind mux on localhost:0
    let mux_inner = UdpNetworkMux::bind(("127.0.0.1", 0))
        .expect("bind mux")
        .with_handle_stun(Arc::new(stun_handler))
        .with_handle_dtls(Arc::new(dtls_handler))
        .with_handle_turn(Arc::new(turn_handler));
    let mux = Arc::new(mux_inner);

    let addr = mux.local_addr().unwrap();
    mux.start().expect("start mux");

    // Sender socket
    let sender = UdpSocket::bind(("127.0.0.1", 0)).expect("bind sender");

    // STUN: first byte in 0..=3
    sender.send_to(&[0, 1, 2], addr).unwrap();
    // DTLS: 20..=63
    sender.send_to(&[20, 9, 9], addr).unwrap();
    // TURN: 0x40..=0x7f
    sender.send_to(&[0x40, 0xaa], addr).unwrap();

    let ok = wait_for_records(500, || {
        let s = STUN_RECORDS
            .get()
            .map(|m| m.lock().unwrap().len())
            .unwrap_or(0);
        let d = DTLS_RECORDS
            .get()
            .map(|m| m.lock().unwrap().len())
            .unwrap_or(0);
        let t = TURN_RECORDS
            .get()
            .map(|m| m.lock().unwrap().len())
            .unwrap_or(0);
        s >= 1 && d >= 1 && t >= 1
    });

    mux.stop();
    assert!(ok, "expected at least one record in each handler");
}

#[ntest::timeout(30_000)]
#[test]
#[cfg(not(target_os = "ios"))]
pub fn ignores_zrtp_rtp_unknown() {
    let _g = TEST_GUARD.get_or_init(|| Mutex::new(())).lock().unwrap();
    clear_all_records();

    let mux_inner = UdpNetworkMux::bind(("127.0.0.1", 0))
        .expect("bind mux")
        .with_handle_stun(Arc::new(stun_handler))
        .with_handle_dtls(Arc::new(dtls_handler))
        .with_handle_turn(Arc::new(turn_handler));
    let mux = Arc::new(mux_inner);

    let addr = mux.local_addr().unwrap();
    mux.start().expect("start mux");

    let sender = UdpSocket::bind(("127.0.0.1", 0)).expect("bind sender");

    // ZRTP: 16..=19
    sender.send_to(&[16, 0], addr).unwrap();
    // RTP: 128..=191
    sender.send_to(&[128, 0], addr).unwrap();
    // UNKNOWN: e.g., 4
    sender.send_to(&[4, 0], addr).unwrap();

    // Wait briefly to allow potential (incorrect) dispatch; expect none
    std::thread::sleep(Duration::from_millis(200));
    let s = STUN_RECORDS
        .get()
        .map(|m| m.lock().unwrap().len())
        .unwrap_or(0);
    let d = DTLS_RECORDS
        .get()
        .map(|m| m.lock().unwrap().len())
        .unwrap_or(0);
    let t = TURN_RECORDS
        .get()
        .map(|m| m.lock().unwrap().len())
        .unwrap_or(0);

    mux.stop();
    assert_eq!(s, 0, "no STUN handler should be invoked");
    assert_eq!(d, 0, "no DTLS handler should be invoked");
    assert_eq!(t, 0, "no TURN handler should be invoked");
}

#[ntest::timeout(30_000)]
#[test]
#[cfg(not(target_os = "ios"))]
pub fn write_sends_payload() {
    // Receiver socket to capture payload
    let receiver = UdpSocket::bind(("127.0.0.1", 0)).expect("bind receiver");
    receiver
        .set_read_timeout(Some(Duration::from_millis(500)))
        .unwrap();
    let recv_addr = receiver.local_addr().unwrap();

    let mux = UdpNetworkMux::bind(("127.0.0.1", 0)).expect("bind mux");

    let payload = b"hello world";
    let nsk = bingle_core::api::bingle_api::NetworkEndpoint::new_direct(recv_addr);
    mux.write(&nsk, payload).expect("write ok");

    let mut buf = [0u8; 64];
    let (n, _from) = receiver.recv_from(&mut buf).expect("recv payload");
    assert_eq!(&buf[..n], payload);
}

#[ntest::timeout(30_000)]
#[test]
#[cfg(not(target_os = "ios"))]
pub fn write_relay_wraps_payload_in_turn_channel_data() {
    // Receiver socket simulating the relay endpoint
    let receiver = UdpSocket::bind(("127.0.0.1", 0)).expect("bind receiver");
    receiver
        .set_read_timeout(Some(Duration::from_millis(500)))
        .unwrap();
    let relay_addr = receiver.local_addr().unwrap();

    let mux = UdpNetworkMux::bind(("127.0.0.1", 0)).expect("bind mux");

    // Use a valid TURN channel range value
    let ch: u16 = 0x4001;
    let payload: &[u8] = b"abc123"; // len = 6 → already 4-byte aligned, padding = 2

    // Build a relay NetworkSourceKey and perform write
    let nsk = bingle_core::api::bingle_api::NetworkEndpoint::new_relay(
        ADDRESS_SPEND.to_string(),
        Some(relay_addr),
        Some(ch),
    );
    mux.write(&nsk, payload).expect("relay write ok");

    // Receive the datagram and verify TURN ChannelData header + payload + padding
    let mut buf = [0u8; 128];
    let (n, _from) = receiver.recv_from(&mut buf).expect("recv wrapped");
    assert!(
        n >= 4 + payload.len(),
        "wrapped length should include header and payload"
    );

    // Header: 2 bytes channel (BE), 2 bytes length (payload len)
    let ch_got = u16::from_be_bytes([buf[0], buf[1]]);
    let len_got = u16::from_be_bytes([buf[2], buf[3]]) as usize;
    assert_eq!(ch_got, ch, "channel number should match");
    assert_eq!(len_got, payload.len(), "length should equal payload length");

    // Body: payload followed by 0 padding to 4-byte boundary
    assert_eq!(&buf[4..4 + len_got], payload, "payload intact");
    let padded_len = (len_got + 3) & !3;
    let padding = padded_len - len_got;
    assert_eq!(n, 4 + padded_len, "datagram length should include padding");
    for i in 0..padding {
        assert_eq!(buf[4 + len_got + i], 0u8, "padding byte should be zero");
    }
}
