use rust_comms::api::bingle_api::{StartOptions, Handle, NetworkSourceKey, BingleApi};
use std::net::SocketAddr;
use rust_comms::api::bingle_api_impl::BingleApiImpl;

#[test]
fn start_creates_dtls_instance() {
    let mut api = BingleApiImpl::new();
    let opts = StartOptions {
        handle: Handle::from("alice"),
        algo_passphrase: Some("test passphrase".to_string()),
        static_ip: None,
        am_relay: false,
        stun_servers: Some(vec![SocketAddr::from(([127, 0, 0, 1], 3478))]),
    };
    let res = api.start(opts);
    // Engine may fail to start depending on DTLS/PKI availability; we only require DTLS instance creation here.
    if res.is_err() {
        eprintln!("api.start returned error: {:?}", res);
    }
    assert!(api.has_dtls(), "DTLS instance should be created on start");
}

#[test]
fn send_message_to_network_without_addr_fails_gracefully() {
    let api = BingleApiImpl::new();
    let nsk = NetworkSourceKey { inet_socket_address: None, relay_channel: None, relay_address: None };
    let ok = api.send_message_to_network(&nsk, &"user1".to_string(), serde_json::json!({"hi": 1}), None);
    assert!(!ok, "Should return false when no direct address is provided");
}

#[cfg(not(target_os = "ios"))]
#[test]
fn relay_check_end_to_end_on_message_receives_response() {
    use std::net::{SocketAddr, UdpSocket};
    use std::sync::{OnceLock, Arc};
    use std::thread;
    use std::time::{Duration, Instant};

    use rust_comms::dtls::{Dtls, DtlsOpenSsl};

    #[path = "../dtls/pki.rs"]
    mod pki;

    static CLIENT_SEEN: OnceLock<serde_json::Value> = OnceLock::new();

    // Spin up a DTLS server that responds to RelayCheck with RelayCheckResponse
    let certs = pki::generate_ed25519_test_certs();
    let server_cert_pem: Vec<u8> = certs.server_crt.clone();
    let server_key_pem: Vec<u8> = certs.server_key.clone();
    let ca_pem: Vec<u8> = certs.ca_crt.clone();

    // Create and start a UDP mux for the server and determine its bound address.
    let mux0 = rust_comms::dtls::UdpNetworkMux::bind(("127.0.0.1", 0)).expect("bind mux");
    let mux = std::sync::Arc::new(mux0);
    let addr: SocketAddr = mux.local_addr().expect("mux addr");

    // Server handler: parse JSON; if RelayCheck, reply with RelayCheckResponse echoing tag
    fn server_handler(server: &dyn Dtls, from: &SocketAddr, _issuer: &str, data: &[u8]) {
        if let Ok(text) = std::str::from_utf8(data) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(text) {
                let is_check = v.get("type").and_then(|x| x.as_str()) == Some("Check")
                    && v.get("app").map(|a| a.is_null()).unwrap_or(true);
                if is_check {
                    let mut obj = serde_json::Map::new();
                    obj.insert("app".to_string(), serde_json::Value::Null);
                    obj.insert("type".to_string(), serde_json::Value::String("CheckResponse".to_string()));
                    obj.insert("available".to_string(), serde_json::Value::Bool(true));
                    if let Some(tag) = v.get("responseTag").and_then(|t| t.as_str()) {
                        obj.insert("tag".to_string(), serde_json::Value::String(tag.to_string()));
                    }
                    if let Ok(bytes) = serde_json::to_vec(&serde_json::Value::Object(obj)) {
                        let _ = server.send(*from, &bytes);
                    }
                    return;
                }
            }
        }
    }

    // Build and start the server
    let mut server = DtlsOpenSsl::new()
        .with_handle_message(Arc::new(server_handler))
        .with_server_signing_cert(server_cert_pem.clone())
        .with_server_signing_private_key(server_key_pem.clone())
        .with_ca_cert(ca_pem.clone());
    mux.start().expect("mux start");
    server.start(mux.clone()).expect("server start");

    // Give server time to bind
    thread::sleep(Duration::from_millis(200));

    // Build BingleApiImpl client and install on_message to capture the RelayCheckResponse
    let mut api = BingleApiImpl::new();
    let opts = StartOptions { handle: Handle::from("client"), algo_passphrase: Some("pass".to_string()), static_ip: None, am_relay: false, stun_servers: Some(vec![SocketAddr::from(([127, 0, 0, 1], 3478))]) };
    let _ = api.start(opts);

    api.set_on_message(Some(Arc::new(|_sender, _handle, msg| {
        let _ = CLIENT_SEEN.set(msg);
    })));

    // Prepare a direct NetworkSourceKey to server and send RelayCheck
    let nsk = NetworkSourceKey { inet_socket_address: Some(addr), relay_channel: None, relay_address: None };
    use uuid::Uuid;
    let req_tag = Uuid::new_v4().to_string();
    let payload = serde_json::json!({ "app": null, "type": "Check", "responseTag": req_tag });

    let ok = api.send_message_to_network(&nsk, &"srv".to_string(), payload, None);
    assert!(ok, "client send failed");

    // Wait for the response to be observed via on_message
    let start = Instant::now();
    while CLIENT_SEEN.get().is_none() && start.elapsed() < Duration::from_secs(3) {
        thread::sleep(Duration::from_millis(20));
    }

    let seen = CLIENT_SEEN.get().expect("did not receive RelayCheckResponse via on_message");
    assert_eq!(seen.get("app"), Some(&serde_json::Value::Null));
    assert_eq!(seen.get("type").and_then(|v| v.as_str()), Some("CheckResponse"));
    assert_eq!(seen.get("available").and_then(|v| v.as_bool()), Some(true));
    assert_eq!(seen.get("tag").and_then(|v| v.as_str()), Some(req_tag.as_str()), "RelayCheckResponse should echo the request's responseTag in 'tag'");
}
