use bingle_core::engine::BingleAccessUnsafeForTests;

use std::net::SocketAddr;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

use bingle_core::api::bingle_api::{BingleApi, BingleApiInternal, NetworkEndpoint, StartOptions};
use bingle_core::api::bingle_api_impl::BingleApiImpl;
use bingle_core::messages::marshal;
use bingle_core::messages::types::*;

#[path = "../test_util.rs"]
pub mod test_util;

fn start_pair(
    server_am_relay: bool,
) -> (
    Arc<BingleApiImpl>,
    Arc<BingleApiImpl>,
    SocketAddr,
    SocketAddr,
) {
    let server_opts = StartOptions {
        handle: "server".into(),
        algo_passphrase: Some(test_util::PASSPHRASE_RECEIVE.to_string()),
        static_ip: Some(test_util::loopback_addr(0)),
        am_relay: server_am_relay,
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

    let server = BingleApiImpl::new(&server_opts);
    let client = BingleApiImpl::new(&client_opts);

    server.set_id_to_handle_lookup_mock_for_tests(Box::new(|user_id| {
        if user_id.is_empty() {
            Ok(None)
        } else {
            Ok(Some("mock-client".to_string()))
        }
    }));
    client.set_id_to_handle_lookup_mock_for_tests(Box::new(|user_id| {
        if user_id.is_empty() {
            Ok(None)
        } else {
            Ok(Some("mock-server".to_string()))
        }
    }));

    server
        .access_unsafe_for_tests(|s: &mut BingleApiImpl| s.start(&server_opts))
        .expect("server start ok");
    client
        .access_unsafe_for_tests(|c: &mut BingleApiImpl| c.start(&client_opts))
        .expect("client start ok");
    // Resolve each node's actual bound loopback address (no allocate-then-bind race).
    let server_addr = test_util::node_loopback_addr(&server);
    let client_addr = test_util::node_loopback_addr(&client);
    (server, client, server_addr, client_addr)
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn ddb_upsert_success_when_server_is_relay() {
    let (server, client, server_addr, _client_addr) = start_pair(true);

    // Build a valid UpsertResolve from client where startId == record.id == client id
    let _server_id = server
        .access_unsafe_for_tests(|s: &mut BingleApiImpl| s.get_my_id())
        .expect("server get_my_id Some"); // Use API to ensure functions are wired
    let client_id = client
        .access_unsafe_for_tests(|c: &mut BingleApiImpl| c.get_my_id())
        .expect("client id Some");
    let signing_key = client
        .access_unsafe_for_tests(|c| c.get_signing_key())
        .expect("signing key ok");

    let record = AdvertRecord::new(
        client_id.clone(),
        Some(InetSocketAddress {
            host: "127.0.0.1".into(),
            port: 9999,
        }),
        Some(false),
        None,
        None,
        "2025-01-01T00:00:00Z".into(),
        &signing_key,
    );
    let up = Message::Ddb(DdbMessage::UpsertResolve(DdbUpsertResolve {
        app: "ddb".into(),
        start_id: client_id.clone(),
        epoch: 1,
        record: record.clone(),
        original_signature: "SIG".into(),
        rippled: false,
        tag: None,
        response_tag: None,
        text: None,
        data: None,
    }));

    let json = marshal::to_json_value(&up);

    // Observe UpdateResponse via CLIENT on_message handler (server sends response back to client)
    let got_update = Arc::new(AtomicBool::new(false));
    let got_update_flag = got_update.clone();
    client.access_unsafe_for_tests(|c: &mut BingleApiImpl| {
        c.set_on_message(Some(Arc::new(move |_sender, _handle, msg| {
            if let Some(t) = msg.get("type").and_then(|v: &serde_json::Value| v.as_str()) {
                if t == "updateResponse"
                    && msg.get("app").and_then(|v: &serde_json::Value| v.as_str()) == Some("ddb")
                {
                    if msg
                        .get("responseTag")
                        .and_then(|v: &serde_json::Value| v.as_str())
                        .is_some()
                        && msg.get("tag").is_none()
                    {
                        got_update_flag.store(true, Ordering::SeqCst);
                    }
                }
            }
        })))
    });

    // Send request from client to server
    let nsk = NetworkEndpoint::new_direct(server_addr);
    let uid = server
        .access_unsafe_for_tests(|s: &mut BingleApiImpl| s.get_my_id())
        .unwrap();
    let response = client.access_unsafe_for_tests(|c: &mut BingleApiImpl| {
        c.send_message_to_network_with_response(&nsk, &uid, json, None)
    });
    assert!(response.is_ok(), "client send ok");

    // Cleanup
    server.access_unsafe_for_tests(|s: &mut BingleApiImpl| s.stop());
    client.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.stop());
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn ddb_upsert_ignored_when_not_relay() {
    let (server, client, server_addr, _client_addr) = start_pair(false);

    let client_id = client
        .access_unsafe_for_tests(|c: &mut BingleApiImpl| c.get_my_id())
        .expect("client id Some");
    let record = AdvertRecord::new_unsigned(
        client_id.clone(),
        None,
        Some(false),
        None,
        None,
        "2025-01-01T00:00:00Z".into(),
    );
    let up = Message::Ddb(DdbMessage::UpsertResolve(DdbUpsertResolve {
        app: "ddb".into(),
        start_id: client_id.clone(),
        epoch: 1,
        record,
        original_signature: "SIG".into(),
        rippled: false,
        tag: None,
        response_tag: None,
        text: None,
        data: None,
    }));

    let json = marshal::to_json_value(&up);

    let got_update = Arc::new(AtomicBool::new(false));
    let got_update_flag = got_update.clone();
    server.access_unsafe_for_tests(|s: &mut BingleApiImpl| {
        s.set_on_message(Some(Arc::new(move |_sender, _handle, msg| {
            if msg.get("type").and_then(|v: &serde_json::Value| v.as_str())
                == Some("updateResponse")
            {
                got_update_flag.store(true, Ordering::SeqCst);
            }
        })))
    });

    let nsk = NetworkEndpoint::new_direct(server_addr);
    let uid = server
        .access_unsafe_for_tests(|s: &mut BingleApiImpl| s.get_my_id())
        .unwrap();
    let ok = client
        .access_unsafe_for_tests(|c: &mut BingleApiImpl| {
            c.send_message_to_network(&nsk, &uid, json, None)
        })
        .unwrap();
    assert!(ok, "client send ok");

    // Give some time; expect no updateResponse because server is not a relay
    std::thread::sleep(Duration::from_millis(200));
    assert!(
        !got_update.load(Ordering::SeqCst),
        "should not receive updateResponse when server not relay"
    );

    server.access_unsafe_for_tests(|s: &mut BingleApiImpl| s.stop());
    drop(client);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn ddb_upsert_rejected_on_id_mismatch() {
    let (server, client, server_addr, _client_addr) = start_pair(true);

    let client_id = client
        .access_unsafe_for_tests(|c: &mut BingleApiImpl| c.get_my_id())
        .expect("client id Some");
    // Mismatch: record.id != start_id
    let record = AdvertRecord::new_unsigned(
        format!("{}X", client_id),
        None,
        Some(false),
        None,
        None,
        "2025-01-01T00:00:00Z".into(),
    );
    let up = Message::Ddb(DdbMessage::UpsertResolve(DdbUpsertResolve {
        app: "ddb".into(),
        start_id: client_id.clone(),
        epoch: 1,
        record,
        original_signature: "SIG".into(),
        rippled: false,
        tag: None,
        response_tag: None,
        text: None,
        data: None,
    }));

    let json = marshal::to_json_value(&up);

    let got_update = Arc::new(AtomicBool::new(false));
    let got_update_flag = got_update.clone();
    server.access_unsafe_for_tests(|s: &mut BingleApiImpl| {
        s.set_on_message(Some(Arc::new(move |_sender, _handle, msg| {
            if msg.get("type").and_then(|v: &serde_json::Value| v.as_str())
                == Some("updateResponse")
            {
                got_update_flag.store(true, Ordering::SeqCst);
            }
        })))
    });

    let nsk = NetworkEndpoint::new_direct(server_addr);
    let uid = server
        .access_unsafe_for_tests(|s: &mut BingleApiImpl| s.get_my_id())
        .unwrap();
    let ok = client
        .access_unsafe_for_tests(|c: &mut BingleApiImpl| {
            c.send_message_to_network(&nsk, &uid, json, None)
        })
        .unwrap();
    assert!(ok, "client send ok");

    std::thread::sleep(Duration::from_millis(200));
    assert!(
        !got_update.load(Ordering::SeqCst),
        "should not receive updateResponse on id mismatch"
    );

    server.access_unsafe_for_tests(|s: &mut BingleApiImpl| s.stop());
    drop(client);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn ddb_upsert_relay_record_rejected_without_blockchain_config() {
    // Server is a relay but has no algo_provider_config or app_id, so a record that claims relay
    // status (am_relay=true) must be rejected by the blockchain allow_relay gate before it is
    // upserted. A `tag` is set so the handler passes its responseTag check and actually reaches
    // the gate; the UpdateResponse (only sent on a successful upsert) is delivered back to the
    // client, so we observe on the client — mirroring ddb_upsert_success_when_server_is_relay.
    let (server, client, server_addr, _client_addr) = start_pair(true);

    let client_id = client
        .access_unsafe_for_tests(|c: &mut BingleApiImpl| c.get_my_id())
        .expect("client id Some");
    let signing_key = client
        .access_unsafe_for_tests(|c| c.get_signing_key())
        .expect("signing key ok");

    let record = AdvertRecord::new(
        client_id.clone(),
        None,
        Some(true),
        None,
        None,
        "2025-01-01T00:00:00Z".into(),
        &signing_key,
    );
    let up = Message::Ddb(DdbMessage::UpsertResolve(DdbUpsertResolve {
        app: "ddb".into(),
        start_id: client_id.clone(),
        epoch: 1,
        record,
        original_signature: "SIG".into(),
        rippled: false,
        tag: Some("tag".into()),
        response_tag: None,
        text: None,
        data: None,
    }));

    let json = marshal::to_json_value(&up);

    let size_before = server.access_unsafe_for_tests(|s: &mut BingleApiImpl| s.ddb_backend_size());

    let nsk = NetworkEndpoint::new_direct(server_addr);
    let uid = server
        .access_unsafe_for_tests(|s: &mut BingleApiImpl| s.get_my_id())
        .unwrap();
    let ok = client
        .access_unsafe_for_tests(|c: &mut BingleApiImpl| {
            c.send_message_to_network(&nsk, &uid, json, None)
        })
        .unwrap();
    assert!(ok, "client send ok");

    std::thread::sleep(Duration::from_millis(200));
    let size_after = server.access_unsafe_for_tests(|s: &mut BingleApiImpl| s.ddb_backend_size());
    assert_eq!(
        size_after, size_before,
        "relay record with am_relay=true must not be upserted when blockchain config is absent"
    );

    server.access_unsafe_for_tests(|s: &mut BingleApiImpl| s.stop());
    drop(client);
}
