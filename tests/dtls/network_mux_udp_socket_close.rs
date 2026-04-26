use std::net::{SocketAddr, UdpSocket};
use std::sync::Arc;

use rust_comms::dtls::UdpNetworkMux;

/// After stop(), the socket should be closed (taken out of the Mutex).
#[ntest::timeout(30_000)]
#[cfg_attr(not(target_os = "ios"), test)]
pub fn socket_is_closed_after_stop() {
    let mux = Arc::new(UdpNetworkMux::bind(("127.0.0.1", 0)).expect("bind mux"));
    assert!(!mux.is_closed(), "socket should be open before start");

    mux.start().expect("start mux");
    assert!(!mux.is_closed(), "socket should be open while running");

    mux.stop();
    assert!(mux.is_closed(), "socket should be closed after stop");
}

/// After stop(), the port should be freed and re-bindable.
#[ntest::timeout(30_000)]
#[cfg_attr(not(target_os = "ios"), test)]
pub fn port_is_freed_after_stop() {
    let mux = Arc::new(UdpNetworkMux::bind(("127.0.0.1", 0)).expect("bind mux"));
    let addr = mux.local_addr().expect("local_addr should succeed");

    mux.start().expect("start mux");
    mux.stop();

    // The port should now be free — rebind should succeed
    let rebind = UdpSocket::bind(addr);
    assert!(rebind.is_ok(), "port {} should be free after stop, but rebind failed: {:?}", addr.port(), rebind.err());
}

/// After the socket is closed, write() should return an error.
#[ntest::timeout(30_000)]
#[cfg_attr(not(target_os = "ios"), test)]
pub fn write_fails_after_socket_closed() {
    let mux = Arc::new(UdpNetworkMux::bind(("127.0.0.1", 0)).expect("bind mux"));
    mux.start().expect("start mux");
    mux.stop();

    assert!(mux.is_closed(), "socket should be closed after stop");

    let target = "127.0.0.1:9999".parse::<SocketAddr>().unwrap();
    let nsk = rust_comms::api::bingle_api::NetworkEndpoint::new_direct(target);
    let result = rust_comms::dtls::NetworkMux::write(mux.as_ref(), &nsk, b"hello");
    assert!(result.is_err(), "write should fail after socket is closed");
    let err_msg = result.unwrap_err();
    assert!(err_msg.contains("closed"), "error should mention closed: {}", err_msg);
}

/// local_addr() should return an error after the socket is closed.
#[ntest::timeout(30_000)]
#[cfg_attr(not(target_os = "ios"), test)]
pub fn local_addr_fails_after_socket_closed() {
    let mux = Arc::new(UdpNetworkMux::bind(("127.0.0.1", 0)).expect("bind mux"));
    let addr = mux.local_addr().expect("local_addr should succeed before start");

    mux.start().expect("start mux");
    mux.stop();

    let result = mux.local_addr();
    assert!(result.is_err(), "local_addr should fail after socket closed");
}

/// is_closed() should return false for a freshly bound mux that was never started.
#[cfg_attr(not(target_os = "ios"), test)]
pub fn is_closed_false_before_start() {
    let mux = UdpNetworkMux::bind(("127.0.0.1", 0)).expect("bind mux");
    assert!(!mux.is_closed(), "freshly bound mux should not be closed");
}
