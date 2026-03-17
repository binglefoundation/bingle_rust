

use std::net::SocketAddr;
use std::sync::{Arc, OnceLock, atomic::{AtomicBool, Ordering}};
use std::time::{Duration, Instant};

use rust_comms::dtls::{Dtls, DtlsOpenSsl};
use rust_comms::protocol::{ISSUER_SUFFIX, VIRTUAL_CA};
use crate::util::test_util::init_test_logging;

#[cfg_attr(not(target_os = "ios"), test)]
pub fn dtls_peer_certificate_handler_issuer_is_trimmed_to_id() {
    // Build a minimal CA (Ed25519) with CN = VIRTUAL_CA and a server cert + client cert.
    use openssl::asn1::Asn1Time;
    use openssl::bn::BigNum;
    use openssl::hash::MessageDigest;
    use openssl::nid::Nid;
    use openssl::pkey::{PKey, Private};
    use openssl::rsa::Rsa;
    use openssl::x509::{X509NameBuilder, X509};

    init_test_logging();
    // CA key + cert (Ed25519, self-signed, CN=VIRTUAL_CA)
    let ca_pkey: PKey<Private> = PKey::generate_ed25519().expect("generate ed25519");
    let mut ca_name_b = X509NameBuilder::new().expect("name builder");
    ca_name_b.append_entry_by_nid(Nid::COMMONNAME, VIRTUAL_CA).expect("set CN");
    let ca_name = ca_name_b.build();
    let mut ca_builder = X509::builder().expect("x509 builder");
    let mut serial = BigNum::new().expect("serial");
    serial.rand(64, openssl::bn::MsbOption::MAYBE_ZERO, false).expect("serial gen");
    let serial = serial.to_asn1_integer().expect("serial asn1");
    ca_builder.set_version(2).expect("set version");
    ca_builder.set_serial_number(&serial).expect("set serial");
    ca_builder.set_subject_name(&ca_name).expect("set subject");
    ca_builder.set_issuer_name(&ca_name).expect("set issuer");
    ca_builder.set_pubkey(&ca_pkey).expect("set pubkey");
    let nb = Asn1Time::days_from_now(0).expect("nb");
    let na = Asn1Time::days_from_now(365).expect("na");
    ca_builder.set_not_before(&nb).expect("set nb");
    ca_builder.set_not_after(&na).expect("set na");
    ca_builder.sign(&ca_pkey, MessageDigest::null()).expect("sign ca");
    let ca_cert = ca_builder.build();
    let ca_pem = ca_cert.to_pem().expect("ca pem");

    // Server key (RSA-2048) + cert signed by CA
    let server_rsa = Rsa::generate(2048).expect("rsa gen");
    let server_key = PKey::from_rsa(server_rsa).expect("pkey from rsa");
    let mut server_name_b = X509NameBuilder::new().expect("srv name builder");
    server_name_b.append_entry_by_nid(Nid::COMMONNAME, "server.").expect("srv cn");
    let server_name = server_name_b.build();
    let mut server_b = X509::builder().expect("srv x509 builder");
    let mut s_serial = BigNum::new().expect("serial");
    s_serial.rand(64, openssl::bn::MsbOption::MAYBE_ZERO, false).expect("serial gen");
    let s_serial = s_serial.to_asn1_integer().expect("serial asn1");
    server_b.set_version(2).expect("set version");
    server_b.set_serial_number(&s_serial).expect("set serial");
    server_b.set_subject_name(&server_name).expect("set subj");
    server_b.set_issuer_name(ca_cert.subject_name()).expect("set issuer");
    server_b.set_pubkey(&server_key).expect("set pubkey");
    server_b.set_not_before(&nb).expect("set nb");
    server_b.set_not_after(&na).expect("set na");
    server_b.sign(&ca_pkey, MessageDigest::null()).expect("sign srv");
    let server_cert_pem = server_b.build().to_pem().expect("srv pem");
    let server_key_pem = server_key.private_key_to_pem_pkcs8().expect("srv key pem");

    // Client key (RSA-2048) + cert signed by CA, subject CN ends with ISSUER_SUFFIX
    let client_rsa = Rsa::generate(2048).expect("rsa gen");
    let client_key = PKey::from_rsa(client_rsa).expect("pkey from rsa");
    let id_without_suffix = "user";
    let client_cn = format!("{}{}", id_without_suffix, ISSUER_SUFFIX);
    let mut client_name_b = X509NameBuilder::new().expect("cli name builder");
    client_name_b.append_entry_by_nid(Nid::COMMONNAME, &client_cn).expect("cli cn");
    let client_name = client_name_b.build();
    let mut client_b = X509::builder().expect("cli x509 builder");
    let mut c_serial = BigNum::new().expect("serial");
    c_serial.rand(64, openssl::bn::MsbOption::MAYBE_ZERO, false).expect("serial gen");
    let c_serial = c_serial.to_asn1_integer().expect("serial asn1");
    client_b.set_version(2).expect("set version");
    client_b.set_serial_number(&c_serial).expect("set serial");
    client_b.set_subject_name(&client_name).expect("set subj");
    client_b.set_issuer_name(ca_cert.subject_name()).expect("set issuer");
    client_b.set_pubkey(&client_key).expect("set pubkey");
    client_b.set_not_before(&nb).expect("set nb");
    client_b.set_not_after(&na).expect("set na");
    client_b.sign(&ca_pkey, MessageDigest::null()).expect("sign client");
    let client_cert_pem = client_b.build().to_pem().expect("cli pem");
    let client_key_pem = client_key.private_key_to_pem_pkcs8().expect("cli key pem");

    // Start server mux and DTLS with peer_certificate_handler installed
    let mux0 = rust_comms::dtls::UdpNetworkMux::bind(("127.0.0.1", 0)).expect("bind mux");
    let mux = std::sync::Arc::new(mux0);
    let addr: SocketAddr = mux.local_addr().expect("mux addr");
    let (seen_flag, seen_issuer): (Arc<AtomicBool>, Arc<OnceLock<String>>) = (Arc::new(AtomicBool::new(false)), Arc::new(OnceLock::new()));

    let mut server = DtlsOpenSsl::new()
        .with_null_encryption()
        .with_handle_peer_certificate(rust_comms::protocol::cert_verify::peer_certificate_handler())
        .with_server_signing_cert(server_cert_pem.clone())
        .with_server_signing_private_key(server_key_pem.clone())
        .with_ca_cert(ca_pem.clone());

    // Install a message handler to capture the issuer string delivered to the app layer
    let flag_clone = seen_flag.clone();
    let issuer_store = seen_issuer.clone();
    server.set_handle_message(Some(Arc::new(move |_server, _from, issuer, _data| {
        let _ = issuer_store.set(issuer.to_string());
        flag_clone.store(true, Ordering::SeqCst);
    })));

    mux.start().expect("mux start");
    server.start(mux.clone()).expect("server start");

    // Start client mux and DTLS
    let cmux0 = rust_comms::dtls::UdpNetworkMux::bind(("127.0.0.1", 0)).expect("bind cmux");
    let cmux = std::sync::Arc::new(cmux0);
    cmux.start().expect("cmux start");
    let mut client = DtlsOpenSsl::new()
        .with_null_encryption()
        .with_handle_peer_certificate(rust_comms::protocol::cert_verify::peer_certificate_handler())
        .with_client_cert(client_cert_pem.clone())
        .with_client_private_key(client_key_pem.clone())
        .with_server_signing_cert(server_cert_pem.clone())
        .with_server_signing_private_key(server_key_pem.clone())
        .with_ca_cert(ca_pem.clone());
    client.start(cmux.clone()).expect("client start");

    // Send a small JSON payload to trigger delivery
    let payload = br#"{\"hello\":\"world\"}"#;
    let _ = client.send(&rust_comms::api::bingle_api::NetworkEndpoint::new_direct(addr), payload).expect("client send");

    // Wait for server to capture issuer
    let start = Instant::now();
    while !seen_flag.load(Ordering::SeqCst) && start.elapsed() < Duration::from_secs(5) {
        std::thread::sleep(Duration::from_millis(20));
    }
    let issuer_seen = seen_issuer.get().cloned();
    assert!(issuer_seen.is_some(), "issuer not delivered to app layer");
    assert_eq!(issuer_seen.unwrap(), id_without_suffix.to_string(), "issuer should be trimmed of ISSUER_SUFFIX");
}