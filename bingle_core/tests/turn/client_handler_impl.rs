use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use bingle_core::turn::turn_client_handler_impl::TurnClientHandlerImpl;
use bingle_core::turn::turn_handler::{TurnClientHandler, TurnHandler};

fn addr(port: u16) -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port)
}

fn build_channel_data(channel: u16, data: &[u8]) -> Vec<u8> {
    let pad = (4 - (data.len() % 4)) % 4;
    let mut out = Vec::with_capacity(4 + data.len() + pad);
    out.extend_from_slice(&channel.to_be_bytes());
    out.extend_from_slice(&(data.len() as u16).to_be_bytes());
    out.extend_from_slice(data);
    if pad > 0 {
        out.extend(std::iter::repeat(0u8).take(pad));
    }
    out
}

// 1) handle_listen should fail on the client (unexpected relay command listen on client)
#[test]
#[cfg(not(target_os = "ios"))]
pub fn client_handle_listen_fails_with_error() {
    let client = TurnClientHandlerImpl::new();
    let src = addr(8001);
    // Expected behavior: client should reject listen and log an error
    assert!(
        !client.handle_listen("SRCID_CLIENT", &src),
        "client-side handle_listen should fail"
    );
}

// 2) handle_turn_incoming from the listener relay on an open channel should return a WrappedMessageWithNetworkEndpoint
#[test]
#[cfg(not(target_os = "ios"))]
pub fn client_incoming_from_listener_relay_on_open_channel() {
    let client = TurnClientHandlerImpl::new();
    let source = addr(8010);
    let dest = addr(8011);
    let relay = addr(48010);
    let ch: u16 = 0x4001;

    // Register the relay via ListenResponse
    TurnClientHandler::handle_listen_response(&client, &relay, "RELAY_A");
    // Also register a channel mapping by an incoming Called event
    TurnClientHandler::handle_called(&client, &source, &dest, ch);

    // Build ChannelData packet
    let payload = b"hello-listener".to_vec();
    let packet = build_channel_data(ch, &payload);

    let wrapped = client
        .handle_turn_incoming(Some(&relay), Some(dest), &packet)
        .expect("expected WrappedMessageWithNetworkEndpoint from registered listener relay");

    // For listener relay branch, ip_address should echo the relay address and endpoint should be relay-type
    assert_eq!(wrapped.ip_address, relay);
    assert_eq!(wrapped.message, payload);
    let nep = wrapped.network_endpoint;
    assert!(nep.is_relay(), "should be a relay endpoint");
    assert_eq!(nep.relay_channel().expect("channel present"), ch);
    assert_eq!(nep.relay_address().expect("relay address present"), relay);
    assert_eq!(nep.relay_id().expect("relay id present"), "RELAY_A");
}

// 3) handle_turn_incoming with a message from a called relay returns the WrappedMessageWithNetworkEndpoint
#[test]
#[cfg(not(target_os = "ios"))]
pub fn client_incoming_from_called_relay_returns_wrapped() {
    let client = TurnClientHandlerImpl::new();
    let source = addr(8020);
    let relay_dest = addr(8021);
    let ch: u16 = 0x4002;

    // Simulate a CallResponse: establishes channel and should register allowed relay id/address mapping
    TurnHandler::handle_call_response(&client, &source, &relay_dest, ch, "RELAY_B");

    // Build ChannelData packet and simulate it arriving from the relay endpoint.
    // NOTE: Current implementation lacks a relay address parameter here; once available, pass relay address.
    let payload = b"hello-called".to_vec();
    let packet = build_channel_data(ch, &payload);

    // Expected: incoming from the relay should be accepted and wrapped as relay endpoint
    // This test is ignored until implementation provides/uses the relay UDP address mapping.
    let wrapped = client.handle_turn_incoming(Some(&relay_dest), Some(relay_dest), &packet);
    assert!(
        wrapped.is_some(),
        "expected wrapped message from called relay"
    );
    let wrapped = wrapped.unwrap();

    // Assert ip_address is 127.0.0.1:8021
    assert_eq!(wrapped.ip_address.to_string(), "127.0.0.1:8021");

    // Assert network_endpoint has correct relay properties
    assert_eq!(
        wrapped.network_endpoint.relay_id().expect("A relay id"),
        "RELAY_B"
    );
    assert_eq!(wrapped.network_endpoint.relay_channel(), Some(16386));
    assert_eq!(
        wrapped
            .network_endpoint
            .relay_address()
            .unwrap()
            .to_string(),
        "127.0.0.1:8021"
    );
    assert_eq!(wrapped.network_endpoint.inet_socket_address(), None);

    // Assert message content matches "hello-called"
    assert_eq!(wrapped.message, b"hello-called");
}

// 4) send_turn_outgoing wraps the message with the appropriate channel and fails if no channel
#[test]
#[cfg(not(target_os = "ios"))]
pub fn client_send_outgoing_wraps_and_fails_without_channel() {
    let client = TurnClientHandlerImpl::new();
    let source = addr(8030);
    let dest = addr(8031);
    let ch: u16 = 0x4003;

    // No mapping yet: should fail to send
    let none = client.send_turn_outgoing(&source, &dest, b"ping");
    assert!(none.is_none(), "send should fail without channel mapping");

    // Establish mapping via Called
    TurnClientHandler::handle_called(&client, &source, &dest, ch);
    let wrapped = client
        .send_turn_outgoing(&source, &dest, b"ping")
        .expect("send should succeed with channel mapping");

    // Validate ChannelData header
    let msg = wrapped.message;
    assert!(msg.len() >= 4);
    let ch_be = u16::from_be_bytes([msg[0], msg[1]]);
    let len = u16::from_be_bytes([msg[2], msg[3]]) as usize;
    assert_eq!(ch_be, ch);
    assert_eq!(len, 4);
    assert_eq!(&msg[4..4 + len], b"ping");
}

// 5) handle_listen_response registers the allowed relay id and address mapping (but not the channel map)
#[test]
#[cfg(not(target_os = "ios"))]
pub fn client_listen_response_registers_allowed_but_not_channel() {
    let client = TurnClientHandlerImpl::new();
    let relay = addr(48040);

    // Register the relay mapping
    TurnClientHandler::handle_listen_response(&client, &relay, "RELAY_LISTEN_ONLY");

    // Incoming from relay should be accepted and wrapped
    let ch: u16 = 0x4004;
    let payload = b"from-relay".to_vec();
    let packet = build_channel_data(ch, &payload);
    let wrapped = client
        .handle_turn_incoming(Some(&relay), Some(addr(1)), &packet)
        .expect("incoming from registered relay should succeed");
    assert_eq!(wrapped.ip_address, relay);
    assert_eq!(wrapped.message, payload);

    // But sending without a channel mapping should still fail
    let src = addr(8040);
    let dst = addr(8041);
    assert!(client.send_turn_outgoing(&src, &dst, b"x").is_none());
}

// 6) handle_call_response registers the allowed relay id/address mapping and also the channel mapping
#[test]
#[cfg(not(target_os = "ios"))]
pub fn client_call_response_registers_allowed_and_channel() {
    let client = TurnClientHandlerImpl::new();
    let source = addr(8050);
    let dest = addr(8051);
    let ch: u16 = 0x4005;

    TurnHandler::handle_call_response(&client, &source, &dest, ch, "RELAY_C");

    // After call response, sending should succeed
    let _wrapped = client
        .send_turn_outgoing(&source, &dest, b"ok")
        .expect("send should succeed after call response");

    // And incoming using mapped channel should also be accepted, even if sender is the source address
    let packet = build_channel_data(ch, b"ok");
    let incoming = client
        .handle_turn_incoming(Some(&source), Some(dest), &packet)
        .expect("incoming should succeed after call response");
    assert_eq!(incoming.message, b"ok");
}

// 7) handle_called validates that we have an open listen to the relay and registers the channel mapping
// Positive path: when listen to relay is open, handle_called registers mapping
#[test]
#[cfg(not(target_os = "ios"))]
pub fn client_called_after_listen_response_registers_channel() {
    let client = TurnClientHandlerImpl::new();
    let source = addr(8060);
    let dest = addr(8061);
    let relay = addr(48060);
    let ch: u16 = 0x4006;

    TurnClientHandler::handle_listen_response(&client, &relay, "RELAY_D");
    TurnClientHandler::handle_called(&client, &source, &dest, ch);

    // Should be able to send now
    let wrapped = client
        .send_turn_outgoing(&source, &dest, b"y")
        .expect("send should succeed after Called with listen");
    assert_eq!(wrapped.ip_address, dest);
}
