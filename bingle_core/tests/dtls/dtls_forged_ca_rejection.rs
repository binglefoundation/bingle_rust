// A DTLS handshake must fail when a peer presents a CA that claims an id whose key it doesn't
// hold. Honest nodes use the PKI the app generates from an account; the other side uses the test
// PKI, whose CA key is random but whose O field names a real id.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::util::pki;
use crate::util::test_util::{
    ADDRESS_RECEIVE, ADDRESS_SPEND, PASSPHRASE_RECEIVE, PASSPHRASE_SPEND, init_test_logging,
};
use algo_ops::AlgoOps;
use bingle_core::api::bingle_api::NetworkEndpoint;
use bingle_core::api::pki::generate_pki_from_ops;
use bingle_core::dtls::{Dtls, DtlsOpenSsl, Result as DtlsResult, UdpNetworkMux};
use bingle_core::protocol::cert_verify::peer_certificate_handler;

/// PEMs for one side of a handshake.
pub struct Creds {
    pub ca: Vec<u8>,
    pub server_crt: Vec<u8>,
    pub server_key: Vec<u8>,
    pub client_crt: Vec<u8>,
    pub client_key: Vec<u8>,
}

/// Credentials the app generates for the account behind `address`.
pub fn genuine(passphrase: &str, address: &str) -> Creds {
    let ops = AlgoOps::new_for_algorand(
        Some(passphrase.to_string()),
        Some(address.to_string()),
        None,
    );
    let (ca, server_crt, server_key, client_crt, client_key) =
        generate_pki_from_ops(&ops).expect("generate pki");
    Creds {
        ca,
        server_crt,
        server_key,
        client_crt,
        client_key,
    }
}

/// Credentials that claim `address` from a CA key unrelated to it.
fn forged(address: &str) -> Creds {
    let c = pki::generate_forged_test_certs(address);
    Creds {
        ca: c.ca_crt,
        server_crt: c.server_crt,
        server_key: c.server_key,
        client_crt: c.client_crt,
        client_key: c.client_key,
    }
}

fn accept_all(_cert_pem: &[u8], _ca_pem: &[u8]) -> DtlsResult<String> {
    Ok(format!("{}.", ADDRESS_SPEND))
}

/// Ids the server's app layer saw messages from.
pub type Seen = Arc<Mutex<Vec<String>>>;

pub fn start_server(
    creds: &Creds,
    handler: bingle_core::dtls::HandlePeerCertificate,
) -> (DtlsOpenSsl, Arc<UdpNetworkMux>, SocketAddr, Seen) {
    let mux = Arc::new(UdpNetworkMux::bind(("127.0.0.1", 0)).expect("bind mux"));
    let addr = mux.local_addr().expect("mux addr");
    let seen: Seen = Arc::new(Mutex::new(Vec::new()));
    let seen_clone = seen.clone();
    let server = DtlsOpenSsl::new("server".to_string())
        .with_handle_peer_certificate(handler)
        .with_server_signing_cert(creds.server_crt.clone())
        .with_server_signing_private_key(creds.server_key.clone())
        .with_ca_cert(creds.ca.clone())
        .with_handle_message(Arc::new(move |_server, _from, issuer, _data| {
            seen_clone.lock().unwrap().push(issuer.to_string());
        }));
    mux.start().expect("mux start");
    server.start(mux.clone()).expect("server start");
    (server, mux, addr, seen)
}

pub fn start_client(
    creds: &Creds,
    handler: bingle_core::dtls::HandlePeerCertificate,
) -> (DtlsOpenSsl, Arc<UdpNetworkMux>) {
    let mux = Arc::new(UdpNetworkMux::bind(("127.0.0.1", 0)).expect("bind client mux"));
    mux.start().expect("client mux start");
    let client = DtlsOpenSsl::new("client".to_string())
        .with_handle_peer_certificate(handler)
        .with_client_cert(creds.client_crt.clone())
        .with_client_private_key(creds.client_key.clone())
        .with_server_signing_cert(creds.server_crt.clone())
        .with_server_signing_private_key(creds.server_key.clone())
        .with_ca_cert(creds.ca.clone());
    client.start(mux.clone()).expect("client start");
    (client, mux)
}

/// Send a few times, then wait up to `wait` for the server to see a message.
pub fn send_and_wait(client: &DtlsOpenSsl, addr: SocketAddr, seen: &Seen, wait: Duration) {
    for _ in 0..6 {
        let _ = client.send(&NetworkEndpoint::new_direct(addr), b"hello");
        thread::sleep(Duration::from_millis(50));
    }
    let start = Instant::now();
    while start.elapsed() < wait {
        if !seen.lock().unwrap().is_empty() {
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
}

/// Control: two honest nodes connect, and the server sees the client's id.
#[ntest::timeout(30_000)]
#[test]
#[cfg(not(target_os = "ios"))]
pub fn dtls_genuine_peers_connect_with_verified_ids() {
    init_test_logging();
    let server_creds = genuine(PASSPHRASE_RECEIVE, ADDRESS_RECEIVE);
    let (_server, _smux, addr, seen) = start_server(&server_creds, peer_certificate_handler());
    let client_creds = genuine(PASSPHRASE_SPEND, ADDRESS_SPEND);
    let (client, _cmux) = start_client(&client_creds, peer_certificate_handler());

    send_and_wait(&client, addr, &seen, Duration::from_secs(5));
    assert_eq!(
        seen.lock().unwrap().first().cloned(),
        Some(ADDRESS_SPEND.to_string())
    );
}

/// The server must refuse a client whose CA claims ADDRESS_SPEND without its key.
#[ntest::timeout(30_000)]
#[test]
#[cfg(not(target_os = "ios"))]
pub fn dtls_server_rejects_client_with_forged_ca() {
    init_test_logging();
    let server_creds = genuine(PASSPHRASE_RECEIVE, ADDRESS_RECEIVE);
    let (_server, _smux, addr, seen) = start_server(&server_creds, peer_certificate_handler());
    let client_creds = forged(ADDRESS_SPEND);
    let (client, _cmux) = start_client(&client_creds, accept_all);

    send_and_wait(&client, addr, &seen, Duration::from_secs(3));
    let seen = seen.lock().unwrap().clone();
    assert!(
        seen.is_empty(),
        "server delivered messages from a forged id: {:?}",
        seen
    );
}

/// The client must refuse a server whose CA claims ADDRESS_RECEIVE without its key, and send it
/// nothing.
#[ntest::timeout(30_000)]
#[test]
#[cfg(not(target_os = "ios"))]
pub fn dtls_client_rejects_server_with_forged_ca() {
    init_test_logging();
    let server_creds = forged(ADDRESS_RECEIVE);
    let (_server, _smux, addr, seen) = start_server(&server_creds, accept_all);
    let client_creds = genuine(PASSPHRASE_SPEND, ADDRESS_SPEND);
    let (client, _cmux) = start_client(&client_creds, peer_certificate_handler());

    send_and_wait(&client, addr, &seen, Duration::from_secs(3));
    let seen = seen.lock().unwrap().clone();
    assert!(
        seen.is_empty(),
        "client sent application data to a server with a forged id: {:?}",
        seen
    );
}

/// With handshake verification off (a debug-only mode), the client's post-connect check must still refuse a server
/// with a forged CA and close the session before sending.
#[ntest::timeout(30_000)]
#[test]
#[cfg(not(target_os = "ios"))]
pub fn dtls_client_post_connect_check_rejects_server_with_forged_ca() {
    init_test_logging();
    let server_creds = forged(ADDRESS_RECEIVE);
    let (_server, _smux, addr, seen) = start_server(&server_creds, accept_all);
    let client_creds = genuine(PASSPHRASE_SPEND, ADDRESS_SPEND);
    let (client, _cmux) = start_client(&client_creds, peer_certificate_handler());
    // App-layer-only verification is a debug-only mode.
    client.set_dangerous_debug(true);
    client.set_app_layer_only_verification(true);

    send_and_wait(&client, addr, &seen, Duration::from_secs(3));
    let seen = seen.lock().unwrap().clone();
    assert!(
        seen.is_empty(),
        "client sent application data to a server with a forged id: {:?}",
        seen
    );
}
