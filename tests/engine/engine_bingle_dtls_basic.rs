#![cfg(not(target_os = "ios"))]

use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use std::time::{Duration, Instant};

use rust_comms::api::bingle_api::{BingleApi, NetworkSourceKey, StartOptions};
use rust_comms::api::bingle_api_impl::BingleApiImpl;
use base64::Engine as _;

#[path = "../test_util.rs"]
mod test_util;

fn find_unused_loopback_port() -> u16 {
    let sock = UdpSocket::bind((IpAddr::V4(Ipv4Addr::LOCALHOST), 0)).expect("bind temp socket");
    let port = sock.local_addr().expect("local addr").port();
    drop(sock);
    port
}

#[ntest::timeout(30_000)]
#[test]
fn engine_basic_bingle_dtls_layer() {
    // Allocate two free loopback ports for server and client static endpoints.
    let server_port = find_unused_loopback_port();
    let client_port = find_unused_loopback_port();
    assert_ne!(server_port, 0);
    assert_ne!(client_port, 0);

    let server_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), server_port);
    let client_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), client_port);

    // Create server and client nodes
    let mut server = BingleApiImpl::new();
    let mut client = BingleApiImpl::new();

    // Install server handlers that print and signal when a message arrives
    let delivered = Arc::new(AtomicBool::new(false));
    let delivered_flag = delivered.clone();
    server.set_on_connect(Some(Arc::new(|sender, handle| {
        log::info!("[server][on_connect] sender={} handle={}", sender, handle);
    })));
    server.set_on_message(Some(Arc::new(move |sender, handle, msg| {
        log::info!("[server][on_message] sender={} handle={} msg={}", sender, handle, msg);
        delivered_flag.store(true, Ordering::SeqCst);
    })));

    // Prepare options: static endpoints, no STUN.
    let server_opts = StartOptions {
        handle: "server".into(),
        algo_passphrase: Some(test_util::PASSPHRASE_RECEIVE.to_string()),
        static_ip: Some(server_addr),
        am_relay: false,
        stun_servers: None,
        algo_provider_config: None,
        algo_network: None,
        app_id: None,
        asset_id: None,
        log_level: None,
    };
    let client_opts = StartOptions {
        handle: "client".into(),
        algo_passphrase: Some(test_util::PASSPHRASE_SPEND.to_string()),
        static_ip: Some(client_addr),
        am_relay: false,
        stun_servers: None,
        algo_provider_config: None,
        algo_network: None,
        app_id: None,
        asset_id: None,
        log_level: None,
    };

    // Start both nodes
    log::info!("[test] starting server at {}", server_addr);
    server.start(server_opts).expect("server start() should succeed");
    log::info!("[test] starting client at {}", client_addr);
    client.start(client_opts).expect("client start() should succeed");

    // Build direct network destination to server and send a simple plaintext JSON message.
    let dest = NetworkSourceKey::new_direct(server_addr);
    let payload = serde_json::json!({
        "text": "hello from client"
    });

    let progress: Arc<rust_comms::api::bingle_api::ProgressCallback> = Arc::new(|pct, msg| {
        log::info!("[client][progress] {}% {}", pct, msg);
    });

    log::info!("[test] client sending message to {}", server_addr);
    let uid = base64::engine::general_purpose::STANDARD.encode([2u8; 36]);
    let ok = client.send_message_to_network(&dest, &uid, payload, Some(progress));
    assert!(ok, "client send_message_to_network should return true");

    // Wait for on_message to be called on the server
    let start = Instant::now();
    while !delivered.load(Ordering::SeqCst) && start.elapsed() < Duration::from_secs(5) {
        std::thread::sleep(Duration::from_millis(25));
    }
    assert!(delivered.load(Ordering::SeqCst), "server on_message handler was not invoked");

    // Cleanup
    server.stop();
    client.stop();
}
