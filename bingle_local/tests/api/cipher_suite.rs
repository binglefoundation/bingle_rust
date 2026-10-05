//! Tests for the cipher suite recorded on a received message (issue #292): the DTLS suite for a
//! message received over a live session, and the sealed envelope's suite for one read from a
//! Sidewinder Mailbox.

use algo_ops::AlgoOps;
use bingle_core::crypto::sealed_envelope::{self, OpenedMessage, suite_name};
use bingle_local::api::{BingleApiLocalImpl, BingleLocalApi, LocalApiConfig, Message};

const DTLS_SUITE: &str = "TLS_AES_256_GCM_SHA384";
const TEST_MNEMONIC: &str = "square flat curtain negative three april hobby culture unit fit drip bronze cactus stage vault pluck captain nation pond pizza grief domain coin abstract path";

#[test]
fn a_message_read_from_the_mailbox_reports_the_envelope_suite() {
    // Seal a real envelope to our own account and open it, as the Mailbox read path does.
    let address = AlgoOps::address_from_passphrase(TEST_MNEMONIC).expect("address");
    let private_key = AlgoOps::seed_from_passphrase(TEST_MNEMONIC).expect("private key");
    let public_key = algo_ops::address_to_byte_key(&address).expect("public key");
    let bytes = sealed_envelope::seal_from_private_key(
        private_key,
        public_key,
        1_700_000_000_123,
        "hello from the mailbox",
    )
    .expect("seal");
    let opened = sealed_envelope::unseal_with_private_key(private_key, &bytes).expect("unseal");

    let msg = Message::from_opened(
        &opened,
        "alice".into(),
        vec!["me".into()],
        1_700_000_050_000,
    );

    let envelope_suite = sealed_envelope::SealedEnvelope::from_bytes(&bytes)
        .expect("parse")
        .suite_name()
        .map(str::to_string);
    assert!(envelope_suite.is_some());
    assert_eq!(msg.cipher_suite, envelope_suite);
}

#[test]
fn an_unknown_envelope_suite_is_reported_as_none() {
    let opened = OpenedMessage {
        sender_id: [0x07u8; 32],
        sent_time: 1,
        message_id: [0x09u8; 16],
        text: "hi".to_string(),
        signature: [0x03u8; 64],
        suite_id: 0xFFFF,
    };
    assert_eq!(suite_name(opened.suite_id), None);

    let msg = Message::from_opened(&opened, "alice".into(), vec!["me".into()], 2);

    assert_eq!(msg.cipher_suite, None);
}

#[test]
fn a_message_received_over_a_live_session_keeps_its_dtls_suite() {
    let mut api = BingleApiLocalImpl::new(LocalApiConfig::default());
    api.add_message(
        "alice".to_string(),
        vec!["me".to_string()],
        1,
        "hi".to_string(),
        Some(DTLS_SUITE.to_string()),
    )
    .expect("add message");

    let stored = api.get_messages().expect("messages");
    assert_eq!(stored[0].cipher_suite.as_deref(), Some(DTLS_SUITE));
}
