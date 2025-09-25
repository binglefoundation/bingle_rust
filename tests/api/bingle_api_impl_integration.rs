use rust_comms::api::bingle_api::{StartOptions, Handle, NetworkSourceKey, BingleApi};
use rust_comms::api::bingle_api_impl::BingleApiImpl;

#[test]
fn start_creates_dtls_instance() {
    let mut api = BingleApiImpl::new();
    let opts = StartOptions {
        handle: Handle::from("alice"),
        algo_passphrase: Some("test passphrase".to_string()),
        static_ip: None,
        am_relay: false,
        stun_servers: None,
    };
    let res = api.start(opts);
    assert!(res.is_ok());
    assert!(api.has_dtls(), "DTLS instance should be created on start");
}

#[test]
fn send_message_to_network_without_addr_fails_gracefully() {
    let api = BingleApiImpl::new();
    let nsk = NetworkSourceKey { inet_socket_address: None, relay_channel: None, relay_address: None };
    let ok = api.send_message_to_network(&nsk, &"user1".to_string(), serde_json::json!({"hi": 1}), None);
    assert!(!ok, "Should return false when no direct address is provided");
}
