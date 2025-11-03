use rust_comms::api::bingle_api::{StartOptions, Handle, NetworkSourceKey, BingleApi};
use rust_comms::api::bingle_api_impl::BingleApiImpl;
use std::net::SocketAddr;
use std::sync::{Arc, OnceLock, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use base64::Engine as _;

#[path = "../test_util.rs"]
mod test_util;

#[cfg(not(target_os = "ios"))]
#[test]
fn bingle_api_relay_check_two_nodes() {
    // Helper to convert Algorand base32 ID to base64(36) as required by API validation.
    fn id_base64_from_base32(addr_b32: &str) -> String {
        let bytes = data_encoding::BASE32_NOPAD
            .decode(addr_b32.as_bytes())
            .expect("base32 decode of Algorand address should succeed");
        assert_eq!(bytes.len(), 36, "decoded Algorand address should be 36 bytes");
        base64::engine::general_purpose::STANDARD.encode(bytes)
    }

    // Helper to allocate a free loopback port to avoid cross-test collisions.
    fn find_unused_loopback_port() -> u16 {
        use std::net::{IpAddr, Ipv4Addr, UdpSocket};
        let sock = UdpSocket::bind((IpAddr::V4(Ipv4Addr::LOCALHOST), 0)).expect("bind temp socket");
        let port = sock.local_addr().expect("local addr").port();
        drop(sock);
        port
    }

    // 1) Start relay node on an unused port with PASSPHRASE_RECEIVE and id ADDRESS_RECEIVE
    let relay_port = find_unused_loopback_port();
    let relay_addr = SocketAddr::from(([127, 0, 0, 1], relay_port));
    let mut relay = BingleApiImpl::new();
    let relay_opts = StartOptions {
        handle: Handle::from("relay"),
        algo_passphrase: Some(test_util::PASSPHRASE_RECEIVE.to_string()),
        static_ip: Some(relay_addr),
        am_relay: true,
        stun_servers: None,
    };
    relay.start(relay_opts).expect("relay start");

    // Install an on_message on the relay that responds to RelayCheck with RelayCheckResponse.
    // We need to send back to the client's socket address; this is provided via sender_handle (from_address string).
    // For user_id, use the known ADDRESS_SPEND (client id).
    let relay_arc: Arc<Mutex<BingleApiImpl>> = Arc::new(Mutex::new(relay));
    let relay_for_cb = relay_arc.clone();
    let client_id_b64 = id_base64_from_base32(test_util::ADDRESS_SPEND);
    {
        let mut guard = relay_arc.lock().unwrap();
        guard.set_on_message(Some(Arc::new(move |sender_id, sender_handle, msg| {
            println!("[test][relay on_message] sender={} handle={} msg={}", sender_id, sender_handle, msg);
            // sender_handle is the peer socket address (string). Parse to SocketAddr. Fail fast if malformed.
            let addr: SocketAddr = sender_handle.parse().expect("sender_handle must parse to SocketAddr");
            let is_check = msg.get("type").and_then(|v| v.as_str()) == Some("Check")
                && msg.get("app").map(|v| v.is_null()).unwrap_or(true);
            if is_check {
                let resp = serde_json::json!({
                    "app": null,
                    "type": "CheckResponse",
                    "available": true,
                });
                let nsk = NetworkSourceKey::new_direct(addr);
                // Validate that locking succeeds and attempt to send the response.
                let _ok = relay_for_cb
                    .lock()
                    .expect("relay Arc<Mutex> should be lockable")
                    .send_message_to_network(&nsk, &client_id_b64, resp, None);
            }
        })));
    }

    // 2) Start client node on port 12346 with PASSPHRASE_SPEND and id ADDRESS_SPEND
    let client_addr = SocketAddr::from(([127, 0, 0, 1], 12346));
    let mut client = BingleApiImpl::new();
    let client_opts = StartOptions {
        handle: Handle::from("client"),
        algo_passphrase: Some(test_util::PASSPHRASE_SPEND.to_string()),
        static_ip: Some(client_addr),
        am_relay: false,
        stun_servers: None,
    };
    client.start(client_opts).expect("client start");

    // Capture RelayCheckResponse on the client
    static CLIENT_SEEN: OnceLock<serde_json::Value> = OnceLock::new();
    client.set_on_message(Some(Arc::new(|sender, handle, msg| {
        println!("[test][client on_message] sender={} handle={} msg={}", sender, handle, msg);
        let _ = CLIENT_SEEN.set(msg.clone());
    })));

    // 3) Send RelayCheck from client to relay directly
    let nsk_relay = NetworkSourceKey::new_direct(relay_addr);
    let relay_id_b64 = id_base64_from_base32(test_util::ADDRESS_RECEIVE);
    let payload = serde_json::json!({ "app": null, "type": "Check" });

    let ok = client.send_message_to_network(&nsk_relay, &relay_id_b64, payload, None);
    assert!(ok, "client send_message_to_network should return true");

    // 4) Await RelayCheckResponse via client's on_message
    let start = Instant::now();
    while CLIENT_SEEN.get().is_none() && start.elapsed() < Duration::from_secs(5) {
        thread::sleep(Duration::from_millis(20));
    }
    let seen = CLIENT_SEEN.get().expect("did not receive RelayCheckResponse at client");
    assert_eq!(seen.get("app"), Some(&serde_json::Value::Null));
    assert_eq!(seen.get("type").and_then(|v| v.as_str()), Some("CheckResponse"));
    assert_eq!(seen.get("available").and_then(|v| v.as_bool()), Some(true));

    // Optional: stop nodes (best-effort)
    if let Ok(mut r) = relay_arc.lock() { r.stop(); }
    client.stop();
}
