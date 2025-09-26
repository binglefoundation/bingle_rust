#![cfg(not(target_os = "ios"))]

use std::net::{SocketAddr, UdpSocket};
use std::sync::{OnceLock, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use rust_comms::dtls::{Dtls, DtlsOpenSsl, Result as DtlsResult};

#[path = "pki.rs"]
mod pki;

// Helpers to parse CN from a PEM cert
fn extract_subject_cn(pem: &[u8]) -> String {
    use openssl::x509::X509;
    use openssl::nid::Nid;
    let cert = X509::from_pem(pem).expect("parse pem");
    cert.subject_name()
        .entries_by_nid(Nid::COMMONNAME)
        .next()
        .and_then(|e| e.data().as_utf8().ok())
        .map(|s| s.to_string())
        .unwrap_or_default()
}

// Server-side peer certificate handler: return the presented certificate's subject CN.
fn server_peer_cert_return_cn(cert_pem: &[u8], _ca_pem: &[u8]) -> DtlsResult<String> {
    Ok(extract_subject_cn(cert_pem))
}

// Client-side peer certificate handler: return the presented certificate's subject CN.
fn client_peer_cert_return_cn(cert_pem: &[u8], _ca_pem: &[u8]) -> DtlsResult<String> {
    Ok(extract_subject_cn(cert_pem))
}

static SERVER_SEEN_ISSUER: OnceLock<String> = OnceLock::new();
static CLIENT_SEEN_ISSUER: OnceLock<String> = OnceLock::new();
static SERVER_SEEN_DATA: OnceLock<Vec<u8>> = OnceLock::new();
static CLIENT_SEEN_DATA: OnceLock<Vec<u8>> = OnceLock::new();

fn server_assert_and_reply(server: &dyn Dtls, from: &SocketAddr, issuer: &str, data: &[u8]) {
    // Record issuer and data we saw
    let _ = SERVER_SEEN_ISSUER.set(issuer.to_string());
    let _ = SERVER_SEEN_DATA.set(data.to_vec());
    // Reply
    let _ = server.send(*from, "", b"hi-from-server");
}

fn client_capture(_server: &dyn Dtls, _from: &SocketAddr, issuer: &str, data: &[u8]) {
    let _ = CLIENT_SEEN_ISSUER.set(issuer.to_string());
    let _ = CLIENT_SEEN_DATA.set(data.to_vec());
}

#[test]
fn issuer_mapping_basic_send_and_reply() {
    // Generate a normal server cert/key/ca and a client cert/key; we'll override CN for client to "A"
    let certs = pki::generate_ed25519_test_certs();
    let server_cert_pem: Vec<u8> = certs.server_crt.clone();
    let server_key_pem: Vec<u8> = certs.server_key.clone();
    let ca_pem: Vec<u8> = certs.ca_crt.clone();

    // Build server
    let mut server = DtlsOpenSsl::new()
        .with_handle_peer_certificate(server_peer_cert_return_cn)
        .with_handle_message(server_assert_and_reply)
        .with_server_signing_cert(server_cert_pem.clone())
        .with_server_signing_private_key(server_key_pem.clone())
        .with_ca_cert(ca_pem.clone());

    // Pick a free UDP port
    let probe = UdpSocket::bind(("127.0.0.1", 0)).expect("bind probe");
    let port = probe.local_addr().unwrap().port();
    drop(probe);
    let addr: SocketAddr = SocketAddr::from(([127, 0, 0, 1], port));

    server.start(addr, None).expect("server start");
    thread::sleep(Duration::from_millis(200));

    // Client uses provided client cert (CN may be anything); issuer mapping will return server CN on client side and client CN on server side
    let client = DtlsOpenSsl::new()
        .with_handle_peer_certificate(client_peer_cert_return_cn)
        .with_handle_message(client_capture)
        .with_client_cert(certs.client_crt.clone())
        .with_client_private_key(certs.client_key.clone())
        .with_ca_cert(ca_pem.clone());

    let server_cn = extract_subject_cn(&server_cert_pem);
    let client_cn = extract_subject_cn(&certs.client_crt);

    // Send from client to server
    let mut ok = false;
    for _ in 0..8 {
        if client.send(addr, "", b"hello").is_ok() { ok = true; break; }
        thread::sleep(Duration::from_millis(50));
    }
    assert!(ok, "client send failed");

    // Wait for server to see the message
    let start = Instant::now();
    while SERVER_SEEN_DATA.get().is_none() && start.elapsed() < Duration::from_secs(3) {
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(SERVER_SEEN_ISSUER.get().cloned().unwrap_or_default(), client_cn, "server did not map client's issuer correctly");

    // Wait for client to see reply
    let start2 = Instant::now();
    while CLIENT_SEEN_DATA.get().is_none() && start2.elapsed() < Duration::from_secs(3) {
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(CLIENT_SEEN_ISSUER.get().cloned().unwrap_or_default(), server_cn, "client did not map server's issuer correctly");
}

// -------------- Multiple clients A,B,C -> Z ---------------
static SERVER_ALL_ISSUERS: OnceLock<Mutex<Vec<String>>> = OnceLock::new();

fn server_collect_issuers(_server: &dyn Dtls, _from: &SocketAddr, issuer: &str, data: &[u8]) {
    if !data.is_empty() {
        let v = SERVER_ALL_ISSUERS.get_or_init(|| Mutex::new(Vec::new()));
        if let Ok(mut list) = v.lock() { list.push(issuer.to_string()); }
    }
}

fn make_self_signed_rsa_cert_with_cn(cn: &str) -> (Vec<u8>, Vec<u8>) {
    use openssl::x509::{X509, X509NameBuilder};
    use openssl::nid::Nid;
    use openssl::pkey::PKey;
    use openssl::rsa::Rsa;
    use openssl::hash::MessageDigest;
    use openssl::asn1::Asn1Time;

    let rsa = Rsa::generate(2048).expect("rsa gen");
    let pkey = PKey::from_rsa(rsa).expect("pkey");

    let mut name = X509NameBuilder::new().unwrap();
    name.append_entry_by_nid(Nid::COMMONNAME, cn).unwrap();
    let name = name.build();

    let mut b = X509::builder().unwrap();
    b.set_version(2).unwrap();
    b.set_subject_name(&name).unwrap();
    b.set_issuer_name(&name).unwrap();
    b.set_pubkey(&pkey).unwrap();
    let nb = Asn1Time::days_from_now(0).unwrap();
    b.set_not_before(&nb).unwrap();
    let na = Asn1Time::days_from_now(365).unwrap();
    b.set_not_after(&na).unwrap();
    b.sign(&pkey, MessageDigest::sha256()).unwrap();
    let cert = b.build();
    let cert_pem = cert.to_pem().unwrap();
    let key_pem = pkey.private_key_to_pem_pkcs8().unwrap();
    (cert_pem, key_pem)
}

#[test]
fn multiple_clients_to_server_have_correct_issuers() {
    // Server using normal certs; peer cert handler extracts CN from client certs
    let certs = pki::generate_ed25519_test_certs();
    let server_cert_pem: Vec<u8> = certs.server_crt.clone();
    let server_key_pem: Vec<u8> = certs.server_key.clone();
    let ca_pem: Vec<u8> = certs.ca_crt.clone();

    // Server
    let mut server = DtlsOpenSsl::new()
        .with_handle_peer_certificate(server_peer_cert_return_cn)
        .with_handle_message(server_collect_issuers)
        .with_server_signing_cert(server_cert_pem.clone())
        .with_server_signing_private_key(server_key_pem.clone())
        .with_ca_cert(ca_pem.clone());

    // Port
    let probe = UdpSocket::bind(("127.0.0.1", 0)).expect("bind probe");
    let port = probe.local_addr().unwrap().port();
    drop(probe);
    let addr: SocketAddr = SocketAddr::from(([127, 0, 0, 1], port));

    server.start(addr, None).expect("server start");
    thread::sleep(Duration::from_millis(200));

    // Create three distinct client certs with CN A,B,C
    let (a_crt, a_key) = make_self_signed_rsa_cert_with_cn("A");
    let (b_crt, b_key) = make_self_signed_rsa_cert_with_cn("B");
    let (c_crt, c_key) = make_self_signed_rsa_cert_with_cn("C");

    // Clients (each extracts server CN for mapping on their side, but we only assert server-side issuers here)
    let client_a = DtlsOpenSsl::new().with_handle_peer_certificate(client_peer_cert_return_cn).with_client_cert(a_crt.clone()).with_client_private_key(a_key.clone()).with_ca_cert(ca_pem.clone());
    let client_b = DtlsOpenSsl::new().with_handle_peer_certificate(client_peer_cert_return_cn).with_client_cert(b_crt.clone()).with_client_private_key(b_key.clone()).with_ca_cert(ca_pem.clone());
    let client_c = DtlsOpenSsl::new().with_handle_peer_certificate(client_peer_cert_return_cn).with_client_cert(c_crt.clone()).with_client_private_key(c_key.clone()).with_ca_cert(ca_pem.clone());

    // Send messages from A, B, C
    let mut ok = false; for _ in 0..6 { if client_a.send(addr, "", b"mA").is_ok() { ok = true; break; } thread::sleep(Duration::from_millis(50)); } assert!(ok);
    thread::sleep(Duration::from_millis(200));
    let mut ok = false; for _ in 0..10 { if client_b.send(addr, "", b"mB").is_ok() { ok = true; break; } thread::sleep(Duration::from_millis(50)); } assert!(ok);
    thread::sleep(Duration::from_millis(200));
    let mut ok = false; for _ in 0..10 { if client_c.send(addr, "", b"mC").is_ok() { ok = true; break; } thread::sleep(Duration::from_millis(50)); } assert!(ok);

    // Wait and assert issuers collected
    let start = Instant::now();
    while SERVER_ALL_ISSUERS.get().and_then(|m| m.lock().ok()).map(|v| v.len()).unwrap_or(0) < 3 && start.elapsed() < Duration::from_secs(4) {
        thread::sleep(Duration::from_millis(50));
    }
    let issuers: Vec<String> = SERVER_ALL_ISSUERS.get().and_then(|m| m.lock().ok()).map(|v| v.clone()).unwrap_or_default();
    assert!(issuers.iter().any(|s| s == "A"), "missing issuer A");
    assert!(issuers.iter().any(|s| s == "B"), "missing issuer B");
    assert!(issuers.iter().any(|s| s == "C"), "missing issuer C");
}
