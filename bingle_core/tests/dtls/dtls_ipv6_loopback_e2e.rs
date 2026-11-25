use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::util::pki;
use bingle_core::api::bingle_api::IpFamily;
use bingle_core::api::bingle_api::NetworkEndpoint;
use bingle_core::dtls::{Dtls, DtlsOpenSsl, UdpNetworkMux};

// The DTLS handlers are plain function pointers, so results travel through statics; the guard keeps
// this file's tests from sharing them concurrently.
static GUARD: Mutex<()> = Mutex::new(());
static CLIENT_ECHOED: Mutex<Option<Vec<u8>>> = Mutex::new(None);
static SERVER_SAW_FROM: Mutex<Option<SocketAddr>> = Mutex::new(None);

fn mock_peer_cert_handler(_cert: &[u8], _ca: &[u8]) -> bingle_core::dtls::Result<String> {
    Ok("MOCK-ISSUER".to_string())
}

fn echo_handler(server: &dyn Dtls, from: &NetworkEndpoint, _issuer: &str, data: &[u8]) {
    if let Some(first) = data.first()
        && (*first == 22 || *first == 23)
    {
        return;
    }
    *SERVER_SAW_FROM.lock().unwrap() = from.inet_socket_address();
    let mut echoed = b"ECHOED: ".to_vec();
    echoed.extend_from_slice(data);
    let _ = server.send(from, &echoed);
}

fn client_handler(_server: &dyn Dtls, _from: &NetworkEndpoint, _issuer: &str, data: &[u8]) {
    *CLIENT_ECHOED.lock().unwrap() = Some(data.to_vec());
}

/// Run a DTLS echo between a server on a mux of `server_family` and a client on a mux of
/// `client_family`, the client dialling the server's port on `dial_ip`. Returns the address the
/// server saw the client at.
fn echo_over(server_family: IpFamily, client_family: IpFamily, dial_ip: IpAddr) -> SocketAddr {
    *CLIENT_ECHOED.lock().unwrap() = None;
    *SERVER_SAW_FROM.lock().unwrap() = None;

    let certs = pki::generate_ed25519_test_certs();
    let mux = Arc::new(UdpNetworkMux::bind_all(0, server_family).expect("bind server mux"));
    let port = mux.local_addr().expect("server addr").port();
    let server = DtlsOpenSsl::new("server".to_string())
        .with_null_encryption()
        .with_handle_message(Arc::new(echo_handler))
        .with_server_signing_cert(certs.server_crt.clone())
        .with_server_signing_private_key(certs.server_key.clone())
        .with_ca_cert(certs.ca_crt.clone())
        .with_handle_peer_certificate(mock_peer_cert_handler);
    mux.start().expect("server mux start");
    server.start(mux.clone()).expect("server start");
    thread::sleep(Duration::from_millis(200));

    let certs_b = pki::generate_ed25519_test_certs();
    let client = DtlsOpenSsl::new("client".to_string())
        .with_null_encryption()
        .with_handle_message(Arc::new(client_handler))
        .with_client_cert(certs_b.client_crt.clone())
        .with_client_private_key(certs_b.client_key.clone())
        .with_server_signing_cert(certs_b.server_crt.clone())
        .with_server_signing_private_key(certs_b.server_key.clone())
        .with_ca_cert(certs_b.ca_crt.clone())
        .with_handle_peer_certificate(mock_peer_cert_handler);
    let cmux = Arc::new(UdpNetworkMux::bind_all(0, client_family).expect("bind client mux"));
    cmux.start().expect("client mux start");
    client.start(cmux.clone()).expect("client start");

    let payload = b"ipv6-echo-payload";
    let to = NetworkEndpoint::new_direct(SocketAddr::new(dial_ip, port));
    let sent = (0..5).any(|_| {
        let ok = client.send(&to, payload).is_ok();
        if !ok {
            thread::sleep(Duration::from_millis(50));
        }
        ok
    });
    assert!(sent, "client DTLS send failed");

    let start = Instant::now();
    while CLIENT_ECHOED.lock().unwrap().is_none() && start.elapsed() < Duration::from_secs(5) {
        thread::sleep(Duration::from_millis(10));
    }
    let echoed = CLIENT_ECHOED
        .lock()
        .unwrap()
        .clone()
        .expect("no echo within timeout");
    let mut expected = b"ECHOED: ".to_vec();
    expected.extend_from_slice(payload);
    assert_eq!(echoed, expected);
    let saw = SERVER_SAW_FROM
        .lock()
        .unwrap()
        .expect("server recorded the sender");

    let _ = client.stop();
    let _ = server.stop();
    cmux.stop();
    mux.stop();
    saw
}

#[ntest::timeout(30_000)]
#[test]
#[cfg(not(target_os = "ios"))]
pub fn dtls_session_over_ipv6_loopback() {
    let _g = GUARD.lock().unwrap_or_else(|e| e.into_inner());
    let saw = echo_over(IpFamily::V6, IpFamily::V6, IpAddr::V6(Ipv6Addr::LOCALHOST));
    assert!(saw.is_ipv6(), "server saw an IPv6 peer: {saw}");
}

#[ntest::timeout(30_000)]
#[test]
#[cfg(not(target_os = "ios"))]
pub fn dual_stack_server_serves_ipv4_and_ipv6_clients() {
    let _g = GUARD.lock().unwrap_or_else(|e| e.into_inner());
    let saw6 = echo_over(
        IpFamily::Auto,
        IpFamily::Auto,
        IpAddr::V6(Ipv6Addr::LOCALHOST),
    );
    assert!(saw6.is_ipv6(), "IPv6 client seen as IPv6: {saw6}");
    let saw4 = echo_over(
        IpFamily::Auto,
        IpFamily::V4,
        IpAddr::V4(Ipv4Addr::LOCALHOST),
    );
    assert!(
        saw4.is_ipv4(),
        "IPv4 client seen as plain IPv4 by the dual-stack server, not ::ffff: ({saw4})"
    );
}
