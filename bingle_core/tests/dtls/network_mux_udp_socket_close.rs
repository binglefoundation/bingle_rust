use std::net::{SocketAddr, UdpSocket};
use std::sync::Arc;
use std::time::Duration;

use bingle_core::dtls::UdpNetworkMux;

/// After stop(), the socket should be closed (taken out of the Mutex).
#[ntest::timeout(30_000)]
#[test]
#[cfg(not(target_os = "ios"))]
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
#[test]
#[cfg(not(target_os = "ios"))]
pub fn port_is_freed_after_stop() {
    let mux = Arc::new(UdpNetworkMux::bind(("127.0.0.1", 0)).expect("bind mux"));
    let addr = mux.local_addr().expect("local_addr should succeed");

    mux.start().expect("start mux");
    mux.stop();

    // The port should now be free — rebind should succeed
    let rebind = UdpSocket::bind(addr);
    assert!(
        rebind.is_ok(),
        "port {} should be free after stop, but rebind failed: {:?}",
        addr.port(),
        rebind.err()
    );
}

/// After the socket is closed, write() should return an error.
#[ntest::timeout(30_000)]
#[test]
#[cfg(not(target_os = "ios"))]
pub fn write_fails_after_socket_closed() {
    let mux = Arc::new(UdpNetworkMux::bind(("127.0.0.1", 0)).expect("bind mux"));
    mux.start().expect("start mux");
    mux.stop();

    assert!(mux.is_closed(), "socket should be closed after stop");

    let target = "127.0.0.1:9999".parse::<SocketAddr>().unwrap();
    let nsk = bingle_core::api::bingle_api::NetworkEndpoint::new_direct(target);
    let result = bingle_core::dtls::NetworkMux::write(mux.as_ref(), &nsk, b"hello");
    assert!(result.is_err(), "write should fail after socket is closed");
    let err_msg = result.unwrap_err();
    assert!(
        err_msg.contains("closed"),
        "error should mention closed: {}",
        err_msg
    );
}

/// local_addr() should return an error after the socket is closed.
#[ntest::timeout(30_000)]
#[test]
#[cfg(not(target_os = "ios"))]
pub fn local_addr_fails_after_socket_closed() {
    let mux = Arc::new(UdpNetworkMux::bind(("127.0.0.1", 0)).expect("bind mux"));
    let _ = mux
        .local_addr()
        .expect("local_addr should succeed before start");

    mux.start().expect("start mux");
    mux.stop();

    let result = mux.local_addr();
    assert!(
        result.is_err(),
        "local_addr should fail after socket closed"
    );
}

/// is_closed() should return false for a freshly bound mux that was never started.
#[test]
#[cfg(not(target_os = "ios"))]
pub fn is_closed_false_before_start() {
    let mux = UdpNetworkMux::bind(("127.0.0.1", 0)).expect("bind mux");
    assert!(!mux.is_closed(), "freshly bound mux should not be closed");
}

/// After a receive error drops the socket, rebind() recovers it on the same port.
#[ntest::timeout(30_000)]
#[test]
#[cfg(not(target_os = "ios"))]
pub fn rebind_recovers_socket_on_same_port() {
    // Regression: a transient network error must not tear the transport down
    // permanently. The receive loop drops the dead socket (freeing the port)
    // and rebinds; this exercises that rebind directly.
    let mux = UdpNetworkMux::bind("127.0.0.1:0").expect("bind");
    let port = mux.local_addr().expect("local_addr").port();

    // Simulate the receive loop's teardown after a recv error: drop the live
    // socket so the port is free to rebind.
    mux.drop_socket_for_tests();
    assert!(mux.is_closed(), "socket should be closed after take");

    // Rebind succeeds on the same port and installs a live socket again.
    let loop_sock = mux.rebind_for_tests().expect("rebind should succeed");
    loop_sock
        .set_read_timeout(Some(Duration::from_millis(500)))
        .unwrap();
    assert!(!mux.is_closed(), "socket should be live after rebind");
    assert_eq!(
        mux.local_addr().unwrap().port(),
        port,
        "rebind should reuse the same port"
    );

    // The rebound socket actually receives traffic on that port.
    let sender = UdpSocket::bind("127.0.0.1:0").unwrap();
    sender
        .send_to(b"ping", ("127.0.0.1", port))
        .expect("send_to");
    let mut buf = [0u8; 16];
    let (n, _from) = loop_sock.recv_from(&mut buf).expect("recv after rebind");
    assert_eq!(
        &buf[..n],
        b"ping",
        "rebound socket should receive datagrams"
    );
}
