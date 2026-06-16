use std::net::{SocketAddr, UdpSocket};
use std::sync::{Mutex, OnceLock, Arc};
use std::time::Duration;

use rust_comms::dtls::{UdpNetworkMux, NetworkMux};

static TEST_GUARD: OnceLock<Mutex<()>> = OnceLock::new();
static REJECTION_RECORDS: OnceLock<Mutex<Vec<(SocketAddr, Vec<u8>)>>> = OnceLock::new();

fn clear_records() {
    if let Some(m) = REJECTION_RECORDS.get() { m.lock().unwrap().clear(); }
}

#[test]
#[cfg(not(target_os = "ios"))]
fn rejects_ipv6_packets() {
    let _g = TEST_GUARD.get_or_init(|| Mutex::new(())).lock().unwrap();
    clear_records();

    // Try to bind to IPv6 loopback. If it fails (no IPv6 support), skip the test.
    let mux_result = UdpNetworkMux::bind("::1:0");
    let mux_inner = match mux_result {
        Ok(m) => m.with_handle_stun(Arc::new(|_src, from, data| {
             let m = REJECTION_RECORDS.get_or_init(|| Mutex::new(Vec::new()));
             m.lock().unwrap().push((*from, data.to_vec()));
        })),
        Err(_) => {
            eprintln!("Skipping test: could not bind to [::1]:0");
            return;
        }
    };
    let mux = Arc::new(mux_inner);
    let addr = mux.local_addr().unwrap();
    mux.start().expect("start mux");

    // Sender socket on IPv6
    let sender = UdpSocket::bind("[::1]:0").expect("bind sender");
    
    // Send a "STUN" packet (starts with 0)
    sender.send_to(&[0, 1, 2], addr).unwrap();

    // Wait briefly
    std::thread::sleep(Duration::from_millis(200));
    
    let count = REJECTION_RECORDS.get().map(|m| m.lock().unwrap().len()).unwrap_or(0);
    
    mux.stop();
    
    // BEFORE FIX: this should fail because count will be 1
    // AFTER FIX: this should pass because count will be 0
    assert_eq!(count, 0, "IPv6 packet should have been rejected");
}
