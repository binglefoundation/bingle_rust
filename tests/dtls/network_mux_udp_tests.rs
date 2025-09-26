use std::net::{SocketAddr, UdpSocket};
use std::sync::{Mutex, OnceLock, Arc};
use std::time::{Duration, Instant};

use rust_comms::dtls::{UdpNetworkMux, NetworkMux};

// Global test guard to serialize tests in this module (Rust tests run in parallel by default)
static TEST_GUARD: OnceLock<Mutex<()>> = OnceLock::new();

// Global recorders for handler invocations
static DTLS_RECORDS: OnceLock<Mutex<Vec<(SocketAddr, Vec<u8>)>>> = OnceLock::new();
static STUN_RECORDS: OnceLock<Mutex<Vec<(SocketAddr, Vec<u8>)>>> = OnceLock::new();
static TURN_RECORDS: OnceLock<Mutex<Vec<(SocketAddr, Vec<u8>)>>> = OnceLock::new();

fn dtls_handler(_src: &dyn NetworkMux, from: &SocketAddr, data: &[u8]) {
    let m = DTLS_RECORDS.get_or_init(|| Mutex::new(Vec::new()));
    m.lock().unwrap().push((*from, data.to_vec()));
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
    if let Some(m) = DTLS_RECORDS.get() { m.lock().unwrap().clear(); }
    if let Some(m) = STUN_RECORDS.get() { m.lock().unwrap().clear(); }
    if let Some(m) = TURN_RECORDS.get() { m.lock().unwrap().clear(); }
}

fn wait_for_records<F: Fn() -> bool>(timeout_ms: u64, predicate: F) -> bool {
    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(timeout_ms) {
        if predicate() { return true; }
        std::thread::sleep(Duration::from_millis(10));
    }
    predicate()
}

#[test]
fn dispatches_stun_dtls_turn() {
    let _g = TEST_GUARD.get_or_init(|| Mutex::new(())).lock().unwrap();
    clear_all_records();

    // Bind mux on localhost:0
    let mux_inner = UdpNetworkMux::bind(("127.0.0.1", 0)).expect("bind mux")
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
        let s = STUN_RECORDS.get().map(|m| m.lock().unwrap().len()).unwrap_or(0);
        let d = DTLS_RECORDS.get().map(|m| m.lock().unwrap().len()).unwrap_or(0);
        let t = TURN_RECORDS.get().map(|m| m.lock().unwrap().len()).unwrap_or(0);
        s >= 1 && d >= 1 && t >= 1
    });

    mux.stop();
    assert!(ok, "expected at least one record in each handler");
}

#[test]
fn ignores_zrtp_rtp_unknown() {
    let _g = TEST_GUARD.get_or_init(|| Mutex::new(())).lock().unwrap();
    clear_all_records();

    let mux_inner = UdpNetworkMux::bind(("127.0.0.1", 0)).expect("bind mux")
        .with_handle_stun(stun_handler)
        .with_handle_dtls(dtls_handler)
        .with_handle_turn(turn_handler);
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
    let s = STUN_RECORDS.get().map(|m| m.lock().unwrap().len()).unwrap_or(0);
    let d = DTLS_RECORDS.get().map(|m| m.lock().unwrap().len()).unwrap_or(0);
    let t = TURN_RECORDS.get().map(|m| m.lock().unwrap().len()).unwrap_or(0);

    mux.stop();
    assert_eq!(s, 0, "no STUN handler should be invoked");
    assert_eq!(d, 0, "no DTLS handler should be invoked");
    assert_eq!(t, 0, "no TURN handler should be invoked");
}

#[test]
fn write_sends_payload() {
    // Receiver socket to capture payload
    let receiver = UdpSocket::bind(("127.0.0.1", 0)).expect("bind receiver");
    receiver.set_read_timeout(Some(Duration::from_millis(500))).unwrap();
    let recv_addr = receiver.local_addr().unwrap();

    let mux = UdpNetworkMux::bind(("127.0.0.1", 0)).expect("bind mux");

    let payload = b"hello world";
    mux.write(recv_addr, payload).expect("write ok");

    let mut buf = [0u8; 64];
    let (n, _from) = receiver.recv_from(&mut buf).expect("recv payload");
    assert_eq!(&buf[..n], payload);
}
