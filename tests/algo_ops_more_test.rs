use rust_comms::algo_ops::{address_to_byte_key, byte_key_to_address, AlgoOps};
use sha2::{Digest, Sha512_256};

#[test]
fn can_create_address_and_private_key_bytes() {
    let mut ops = AlgoOps::new(None, None, None);
    let addr = ops.create_address(false, false).expect("create address");
    assert!(!addr.is_empty());
    let pk = ops.public_key_bytes().expect("pk from self address");
    assert_eq!(pk.len(), 32);
    let sk = ops.private_key_bytes().expect("private key available");
    assert_eq!(sk.len(), 32);
    // Roundtrip address from pk equals the created address
    let addr2 = byte_key_to_address(&pk).expect("addr from pk");
    assert_eq!(addr2, addr);
}

#[test]
fn can_sign_and_verify_and_detect_tamper() {
    let mut ops = AlgoOps::new(None, None, None);
    let addr = ops.create_address(false, false).expect("create address");
    let text = "Hello from Rust";
    let sig = ops.sign(text).expect("sign");

    // Verify with a verifier that only has the address
    let verifier = AlgoOps::new(None, Some(addr.clone()), None);
    let ok = verifier.verify(text, &sig).expect("verify");
    assert!(ok);

    // Tamper the signature
    let mut s = sig.into_bytes();
    if !s.is_empty() { s[0] ^= 0x01; }
    let tampered = String::from_utf8(s).unwrap();
    let ok2 = AlgoOps::new(None, Some(addr), None).verify(text, &tampered).expect("verify tampered");
    assert!(!ok2);
}

#[test]
fn contract_address_matches_spec_hash() {
    let ops = AlgoOps::new(None, None, None);
    let app_id = 42u64;
    let addr = ops.contract_address(app_id).expect("contract address");
    let pk = address_to_byte_key(&addr).expect("decode addr");
    let mut hasher = Sha512_256::new();
    hasher.update(b"appID");
    hasher.update(app_id.to_be_bytes());
    let expected: [u8; 32] = hasher.finalize().into();
    assert_eq!(pk, expected);
}

#[test]
fn global_state_requires_address_and_returns_none_on_network_error() {
    // Missing address -> error
    let ops = AlgoOps::new(None, None, None);
    assert!(ops.global_state(None).is_err());

    // With an address but unreachable network -> None
    let mut pk = [0u8; 32];
    for i in 0..32 { pk[i] = i as u8; }
    let addr = byte_key_to_address(&pk).unwrap();
    let cfg = rust_comms::algo_ops::AlgoProviderConfig {
        client_api_url: "http://nowhere.not".to_string(),
        client_api_port: 666,
        indexer_api_url: "http://nowhere.not".to_string(),
        indexer_api_port: 666,
        token: Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string()),
        token_key: Some("X-API-Key".to_string()),
    };
    let ops2 = AlgoOps::new(None, Some(addr), Some(cfg));
    let res = ops2.global_state(None).unwrap();
    assert!(res.is_none());
}

#[test]
fn local_state_errors_on_invalid_address_and_none_on_network_error() {
    // Invalid address -> error
    let ops = AlgoOps::new(None, None, None);
    assert!(ops.local_state_for_account(1, "SOMEADDR").is_err());

    // Valid address but unreachable network -> None
    let pk = [42u8; 32];
    let addr = byte_key_to_address(&pk).unwrap();
    let cfg = rust_comms::algo_ops::AlgoProviderConfig {
        client_api_url: "http://nowhere.not".to_string(),
        client_api_port: 666,
        indexer_api_url: "http://nowhere.not".to_string(),
        indexer_api_port: 666,
        token: Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string()),
        token_key: Some("X-API-Key".to_string()),
    };
    let ops2 = AlgoOps::new(None, None, Some(cfg));
    let res = ops2.local_state_for_account(1, &addr).unwrap();
    assert!(res.is_none());
}


#[test]
fn send_algo_requires_account_access() {
    // No passphrase/address set
    let ops = AlgoOps::new(None, None, None);
    let err = ops.send_algo("SOMEADDR", 1.0).unwrap_err();
    let msg = format!("{}", err);
    assert!(msg.contains("account access"));
}

#[test]
fn send_algo_requires_positive_amount() {
    let mut ops = AlgoOps::new(None, None, None);
    let addr = ops.create_address(false, false).unwrap();
    // Zero amount is invalid
    let err = ops.send_algo(&addr, 0.0).unwrap_err();
    assert!(format!("{}", err).contains("amount must be positive"));
    // Negative also invalid
    let err2 = ops.send_algo(&addr, -1.0).unwrap_err();
    assert!(format!("{}", err2).contains("amount must be positive"));
}

#[test]
fn send_algo_validates_receiver_and_errors_on_unreachable_network() {
    let cfg = rust_comms::algo_ops::AlgoProviderConfig {
        client_api_url: "http://nowhere.not".to_string(),
        client_api_port: 666,
        indexer_api_url: "http://nowhere.not".to_string(),
        indexer_api_port: 666,
        token: Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string()),
        token_key: Some("X-API-Key".to_string()),
    };
    let mut ops = AlgoOps::new(None, None, Some(cfg));
    let addr = ops.create_address(false, false).unwrap();
    // Invalid receiver format
    assert!(ops.send_algo("NOTANADDRESS", 0.001).is_err());
    // Valid receiver (self) but unreachable network -> expect error
    assert!(ops.send_algo(&addr, 0.001).is_err());
}
