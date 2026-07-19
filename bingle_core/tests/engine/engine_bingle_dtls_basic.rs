use bingle_core::engine::BingleAccessUnsafeForTests;

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

use bingle_core::api::bingle_api::{BingleApi, NetworkEndpoint, StartOptions};
use bingle_core::api::bingle_api_impl::BingleApiImpl;

#[path = "../test_util.rs"]
pub mod test_util;

#[ntest::timeout(30_000)]
#[test]
#[cfg(not(target_os = "ios"))]
pub fn engine_basic_bingle_dtls_layer() {
    // Create server and client nodes (bound to OS-assigned loopback ports below).
    let server = BingleApiImpl::new(&StartOptions::new("".into()));
    let client = BingleApiImpl::new(&StartOptions::new("".into()));

    // Reverse-lookup seam: ensure on_plain_text can resolve sender handle by id in this test environment
    server.set_id_to_handle_lookup_mock_for_tests(Box::new(|_uid| Ok(Some("client".to_string()))));

    // Install server handlers that print and signal when a message arrives
    let delivered = Arc::new(AtomicBool::new(false));
    let delivered_flag = delivered.clone();
    server.access_unsafe_for_tests(|s: &mut BingleApiImpl| {
        s.set_on_connect(Some(Arc::new(|sender, handle| {
            tracing::info!("[server][on_connect] sender={} handle={}", sender, handle);
        })))
    });
    server.access_unsafe_for_tests(|s: &mut BingleApiImpl| {
        s.set_on_message(Some(Arc::new(move |sender, handle, msg| {
            tracing::info!(
                "[server][on_message] sender={} handle={} msg={}",
                sender,
                handle,
                msg
            );
            delivered_flag.store(true, Ordering::SeqCst);
        })))
    });

    // Prepare options: static endpoints, no STUN.
    let server_opts = StartOptions {
        handle: "server".into(),
        algo_passphrase: Some(test_util::PASSPHRASE_RECEIVE.to_string()),
        static_ip: Some(test_util::loopback_addr(0)),
        am_relay: false,
        stun_servers: None,
        algo_provider_config: None,
        algo_network: None,
        app_id: None,
        asset_id: None,
        log_level: None,
        handle_cache_expiry: None,
        dangerous_debug: true,
        log_mode: bingle_core::util::logging::LogMode::Plain,
        wait_response_timeout: None,
    };
    let client_opts = StartOptions {
        handle: "client".into(),
        algo_passphrase: Some(test_util::PASSPHRASE_SPEND.to_string()),
        static_ip: Some(test_util::loopback_addr(0)),
        am_relay: false,
        stun_servers: None,
        algo_provider_config: None,
        algo_network: None,
        app_id: None,
        asset_id: None,
        log_level: None,
        handle_cache_expiry: None,
        dangerous_debug: true,
        log_mode: bingle_core::util::logging::LogMode::Plain,
        wait_response_timeout: None,
    };

    // Start both nodes (bound to OS-assigned loopback ports).
    server
        .access_unsafe_for_tests(|s: &mut BingleApiImpl| s.start(&server_opts))
        .expect("server start() should succeed");
    client
        .access_unsafe_for_tests(|c: &mut BingleApiImpl| c.start(&client_opts))
        .expect("client start() should succeed");

    // Resolve the server's actual bound loopback address for direct addressing.
    let server_addr = test_util::node_loopback_addr(&server);
    tracing::info!("[test] server started at {}", server_addr);

    // Build direct network destination to server and send a simple plaintext JSON message.
    let dest = NetworkEndpoint::new_direct(server_addr);
    let payload = serde_json::json!({
        "text": "hello from client"
    });

    let progress: Arc<bingle_core::api::bingle_api::ProgressCallback> = Arc::new(|pct, msg| {
        tracing::info!("[client][progress] {}% {}", pct, msg);
    });

    tracing::info!("[test] client sending message to {}", server_addr);
    let uid = server
        .access_unsafe_for_tests(|s: &mut BingleApiImpl| s.get_my_id())
        .expect("server id Some");
    let ok = client
        .access_unsafe_for_tests(|c: &mut BingleApiImpl| {
            c.send_message_to_network(&dest, &uid, payload, Some(progress))
        })
        .unwrap();
    assert!(ok, "client send_message_to_network should return true");

    // Wait for on_message to be called on the server
    let start = Instant::now();
    while !delivered.load(Ordering::SeqCst) && start.elapsed() < Duration::from_secs(5) {
        std::thread::sleep(Duration::from_millis(25));
    }
    assert!(
        delivered.load(Ordering::SeqCst),
        "server on_message handler was not invoked"
    );

    // Cleanup
    server.access_unsafe_for_tests(|s: &mut BingleApiImpl| s.stop());
    client.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.stop());
}
