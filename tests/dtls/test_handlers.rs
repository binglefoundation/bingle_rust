use crate::util::test_util;
use rust_comms::dtls::Dtls;
use std::sync::OnceLock;

pub static SERVER_HELLO: OnceLock<Vec<u8>> = OnceLock::new();
pub static SERVER_CLIENT_ECHOED: OnceLock<Vec<u8>> = OnceLock::new();
pub static CLIENT_PING_SEEN: OnceLock<Vec<u8>> = OnceLock::new();

pub fn clear_handlers() {
    // OnceLock cannot be cleared easily, so for tests we might need something else if we reuse them in the same process.
    // But since these are separate test files (usually separate processes or at least separate test functions), it might be okay.
    // Actually, if we run multiple tests in the same process, OnceLock is a problem.
}

pub fn mock_peer_cert_handler(_cert: &[u8], _ca: &[u8]) -> rust_comms::dtls::Result<String> {
    Ok(test_util::ADDRESS_SPEND.to_string())
}

pub fn client_echo_handler(
    server: &dyn Dtls,
    from: &rust_comms::api::bingle_api::NetworkEndpoint,
    _issuer: &str,
    data: &[u8],
) {
    tracing::info!("client_echo_handler: {:?}", data);
    // Record that client received the Ping
    if data == b"Ping" {
        let _ = CLIENT_PING_SEEN.set(data.to_vec());
    }
    // Echo back to the server with the required prefix
    let mut echoed = b"CLIENT ECHOED: ".to_vec();
    echoed.extend_from_slice(data);
    let _ = server.send(from, &echoed);
}

pub fn server_capture_and_trigger_handler(
    server: &dyn Dtls,
    from: &rust_comms::api::bingle_api::NetworkEndpoint,
    _issuer: &str,
    data: &[u8],
) {
    tracing::info!("server_capture_and_trigger_handler: {:?}", data);
    // Capture the initial Hello and immediately send Ping to the client
    if data == b"Hello" {
        let _ = SERVER_HELLO.set(data.to_vec());
        let _ = server.send(from, b"Ping");
        return;
    }
    // Capture the client's echoed message
    if data.starts_with(b"CLIENT ECHOED: ") {
        let _ = SERVER_CLIENT_ECHOED.set(data.to_vec());
    }
}
