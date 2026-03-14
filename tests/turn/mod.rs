use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use rust_comms::turn::turn_handler::{TurnHandler, TurnHandlerImpl, TurnRelayHandler};

// Include additional TURN tests in this directory
#[path = "handle_listen_validation.rs"]
pub mod handle_listen_validation;

// Client-side tests
#[path = "client_impl.rs"]
pub mod client_impl;

// Relay-side tests
#[path = "relay_impl.rs"]
pub mod relay_impl;

// Client handler (impl split) tests
#[path = "client_handler_impl.rs"]
pub mod client_handler_impl;

fn addr(port: u16) -> SocketAddr { SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port) }

#[cfg_attr(not(target_os = "ios"), test)]
pub fn unit_turn_handle_call_allocates_in_range_and_reuses() {
    let handler = TurnHandlerImpl::new();
    let peer = addr(5000);

    let ch1 = TurnRelayHandler::handle_call(&handler, &peer, &peer);
    assert!(ch1 >= 0, "channel should be non-negative");
    let ch1u = ch1 as u16;
    assert!(ch1u >= 0x4000 && ch1u <= 0x7FFE, "channel in TURN range: {:#X}", ch1u);

    let ch2 = TurnRelayHandler::handle_call(&handler, &peer, &peer);
    assert_eq!(ch1, ch2, "channel must be reused for same (source,dest)");
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn unit_turn_wraps_and_unwraps_channel_data_with_padding() {
    let handler = TurnHandlerImpl::new();
    let src = addr(6001);
    let dst = addr(6000);
    // Register listen for gating: destination (callee) must be allowed
    assert!(handler.handle_listen("DSTID3", &dst));
    let ch = TurnRelayHandler::handle_call(&handler, &src, &dst);
    assert!(ch >= 0);

    let payload = b"abc"; // len 3 -> padding 1 byte
    let wrapped = handler.send_turn_outgoing(&src, &dst, payload);
    assert!(wrapped.is_some(), "expected wrapped message");
    let wrapped = wrapped.unwrap();

    // Validate ChannelData header
    let msg = wrapped.message;
    assert!(msg.len() >= 4);
    let ch_be = u16::from_be_bytes([msg[0], msg[1]]) as i32;
    let len = u16::from_be_bytes([msg[2], msg[3]]) as usize;
    assert_eq!(ch_be, ch);
    assert_eq!(len, payload.len());
    // Padding to 4-byte boundary should be present
    let padded_len = (len + 3) & !3;
    assert_eq!(msg.len(), 4 + padded_len);

    // Now feed back to incoming parser
    let incoming = handler.handle_turn_incoming(Some(&src), Some(dst), &msg);
    assert!(incoming.is_some(), "expected incoming to parse");
    let incoming = incoming.unwrap();
    assert_eq!(incoming.ip_address, dst);
    assert_eq!(incoming.message, payload);
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn unit_turn_incoming_invalid_packets_return_none() {
    let handler = TurnHandlerImpl::new();
    // Too short
    assert!(handler.handle_turn_incoming(Some(&addr(1)), Some(addr(2)), &[0x40]).is_none());
    // Declared len longer than actual
    let bad = [0x40, 0x00, 0x00, 0x10];
    assert!(handler.handle_turn_incoming(Some(&addr(1)), Some(addr(2)), &bad).is_none());
}
