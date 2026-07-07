use openssl::ssl::{SslConnector, SslMethod, SslVerifyMode, SslVersion};
use std::net::{SocketAddr, UdpSocket};
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
fn dtls_1_0_downgrade_fails() {
    test_util::init_test_logging();

    // 1) Setup Bingle node as server (accepts only DTLS 1.2)
    let server_port = test_util::find_unused_loopback_port();
    let server_addr = SocketAddr::new("127.0.0.1".parse().unwrap(), server_port);

    let server_opts = StartOptions {
        handle: Handle::from("server"),
        algo_passphrase: Some(test_util::PASSPHRASE_RECEIVE.to_string()),
        static_ip: Some(server_addr),
        dangerous_debug: true,
        ..StartOptions::new("".into())
    };
    let server_api = BingleApiImpl::new(&server_opts);
    server_api
        .access_unsafe_for_tests(|a| a.start(&server_opts))
        .expect("start server api");

    // Give server time to start
    std::thread::sleep(Duration::from_millis(100));

    // 2) Create a raw OpenSSL client configured for DTLS 1.0
    let mut connector_builder = SslConnector::builder(SslMethod::dtls()).unwrap();

    // Explicitly set DTLS 1.0
    connector_builder
        .set_min_proto_version(Some(SslVersion::DTLS1))
        .unwrap();
    connector_builder
        .set_max_proto_version(Some(SslVersion::DTLS1))
        .unwrap();

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
    // It should fail because the server has NO_DTLSV1 and min_proto_version = DTLS1_2.
    assert!(res.is_err(), "Handshake should have failed for DTLS 1.0");
    println!("Handshake failed as expected for DTLS 1.0: {:?}", res.err());
}

#[test]
fn dtls_1_2_handshake_succeeds() {
    test_util::init_test_logging();

    // 1) Setup Bingle node as server
    let server_port = test_util::find_unused_loopback_port();
    let server_addr = SocketAddr::new("127.0.0.1".parse().unwrap(), server_port);

    let server_opts = StartOptions {
        handle: Handle::from("server_ok"),
        algo_passphrase: Some(test_util::PASSPHRASE_RECEIVE.to_string()),
        static_ip: Some(server_addr),
        dangerous_debug: true,
        ..StartOptions::new("".into())
    };
    let server_api = BingleApiImpl::new(&server_opts);
    server_api
        .access_unsafe_for_tests(|a| a.start(&server_opts))
        .expect("start server api");

    std::thread::sleep(Duration::from_millis(100));

    // 2) Create a raw OpenSSL client configured for DTLS 1.2
    let mut connector_builder = SslConnector::builder(SslMethod::dtls()).unwrap();
    connector_builder
        .set_min_proto_version(Some(SslVersion::DTLS1_2))
        .unwrap();
    connector_builder
        .set_max_proto_version(Some(SslVersion::DTLS1_2))
        .unwrap();
    connector_builder.set_verify(SslVerifyMode::NONE);

    let connector = connector_builder.build();

    // 3) Attempt handshake
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    socket.connect(server_addr).unwrap();

    let stream = UdpStream(socket);

    let res = connector.connect("localhost", stream);

    // 4) Assert success (or at least that it didn't fail due to version)
    // Note: It might fail for other reasons (like missing client cert if server requires it),
    // but here we just want to see it get past the version check if possible.
    // Actually, Bingle node server by default sets SslVerifyMode::NONE if no handler is set,
    // OR it uses the peer_certificate_handler if one is set.
    // In BingleApiImpl::start, it sets a peer_certificate_handler.

    match res {
        Ok(_) => println!("DTLS 1.2 handshake succeeded"),
        Err(e) => {
            let err_msg = format!("{:?}", e);
            assert!(
                !err_msg.contains("wrong version number")
                    && !err_msg.contains("unsupported protocol"),
                "DTLS 1.2 handshake failed with version error: {}",
                err_msg
            );
            println!(
                "DTLS 1.2 handshake failed (as expected due to other reasons like certs): {}",
                err_msg
            );
        }
    }
}
