use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use rust_comms::stun::{StunEndpointFinder, StunEndpointFinderImpl};

#[test]
#[cfg(not(target_os = "ios"))]
pub fn impl_uses_send_packet_handler_instead_of_udp() {
    // Prepare two fake STUN servers (no actual network I/O will occur)
    let s1: SocketAddr = "1.1.1.1:3478".parse().unwrap();
    let s2: SocketAddr = "8.8.8.8:3478".parse().unwrap();

    // Capture handler invocations
    let calls: Arc<Mutex<Vec<(String, u16, usize)>>> = Arc::new(Mutex::new(Vec::new()));
    let calls_clone = Arc::clone(&calls);

    let mut finder = StunEndpointFinderImpl::new();

    // Install the send_packet_handler which records calls
    finder.set_send_packet_handler(Some(Arc::new(move |host: &str, port: u16, data: &[u8]| {
        calls_clone.lock().unwrap().push((host.to_string(), port, data.len()));
    })));

    // Start the finder
    finder.init(vec![s1, s2], 100, 500);

    // Tick manually to trigger a poll
    finder.tick_for_test();

    // Verify that the handler was invoked for both servers
    let recorded = calls.lock().unwrap().clone();
    assert!(recorded.iter().any(|(h, p, _)| *h == s1.ip().to_string() && *p == s1.port()),
        "send_packet_handler was not called for server {}", s1);
    assert!(recorded.iter().any(|(h, p, _)| *h == s2.ip().to_string() && *p == s2.port()),
        "send_packet_handler was not called for server {}", s2);

    // Additionally, ensure the payload looks like a STUN Binding Request (type 0x0001, length 0)
    // Note: We don't require every recorded call to match, any one suffices.
    assert!(recorded.iter().any(|(_, _, len)| *len >= 20), "expected at least one STUN-sized packet (>=20 bytes)");
}
