use openssl::ssl::{SslConnector, SslMethod, SslVerifyMode, SslVersion};
use std::net::UdpSocket;
use std::time::Duration;

use bingle_core::api::bingle_api::{BingleApi, Handle, StartOptions};
use bingle_core::api::bingle_api_impl::BingleApiImpl;
use bingle_core::engine::BingleAccessUnsafeForTests;

#[path = "../test_util.rs"]
pub mod test_util;

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
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn cbc_ciphers_vulnerable_to_lucky13_are_rejected() {
    test_util::init_test_logging();

    // 1) Setup Bingle node as server, bound to an OS-assigned loopback port.
    let server_opts = StartOptions {
        handle: Handle::from("server_lucky13_test"),
        algo_passphrase: Some(test_util::PASSPHRASE_RECEIVE.to_string()),
        static_ip: Some(test_util::loopback_addr(0)),
        dangerous_debug: false,
        ..StartOptions::new("".into())
    };
    let server_api = BingleApiImpl::new(&server_opts);
    server_api
        .access_unsafe_for_tests(|a| a.start(&server_opts))
        .expect("start server api");
    let server_addr = test_util::node_loopback_addr(&server_api);

    std::thread::sleep(Duration::from_millis(200));

    // 2) Create a raw OpenSSL client configured for CBC ciphers
    // These are vulnerable to Lucky13 if not mitigated with Encrypt-then-MAC or constant-time processing.
    // Modern security policies (like Mozilla Intermediate) reject them.
    let cbc_ciphers = [
        "AES128-SHA",
        "AES256-SHA",
        "ECDHE-RSA-AES128-SHA",
        "ECDHE-RSA-AES256-SHA",
    ];

    for cipher in cbc_ciphers {
        let mut connector_builder =
            SslConnector::builder(SslMethod::dtls()).expect("ssl connector builder");
        connector_builder
            .set_min_proto_version(Some(SslVersion::DTLS1_2))
            .expect("set min proto");
        connector_builder.set_security_level(0); // Allow us to try them

        if connector_builder.set_cipher_list(cipher).is_err() {
            println!(
                "Skipping cipher {} - not supported by local OpenSSL client",
                cipher
            );
            continue;
        }

        connector_builder.set_verify(SslVerifyMode::NONE);
        let connector = connector_builder.build();

        let socket = UdpSocket::bind("127.0.0.1:0").expect("bind socket");
        socket
            .set_read_timeout(Some(Duration::from_secs(1)))
            .expect("set read timeout");
        socket.connect(server_addr).expect("connect socket");
        let stream = UdpStream(socket);

        let res = connector.connect("localhost", stream);

        assert!(
            res.is_err(),
            "Handshake should have failed for CBC cipher {} which is vulnerable to Lucky13",
            cipher
        );
        let err_msg = format!("{:?}", res.err());
        assert!(
            err_msg.contains("handshake failure") || err_msg.contains("no shared cipher"),
            "Unexpected error for {}: {}",
            cipher,
            err_msg
        );
        println!("Confirmed: CBC cipher {} was rejected by server", cipher);
    }
}

#[test]
fn aead_ciphers_not_vulnerable_to_lucky13_are_accepted() {
    test_util::init_test_logging();

    // 1) Setup Bingle node as server
    let server_opts = StartOptions {
        handle: Handle::from("server_aead_test"),
        algo_passphrase: Some(test_util::PASSPHRASE_RECEIVE.to_string()),
        static_ip: Some(test_util::loopback_addr(0)),
        dangerous_debug: false,
        ..StartOptions::new("".into())
    };
    let server_api = BingleApiImpl::new(&server_opts);
    server_api
        .access_unsafe_for_tests(|a| a.start(&server_opts))
        .expect("start server api");
    let server_addr = test_util::node_loopback_addr(&server_api);

    std::thread::sleep(Duration::from_millis(200));

    // 2) Create a raw OpenSSL client configured for AEAD ciphers
    let aead_ciphers = [
        "ECDHE-RSA-AES128-GCM-SHA256",
        "ECDHE-RSA-AES256-GCM-SHA384",
        "ECDHE-RSA-CHACHA20-POLY1305",
    ];

    for cipher in aead_ciphers {
        let mut connector_builder =
            SslConnector::builder(SslMethod::dtls()).expect("ssl connector builder");
        connector_builder
            .set_min_proto_version(Some(SslVersion::DTLS1_2))
            .expect("set min proto");

        if connector_builder.set_cipher_list(cipher).is_err() {
            println!(
                "Skipping cipher {} - not supported by local OpenSSL client",
                cipher
            );
            continue;
        }

        connector_builder.set_verify(SslVerifyMode::NONE);
        let connector = connector_builder.build();

        let socket = UdpSocket::bind("127.0.0.1:0").expect("bind socket");
        socket
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("set read timeout");
        socket.connect(server_addr).expect("connect socket");
        let stream = UdpStream(socket);

        let res = connector.connect("localhost", stream);

        // Handshake might fail due to missing certificates, but it SHOULD NOT fail with "no shared cipher"
        if let Err(e) = res {
            let err_msg = format!("{:?}", e);
            assert!(
                !err_msg.contains("no shared cipher"),
                "AEAD cipher {} should be supported by server",
                cipher
            );
            println!(
                "AEAD cipher {} reached handshake, failed later as expected (likely certs): {}",
                cipher, err_msg
            );
        } else {
            println!("AEAD cipher {} handshake succeeded", cipher);
        }
    }
}

#[test]
fn default_negotiated_cipher_is_aead() {
    test_util::init_test_logging();

    // 1) Setup Bingle node as server
    let server_opts = StartOptions {
        handle: Handle::from("server_default_cipher_test"),
        algo_passphrase: Some(test_util::PASSPHRASE_RECEIVE.to_string()),
        static_ip: Some(test_util::loopback_addr(0)),
        dangerous_debug: false,
        ..StartOptions::new("".into())
    };
    let server_api = BingleApiImpl::new(&server_opts);
    server_api
        .access_unsafe_for_tests(|a| a.start(&server_opts))
        .expect("start server api");
    let server_addr = test_util::node_loopback_addr(&server_api);

    std::thread::sleep(Duration::from_millis(200));

    // 2) Create a raw OpenSSL client with default ciphers and proper certificates
    let ops = test_util::ops_from_mnemonic(
        test_util::ADDRESS_RECEIVE,
        test_util::PASSPHRASE_RECEIVE,
        test_util::localnet_config(),
    );
    let (ca_pem, _srv_crt, _srv_key, cli_crt, cli_key) =
        bingle_core::api::pki::generate_pki_from_ops(&ops).expect("pki gen");

    let mut connector_builder =
        SslConnector::builder(SslMethod::dtls()).expect("ssl connector builder");
    connector_builder
        .set_min_proto_version(Some(SslVersion::DTLS1_2))
        .expect("set min proto");
    connector_builder.set_verify(SslVerifyMode::NONE);

    let client_x509 = openssl::x509::X509::from_pem(&cli_crt).expect("client cert");
    let client_key = openssl::pkey::PKey::private_key_from_pem(&cli_key).expect("client key");
    connector_builder
        .set_certificate(&client_x509)
        .expect("set cert");
    connector_builder
        .set_private_key(&client_key)
        .expect("set key");
    let ca_x509 = openssl::x509::X509::from_pem(&ca_pem).expect("ca cert");
    connector_builder
        .add_extra_chain_cert(ca_x509)
        .expect("add ca cert");

    let connector = connector_builder.build();

    let socket = UdpSocket::bind("127.0.0.1:0").expect("bind socket");
    socket
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("set read timeout");
    socket.connect(server_addr).expect("connect socket");
    let stream = UdpStream(socket);

    let res = connector.connect("localhost", stream);

    match res {
        Ok(ssl_stream) => {
            let cipher = ssl_stream.ssl().current_cipher().expect("current cipher");
            let cipher_name = cipher.name().to_string();
            println!("Default negotiated cipher: {}", cipher_name);

            // Most AEAD ciphers in TLS 1.2 have "GCM" or "POLY1305" in their name
            assert!(
                cipher_name.contains("GCM") || cipher_name.contains("POLY1305"),
                "Default cipher {} is not an AEAD cipher, potentially vulnerable to Lucky13",
                cipher_name
            );
        }
        Err(e) => {
            panic!(
                "Handshake failed with default ciphers even with certificates: {:?}",
                e
            );
        }
    }
}
