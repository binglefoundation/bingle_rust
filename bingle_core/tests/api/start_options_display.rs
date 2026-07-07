use bingle_core::api::bingle_api::StartOptions;

#[test]
pub fn display_does_not_leak_passphrase() {
    let opts = StartOptions {
        algo_passphrase: Some("super secret mnemonic words".into()),
        ..StartOptions::new("tester".into())
    };
    let shown = format!("{}", opts);
    assert!(
        !shown.contains("super secret mnemonic words"),
        "passphrase leaked in Display: {shown}"
    );
    assert!(
        shown.contains("algo_passphrase: <set>"),
        "expected masked passphrase marker in: {shown}"
    );
    // Non-sensitive fields should still be visible for debugging.
    assert!(
        shown.contains("tester"),
        "handle should be shown in: {shown}"
    );
}

#[test]
pub fn display_shows_none_when_no_passphrase() {
    let opts = StartOptions::new("tester".into());
    let shown = format!("{}", opts);
    assert!(
        shown.contains("algo_passphrase: None"),
        "expected None marker in: {shown}"
    );
}
