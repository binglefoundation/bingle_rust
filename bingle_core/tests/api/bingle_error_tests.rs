use bingle_core::api::bingle_api::BingleError;

#[test]
fn test_bingle_error_retryable_display() {
    let err = BingleError::Retryable("connection timeout".to_string());
    assert_eq!(format!("{}", err), "Retryable error: connection timeout");
}

#[test]
fn test_bingle_error_other_display() {
    let err = BingleError::Other("fatal error".to_string());
    assert_eq!(format!("{}", err), "fatal error");
}

#[test]
fn test_bingle_error_from_string() {
    let s = "some error".to_string();
    let err: BingleError = s.into();
    match err {
        BingleError::Other(msg) => assert_eq!(msg, "some error"),
        _ => panic!("Expected BingleError::Other"),
    }
}
