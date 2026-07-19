// Extracted from tests/security/renegotiation_test.rs.
// The live DTLS handshake test binds sockets, sleeps, and performs a real loopback
// handshake, so it intermittently fails or hangs. Its companion config-only test
// (dtls_configuration_disables_renegotiation) is stable and stays in the unit suite.
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
fn dtls_handshake_succeeds_with_renegotiation_disabled() {
    test_util::init_test_logging();

    // 1) Setup Bingle node as server, bound to an OS-assigned loopback port.
    let server_opts = StartOptions {
        handle: Handle::from("server_reneg_ok_test"),
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

    // 2) Setup client with valid certs
    let ops = test_util::ops_from_mnemonic(
        test_util::ADDRESS_RECEIVE,
        test_util::PASSPHRASE_RECEIVE,
        test_util::localnet_config(),
    );
    let (ca_pem, _srv_crt, _srv_key, cli_crt, cli_key) =
        bingle_core::api::pki::generate_pki_from_ops(&ops).expect("generate pki");

    let mut connector_builder =
        SslConnector::builder(SslMethod::dtls()).expect("connector builder");
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
        .expect("set pkey");

    // Add CA cert to chain so server can verify client
    let ca_x509 = openssl::x509::X509::from_pem(&ca_pem).expect("ca cert");
    connector_builder
        .add_extra_chain_cert(ca_x509)
        .expect("add ca cert");

    let connector = connector_builder.build();

    let socket = UdpSocket::bind("127.0.0.1:0").expect("bind socket");
    socket
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("set timeout");
    socket.connect(server_addr).expect("connect socket");

    let stream = UdpStream(socket);

    let _ssl_stream = connector
        .connect("localhost", stream)
        .expect("handshake failed");
    println!("Handshake succeeded with renegotiation disabled.");
}
