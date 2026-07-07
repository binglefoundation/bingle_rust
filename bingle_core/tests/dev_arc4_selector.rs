use sha2::{Digest, Sha512_256};

#[test]
#[cfg(not(target_os = "ios"))]
pub fn print_set_allow_static_selector() {
    let sig = "set_allow_static(address,uint64)void";
    let mut h = Sha512_256::new();
    h.update(sig.as_bytes());
    let digest: [u8; 32] = h.finalize().into();
    tracing::info!(
        "selector for {}: 0x{:02x}{:02x}{:02x}{:02x}",
        sig,
        digest[0],
        digest[1],
        digest[2],
        digest[3]
    );
    assert!(true);
}
