use rust_comms::engine::BingleAccessUnsafeForTests;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rust_comms::api::bingle_api::{BingleApi, Handle, NetworkEndpoint, StartOptions};
use rust_comms::api::bingle_api_impl::BingleApiImpl;
use rust_comms::dtls::{Dtls, DtlsOpenSsl};
use rust_comms::dtls::network_mux_udp::UdpNetworkMux;
use rust_comms::dtls::network_mux_trait::NetworkMux;
use rust_comms::messages::{Message, PlainTextMessage, RelayMessage};
use rust_comms::messages::types::{RelayCall, RelayListen};
use rust_comms::turn::turn_handler::{TurnClientHandler, TurnHandler};
use crate::relay::relay_states::test_util::init_test_logging;
use crate::util::test_util::{ADDRESS_10MIL, ADDRESS_RECEIVE, ADDRESS_SPEND};

#[path = "../test_util.rs"]
pub mod test_util;

#[path = "../dtls/pki.rs"]
pub mod pki;

fn addr(port: u16) -> SocketAddr { SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port) }

#[cfg_attr(not(target_os = "ios"), test)]

pub fn dtls_send_via_relay_end_to_end() {
    init_test_logging();

    // 1) Start a DTLS target node (server) with its own UDP mux and capture received payloads
    let certs = pki::generate_ed25519_test_certs();
    let server_cert_pem: Vec<u8> = certs.server_crt.clone();
    let server_key_pem: Vec<u8> = certs.server_key.clone();
    let ca_pem: Vec<u8> = certs.ca_crt.clone();

    let mut mux_target = UdpNetworkMux::bind((Ipv4Addr::LOCALHOST, 12000)).expect("bind target mux");

    // Server will record the first plaintext message it receives
    let received: Arc<Mutex<Vec<Vec<u8>>>> = Arc::new(Mutex::new(Vec::new()));
    let received_clone = received.clone();

    let mut dtls_server = DtlsOpenSsl::new()
        .with_server_signing_cert(server_cert_pem.clone())
        .with_server_signing_private_key(server_key_pem.clone())
        .with_client_cert(certs.client_crt.clone())
        .with_client_private_key(certs.client_key.clone())
        .with_ca_cert(ca_pem.clone())
        // App-layer verification keeps things simple for tests using custom PKI
        .with_app_layer_only_verification(true)
        .with_handle_message(Arc::new(move |_server, _from, _issuer, data| {
            if let Ok(mut g) = received_clone.lock() { g.push(data.to_vec()); }
        }));

    // Add TURN handler to dtls_server for client mode (non-relay)
    let turn_client = Arc::new(rust_comms::turn::turn_handler::TurnClientImpl::new());

    // Install TURN handler on the mutable mux_target before wrapping in Arc
    {
        let turn_client_clone = turn_client.clone();
        let local_address = Some(mux_target.local_addr().unwrap());
        let th: Arc<dyn Fn(&dyn NetworkMux, &SocketAddr, &[u8]) + Send + Sync> = Arc::new(move |source: &dyn NetworkMux, from: &SocketAddr, packet: &[u8]| {
            // Handle TURN ChannelData for client (non-relay) mode
            if let Some(wrapped) = turn_client_clone.handle_turn_incoming(Some(from), local_address, packet) {
                // Non-relay role: this packet is for us. Re-inject the stripped payload into the UDP mux
                if let Some(udp) = source.as_any().downcast_ref::<rust_comms::dtls::network_mux_udp::UdpNetworkMux>() {
                    udp.reprocess(&wrapped.network_endpoint, &wrapped.message);
                    log::info!("[handle turn target] reprocessed {} bytes from {}", wrapped.message.len(), wrapped.network_endpoint);
                } else {
                    log::warn!("[handle turn target] source is not UdpNetworkMux; cannot reprocess");
                }
            } else {
                log::debug!("[handle_turn target] handle_turn_incoming returned None (ignored)");
            }
        });
        mux_target.set_handle_turn(Some(&th));
    }
    let mux_target_arc = Arc::new(mux_target);
    mux_target_arc.start().expect("start target mux");
    dtls_server.start(mux_target_arc.clone()).expect("start dtls server");

    // 2) Start a relay node using the BingleApiImpl pattern (as in endpoint_identify_via_forced_stun)
    let relay_port = 13000; // test_util::find_unused_loopback_port();
    let relay_addr = addr(relay_port);
    let relay_opts = StartOptions {
        handle: Handle::from("relay"),
        algo_passphrase: Some(test_util::PASSPHRASE_10MIL.to_string()),
        static_ip: Some(relay_addr),
        am_relay: true,
        stun_servers: None,
        algo_provider_config: None,
        algo_network: None,
        app_id: None,
        asset_id: None,
        log_level: None, handle_cache_expiry: None,
    };
    let relay_api = BingleApiImpl::new(&relay_opts);
    relay_api.access_unsafe_for_tests(|a: &mut BingleApiImpl| a.start(&relay_opts)).expect("start relay api");

    // 3) Send RelayListen from the DTLS target node to the relay and validate registration
    let listen_msg = Message::Relay(RelayMessage::Listen(RelayListen { app: None }));
    let listen_msg_bytes = serde_json::to_vec(&listen_msg).expect("serialize listenMsg");
    dtls_server.send(&NetworkEndpoint::new_direct(relay_addr), &listen_msg_bytes).expect("send listenMsg");

    turn_client.handle_listen(ADDRESS_10MIL, &relay_addr);

    // 1) Start a DTLS source node (client) with its own UDP mux and capture received payloads
    let certs2 = pki::generate_ed25519_test_certs_with_key(ADDRESS_SPEND.to_string().as_str());
    let server_cert_pem2: Vec<u8> = certs2.server_crt.clone();
    let server_key_pem2: Vec<u8> = certs2.server_key.clone();
    let ca_pem2: Vec<u8> = certs2.ca_crt.clone();

    let mut mux_client = UdpNetworkMux::bind((Ipv4Addr::LOCALHOST, 14000)).expect("bind target mux 2");

    // Add TURN handler to dtls_client for client mode (non-relay)
    let turn_client2 = Arc::new(rust_comms::turn::turn_handler::TurnClientImpl::new());
    let turn_client2_clone = turn_client2.clone();

    // Install TURN handler on the mutable mux_client before wrapping in Arc
    {
        let local_address = Some(mux_client.local_addr().unwrap());
        let th: Arc<dyn Fn(&dyn NetworkMux, &SocketAddr, &[u8]) + Send + Sync> = Arc::new(move |source: &dyn NetworkMux, from: &SocketAddr, packet: &[u8]| {
        // Handle TURN ChannelData for client (non-relay) mode
        if let Some(wrapped) = turn_client2_clone.handle_turn_incoming(Some(from), local_address, packet) {
            // Non-relay role: this packet is for us. Re-inject the stripped payload into the UDP mux
            if let Some(udp) = source.as_any().downcast_ref::<rust_comms::dtls::network_mux_udp::UdpNetworkMux>() {
                udp.reprocess(&wrapped.network_endpoint, &wrapped.message);
                log::info!("[handle turn client] reprocessed {} bytes from {}", wrapped.message.len(), wrapped.network_endpoint);
            } else {
                log::warn!("[handle turn client] source is not UdpNetworkMux; cannot reprocess");
            }
        } else {
            log::debug!("[handle_turn client] handle_turn_incoming returned None (ignored)");
        }
    });
        mux_client.set_handle_turn(Some(&th));
    }

    let mux_client_arc = Arc::new(mux_client);

    // Shared state to capture the channel from RelayResponse
    let captured_channel: Arc<Mutex<Option<u16>>> = Arc::new(Mutex::new(None));
    let captured_channel_clone = captured_channel.clone();

    let mut dtls_client = DtlsOpenSsl::new()
        .with_server_signing_cert(server_cert_pem2.clone())
        .with_server_signing_private_key(server_key_pem2.clone())
        .with_client_cert(certs2.client_crt.clone())
        .with_client_private_key(certs2.client_key.clone())
        .with_ca_cert(ca_pem2.clone())
        // App-layer verification keeps things simple for tests using custom PKI
        .with_app_layer_only_verification(true)
        .with_handle_message(Arc::new(move |_server, _from, issuer, data| {
            log::info!("dtls_client received: {:?} on ${issuer}", data);
            if let Ok(message) = serde_json::from_slice::<Message>(data) {
                log::info!("Parsed message: {:?}", message);
                if let Message::Relay(RelayMessage::RelayResponse(relay_response)) = message {
                    if let Some(channel) = relay_response.channel {
                        if let Ok(mut guard) = captured_channel_clone.lock() {
                            *guard = Some(channel);
                            log::info!("Captured channel: {}", channel);
                        }
                    }
                }
            } else {
                log::warn!("Failed to parse message from JSON bytes");
            }
        }));

    mux_client_arc.start().expect("start client mux");
    dtls_client.start(mux_client_arc.clone()).expect("start dtls client");

    // 4) Send RelayCall from dtls client to relay
    let call_msg = Message::Relay(RelayMessage::Call(RelayCall { app: None, called_id: ADDRESS_RECEIVE.to_string() }));
    let call_msg_bytes = serde_json::to_vec(&call_msg).expect("serialize call_msg");
    dtls_client.send(&NetworkEndpoint::new_direct(relay_addr), &call_msg_bytes).expect("send listenMsg");

    // Wait up to 3s for the channel to be captured from RelayResponse
    let start = Instant::now();
    let mut channel_received = false;
    while start.elapsed() < Duration::from_secs(3) {
        if let Ok(guard) = captured_channel.lock() {
            if guard.is_some() {
                channel_received = true;
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(channel_received, "Should receive RelayResponse with channel");

    // Extract the channel value and create relay endpoint
    let channel = {
        let guard = captured_channel.lock().expect("lock captured_channel");
        guard.expect("channel should be present")
    };

    turn_client2.handle_call_response(&(relay_addr.clone()),
                                      &(mux_target_arc.local_addr().expect("mux target has local addr")), channel, ADDRESS_10MIL
    );
    
    let relay_endpoint = NetworkEndpoint::new_relay(ADDRESS_10MIL.to_string(), Some(relay_addr), Some(channel));

    let test_msg = Message::PlainText(PlainTextMessage { app:None, r#type:None, text: "Via relay".to_string() });
    let test_msg_bytes = serde_json::to_vec(&test_msg).expect("serialize listenMsg");

    dtls_client.send(&relay_endpoint, &test_msg_bytes).expect("send test_msg");
}

// Minimal API stub for Router context in this test
#[allow(dead_code)]
struct MockApi;
impl rust_comms::api::bingle_api::BingleApi for MockApi { 
    fn set_on_listening(&mut self, _handler: Option<std::sync::Arc<rust_comms::api::bingle_api::OnListeningHandler>>) {} 
    fn get_algo_provider_config(&self) -> Option<rust_comms::blockchain::algo_ops::AlgoChainConfig> { None } 
    fn get_handle(&self) -> Option<String> { None } 
    fn get_user_id(&self) -> Option<String> { None }
    fn debug_print_options(&self) {}
    fn get_my_id(&self) -> Option<String> { None }
    fn get_app_id(&self) -> Option<u64> { None }
    fn start(&mut self, _options: &StartOptions) -> Result<(), String> { Ok(()) }
    fn stop(&mut self) {}
    fn network_change(&mut self) {}
    fn handle_lookup(&self, _handle: &rust_comms::api::bingle_api::Handle) -> Result<Option<rust_comms::api::bingle_api::UserId>, String> { Ok(None) }
    fn send_message_to_id(&self, _user_id: &rust_comms::api::bingle_api::UserId, _message: serde_json::Value, _progress: Option<Arc<rust_comms::api::bingle_api::ProgressCallback>>) -> bool { false }
    fn send_message_to_handle(&self, _handle: &rust_comms::api::bingle_api::Handle, _message: serde_json::Value, _progress: Option<Arc<rust_comms::api::bingle_api::ProgressCallback>>) -> bool { false }
    fn send_message_to_network(&self, _network_source_key: &NetworkEndpoint, _user_id: &rust_comms::api::bingle_api::UserId, _message: serde_json::Value, _progress: Option<Arc<rust_comms::api::bingle_api::ProgressCallback>>) -> bool { false }
    fn send_message_to_id_with_response(&self, _user_id: &rust_comms::api::bingle_api::UserId, _message: serde_json::Value, _progress: Option<Arc<rust_comms::api::bingle_api::ProgressCallback>>) -> Result<serde_json::Value, String> { Err("ni".into()) }
    fn send_message_to_handle_with_response(&self, _handle: &rust_comms::api::bingle_api::Handle, _message: serde_json::Value, _progress: Option<Arc<rust_comms::api::bingle_api::ProgressCallback>>) -> Result<serde_json::Value, String> { Err("ni".into()) }
    fn send_message_to_network_with_response(&self, _network_source_key: &NetworkEndpoint, _user_id: &rust_comms::api::bingle_api::UserId, _message: serde_json::Value, _progress: Option<Arc<rust_comms::api::bingle_api::ProgressCallback>>) -> Result<serde_json::Value, String> { Err("ni".into()) }
    fn set_on_message(&mut self, _handler: Option<Arc<rust_comms::api::bingle_api::OnMessageHandler>>) {}
    fn set_on_connect(&mut self, _handler: Option<Arc<rust_comms::api::bingle_api::OnConnectHandler>>) {}
}

impl rust_comms::api::bingle_api::BingleApiInternal for MockApi {
    fn set_state(&self, _s: rust_comms::engine::EngineState) {}
    fn get_state(&self) -> rust_comms::engine::EngineState { rust_comms::engine::EngineState::StunIdentify }
    fn set_nat_type(&self, _n: rust_comms::engine::NatType) {}
    fn get_last_public_addr(&self) -> Option<std::net::SocketAddr> { None }
    fn ddb_register_ip(&self, _e: std::net::SocketAddr, _a: bool) -> Result<(), String> { Ok(()) }
    fn ddb_register_relay(&self, _r: String, _s: Option<String>) -> Result<(), String> { Ok(()) }
    fn update_turn_listener_relay(&self, _r: String, _a: std::net::SocketAddr) -> Result<(), String> { Ok(()) }
    fn turn_client_handle_listen_response(&self, _a: std::net::SocketAddr, _r: String) {}
    fn turn_lookup_addr_by_id(&self, _i: String) -> Option<std::net::SocketAddr> { None }
    fn turn_handle_call(&self, _s: std::net::SocketAddr, _d: std::net::SocketAddr) -> i32 { -1 }
    fn turn_handle_listen(&self, _i: String, _s: std::net::SocketAddr) -> bool { false }
    fn turn_handle_called(&self, _s: std::net::SocketAddr, _d: std::net::SocketAddr, _c: u16) {}
    fn notify_listening(&self, _l: bool) {}
    fn get_relay_state(&self) -> String { "off".into() }
}
