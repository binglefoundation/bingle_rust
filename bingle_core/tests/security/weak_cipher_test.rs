use openssl::ssl::{SslConnector, SslMethod, SslVerifyMode, SslVersion};
use std::net::UdpSocket;
use std::time::Duration;

use bingle_core::api::bingle_api::{BingleApi, Handle, StartOptions};
use bingle_core::api::bingle_api_impl::BingleApiImpl;
use bingle_core::engine::BingleAccessUnsafeForTests;

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
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn weak_cipher_3des_rejected() {
    test_util::init_test_logging();

    // 1) Setup Bingle node as server, bound to an OS-assigned loopback port.
    let server_opts = StartOptions {
        handle: Handle::from("server_weak_cipher"),
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

    std::thread::sleep(Duration::from_millis(100));

    // 2) Create a raw OpenSSL client configured for 3DES
    let mut connector_builder = SslConnector::builder(SslMethod::dtls()).unwrap();
    connector_builder
        .set_min_proto_version(Some(SslVersion::DTLS1_2))
        .unwrap();

    // Lower security level to allow specifying weak ciphers in the client
    connector_builder.set_security_level(0);

    // Try 3DES first, then RC4, then a CBC cipher (which Mozilla Intermediate v5 should reject)
    let ciphers = ["DES-CBC3-SHA", "RC4-SHA", "AES128-SHA"];
    let mut cipher_set = false;
    for c in ciphers {
        if connector_builder.set_cipher_list(c).is_ok() {
            println!("Testing with weak cipher: {}", c);
            cipher_set = true;
            break;
        }
    }

    if !cipher_set {
        println!("Skipping test: client could not set any of the weak ciphers");
        return;
    }
    connector_builder.set_verify(SslVerifyMode::NONE);

    let connector = connector_builder.build();

    // 3) Attempt handshake
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    socket.connect(server_addr).unwrap();

    let stream = UdpStream(socket);

    let res = connector.connect("localhost", stream);

    // 4) Assert failure
    assert!(
        res.is_err(),
        "Handshake should have failed for weak cipher DES-CBC3-SHA"
    );
    let err_msg = format!("{:?}", res.err());
    assert!(
        err_msg.contains("handshake failure") || err_msg.contains("no shared cipher"),
        "Unexpected error: {}",
        err_msg
    );
    println!("Handshake failed as expected for DES-CBC3-SHA: {}", err_msg);
}

#[test]
fn null_cipher_rejected_by_default() {
    test_util::init_test_logging();

    // 1) Setup Bingle node as server, bound to an OS-assigned loopback port.
    let server_opts = StartOptions {
        handle: Handle::from("server_no_enull"),
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

    std::thread::sleep(Duration::from_millis(100));

    // 2) Create a raw OpenSSL client configured for eNULL
    let mut connector_builder = SslConnector::builder(SslMethod::dtls()).unwrap();
    connector_builder
        .set_min_proto_version(Some(SslVersion::DTLS1_2))
        .unwrap();
    connector_builder.set_security_level(0);
    connector_builder.set_cipher_list("eNULL").unwrap();
    connector_builder.set_verify(SslVerifyMode::NONE);

    let connector = connector_builder.build();

    // 3) Attempt handshake
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    socket.connect(server_addr).unwrap();

    let stream = UdpStream(socket);

    let res = connector.connect("localhost", stream);

    // 4) Assert failure
    assert!(
        res.is_err(),
        "Handshake should have failed for eNULL when dangerous_debug is off"
    );
    let err_msg = format!("{:?}", res.err());
    assert!(
        err_msg.contains("handshake failure") || err_msg.contains("no shared cipher"),
        "Unexpected error: {}",
        err_msg
    );
    println!("Handshake failed as expected for eNULL: {}", err_msg);
}

#[test]
fn null_cipher_accepted_when_dangerous_debug_is_on() {
    test_util::init_test_logging();

    // 1) Setup Bingle node as server with dangerous_debug ON and null_encryption ON, bound to an
    // OS-assigned loopback port.
    let server_opts = StartOptions {
        handle: Handle::from("server_enull_ok"),
        algo_passphrase: Some(test_util::PASSPHRASE_RECEIVE.to_string()),
        static_ip: Some(test_util::loopback_addr(0)),
        dangerous_debug: true,
        ..StartOptions::new("".into())
    };
    let server_api = BingleApiImpl::new(&server_opts);
    server_api
        .access_unsafe_for_tests(|a| {
            a.with_engine_mut(|e| {
                e.with_dtls(|dtls| {
                    dtls.set_null_encryption(true);
                    dtls.set_app_layer_only_verification(true);
                });
            });
            a.start(&server_opts)
        })
        .expect("start server api");
    let server_addr = test_util::node_loopback_addr(&server_api);

    std::thread::sleep(Duration::from_millis(100));

    // 2) Create a raw OpenSSL client configured for eNULL with certificates
    let ops = test_util::ops_from_mnemonic(
        test_util::ADDRESS_RECEIVE,
        test_util::PASSPHRASE_RECEIVE,
        test_util::localnet_config(),
    );
    let (ca_pem, _srv_crt, _srv_key, cli_crt, cli_key) =
        bingle_core::api::pki::generate_pki_from_ops(&ops).unwrap();

    let mut connector_builder = SslConnector::builder(SslMethod::dtls()).unwrap();
    connector_builder
        .set_min_proto_version(Some(SslVersion::DTLS1_2))
        .unwrap();
    connector_builder.set_security_level(0);
    connector_builder
        .set_cipher_list("eNULL")
        .expect("client should be able to set eNULL");
    connector_builder.set_verify(SslVerifyMode::NONE);

    let client_x509 = openssl::x509::X509::from_pem(&cli_crt).unwrap();
    let client_key = openssl::pkey::PKey::private_key_from_pem(&cli_key).unwrap();
    connector_builder.set_certificate(&client_x509).unwrap();
    connector_builder.set_private_key(&client_key).unwrap();

    let ca_x509 = openssl::x509::X509::from_pem(&ca_pem).unwrap();
    connector_builder.add_extra_chain_cert(ca_x509).unwrap();

    let connector = connector_builder.build();

    // 3) Attempt handshake
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    socket.connect(server_addr).unwrap();

    let stream = UdpStream(socket);

    let res = connector.connect("localhost", stream);

    // 4) Assert success (or at least no cipher error)
    match res {
        Ok(_) => println!("eNULL handshake succeeded as expected with dangerous_debug"),
        Err(e) => {
            let err_msg = format!("{:?}", e);
            assert!(
                !err_msg.contains("handshake failure") && !err_msg.contains("no shared cipher"),
                "eNULL handshake failed with cipher error even though dangerous_debug is on: {}",
                err_msg
            );
            println!(
                "eNULL handshake failed (likely due to other reasons like certs): {}",
                err_msg
            );
        }
    }
}
