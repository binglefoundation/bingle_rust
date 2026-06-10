use std::net::{SocketAddr, UdpSocket};
use std::time::Duration;
use openssl::ssl::{SslMethod, SslConnector, SslAcceptor, SslVerifyMode, SslVersion};
use openssl::rsa::Rsa;
use openssl::pkey::{PKey, Private};
use openssl::x509::X509;
use openssl::dh::Dh;

use rust_comms::api::bingle_api::{BingleApi, Handle, StartOptions};
use rust_comms::api::bingle_api_impl::BingleApiImpl;
use rust_comms::engine::BingleAccessUnsafeForTests;

#[path = "../test_util.rs"]
pub mod test_util;

#[path = "../dtls/pki.rs"]
pub mod pki;

#[derive(Debug)]
struct UdpStream(UdpSocket);
impl std::io::Read for UdpStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.0.recv(buf)
    }
}
impl std::io::Write for UdpStream {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.send(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
}

fn make_ee_rsa(ca_cert: &X509, ca_key: &PKey<Private>, cn: &str, bits: u32) -> (X509, PKey<Private>) {
    let rsa = Rsa::generate(bits).expect("rsa gen");
    let pkey = PKey::from_rsa(rsa).expect("pkey from rsa");

    let mut name = openssl::x509::X509NameBuilder::new().expect("name builder");
    name.append_entry_by_nid(openssl::nid::Nid::COMMONNAME, cn).expect("cn");
    let name = name.build();

    let mut builder = X509::builder().expect("x509 builder");
    builder.set_version(2).expect("version");
    
    let mut serial = openssl::bn::BigNum::new().expect("bignum");
    serial.rand(64, openssl::bn::MsbOption::MAYBE_ZERO, false).expect("rand");
    builder.set_serial_number(&serial.to_asn1_integer().expect("asn1 serial")).expect("serial");
    
    builder.set_subject_name(&name).expect("subject");
    builder.set_issuer_name(ca_cert.subject_name()).expect("issuer");
    builder.set_pubkey(&pkey).expect("pubkey");

    let not_before = openssl::asn1::Asn1Time::days_from_now(0).expect("not before");
    builder.set_not_before(&not_before).expect("set not before");
    let not_after = openssl::asn1::Asn1Time::days_from_now(2).expect("not after");
    builder.set_not_after(&not_after).expect("set not after");

    let bc = openssl::x509::extension::BasicConstraints::new().critical().build().expect("bc");
    builder.append_extension(bc).expect("append bc");
    let ku = openssl::x509::extension::KeyUsage::new()
        .critical()
        .digital_signature()
        .key_encipherment()
        .build()
        .expect("ku");
    builder.append_extension(ku).expect("append ku");
    let eku = openssl::x509::extension::ExtendedKeyUsage::new()
        .server_auth()
        .client_auth()
        .build()
        .expect("eku");
    builder.append_extension(eku).expect("append eku");

    builder.sign(ca_key, openssl::hash::MessageDigest::null()).expect("sign");
    (builder.build(), pkey)
}

#[test]
fn rsa_1024_client_cert_rejected() {
    test_util::init_test_logging();

    // 1) Setup Bingle node as server
    let server_port = test_util::find_unused_loopback_port();
    let server_addr = SocketAddr::new("127.0.0.1".parse().unwrap(), server_port);
    
    let server_opts = StartOptions {
        handle: Handle::from("server_rsa_rejection"),
        algo_passphrase: Some(test_util::PASSPHRASE_RECEIVE.to_string()),
        static_ip: Some(server_addr),
        dangerous_debug: false,
        ..StartOptions::new("".into())
    };
    let server_api = BingleApiImpl::new(&server_opts);
    server_api.access_unsafe_for_tests(|a| a.start(&server_opts)).expect("start server api");

    std::thread::sleep(Duration::from_millis(100));

    // 2) Generate RSA 1024 client certificate
    let (ca_cert, ca_key) = pki::make_ca(test_util::ADDRESS_RECEIVE);
    let (cli_cert, cli_key) = make_ee_rsa(&ca_cert, &ca_key, &format!("{}.", test_util::ADDRESS_RECEIVE), 1024);

    // 3) Create a raw OpenSSL client using RSA 1024 cert
    let mut connector_builder = SslConnector::builder(SslMethod::dtls()).unwrap();
    connector_builder.set_min_proto_version(Some(SslVersion::DTLS1_2)).unwrap();
    
    // We must lower security level on client to even USE 1024-bit key
    connector_builder.set_security_level(0);
    
    connector_builder.set_certificate(&cli_cert).unwrap();
    connector_builder.set_private_key(&cli_key).unwrap();
    connector_builder.set_verify(SslVerifyMode::NONE);
    
    let connector = connector_builder.build();
    
    // 4) Attempt handshake
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    socket.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    socket.connect(server_addr).unwrap();
    
    let stream = UdpStream(socket);
    
    let res = connector.connect("localhost", stream);
    
    // 5) Assert failure
    assert!(res.is_err(), "Handshake should have failed for 1024-bit RSA client certificate");
    let err_msg = format!("{:?}", res.err());
    println!("Handshake failed as expected for RSA 1024: {}", err_msg);
}

#[test]
fn dh_1024_rejected() {
    test_util::init_test_logging();

    // 1) Setup a raw OpenSSL server with 1024-bit DH params
    let server_port = test_util::find_unused_loopback_port();
    let server_addr = SocketAddr::new("127.0.0.1".parse().unwrap(), server_port);
    
    let certs = pki::generate_ed25519_test_certs();
    let srv_crt = X509::from_pem(&certs.server_crt).unwrap();
    let srv_key = PKey::private_key_from_pem(&certs.server_key).unwrap();

    let mut acceptor_builder = SslAcceptor::mozilla_intermediate_v5(SslMethod::dtls()).unwrap();
    // Lower security level on server to allow it to offer 1024-bit DH
    acceptor_builder.set_security_level(0);
    
    acceptor_builder.set_certificate(&srv_crt).unwrap();
    acceptor_builder.set_private_key(&srv_key).unwrap();
    
    // Set 1024-bit DH parameters
    let dh = Dh::get_1024_160().unwrap();
    acceptor_builder.set_tmp_dh(&dh).unwrap();
    
    // Force DHE to ensure DH is used
    acceptor_builder.set_cipher_list("DHE-RSA-AES128-GCM-SHA256").unwrap();
    
    let acceptor = acceptor_builder.build();
    
    let socket = UdpSocket::bind(server_addr).unwrap();
    socket.set_read_timeout(Some(Duration::from_millis(500))).unwrap();
    
    let server_thread = std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        if let Ok((len, addr)) = socket.recv_from(&mut buf) {
            let client_sock = UdpSocket::bind("127.0.0.1:0").unwrap();
            client_sock.connect(addr).unwrap();
            let _ = client_sock.send(&buf[..len]);
            let stream = UdpStream(client_sock);
            let _ = acceptor.accept(stream);
        }
    });

    // 2) Setup Bingle node as client
    let client_opts = StartOptions {
        handle: Handle::from("client_dh_rejection"),
        algo_passphrase: Some(test_util::PASSPHRASE_SPEND.to_string()),
        static_ip: Some("127.0.0.1:0".parse().unwrap()),
        dangerous_debug: false, 
        ..StartOptions::new("".into())
    };
    let client_api = BingleApiImpl::new(&client_opts);
    client_api.access_unsafe_for_tests(|a| a.start(&client_opts)).expect("start client api");

    std::thread::sleep(Duration::from_millis(100));

    // 3) Attempt send to server
    let nsk = rust_comms::api::bingle_api::NetworkEndpoint::new_direct(server_addr);
    let ok = client_api.send_message_to_network(&nsk, &test_util::ADDRESS_RECEIVE.to_string(), serde_json::json!({"test": 1}), None);
    
    // 4) Assert failure
    assert!(!ok.expect("send_message_to_network should succeed"), "Handshake should have failed for 1024-bit DH parameters");
    println!("Handshake failed as expected for DH 1024");
    
    let _ = server_thread.join();
}

#[test]
fn rsa_1024_server_cert_rejected() {
    test_util::init_test_logging();

    // 1) Setup a raw OpenSSL server with 1024-bit RSA cert
    let server_port = test_util::find_unused_loopback_port();
    let server_addr = SocketAddr::new("127.0.0.1".parse().unwrap(), server_port);
    
    let (ca_cert, ca_key) = pki::make_ca(test_util::ADDRESS_RECEIVE);
    let (srv_cert, srv_key) = make_ee_rsa(&ca_cert, &ca_key, &format!("{}.", test_util::ADDRESS_RECEIVE), 1024);

    let mut acceptor_builder = SslAcceptor::mozilla_intermediate_v5(SslMethod::dtls()).unwrap();
    acceptor_builder.set_security_level(0);
    acceptor_builder.set_certificate(&srv_cert).unwrap();
    acceptor_builder.set_private_key(&srv_key).unwrap();
    
    let acceptor = acceptor_builder.build();
    
    let socket = UdpSocket::bind(server_addr).unwrap();
    socket.set_read_timeout(Some(Duration::from_millis(500))).unwrap();
    
    let server_thread = std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        if let Ok((len, addr)) = socket.recv_from(&mut buf) {
            let client_sock = UdpSocket::bind("127.0.0.1:0").unwrap();
            client_sock.connect(addr).unwrap();
            let _ = client_sock.send(&buf[..len]);
            let stream = UdpStream(client_sock);
            let _ = acceptor.accept(stream);
        }
    });

    // 2) Setup Bingle node as client
    let client_opts = StartOptions {
        handle: Handle::from("client_rsa_rejection"),
        algo_passphrase: Some(test_util::PASSPHRASE_SPEND.to_string()),
        static_ip: Some("127.0.0.1:0".parse().unwrap()),
        dangerous_debug: false,
        ..StartOptions::new("".into())
    };
    let client_api = BingleApiImpl::new(&client_opts);
    client_api.access_unsafe_for_tests(|a| a.start(&client_opts)).expect("start client api");

    std::thread::sleep(Duration::from_millis(100));

    // 3) Attempt send
    let nsk = rust_comms::api::bingle_api::NetworkEndpoint::new_direct(server_addr);
    let ok = client_api.send_message_to_network(&nsk, &test_util::ADDRESS_RECEIVE.to_string(), serde_json::json!({"test": 1}), None);
    
    // 4) Assert failure
    assert!(!ok.expect("send_message_to_network should succeed"), "Handshake should have failed for 1024-bit RSA server certificate");
    println!("Handshake failed as expected for RSA 1024 server cert");
    
    let _ = server_thread.join();
}
