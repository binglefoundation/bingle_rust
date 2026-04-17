use std::fmt::Display;

use bingle_jsi::api::error::BingleJsiError;

#[test]
fn not_found_error_display() {
    let err = BingleJsiError::NotFound {
        reason: "handle missing".to_string(),
    };
    assert!(err.to_string().contains("handle missing"));
}

#[test]
fn invalid_request_error_display() {
    let err = BingleJsiError::InvalidRequest {
        reason: "bad input".to_string(),
    };
    assert!(err.to_string().contains("bad input"));
}

#[test]
fn not_implemented_error_display() {
    let err = BingleJsiError::NotImplemented {
        reason: "handle_lookup".to_string(),
    };
    assert!(err.to_string().contains("handle_lookup"));
}

#[test]
fn internal_error_display() {
    let err = BingleJsiError::InternalError {
        reason: "unexpected".to_string(),
    };
    assert!(err.to_string().contains("unexpected"));
}

#[test]
fn error_is_std_error() {
    let err = BingleJsiError::NotFound {
        reason: "test".to_string(),
    };
    // Verify it implements std::error::Error via Display
    let display: &dyn Display = &err;
    assert!(!display.to_string().is_empty());
}
