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
    let received: Arc<std::sync::Mutex<Option<serde_json::Value>>> =
        Arc::new(std::sync::Mutex::new(None));
    let received_slot = received.clone();
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
            if let Ok(mut slot) = received_slot.lock() {
                *slot = Some(msg.clone());
            }
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

    // The dialling side records the suite negotiated for the session (issue #292), and it is the
    // same suite the accepting side recorded for it.
    let client_suite = client.engine_for_tests().dtls().get_cipher_suite(&dest);
    let client_ep = NetworkEndpoint::new_direct(test_util::node_loopback_addr(&client));
    let server_suite = server
        .engine_for_tests()
        .dtls()
        .get_cipher_suite(&client_ep);
    assert!(
        client_suite.is_some(),
        "the client should record the negotiated suite"
    );
    assert_eq!(
        client_suite, server_suite,
        "both ends should record the same suite"
    );

    // The text message arrived signed by the client over the canonical fields (issue #94).
    let message = received
        .lock()
        .ok()
        .and_then(|slot| slot.clone())
        .expect("received message");
    let client_id = client
        .access_unsafe_for_tests(|c: &mut BingleApiImpl| c.get_my_id())
        .expect("client id Some");
    let sender_key = algo_ops::address_to_byte_key(&client_id).expect("client key");
    let recipient_key = algo_ops::address_to_byte_key(&uid).expect("server key");
    let sent_time = message["sent_time"]
        .as_i64()
        .expect("sent_time on the wire");
    let signature = base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        message["signature"]
            .as_str()
            .expect("signature on the wire"),
    )
    .expect("base64 signature");
    let signed = bingle_core::crypto::sealed_envelope::canonical_signed_message(
        &sender_key,
        &recipient_key,
        sent_time,
        "hello from client",
    );
    let verifying_key =
        ed25519_dalek::VerifyingKey::from_bytes(&sender_key).expect("client verifying key");
    let signature = ed25519_dalek::Signature::from_slice(&signature).expect("64-byte signature");
    assert!(
        ed25519_dalek::Verifier::verify(&verifying_key, &signed, &signature).is_ok(),
        "the received text message's signature verifies with the sender's key"
    );

    // Cleanup
    server.access_unsafe_for_tests(|s: &mut BingleApiImpl| s.stop());
    client.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.stop());
}
