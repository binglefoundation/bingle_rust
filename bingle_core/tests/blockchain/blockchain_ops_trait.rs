//! Offline unit tests for the `BlockChainOps` / `AssetOps` trait impls on `AlgoOps`.
//!
//! These cover the delegation for the methods that do not need a live node: keypair/address
//! creation, key export, and sign/verify. The networked trait methods (`send_payment`,
//! `account_balance`, `account_balance_at`, `wait_for_confirmation`, and the `AssetOps`
//! methods) are exercised by the blockchain integration tests.

use bingle_core::blockchain::algo_ops::AlgoOps;
use bingle_core::blockchain::blockchain_ops::BlockChainOps;

// Same-named methods (`sign`, `verify`, `generate_keypair`) exist both inherently and on the
// trait; inherent methods win for `ops.method()` calls, so the trait impls are invoked here via
// fully-qualified syntax to make sure it is the trait surface under test.

#[test]
fn trait_generate_keypair_derives_matching_address() {
    let (id, passphrase) = <AlgoOps as BlockChainOps>::generate_keypair();
    assert!(!id.is_empty(), "id must not be empty");
    assert_eq!(
        passphrase.split_whitespace().count(),
        25,
        "passphrase must be 25 words"
    );

    let ops = AlgoOps::new_for_algorand(Some(passphrase), None, None);

    // Distinctly-named trait methods resolve to the trait without qualification.
    assert_eq!(ops.address().expect("address"), id);
    assert_eq!(ops.public_key().expect("public key").len(), 32);
    assert_eq!(ops.private_key().expect("private key").len(), 32);

    let sig = <AlgoOps as BlockChainOps>::sign(&ops, "hello world").expect("sign");
    assert!(
        <AlgoOps as BlockChainOps>::verify(&ops, "hello world", &sig).expect("verify"),
        "signature must verify"
    );
}

#[test]
fn trait_create_address_populates_account() {
    let mut ops = AlgoOps::new_for_algorand(None, None, None);

    // Trait `create_address` takes no flags (unlike the inherent two-flag method), so it must
    // be called via fully-qualified syntax — otherwise the inherent method wins.
    let addr = <AlgoOps as BlockChainOps>::create_address(&mut ops).expect("create address");
    assert!(!addr.is_empty(), "created address must not be empty");
    assert_eq!(ops.address().expect("address"), addr);
    assert_eq!(ops.private_key().expect("private key").len(), 32);

    let sig = <AlgoOps as BlockChainOps>::sign(&ops, "payload").expect("sign");
    assert!(
        <AlgoOps as BlockChainOps>::verify(&ops, "payload", &sig).expect("verify"),
        "signature from a freshly created address must verify"
    );
}

#[test]
fn new_for_algorand_matches_new_for_address_derivation() {
    let (_id, passphrase) = AlgoOps::generate_keypair();
    let via_new = AlgoOps::new(Some(passphrase.clone()), None, None);
    let via_ctor = AlgoOps::new_for_algorand(Some(passphrase), None, None);
    assert_eq!(
        via_new.address, via_ctor.address,
        "new_for_algorand must derive the same address as new"
    );
}

#[test]
fn trait_verify_rejects_tampered_text() {
    let (_id, passphrase) = AlgoOps::generate_keypair();
    let ops = AlgoOps::new_for_algorand(Some(passphrase), None, None);
    let sig = <AlgoOps as BlockChainOps>::sign(&ops, "original").expect("sign");
    assert!(
        !<AlgoOps as BlockChainOps>::verify(&ops, "tampered", &sig).expect("verify"),
        "signature must not verify against different text"
    );
}
