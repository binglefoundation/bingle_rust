use bingle_local::api::{BingleLocalApi, BingleApiLocalImpl};

#[test]
fn test_generate_keypair_works() {
    let mut api = BingleApiLocalImpl::new();
    let kp = api.generate_keypair().expect("keypair");
    assert!(!kp.id.is_empty(), "id should not be empty");
    assert!(!kp.passphrase.is_empty(), "passphrase should not be empty");
    assert!(kp.passphrase.starts_with("b64:"), "passphrase should be base64 with b64: prefix");
}

#[test]
fn test_get_algo_ops_uses_existing_keypair() {
    let mut api = BingleApiLocalImpl::new();
    let kp = api.generate_keypair().expect("keypair");
    let ops = api.get_algo_ops().expect("ops");
    // Address should be derived from the stored passphrase and equal to the generated id
    let addr = ops.address.expect("address should be present");
    assert_eq!(addr, kp.id);
}

#[test]
fn test_get_algo_ops_errors_when_missing() {
    let api = BingleApiLocalImpl::new();
    let res = api.get_algo_ops();
    assert!(res.is_err(), "get_algo_ops should error when no keypair is set");
}

#[test]
fn test_get_algo_ops_caches_instance() {
    let mut api = BingleApiLocalImpl::new();
    let _ = api.generate_keypair().expect("keypair");
    let ops1 = api.get_algo_ops().expect("ops1");
    let ops2 = api.get_algo_ops().expect("ops2");
    assert_eq!(ops1.address, ops2.address, "cached AlgoOps should be reused across calls");
}
