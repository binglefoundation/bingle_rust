use rust_comms::algo_ops::{address_to_byte_key, byte_key_to_address, AlgoOps};

#[test]
fn can_convert_address_to_bytes_and_back_roundtrip() {
    // Deterministic 32-byte public key
    let mut pk = [0u8; 32];
    for i in 0..32 {
        pk[i] = i as u8;
    }
    let addr = byte_key_to_address(&pk).expect("can encode address");
    let decoded = address_to_byte_key(&addr).expect("can decode address");
    assert_eq!(decoded, pk);
}

#[test]
fn public_key_bytes_errors_without_address() {
    let ops = AlgoOps::new(None, None, None);
    let err = ops.public_key_bytes().unwrap_err();
    let msg = format!("{}", err);
    assert!(msg.contains("needs an address"));
}

#[test]
fn address_to_byte_key_rejects_invalid() {
    let bad = "INVALIDADDRESS123";
    let res = address_to_byte_key(bad);
    assert!(res.is_err());
}

#[test]
fn public_key_bytes_succeeds_with_valid_address() {
    // Build a valid address from a deterministic pk and then feed it back via AlgoOps
    let mut pk = [0u8; 32];
    for i in 0..32 { pk[i] = (255 - i as u8) as u8; }
    let addr = byte_key_to_address(&pk).expect("encode addr");

    let ops = AlgoOps::new(None, Some(addr.clone()), None);
    let extracted = ops.public_key_bytes().expect("extract pk");
    assert_eq!(extracted, pk);
}
