use std::net::SocketAddr;
use std::sync::{Arc, Mutex, OnceLock};

use rust_comms::dtls::network_mux_udp::UdpNetworkMux;
use rust_comms::dtls::network_mux_trait::NetworkMux;

// Global recorders for handler invocations
static DTLS_REC: OnceLock<Mutex<Vec<(SocketAddr, Vec<u8>)>>> = OnceLock::new();
static STUN_REC: OnceLock<Mutex<Vec<(SocketAddr, Vec<u8>)>>> = OnceLock::new();
static TURN_REC: OnceLock<Mutex<Vec<(SocketAddr, Vec<u8>)>>> = OnceLock::new();

fn dtls_handler(_src: &dyn NetworkMux, from: &SocketAddr, data: &[u8]) {
    let m = DTLS_REC.get_or_init(|| Mutex::new(Vec::new()));
    m.lock().unwrap().push((*from, data.to_vec()));
}
fn stun_handler(_src: &dyn NetworkMux, from: &SocketAddr, data: &[u8]) {
    let m = STUN_REC.get_or_init(|| Mutex::new(Vec::new()));
    m.lock().unwrap().push((*from, data.to_vec()));
}
fn turn_handler(_src: &dyn NetworkMux, from: &SocketAddr, data: &[u8]) {
    let m = TURN_REC.get_or_init(|| Mutex::new(Vec::new()));
    m.lock().unwrap().push((*from, data.to_vec()));
}

fn clear() {
    if let Some(m) = DTLS_REC.get() { m.lock().unwrap().clear(); }
    if let Some(m) = STUN_REC.get() { m.lock().unwrap().clear(); }
    if let Some(m) = TURN_REC.get() { m.lock().unwrap().clear(); }
}

#[test]
fn reprocess_dispatches_and_enqueues_dtls() {
    clear();
    // Create a mux and install handlers
    let mut mux = UdpNetworkMux::bind(("127.0.0.1", 0)).expect("bind mux");
    mux.set_handle_dtls(Some(Arc::new(dtls_handler)));
    mux.set_handle_stun(Some(Arc::new(stun_handler)));
    mux.set_handle_turn(Some(Arc::new(turn_handler)));

    let local_from = mux.local_addr().unwrap();

    // Prepare classified payloads
    let stun = [0u8, 1, 2]; // STUN range 0..=3
    let dtls = [20u8, 9, 9, 9, 9]; // DTLS range 20..=63
    let turn = [0x40u8, 0xaa, 0xbb, 0xcc]; // TURN ChannelData range 0x40..=0x7f

    // Re-dispatch
    mux.reprocess(local_from, &stun);
    mux.reprocess(local_from, &dtls);
    mux.reprocess(local_from, &turn);

    // Handlers should have been invoked with provided 'from' equal to local addr
    let stun_recs = STUN_REC.get().unwrap().lock().unwrap().clone();
    assert_eq!(stun_recs.len(), 1);
    assert_eq!(stun_recs[0].0, local_from);
    assert_eq!(stun_recs[0].1, stun);

    let dtls_recs = DTLS_REC.get().unwrap().lock().unwrap().clone();
    assert_eq!(dtls_recs.len(), 1);
    assert_eq!(dtls_recs[0].0, local_from);
    assert_eq!(dtls_recs[0].1, dtls);

    let turn_recs = TURN_REC.get().unwrap().lock().unwrap().clone();
    assert_eq!(turn_recs.len(), 1);
    assert_eq!(turn_recs[0].0, local_from);
    assert_eq!(turn_recs[0].1, turn);

    // DTLS should also be enqueued into the internal DTLS queue
    let mut buf = [0u8; 64];
    let (n, from) = mux.dtls_peek_from(&mut buf).expect("dtls_peek_from should have data");
    assert_eq!(from, local_from);
    assert_eq!(&buf[..n], &dtls);

    // And dtls_recv_from should pop it
    let mut buf2 = [0u8; 64];
    let (n2, from2) = mux.dtls_recv_from(&mut buf2).expect("dtls_recv_from should pop data");
    assert_eq!(from2, local_from);
    assert_eq!(&buf2[..n2], &dtls);

    // After pop, queue should be empty
    let mut tmp = [0u8; 8];
    let res = mux.dtls_peek_from(&mut tmp);
    assert!(res.is_err(), "queue should be empty after pop");
}
