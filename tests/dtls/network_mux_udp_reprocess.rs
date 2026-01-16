use std::net::SocketAddr;
use std::sync::{Arc, Mutex, OnceLock};
use rust_comms::api::network_endpoint::NetworkEndpoint;
use rust_comms::dtls::network_mux_udp::UdpNetworkMux;
use rust_comms::dtls::network_mux_trait::NetworkMux;

// Global recorders for handler invocations
static DTLS_REC: OnceLock<Mutex<Vec<(NetworkEndpoint, Vec<u8>)>>> = OnceLock::new();
static STUN_REC: OnceLock<Mutex<Vec<(SocketAddr, Vec<u8>)>>> = OnceLock::new();
static TURN_REC: OnceLock<Mutex<Vec<(SocketAddr, Vec<u8>)>>> = OnceLock::new();

fn dtls_handler(_src: &dyn NetworkMux, from: &NetworkEndpoint, data: &[u8]) {
    let m = DTLS_REC.get_or_init(|| Mutex::new(Vec::new()));
    m.lock().unwrap().push((from.clone(), data.to_vec()));
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

    let local_from = NetworkEndpoint::new_direct(mux.local_addr().unwrap());

    // Prepare classified payloads
    let stun = [0u8, 1, 2]; // STUN range 0..=3
    let dtls = [20u8, 9, 9, 9, 9]; // DTLS range 20..=63
    let turn = [0x40u8, 0xaa, 0xbb, 0xcc]; // TURN ChannelData range 0x40..=0x7f

    // Re-dispatch
    mux.reprocess(&local_from, &stun);
    mux.reprocess(&local_from, &dtls);
    mux.reprocess(&local_from, &turn);

    // Handlers should have been invoked with provided 'from' equal to local addr
    let stun_recs = STUN_REC.get().unwrap().lock().unwrap().clone();
    assert_eq!(stun_recs.len(), 1);
    assert_eq!(stun_recs[0].0, local_from.inet_socket_address().unwrap());
    assert_eq!(stun_recs[0].1, stun);

    let dtls_recs = DTLS_REC.get().unwrap().lock().unwrap().clone();
    assert_eq!(dtls_recs.len(), 1);
    assert_eq!(dtls_recs[0].0, local_from);
    assert_eq!(dtls_recs[0].1, dtls);

    let turn_recs = TURN_REC.get().unwrap().lock().unwrap().clone();
    assert_eq!(turn_recs.len(), 1);
    assert_eq!(turn_recs[0].0, local_from.inet_socket_address().unwrap());
    assert_eq!(turn_recs[0].1, turn);
}
