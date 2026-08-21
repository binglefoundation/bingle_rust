use std::collections::HashMap;

use bingle_jsi::api::types::{
    BingleMessage, Contact, ContactSource, InetSocketAddress, Keypair, KeypairStatus,
    KeypairStatusResponse, Message, NatType, NatTypeResponse, NetworkSourceKey, VersionInfo,
};

#[test]
fn inet_socket_address_construction() {
    let addr = InetSocketAddress {
        host: "127.0.0.1".to_string(),
        port: 8080,
    };
    assert_eq!(addr.host, "127.0.0.1");
    assert_eq!(addr.port, 8080);
}

#[test]
fn network_source_key_all_none() {
    let nsk = NetworkSourceKey {
        inet_socket_address: None,
        relay_channel: None,
        relay_address: None,
        relay_id: None,
    };
    assert!(nsk.inet_socket_address.is_none());
    assert!(nsk.relay_channel.is_none());
    assert!(nsk.relay_address.is_none());
    assert!(nsk.relay_id.is_none());
}

#[test]
fn network_source_key_with_values() {
    let nsk = NetworkSourceKey {
        inet_socket_address: Some(InetSocketAddress {
            host: "10.0.0.1".to_string(),
            port: 3478,
        }),
        relay_channel: Some(16384),
        relay_address: Some(InetSocketAddress {
            host: "relay.example.com".to_string(),
            port: 3479,
        }),
        relay_id: Some("ALGO_ADDRESS".to_string()),
    };
    let addr = nsk
        .inet_socket_address
        .expect("inet_socket_address should be Some");
    assert_eq!(addr.host, "10.0.0.1");
    assert_eq!(addr.port, 3478);
    let channel = nsk.relay_channel.expect("relay_channel should be Some");
    assert_eq!(channel, 16384);
    let relay = nsk.relay_address.expect("relay_address should be Some");
    assert_eq!(relay.host, "relay.example.com");
    let relay_id = nsk.relay_id.expect("relay_id should be Some");
    assert_eq!(relay_id, "ALGO_ADDRESS");
}

#[test]
fn bingle_message_plain_text() {
    let msg = BingleMessage {
        app: None,
        r#type: None,
        tag: None,
        response_tag: None,
        text: Some("hello".to_string()),
        data: None,
        cipher_suite: None,
    };
    assert!(msg.app.is_none());
    assert!(msg.r#type.is_none());
    let text = msg.text.expect("text should be Some");
    assert_eq!(text, "hello");
}

#[test]
fn bingle_message_typed() {
    let msg = BingleMessage {
        app: Some("chat".to_string()),
        r#type: Some("markdown".to_string()),
        tag: Some("tag1".to_string()),
        response_tag: Some("resp1".to_string()),
        text: Some("Hello".to_string()),
        data: Some(r#"{"markdown":"**Hello**"}"#.to_string()),
        cipher_suite: None,
    };
    let app = msg.app.expect("app should be Some");
    assert_eq!(app, "chat");
    let typ = msg.r#type.expect("type should be Some");
    assert_eq!(typ, "markdown");
    let tag = msg.tag.expect("tag should be Some");
    assert_eq!(tag, "tag1");
    let response_tag = msg.response_tag.expect("response_tag should be Some");
    assert_eq!(response_tag, "resp1");
}

#[test]
fn version_info_construction() {
    let vi = VersionInfo {
        version: "0.1.2".to_string(),
        git_sha: Some("abc123".to_string()),
        build_timestamp: "2024-01-01T00:00:00Z".to_string(),
        build_number: "42".to_string(),
    };
    assert_eq!(vi.version, "0.1.2");
    let sha = vi.git_sha.expect("git_sha should be Some");
    assert_eq!(sha, "abc123");
}

#[test]
fn version_info_no_git_sha() {
    let vi = VersionInfo {
        version: "0.1.0".to_string(),
        git_sha: None,
        build_timestamp: "2024-01-01".to_string(),
        build_number: "1".to_string(),
    };
    assert!(vi.git_sha.is_none());
}

#[test]
fn keypair_construction() {
    let kp = Keypair {
        id: "ALGO_ADDR".to_string(),
        passphrase: "word1 word2 word3".to_string(),
    };
    assert_eq!(kp.id, "ALGO_ADDR");
    assert_eq!(kp.passphrase, "word1 word2 word3");
}

#[test]
fn contact_source_variants() {
    assert_eq!(ContactSource::Manual, ContactSource::Manual);
    assert_eq!(ContactSource::Received, ContactSource::Received);
    assert_ne!(ContactSource::Manual, ContactSource::Received);
}

#[test]
fn contact_construction() {
    let mut fields = HashMap::new();
    fields.insert("email".to_string(), "test@example.com".to_string());
    let contact = Contact {
        handle: "alice".to_string(),
        id: "ID123".to_string(),
        fields,
    };
    assert_eq!(contact.handle, "alice");
    assert_eq!(contact.id, "ID123");
    let email = contact
        .fields
        .get("email")
        .expect("email field should exist");
    assert_eq!(email, "test@example.com");
}

#[test]
fn contact_empty_fields() {
    let contact = Contact {
        handle: "bob".to_string(),
        id: "ID456".to_string(),
        fields: HashMap::new(),
    };
    assert!(contact.fields.is_empty());
}

#[test]
fn message_construction() {
    let msg = Message {
        sender_handle: "alice".to_string(),
        recipient_handles: vec!["bob".to_string(), "carol".to_string()],
        timestamp: 1700000000,
        text: "Hello everyone".to_string(),
        cipher_suite: None,
        progress: Some(1.0),
        failure_reason: None,
        failure_kind: None,
    };
    assert_eq!(msg.sender_handle, "alice");
    assert_eq!(msg.recipient_handles.len(), 2);
    assert_eq!(msg.timestamp, 1700000000);
    assert_eq!(msg.text, "Hello everyone");
    assert!(msg.cipher_suite.is_none());
}

#[test]
fn message_with_cipher_suite() {
    let msg = Message {
        sender_handle: "alice".to_string(),
        recipient_handles: vec!["bob".to_string()],
        timestamp: 1700000001,
        text: "Encrypted hello".to_string(),
        cipher_suite: Some("TLS_AES_256_GCM_SHA384".to_string()),
        progress: Some(0.5),
        failure_reason: Some("Retrying...".to_string()),
        failure_kind: None,
    };
    let cs = msg.cipher_suite.expect("cipher_suite should be Some");
    assert_eq!(cs, "TLS_AES_256_GCM_SHA384");
    assert_eq!(msg.progress, Some(0.5));
    assert_eq!(msg.failure_reason, Some("Retrying...".to_string()));
}

#[test]
fn keypair_status_variants() {
    assert_eq!(KeypairStatus::None, KeypairStatus::None);
    assert_eq!(KeypairStatus::Unfunded, KeypairStatus::Unfunded);
    assert_eq!(KeypairStatus::Funded, KeypairStatus::Funded);
    assert_eq!(KeypairStatus::Active, KeypairStatus::Active);
    assert_ne!(KeypairStatus::None, KeypairStatus::Active);
}

#[test]
fn keypair_status_response_none() {
    let resp = KeypairStatusResponse {
        status: KeypairStatus::None,
        id: None,
        handle: None,
        required_algo: None,
        stale: false,
    };
    assert_eq!(resp.status, KeypairStatus::None);
    assert!(resp.id.is_none());
    assert!(resp.handle.is_none());
    assert!(resp.required_algo.is_none());
    assert!(!resp.stale);
}

#[test]
fn keypair_status_response_active() {
    let resp = KeypairStatusResponse {
        status: KeypairStatus::Active,
        id: Some("ALGO_ADDR".to_string()),
        handle: Some("alice".to_string()),
        required_algo: None,
        stale: true,
    };
    assert_eq!(resp.status, KeypairStatus::Active);
    let id = resp.id.expect("id should be Some");
    assert_eq!(id, "ALGO_ADDR");
    let handle = resp.handle.expect("handle should be Some");
    assert_eq!(handle, "alice");
    // A last-known status (returned during a blockchain outage) is flagged stale.
    assert!(resp.stale);
}

#[test]
fn keypair_status_response_unfunded() {
    let resp = KeypairStatusResponse {
        status: KeypairStatus::Unfunded,
        id: Some("ADDR".to_string()),
        handle: None,
        required_algo: Some(0.1),
        stale: false,
    };
    assert_eq!(resp.status, KeypairStatus::Unfunded);
    let algo = resp.required_algo.expect("required_algo should be Some");
    assert!((algo - 0.1).abs() < f64::EPSILON);
}

#[test]
fn nat_type_variants() {
    assert_eq!(NatType::Unknown, NatType::Unknown);
    assert_eq!(NatType::NoConnection, NatType::NoConnection);
    assert_eq!(NatType::Symmetric, NatType::Symmetric);
    assert_eq!(NatType::Restricted, NatType::Restricted);
    assert_eq!(NatType::FullCone, NatType::FullCone);
    assert_ne!(NatType::Unknown, NatType::FullCone);
}

#[test]
fn nat_type_response_construction() {
    let resp = NatTypeResponse {
        nat_type: NatType::FullCone,
    };
    assert_eq!(resp.nat_type, NatType::FullCone);
}
