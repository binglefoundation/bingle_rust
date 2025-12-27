#![cfg(not(target_os = "ios"))]

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{atomic::{AtomicBool, Ordering}, Arc};
use std::time::{Duration};

use rust_comms::api::bingle_api::{BingleApi, NetworkSourceKey, StartOptions};
use rust_comms::api::bingle_api_impl::BingleApiImpl;
use rust_comms::messages::marshal;
use rust_comms::messages::types::*;

#[path = "../test_util.rs"]
mod test_util;

fn start_pair(server_am_relay: bool) -> (BingleApiImpl, BingleApiImpl, SocketAddr, SocketAddr) {
    let server_port = test_util::find_unused_loopback_port();
    let client_port = test_util::find_unused_loopback_port();
    let server_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), server_port);
    let client_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), client_port);

    let mut server = BingleApiImpl::new(&StartOptions::default());
    let mut client = BingleApiImpl::new(&StartOptions::default());

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
    server.start(&server_opts).expect("server start ok");
    client.start(&client_opts).expect("client start ok");
    (server, client, server_addr, client_addr)
}

#[test]
fn ddb_upsert_success_when_server_is_relay() {
    let (mut server, mut client, server_addr, _client_addr) = start_pair(true);

    // Build a valid UpsertResolve from client where startId == record.id == client id
    let _server_id = server.get_my_id().expect("server get_my_id Some"); // Use API to ensure functions are wired
    let client_id = client.get_my_id().expect("client id Some");

    let record = AdvertRecord { id: client_id.clone(), endpoint: Some(InetSocketAddress{ host: "127.0.0.1".into(), port: 9999 }), am_relay: Some(false), relay_id: None, relay_sig: None, date: "2025-01-01T00:00:00Z".into(), sig: None };
    let up = Message::Ddb(DdbMessage::UpsertResolve(DdbUpsertResolve {
        app: "ddb".into(),
        start_id: client_id.clone(),
        epoch: 1,
        record: record.clone(),
        original_signature: "SIG".into(),
        rippled: false,
        tag: None,
        response_tag: Some("rt1".into()),
        text: None,
        data: None,
    }));

    let json = marshal::to_json_value(&up);

    // Observe UpdateResponse via CLIENT on_message handler (server sends response back to client)
    let got_update = Arc::new(AtomicBool::new(false));
    let got_update_flag = got_update.clone();
    client.set_on_message(Some(Arc::new(move |_sender, _handle, msg| {
        if let Some(t) = msg.get("type").and_then(|v| v.as_str()) {
            if t == "updateResponse" && msg.get("app").and_then(|v| v.as_str()) == Some("ddb") {
                if msg.get("responseTag").and_then(|v| v.as_str()) == Some("rt1") {
                    got_update_flag.store(true, Ordering::SeqCst);
                }
            }
        }
    })));

    // Send request from client to server
    let nsk = NetworkSourceKey::new_direct(server_addr);
    let uid = server.get_my_id().unwrap();
    let response = client.send_message_to_network_with_response(&nsk, &uid, json, None);
    assert!(response.is_ok(), "client send ok");

    // Cleanup
    server.stop();
    client.stop();
}

#[test]
fn ddb_upsert_ignored_when_not_relay() {
    let (mut server, client, server_addr, _client_addr) = start_pair(false);

    let client_id = client.get_my_id().expect("client id Some");
    let record = AdvertRecord { id: client_id.clone(), endpoint: None, am_relay: Some(false), relay_id: None, relay_sig: None, date: "2025-01-01T00:00:00Z".into(), sig: None };
    let up = Message::Ddb(DdbMessage::UpsertResolve(DdbUpsertResolve { app: "ddb".into(), start_id: client_id.clone(), epoch: 1, record, original_signature: "SIG".into(), rippled: false, tag: None, response_tag: Some("r2".into()), text: None, data: None }));

    let json = marshal::to_json_value(&up);

    let got_update = Arc::new(AtomicBool::new(false));
    let got_update_flag = got_update.clone();
    server.set_on_message(Some(Arc::new(move |_sender, _handle, msg| {
        if msg.get("type").and_then(|v| v.as_str()) == Some("updateResponse") { got_update_flag.store(true, Ordering::SeqCst); }
    })));

    let nsk = NetworkSourceKey::new_direct(server_addr);
    let uid = server.get_my_id().unwrap();
    let ok = client.send_message_to_network(&nsk, &uid, json, None);
    assert!(ok, "client send ok");

    // Give some time; expect no updateResponse because server is not a relay
    std::thread::sleep(Duration::from_millis(200));
    assert!(!got_update.load(Ordering::SeqCst), "should not receive updateResponse when server not relay");

    server.stop();
    drop(client);
}

#[test]
fn ddb_upsert_rejected_on_id_mismatch() {
    let (mut server, client, server_addr, _client_addr) = start_pair(true);

    let client_id = client.get_my_id().expect("client id Some");
    // Mismatch: record.id != start_id
    let record = AdvertRecord { id: format!("{}X", client_id), endpoint: None, am_relay: Some(false), relay_id: None, relay_sig: None, date: "2025-01-01T00:00:00Z".into(), sig: None };
    let up = Message::Ddb(DdbMessage::UpsertResolve(DdbUpsertResolve { app: "ddb".into(), start_id: client_id.clone(), epoch: 1, record, original_signature: "SIG".into(), rippled: false, tag: None, response_tag: Some("r3".into()), text: None, data: None }));

    let json = marshal::to_json_value(&up);

    let got_update = Arc::new(AtomicBool::new(false));
    let got_update_flag = got_update.clone();
    server.set_on_message(Some(Arc::new(move |_sender, _handle, msg| {
        if msg.get("type").and_then(|v| v.as_str()) == Some("updateResponse") { got_update_flag.store(true, Ordering::SeqCst); }
    })));

    let nsk = NetworkSourceKey::new_direct(server_addr);
    let uid = server.get_my_id().unwrap();
    let ok = client.send_message_to_network(&nsk, &uid, json, None);
    assert!(ok, "client send ok");

    std::thread::sleep(Duration::from_millis(200));
    assert!(!got_update.load(Ordering::SeqCst), "should not receive updateResponse on id mismatch");

    server.stop();
    drop(client);
}
