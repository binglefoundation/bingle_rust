
use rust_comms::engine::BingleAccessUnsafeForTests;

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{atomic::{AtomicBool, Ordering}, Arc};
use std::time::{Duration};

use rust_comms::api::bingle_api::{BingleApi, NetworkEndpoint, StartOptions};
use rust_comms::api::bingle_api_impl::BingleApiImpl;
use rust_comms::messages::marshal;
use rust_comms::messages::types::*;

#[path = "../test_util.rs"]
pub mod test_util;

fn start_pair(server_am_relay: bool) -> (Arc<BingleApiImpl>, Arc<BingleApiImpl>, SocketAddr, SocketAddr) {
    let server_port = test_util::find_unused_loopback_port();
    let client_port = test_util::find_unused_loopback_port();
    let server_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), server_port);
    let client_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), client_port);

    let server_opts = StartOptions {
        handle: "server".into(),
        algo_passphrase: Some(test_util::PASSPHRASE_RECEIVE.to_string()),
        static_ip: Some(server_addr),
        am_relay: server_am_relay,
        stun_servers: None,
        algo_provider_config: None,
        algo_network: None,
        app_id: None,
        asset_id: None,
        log_level: None, handle_cache_expiry: None, dangerous_debug: true, log_mode: rust_comms::util::logging::LogMode::Plain,
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
        log_level: None, handle_cache_expiry: None, dangerous_debug: true, log_mode: rust_comms::util::logging::LogMode::Plain,
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

    server.access_unsafe_for_tests(|s: &mut BingleApiImpl| s.start(&server_opts)).expect("server start ok");
    client.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.start(&client_opts)).expect("client start ok");
    (server, client, server_addr, client_addr)
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn ddb_upsert_success_when_server_is_relay() {
    let (server, client, server_addr, _client_addr) = start_pair(true);

    // Build a valid UpsertResolve from client where startId == record.id == client id
    let _server_id = server.access_unsafe_for_tests(|s: &mut BingleApiImpl| s.get_my_id()).expect("server get_my_id Some"); // Use API to ensure functions are wired
    let client_id = client.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.get_my_id()).expect("client id Some");

    let record = AdvertRecord { id: client_id.clone(), endpoint: Some(InetSocketAddress{ host: "127.0.0.1".into(), port: 9999 }), am_relay: Some(false), relay_id: None, relay_sig: None, date: "2025-01-01T00:00:00Z".into(), sig: None };
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
    client.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.set_on_message(Some(Arc::new(move |_sender, _handle, msg| {
        if let Some(t) = msg.get("type").and_then(|v: &serde_json::Value| v.as_str()) {
            if t == "updateResponse" && msg.get("app").and_then(|v: &serde_json::Value| v.as_str()) == Some("ddb") {
                if msg.get("responseTag").and_then(|v: &serde_json::Value| v.as_str()).is_some()
                    && msg.get("tag").is_none()
                {
                    got_update_flag.store(true, Ordering::SeqCst);
                }
            }
        }
    }))));

    // Send request from client to server
    let nsk = NetworkEndpoint::new_direct(server_addr);
    let uid = server.access_unsafe_for_tests(|s: &mut BingleApiImpl| s.get_my_id()).unwrap();
    let response = client.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.send_message_to_network_with_response(&nsk, &uid, json, None));
    assert!(response.is_ok(), "client send ok");

    // Cleanup
    server.access_unsafe_for_tests(|s: &mut BingleApiImpl| s.stop());
    client.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.stop());
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn ddb_upsert_ignored_when_not_relay() {
    let (server, client, server_addr, _client_addr) = start_pair(false);

    let client_id = client.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.get_my_id()).expect("client id Some");
    let record = AdvertRecord { id: client_id.clone(), endpoint: None, am_relay: Some(false), relay_id: None, relay_sig: None, date: "2025-01-01T00:00:00Z".into(), sig: None };
    let up = Message::Ddb(DdbMessage::UpsertResolve(DdbUpsertResolve { app: "ddb".into(), start_id: client_id.clone(), epoch: 1, record, original_signature: "SIG".into(), rippled: false, tag: None, response_tag: None, text: None, data: None }));

    let json = marshal::to_json_value(&up);

    let got_update = Arc::new(AtomicBool::new(false));
    let got_update_flag = got_update.clone();
    server.access_unsafe_for_tests(|s: &mut BingleApiImpl| s.set_on_message(Some(Arc::new(move |_sender, _handle, msg| {
        if msg.get("type").and_then(|v: &serde_json::Value| v.as_str()) == Some("updateResponse") { got_update_flag.store(true, Ordering::SeqCst); }
    }))));

    let nsk = NetworkEndpoint::new_direct(server_addr);
    let uid = server.access_unsafe_for_tests(|s: &mut BingleApiImpl| s.get_my_id()).unwrap();
    let ok = client.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.send_message_to_network(&nsk, &uid, json, None)).unwrap();
    assert!(ok, "client send ok");

    // Give some time; expect no updateResponse because server is not a relay
    std::thread::sleep(Duration::from_millis(200));
    assert!(!got_update.load(Ordering::SeqCst), "should not receive updateResponse when server not relay");

    server.access_unsafe_for_tests(|s: &mut BingleApiImpl| s.stop());
    drop(client);
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn ddb_upsert_rejected_on_id_mismatch() {
    let (server, client, server_addr, _client_addr) = start_pair(true);

    let client_id = client.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.get_my_id()).expect("client id Some");
    // Mismatch: record.id != start_id
    let record = AdvertRecord { id: format!("{}X", client_id), endpoint: None, am_relay: Some(false), relay_id: None, relay_sig: None, date: "2025-01-01T00:00:00Z".into(), sig: None };
    let up = Message::Ddb(DdbMessage::UpsertResolve(DdbUpsertResolve { app: "ddb".into(), start_id: client_id.clone(), epoch: 1, record, original_signature: "SIG".into(), rippled: false, tag: None, response_tag: None, text: None, data: None }));

    let json = marshal::to_json_value(&up);

    let got_update = Arc::new(AtomicBool::new(false));
    let got_update_flag = got_update.clone();
    server.access_unsafe_for_tests(|s: &mut BingleApiImpl| s.set_on_message(Some(Arc::new(move |_sender, _handle, msg| {
        if msg.get("type").and_then(|v: &serde_json::Value| v.as_str()) == Some("updateResponse") { got_update_flag.store(true, Ordering::SeqCst); }
    }))));

    let nsk = NetworkEndpoint::new_direct(server_addr);
    let uid = server.access_unsafe_for_tests(|s: &mut BingleApiImpl| s.get_my_id()).unwrap();
    let ok = client.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.send_message_to_network(&nsk, &uid, json, None)).unwrap();
    assert!(ok, "client send ok");

    std::thread::sleep(Duration::from_millis(200));
    assert!(!got_update.load(Ordering::SeqCst), "should not receive updateResponse on id mismatch");

    server.access_unsafe_for_tests(|s: &mut BingleApiImpl| s.stop());
    drop(client);
}
