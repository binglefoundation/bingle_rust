use bingle_local::api::{BingleLocalApi, BingleApiLocalImpl};

#[test]
fn test_generate_keypair_works() {
    let mut api = BingleApiLocalImpl::new();
    let kp = api.generate_keypair().expect("keypair");
    assert!(!kp.id.is_empty(), "id should not be empty");
    assert!(!kp.passphrase.is_empty(), "passphrase should not be empty");
    assert!(kp.passphrase.starts_with("b64:"), "passphrase should be base64 with b64: prefix");
}
