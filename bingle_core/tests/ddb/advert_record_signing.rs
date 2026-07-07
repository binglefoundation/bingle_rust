use bingle_core::ddb::{AdvertRecord, InetSocketAddress};
use ed25519_dalek::SigningKey;
use rand_core::OsRng;

#[test]
fn test_advert_record_signing_and_verification() {
    let mut csprng = OsRng;
    let signing_key = SigningKey::generate(&mut csprng);
    let public_key = signing_key.verifying_key();

    // Convert public key to Algorand address
    let pk_bytes: [u8; 32] = public_key.to_bytes();
    let address = bingle_core::blockchain::algo_ops::byte_key_to_address(&pk_bytes).unwrap();

    let record = AdvertRecord::new(
        address.clone(),
        Some(InetSocketAddress {
            host: "127.0.0.1".into(),
            port: 1234,
        }),
        Some(true),
        None,
        None,
        "2026-06-16T18:31:00Z".into(),
        &signing_key,
    );

    assert!(record.sig.is_some());
    assert!(record.verify(), "Signature should be valid");
}

#[test]
fn test_advert_record_verification_failure() {
    let mut csprng = OsRng;
    let signing_key = SigningKey::generate(&mut csprng);
    let public_key = signing_key.verifying_key();

    let pk_bytes: [u8; 32] = public_key.to_bytes();
    let address = bingle_core::blockchain::algo_ops::byte_key_to_address(&pk_bytes).unwrap();

    let mut record = AdvertRecord::new(
        address.clone(),
        Some(InetSocketAddress {
            host: "127.0.0.1".into(),
            port: 1234,
        }),
        Some(true),
        None,
        None,
        "2026-06-16T18:31:00Z".into(),
        &signing_key,
    );

    // Tamper with the record
    record.date = "2026-06-16T18:32:00Z".into();
    assert!(
        !record.verify(),
        "Signature should be invalid after tampering"
    );

    // Tamper with the signature
    record.date = "2026-06-16T18:31:00Z".into(); // Restore date
    record.sig = Some("A".repeat(88)); // Invalid signature
    assert!(
        !record.verify(),
        "Invalid signature string should fail verification"
    );
}

#[test]
fn test_advert_record_unsigned_fails_verification() {
    let record = AdvertRecord::new_unsigned(
        "7K56TK6T6T6T6T6T6T6T6T6T6T6T6T6T6T6T6T6T6T6T6T6T6T6T6T6T6T6T6T6T6T6T6T6T6T".into(),
        None,
        None,
        None,
        None,
        "2026-06-16T18:31:00Z".into(),
    );

    assert!(!record.verify(), "Unsigned record should fail verification");
}
