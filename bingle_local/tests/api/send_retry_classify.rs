// Tests for the typed send-failure classifier (issue #99): bingle_local::api::send_retry.
// Verifies that a send result maps to the right SendFailureKind, that the retryable split is
// correct per variant, and that legacy untyped errors still classify sensibly.
use bingle_core::api::bingle_api::{BingleError, SendFailureKind};
use bingle_local::api::send_retry::{SendFailure, classify_send_error};

/// Build a typed send error result for a given kind.
fn send_err(kind: SendFailureKind) -> Result<bool, BingleError> {
    Err(BingleError::Send {
        kind,
        detail: "detail".to_string(),
    })
}

#[test]
fn delivered_result_has_no_failure() {
    assert!(classify_send_error(&Ok(true)).is_none());
}

#[test]
fn not_accepted_ok_false_is_permanent_unknown() {
    // A bare Ok(false) means an internal guard rejected the send with no error; it is not
    // retryable (issue #99).
    let f = classify_send_error(&Ok(false)).expect("a failure");
    assert_eq!(f.kind, SendFailureKind::Unknown);
    assert!(!f.kind.is_retryable());
}

#[test]
fn typed_error_kind_is_preserved() {
    for kind in [
        SendFailureKind::HandleNotFound,
        SendFailureKind::HandleLookupFailed,
        SendFailureKind::RecipientNotAdvertised,
        SendFailureKind::InvalidRecipientId,
        SendFailureKind::NoRelayAvailable,
        SendFailureKind::RelayAllocationFailed,
        SendFailureKind::PeerUnreachable,
        SendFailureKind::NoResponse,
        SendFailureKind::MalformedAdvert,
        SendFailureKind::ProtocolError,
        SendFailureKind::NotReady,
        SendFailureKind::Unknown,
    ] {
        let f = classify_send_error(&send_err(kind)).expect("a failure");
        assert_eq!(f.kind, kind, "kind must round-trip through the classifier");
    }
}

#[test]
fn retryable_split_matches_intent() {
    // Transient causes keep the message pending; permanent causes give up.
    let retryable = [
        SendFailureKind::HandleLookupFailed,
        SendFailureKind::RecipientNotAdvertised,
        SendFailureKind::NoRelayAvailable,
        SendFailureKind::RelayAllocationFailed,
        SendFailureKind::PeerUnreachable,
        SendFailureKind::NoResponse,
        SendFailureKind::NotReady,
    ];
    let permanent = [
        SendFailureKind::HandleNotFound,
        SendFailureKind::InvalidRecipientId,
        SendFailureKind::MalformedAdvert,
        SendFailureKind::ProtocolError,
        SendFailureKind::Unknown,
    ];
    for k in retryable {
        assert!(k.is_retryable(), "{k:?} should be retryable");
    }
    for k in permanent {
        assert!(!k.is_retryable(), "{k:?} should be permanent");
    }
}

#[test]
fn recipient_not_advertised_is_retryable() {
    // The user's example: the recipient id has no AdvertRecord (they are not connected). Stays
    // pending so it delivers once they reconnect.
    let f =
        classify_send_error(&send_err(SendFailureKind::RecipientNotAdvertised)).expect("failure");
    assert!(f.kind.is_retryable());
    assert!(f.reason.contains("not connected"), "reason: {}", f.reason);
}

#[test]
fn handle_not_found_is_permanent() {
    let f = classify_send_error(&send_err(SendFailureKind::HandleNotFound)).expect("failure");
    assert!(!f.kind.is_retryable());
    assert!(f.reason.contains("registered"), "reason: {}", f.reason);
}

#[test]
fn legacy_retryable_error_is_transient() {
    // An untyped Retryable error (not produced via BingleError::Send) still classifies transient.
    let f =
        classify_send_error(&Err(BingleError::Retryable("relay timeout".into()))).expect("failure");
    assert!(f.kind.is_retryable());
}

#[test]
fn legacy_other_error_uses_keyword_fallback() {
    // A connectivity keyword in a legacy Other error is treated as transient...
    let transient = classify_send_error(&Err(BingleError::Other("no available relay".into())))
        .expect("failure");
    assert!(transient.kind.is_retryable(), "keyword should be transient");

    // ...while an unrelated legacy error is permanent.
    let permanent = classify_send_error(&Err(BingleError::Other("account not opted in".into())))
        .expect("failure");
    assert!(
        !permanent.kind.is_retryable(),
        "non-keyword should be permanent"
    );
    assert!(permanent.reason.contains("Message failed to send"));
}

#[test]
fn send_failure_is_constructible_and_comparable() {
    // SendFailure is a plain value type used across crates; make sure equality works for tests.
    let a = SendFailure {
        kind: SendFailureKind::PeerUnreachable,
        reason: "x".to_string(),
    };
    let b = a.clone();
    assert_eq!(a, b);
}
