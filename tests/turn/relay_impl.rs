use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use rust_comms::api::network_endpoint::NetworkEndpoint;
use rust_comms::turn::turn_handler::{TurnHandler, TurnRelayHandler};
use rust_comms::turn::turn_relay_handler_impl::TurnRelayHandlerImpl;

fn addr(port: u16) -> SocketAddr { SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port) }

fn build_channel_data(channel: u16, data: &[u8]) -> Vec<u8> {
    let pad = (4 - (data.len() % 4)) % 4;
    let mut out = Vec::with_capacity(4 + data.len() + pad);
    out.extend_from_slice(&channel.to_be_bytes());
    out.extend_from_slice(&(data.len() as u16).to_be_bytes());
    out.extend_from_slice(data);
    if pad > 0 { out.extend(std::iter::repeat(0u8).take(pad)); }
    out
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn relay_handle_listen_updates_allowed_entries() {
    let handler = TurnRelayHandlerImpl::new();
    let src = addr(41001);
    let id = "RELAY_ID_A";

    // Register listen for this (id, addr)
    assert!(handler.handle_listen(id, &src), "handle_listen should succeed");

    // Validate allowed* entries via public behavior: is_ip_allowed and inbound path without channel mapping
    assert!(handler.is_ip_allowed(src.ip()), "expected IP to be allowed after listen");

    // Build a ChannelData packet with arbitrary channel; since no ch mapping exists, handler should
    // return a WrappedMessageWithNetworkEndpoint that points at the sender with relay endpoint using our id
    let ch: u16 = 0x4001;
    let payload = b"hello".to_vec();
    let packet = build_channel_data(ch, &payload);

    let wrapped = handler
        .handle_turn_incoming(Some(&src), Some(addr(9_999)), &packet)
        .expect("handle_turn_incoming should accept packet from allowed address");

    assert_eq!(wrapped.ip_address, src, "expected ip_address to echo sender for unmapped channel");
    assert_eq!(wrapped.message, payload, "payload must be preserved");

    let nep: NetworkEndpoint = wrapped.network_endpoint;
    assert!(nep.is_relay(), "expected a relay endpoint");
    assert_eq!(nep.relay_channel().expect("relay channel present"), ch);
    // In this branch TurnRelayHandlerImpl sets relay_address to the sender address
    assert_eq!(nep.relay_address().expect("relay address present"), src);
    assert_eq!(nep.relay_id().expect("relay id present"), id);
}

#[cfg_attr(not(target_os = "ios"), test)]
pub fn relay_handle_call_sets_mappings_and_incoming_both_directions() {
    let handler = TurnRelayHandlerImpl::new();
    let source = addr(42000);
    let dest = addr(42001);
    let relay_public = addr(47000);

    // Allocate a channel for (source -> dest); this should also register both addresses as allowed
    let ch_i32 = TurnRelayHandler::handle_call(&handler, "SRCID", "DSTID", &source, &dest);
    assert!(ch_i32 >= 0, "channel should be allocated");
    let ch = ch_i32 as u16;

    // Outgoing from source to dest should wrap with ChannelData and target dest address
    let payload_src_to_dst = b"abc";
    let wrapped = handler
        .send_turn_outgoing(&source, &dest, payload_src_to_dst)
        .expect("send_turn_outgoing should wrap on active channel");
    assert_eq!(wrapped.ip_address, dest, "destination should be the TURN mapped dest address");

    // Validate header fields
    let msg = wrapped.message;
    assert!(msg.len() >= 4, "ChannelData must have header");
    let ch_be = u16::from_be_bytes([msg[0], msg[1]]);
    let len = u16::from_be_bytes([msg[2], msg[3]]) as usize;
    assert_eq!(ch_be, ch, "header channel must match allocated channel");
    assert_eq!(len, payload_src_to_dst.len(), "header length must match payload");

    // Feed wrapped packet back as if it arrived from source at the relay
    let incoming = handler
        .handle_turn_incoming(Some(&source), Some(relay_public), &msg)
        .expect("incoming from source over active channel should be accepted");
    assert_eq!(incoming.ip_address, dest, "from source -> deliver to dest");
    assert_eq!(incoming.message, payload_src_to_dst, "payload preserved");
    let nep = incoming.network_endpoint;
    assert!(nep.is_relay(), "for active channel, network endpoint should identify relay");
    assert_eq!(nep.relay_channel().unwrap(), ch, "relay channel set");
    assert_eq!(nep.relay_address().unwrap(), relay_public, "relay public address set");
    assert!(nep.relay_id().is_some(), "relay id should be present for allowed address");

    // Now simulate a packet arriving from the dest back to the relay on the same channel
    let payload_dst_to_src = b"reply".to_vec();
    let packet_back = build_channel_data(ch, &payload_dst_to_src);
    let incoming_back = handler
        .handle_turn_incoming(Some(&dest), Some(relay_public), &packet_back)
        .expect("incoming from dest over active channel should be accepted");
    assert_eq!(incoming_back.ip_address, source, "from dest -> deliver to source");
    assert_eq!(incoming_back.message, payload_dst_to_src, "payload preserved in reverse");
    let nep_back = incoming_back.network_endpoint;
    assert!(nep_back.is_relay());
    assert_eq!(nep_back.relay_channel().unwrap(), ch);
    assert_eq!(nep_back.relay_address().unwrap(), relay_public);
    assert!(nep_back.relay_id().is_some());
}
