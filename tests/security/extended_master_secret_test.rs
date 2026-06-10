use std::net::{SocketAddr, UdpSocket};
use std::time::Duration;
use openssl::ssl::{SslMethod, SslConnector, SslVerifyMode, SslVersion};

use rust_comms::api::bingle_api::{BingleApi, Handle, StartOptions};
use rust_comms::api::bingle_api_impl::BingleApiImpl;
use rust_comms::engine::BingleAccessUnsafeForTests;

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
    fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
}

#[test]
fn dtls_handshake_negotiates_extended_master_secret() {
    test_util::init_test_logging();

    // 1) Setup Bingle node as server
    let server_port = test_util::find_unused_loopback_port();
    let server_addr = SocketAddr::new("127.0.0.1".parse().expect("valid ip"), server_port);
    
    let server_opts = StartOptions {
        handle: Handle::from("server_ems_test"),
        algo_passphrase: Some(test_util::PASSPHRASE_RECEIVE.to_string()),
        static_ip: Some(server_addr),
        dangerous_debug: false, 
        ..StartOptions::new("".into())
    };
    let server_api = BingleApiImpl::new(&server_opts);
    server_api.access_unsafe_for_tests(|a| a.start(&server_opts)).expect("start server api");

    std::thread::sleep(Duration::from_millis(200));

    // 2) Setup client with valid certs
    let ops = test_util::ops_from_mnemonic(test_util::ADDRESS_RECEIVE, test_util::PASSPHRASE_RECEIVE, test_util::localnet_config());
    let (ca_pem, _srv_crt, _srv_key, cli_crt, cli_key) = rust_comms::api::pki::generate_pki_from_ops(&ops).expect("generate pki");

    let mut connector_builder = SslConnector::builder(SslMethod::dtls()).expect("connector builder");
    connector_builder.set_min_proto_version(Some(SslVersion::DTLS1_2)).expect("set min proto");
    connector_builder.set_verify(SslVerifyMode::NONE);

    let client_x509 = openssl::x509::X509::from_pem(&cli_crt).expect("client cert");
    let client_key = openssl::pkey::PKey::private_key_from_pem(&cli_key).expect("client key");
    connector_builder.set_certificate(&client_x509).expect("set cert");
    connector_builder.set_private_key(&client_key).expect("set pkey");
    
    // Add CA cert to chain so server can verify client
    let ca_x509 = openssl::x509::X509::from_pem(&ca_pem).expect("ca cert");
    connector_builder.add_extra_chain_cert(ca_x509).expect("add ca cert");

    let connector = connector_builder.build();
    
    let socket = UdpSocket::bind("127.0.0.1:0").expect("bind socket");
    socket.set_read_timeout(Some(Duration::from_secs(5))).expect("set timeout");
    socket.connect(server_addr).expect("connect socket");
    
    let stream = UdpStream(socket);
    
    let ssl_stream = connector.connect("localhost", stream).expect("handshake failed");
    
    // 3) Verify EMS negotiation
    // SSL_get_extms_support(s) is a macro: SSL_ctrl((s),SSL_CTRL_GET_EXTMS_SUPPORT,0,NULL)
    // SSL_CTRL_GET_EXTMS_SUPPORT is 122
    const SSL_CTRL_GET_EXTMS_SUPPORT: std::os::raw::c_int = 122;

    use openssl_sys::SSL;
    
    let extms = unsafe {
        // SslRef is a transparent wrapper around SSL
        let ssl_ptr = ssl_stream.ssl() as *const _ as *mut SSL;
        openssl_sys::SSL_ctrl(ssl_ptr, SSL_CTRL_GET_EXTMS_SUPPORT, 0, std::ptr::null_mut()) == 1
    };
    
    println!("Extended Master Secret negotiated: {}", extms);
    assert!(extms, "Extended Master Secret should be negotiated");
    
    println!("Handshake succeeded with Extended Master Secret negotiated.");
}
