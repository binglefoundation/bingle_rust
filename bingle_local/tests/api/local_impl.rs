use bingle_local::api::{BingleApiLocalImpl, BingleLocalApi, LocalApiConfig};

#[test]
fn test_generate_keypair_works() {
    let mut api = BingleApiLocalImpl::new(LocalApiConfig::default());
    let kp = api.generate_keypair().expect("keypair");
    assert!(!kp.id.is_empty(), "id should not be empty");
    assert!(!kp.passphrase.is_empty(), "passphrase should not be empty");
    assert!(
        kp.passphrase.split_whitespace().count() == 25,
        "passphrase should be a 25-word Algorand mnemonic"
    );
}

#[test]
fn test_import_keypair_adopts_account() {
    // Generate a keypair to obtain a valid mnemonic, then import that same mnemonic into a
    // fresh api instance and confirm it adopts the identical account (id derived from passphrase).
    let mut source = BingleApiLocalImpl::new(LocalApiConfig::default());
    let generated = source.generate_keypair().expect("keypair");

    let mut api = BingleApiLocalImpl::new(LocalApiConfig::default());
    let imported = api
        .import_keypair(generated.passphrase.clone())
        .expect("import should succeed for a valid mnemonic");

    assert_eq!(
        imported.id, generated.id,
        "imported id must match the account"
    );
    assert_eq!(imported.passphrase, generated.passphrase);
    // The imported keypair is the current one: get_algo_ops derives the same address.
    let ops = api.get_algo_ops().expect("ops");
    assert_eq!(ops.address.expect("address"), generated.id);
}

#[test]
fn test_import_keypair_rejects_invalid_passphrase() {
    let mut api = BingleApiLocalImpl::new(LocalApiConfig::default());
    let res = api.import_keypair("not a valid mnemonic".to_string());
    assert!(res.is_err(), "importing an invalid mnemonic must fail");
    // A failed import must not leave a partial keypair behind.
    assert!(
        api.get_algo_ops().is_err(),
        "no keypair should be set after a failed import"
    );
}

#[test]
fn test_get_algo_ops_uses_existing_keypair() {
    let mut api = BingleApiLocalImpl::new(LocalApiConfig::default());
    let kp = api.generate_keypair().expect("keypair");
    let ops = api.get_algo_ops().expect("ops");
    // Address should be derived from the stored passphrase and equal to the generated id
    let addr = ops.address.expect("address should be present");
    assert_eq!(addr, kp.id);
}

#[test]
fn test_get_algo_ops_errors_when_missing() {
    let api = BingleApiLocalImpl::new(LocalApiConfig::default());
    let res = api.get_algo_ops();
    assert!(
        res.is_err(),
        "get_algo_ops should error when no keypair is set"
    );
}

#[test]
fn test_get_algo_ops_caches_instance() {
    let mut api = BingleApiLocalImpl::new(LocalApiConfig::default());
    let _ = api.generate_keypair().expect("keypair");
    let ops1 = api.get_algo_ops().expect("ops1");
    let ops2 = api.get_algo_ops().expect("ops2");
    assert_eq!(
        ops1.address, ops2.address,
        "cached AlgoOps should be reused across calls"
    );
}
