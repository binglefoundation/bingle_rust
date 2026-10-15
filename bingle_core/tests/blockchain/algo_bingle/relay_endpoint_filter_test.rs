// Issue #324: relay discovery keeps only accounts with a static endpoint and the allow_relay
// permission, read from the indexer's decoded local state (packed bitfield or legacy scalars).
use bingle_core::blockchain::algo_bingle::{AlgoBingle, allow_flags};

fn kv(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

fn packed(bits: u64) -> String {
    (bits | allow_flags::BIT_MIGRATED).to_string()
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn test_relay_allowed_packed_bitfield() {
    let allowed = packed(allow_flags::BIT_STATIC | allow_flags::BIT_RELAY);
    assert!(AlgoBingle::relay_allowed_in_local_state(&kv(&[(
        "allow_static",
        &allowed
    )])));
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn test_relay_not_allowed_packed_bitfield_static_only() {
    // Permission withdrawn, endpoint left in place: static still set, relay cleared.
    let static_only = packed(allow_flags::BIT_STATIC);
    assert!(!AlgoBingle::relay_allowed_in_local_state(&kv(&[
        ("allow_static", &static_only),
        ("static_endpoint", "127.0.0.1:4000"),
    ])));
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn test_relay_allowed_legacy_scalar() {
    assert!(AlgoBingle::relay_allowed_in_local_state(&kv(&[
        ("allow_static", "1"),
        ("allow_relay", "1"),
    ])));
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn test_relay_not_allowed_legacy_scalar_cleared() {
    assert!(!AlgoBingle::relay_allowed_in_local_state(&kv(&[
        ("allow_static", "1"),
        ("allow_relay", "0"),
    ])));
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn test_relay_not_allowed_without_flags() {
    assert!(!AlgoBingle::relay_allowed_in_local_state(&kv(&[(
        "static_endpoint",
        "127.0.0.1:4000"
    )])));
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn test_static_endpoint_joins_overflow() {
    assert_eq!(
        AlgoBingle::static_endpoint_from_local_state(&kv(&[
            ("static_endpoint", "abc"),
            ("static_endpoint_x", "def"),
        ])),
        Some("abcdef".to_string())
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn test_static_endpoint_absent() {
    assert_eq!(
        AlgoBingle::static_endpoint_from_local_state(&kv(&[("allow_relay", "1")])),
        None
    );
}
