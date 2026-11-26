//! FRPT block-size measurement for the largest realistic payloads (bingle_rust #518, step 1).
//!
//! Each payload is built the way the sending code builds it and encoded exactly as it reaches
//! `PacketTransport::send`: `serde_json::to_vec` of the message JSON (see
//! `BingleApiImpl::send_over_dtls`). The FRPT DATA_SINGLE block is those bytes; the packet adds
//! the 4-byte FRPT header.
//!
//! No maximum text-message length exists anywhere in bingle_core, bingle_jsi or bingle_cli, so the
//! plain-text cases use long messages (1000 ASCII characters and 1000 four-byte emoji).
//!
//! Run with `cargo test -p bingle_core --test unit frpt_block_sizes -- --nocapture` to see the table.

use base64::{Engine as _, engine::general_purpose};
use bingle_core::crypto::live_signature::sign_text_message;
use bingle_core::ddb::{AdvertRecord, InetSocketAddress};
use bingle_core::engine::RelayState;
use bingle_core::messages::marshal::to_json_value;
use bingle_core::messages::types::{
    DdbDumpResolve, DdbInitResponse, DdbMessage, DdbRelaysStatusResponse, DdbSignon,
    DdbUpsertResolve, FailVote, Message, ReportFailMessage, ReportFailedRipple,
};
use ed25519_dalek::{Signer, SigningKey};
use serde_json::Value as JsonValue;

/// FRPT header length in bytes (packet type + flags + transaction id).
const FRPT_HEADER_LEN: usize = 4;
/// Current single-packet capacity: 1492-byte packet minus the header.
const SINGLE_PACKET_BLOCK_CAPACITY: usize = 1492 - FRPT_HEADER_LEN;
/// Proposed conservative packet budget the measurements are compared against.
const CONSERVATIVE_PACKET_BUDGET: usize = 1180;
/// Number of relays in the realistic relay set.
const RELAY_COUNT: usize = 20;

/// A deterministic Ed25519 key for test account `n`.
fn key(n: u8) -> SigningKey {
    SigningKey::from_bytes(&[n.wrapping_add(1); 32])
}

/// The 58-character Algorand address of `key`.
fn address(key: &SigningKey) -> String {
    algo_ops::byte_key_to_address(&key.verifying_key().to_bytes())
        .expect("a 32-byte public key always encodes as an Algorand address")
}

/// The bytes `PacketTransport::send` receives for `message` (as in `send_over_dtls`).
fn wire_bytes(message: &JsonValue) -> Vec<u8> {
    serde_json::to_vec(message).expect("message JSON serializes")
}

/// A request tag as `send_message_to_network_with_response_timeout` adds it (UUID v4 string).
fn with_request_tag(message: JsonValue) -> JsonValue {
    let mut fields = match message {
        JsonValue::Object(fields) => fields,
        other => panic!("request message is not a JSON object: {other}"),
    };
    fields.remove("responseTag");
    fields.insert(
        "tag".to_string(),
        JsonValue::String(uuid::Uuid::new_v4().to_string()),
    );
    JsonValue::Object(fields)
}

/// A response tag echoing a request tag.
fn response_tag() -> Option<String> {
    Some(uuid::Uuid::new_v4().to_string())
}

/// A signed text message as `send_message_to_handles` (bingle_jsi) and `sign_text_message` build it.
fn signed_text(text: &str) -> JsonValue {
    let sender = key(1);
    let recipient = key(2);
    let mut message = serde_json::json!({ "text": text, "sent_time": 1_764_000_000_000i64 });
    let signed = sign_text_message(
        &mut message,
        &sender,
        &recipient.verifying_key().to_bytes(),
        1_764_000_000_000,
    );
    assert!(signed, "a plain text message is signed");
    message
}

/// A worst-case IPv6 endpoint (full-length textual form) for relay `n`.
fn ipv6_endpoint(n: usize) -> InetSocketAddress {
    InetSocketAddress {
        host: format!(
            "2406:da1c:fe2:{:x}00:1234:5678:9abc:def{:x}",
            n + 0xa,
            n % 16
        ),
        port: 65_001,
    }
}

/// A full-length IPv4 endpoint for relay `n`.
fn ipv4_endpoint(n: usize) -> InetSocketAddress {
    InetSocketAddress {
        host: format!("203.119.{}.{}", 100 + n, 200 + n),
        port: 65_001,
    }
}

/// A client advert carrying its relay id and a relay signature, signed by the client.
///
/// The relay signature is a real Ed25519 signature by the relay key over the client's unsigned
/// record, base64-encoded (88 characters), matching the field's documented shape; production
/// currently registers with `relay_sig: None`, so this is the worst case.
fn client_advert_with_relay_sig(endpoint: Option<InetSocketAddress>) -> AdvertRecord {
    let client = key(1);
    let relay = key(3);
    let unsigned = AdvertRecord::new_unsigned(
        address(&client),
        endpoint.clone(),
        Some(false),
        Some(address(&relay)),
        None,
        "2026-11-24T00:00:00Z".to_string(),
    );
    let unsigned_bytes = serde_json::to_vec(&unsigned).expect("advert serializes");
    let relay_sig = general_purpose::STANDARD.encode(relay.sign(&unsigned_bytes).to_bytes());
    AdvertRecord::new(
        address(&client),
        endpoint,
        Some(false),
        Some(address(&relay)),
        Some(relay_sig),
        "2026-11-24T00:00:00Z".to_string(),
        &client,
    )
}

/// An upsert as `DdbClientImpl::register_relay` builds and sends it (with the request tag).
fn upsert(record: AdvertRecord) -> JsonValue {
    let original_signature = record
        .sig
        .clone()
        .expect("advert built with a signing key carries a signature");
    let message = Message::Ddb(DdbMessage::UpsertResolve(DdbUpsertResolve {
        app: "ddb".to_string(),
        start_id: record.id.clone(),
        epoch: 1,
        record,
        original_signature,
        rippled: false,
        tag: None,
        response_tag: None,
        text: None,
        data: None,
    }));
    with_request_tag(to_json_value(&message))
}

/// A relay-status response for a 20-relay set, as `on_ddb_get_relays_status` builds it.
fn relays_status(endpoint_of: fn(usize) -> InetSocketAddress) -> JsonValue {
    let relay_ids: Vec<String> = (0..RELAY_COUNT)
        .map(|n| address(&key(10 + n as u8)))
        .collect();
    let relay_endpoints: Vec<InetSocketAddress> = (0..RELAY_COUNT).map(endpoint_of).collect();
    let message = Message::Ddb(DdbMessage::RelaysStatusResponse(DdbRelaysStatusResponse {
        app: "ddb".into(),
        responder_state: RelayState::Available,
        epoch_id: 1_764_000_000,
        tree_order: 2,
        relay_states: vec![RelayState::Available; relay_ids.len()],
        relay_ids,
        relay_endpoints: Some(relay_endpoints),
        response_tag: response_tag(),
        text: None,
        data: None,
    }));
    to_json_value(&message)
}

/// A report-fail ripple, as `on_report_fail` builds it, with `vote_count` signed votes.
fn report_failed_ripple(vote_count: usize) -> JsonValue {
    let failed = key(4);
    let votes: Vec<FailVote> = (0..vote_count)
        .map(|n| {
            let voter = key(10 + n as u8);
            FailVote {
                confirming_id: address(&voter),
                signature: general_purpose::STANDARD
                    .encode(voter.sign(address(&failed).as_bytes()).to_bytes()),
            }
        })
        .collect();
    let message = Message::ReportFail(ReportFailMessage::ReportFailedRipple(ReportFailedRipple {
        app: "reportFail".to_string(),
        tag: None,
        response_tag: None,
        failed_relay_id: address(&failed),
        fail_type: "unreachable".to_string(),
        timestamp: "2026-11-24T00:00:00.000000000Z".to_string(),
        confirmations: votes,
        disputes: vec![],
    }));
    to_json_value(&message)
}

/// Every measured payload, named, as it reaches `PacketTransport::send`.
fn payloads() -> Vec<(String, Vec<u8>)> {
    let client = key(1);
    let ascii_text = "a".repeat(1000);
    let emoji_text = "\u{1F600}".repeat(1000);
    let plain_advert = AdvertRecord::new(
        address(&client),
        Some(ipv4_endpoint(0)),
        Some(true),
        None,
        None,
        "1970-01-01T00:00:00Z".to_string(),
        &client,
    );
    let signon = Message::Ddb(DdbMessage::Signon(DdbSignon {
        app: "ddb".to_string(),
        start_id: address(&client),
        original_signature: None,
        rippled: Some(false),
        tag: None,
        response_tag: None,
        text: None,
        data: None,
    }));
    let init_response = Message::Ddb(DdbMessage::InitResponse(DdbInitResponse {
        app: "ddb".to_string(),
        db_count: 100_000,
        response_tag: response_tag(),
        text: None,
        data: None,
    }));
    let dump = Message::Ddb(DdbMessage::DumpResolve(DdbDumpResolve {
        app: "ddb".into(),
        record: client_advert_with_relay_sig(Some(ipv6_endpoint(0))),
        tag: None,
        response_tag: None,
        text: None,
        data: None,
    }));

    let mut out = vec![
        (
            "PlainText signed, 1000 ASCII chars".to_string(),
            wire_bytes(&signed_text(&ascii_text)),
        ),
        (
            "PlainText signed, 1000 emoji (4-byte UTF-8)".to_string(),
            wire_bytes(&signed_text(&emoji_text)),
        ),
        (
            "AdvertRecord + relaySig, IPv6 (record only)".to_string(),
            wire_bytes(
                &serde_json::to_value(client_advert_with_relay_sig(Some(ipv6_endpoint(0))))
                    .expect("advert serializes"),
            ),
        ),
        ("DdbSignon".to_string(), wire_bytes(&to_json_value(&signon))),
        (
            "DdbUpsertResolve register_ip (IPv4, relay)".to_string(),
            wire_bytes(&upsert(plain_advert)),
        ),
        (
            "DdbUpsertResolve + relaySig, IPv6".to_string(),
            wire_bytes(&upsert(client_advert_with_relay_sig(Some(ipv6_endpoint(
                0,
            ))))),
        ),
        (
            "DdbDumpResolve + relaySig, IPv6".to_string(),
            wire_bytes(&to_json_value(&dump)),
        ),
        (
            format!("DdbRelaysStatusResponse {RELAY_COUNT} relays IPv4"),
            wire_bytes(&relays_status(ipv4_endpoint)),
        ),
        (
            format!("DdbRelaysStatusResponse {RELAY_COUNT} relays IPv6"),
            wire_bytes(&relays_status(ipv6_endpoint)),
        ),
        (
            "DdbInitResponse (dbCount only)".to_string(),
            wire_bytes(&to_json_value(&init_response)),
        ),
        (
            "ReportFailedRipple (as sent, no votes)".to_string(),
            wire_bytes(&report_failed_ripple(0)),
        ),
        (
            format!("ReportFailedRipple {RELAY_COUNT} signed votes"),
            wire_bytes(&report_failed_ripple(RELAY_COUNT)),
        ),
    ];
    out.sort_by_key(|(_, bytes)| bytes.len());
    out
}

#[test]
#[cfg(not(target_os = "ios"))]
fn frpt_block_sizes_of_largest_realistic_payloads() {
    let measured = payloads();
    println!();
    println!(
        "{:<48} | {:>11} | {:>12} | over {}?",
        "name", "block bytes", "packet bytes", CONSERVATIVE_PACKET_BUDGET
    );
    println!("{}", "-".repeat(96));
    for (name, bytes) in &measured {
        let block = bytes.len();
        let packet = block + FRPT_HEADER_LEN;
        let over_budget = if packet > CONSERVATIVE_PACKET_BUDGET {
            "YES"
        } else {
            "no"
        };
        let over_capacity = if block > SINGLE_PACKET_BLOCK_CAPACITY {
            "   <<< EXCEEDS CURRENT SINGLE-PACKET CAPACITY (1488-byte block)"
        } else {
            ""
        };
        println!("{name:<48} | {block:>11} | {packet:>12} | {over_budget}{over_capacity}");
        assert!(block > 0, "{name} encodes to a non-empty block");
    }

    // control payloads that fit today must keep fitting: they cannot be fragmented yet
    for (name, bytes) in &measured {
        let is_text = name.starts_with("PlainText");
        let is_worst_case_variant = name.contains("relays") || name.contains("signed votes");
        if !is_text && !is_worst_case_variant {
            assert!(
                bytes.len() <= SINGLE_PACKET_BLOCK_CAPACITY,
                "{name} ({} bytes) exceeds the single-packet block capacity",
                bytes.len()
            );
        }
    }
}
