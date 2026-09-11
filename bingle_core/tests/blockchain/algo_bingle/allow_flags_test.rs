// Offline unit tests for the packed allow-flag bitfield decode (issue #232): `effective_allow_bits`
// must decode both the packed encoding (MIGRATED sentinel set) and the legacy separate
// allow_static / allow_relay scalars, and the new set_allow_sw_* bindings must validate their params.
use algo_ops::AlgoOps;
use bingle_core::blockchain::algo_bingle::{AlgoBingle, allow_flags};

fn kv(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

fn ab() -> AlgoBingle {
    let ops = AlgoOps::new_for_algorand(
        None,
        Some("P577PSTDICQ6PQFBR5YMDMJ2YVK7LT5V4GOPNVDLCEDJIL7XGRWC5BRFWA".to_string()),
        None,
    );
    AlgoBingle::new(ops, 1, 1)
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn test_effective_allow_bits_legacy_static_only() {
    // Legacy account: allow_static scalar = 1, no allow_relay. Static set, everything else clear.
    let bits = AlgoBingle::effective_allow_bits(&kv(&[("allow_static", "1")]));
    assert_eq!(bits & allow_flags::BIT_STATIC, allow_flags::BIT_STATIC);
    assert_eq!(bits & allow_flags::BIT_RELAY, 0);
    assert_eq!(bits & allow_flags::BIT_SW_NODE, 0);
    assert_eq!(bits & allow_flags::BIT_SW_CLIENT, 0);
    assert_eq!(bits & allow_flags::BIT_MIGRATED, 0);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn test_effective_allow_bits_legacy_relay_only_without_static_key() {
    // Legacy account granted only relay: allow_relay = 1 and no allow_static key at all. The relay
    // grant must still be seen (the folder does not early-return on a missing allow_static slot).
    let bits = AlgoBingle::effective_allow_bits(&kv(&[("allow_relay", "1")]));
    assert_eq!(bits & allow_flags::BIT_RELAY, allow_flags::BIT_RELAY);
    assert_eq!(bits & allow_flags::BIT_STATIC, 0);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn test_effective_allow_bits_legacy_both() {
    let bits =
        AlgoBingle::effective_allow_bits(&kv(&[("allow_static", "1"), ("allow_relay", "1")]));
    assert_eq!(bits & allow_flags::BIT_STATIC, allow_flags::BIT_STATIC);
    assert_eq!(bits & allow_flags::BIT_RELAY, allow_flags::BIT_RELAY);
    assert_eq!(bits & allow_flags::BIT_SW_NODE, 0);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn test_effective_allow_bits_packed_ignores_legacy_relay_key() {
    // A migrated account: allow_static holds the packed bitfield (static + sw_node). A stale legacy
    // allow_relay key must be ignored because the sentinel is set (packed value is authoritative).
    let packed = allow_flags::BIT_MIGRATED | allow_flags::BIT_STATIC | allow_flags::BIT_SW_NODE;
    let entries = kv(&[("allow_static", &packed.to_string()), ("allow_relay", "1")]);
    let bits = AlgoBingle::effective_allow_bits(&entries);
    assert_eq!(bits, packed);
    assert_eq!(bits & allow_flags::BIT_STATIC, allow_flags::BIT_STATIC);
    assert_eq!(bits & allow_flags::BIT_SW_NODE, allow_flags::BIT_SW_NODE);
    // Relay bit was NOT set in the packed value, so it reads false despite the stale legacy key.
    assert_eq!(bits & allow_flags::BIT_RELAY, 0);
    assert_eq!(bits & allow_flags::BIT_SW_CLIENT, 0);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn test_effective_allow_bits_packed_sw_client() {
    let packed = allow_flags::BIT_MIGRATED | allow_flags::BIT_SW_CLIENT;
    let bits = AlgoBingle::effective_allow_bits(&kv(&[("allow_static", &packed.to_string())]));
    assert_eq!(
        bits & allow_flags::BIT_SW_CLIENT,
        allow_flags::BIT_SW_CLIENT
    );
    assert_eq!(bits & allow_flags::BIT_STATIC, 0);
    assert_eq!(bits & allow_flags::BIT_RELAY, 0);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn test_effective_allow_bits_migrated_only_is_all_false() {
    // Sentinel set but no permission bits => every flag reads false.
    let packed = allow_flags::BIT_MIGRATED;
    let bits = AlgoBingle::effective_allow_bits(&kv(&[("allow_static", &packed.to_string())]));
    assert_eq!(bits & allow_flags::BIT_STATIC, 0);
    assert_eq!(bits & allow_flags::BIT_RELAY, 0);
    assert_eq!(bits & allow_flags::BIT_SW_NODE, 0);
    assert_eq!(bits & allow_flags::BIT_SW_CLIENT, 0);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn test_effective_allow_bits_empty_is_zero() {
    assert_eq!(AlgoBingle::effective_allow_bits(&kv(&[])), 0);
    // Unrelated keys only => still zero.
    assert_eq!(
        AlgoBingle::effective_allow_bits(&kv(&[("Handle", "alice"), ("HandleTime", "5")])),
        0
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn test_set_allow_sw_node_param_validation() {
    let ab = ab();
    let res = ab.set_allow_sw_node(
        0,
        "P577PSTDICQ6PQFBR5YMDMJ2YVK7LT5V4GOPNVDLCEDJIL7XGRWC5BRFWA",
        true,
    );
    assert!(res.is_err());
    assert!(res.unwrap_err().to_string().contains("app_id must be > 0"));

    let res = ab.set_allow_sw_node(1, "invalid_addr", true);
    assert!(res.is_err());
    assert!(
        res.unwrap_err()
            .to_string()
            .contains("invalid target address")
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn test_set_allow_sw_client_param_validation() {
    let ab = ab();
    let res = ab.set_allow_sw_client(
        0,
        "P577PSTDICQ6PQFBR5YMDMJ2YVK7LT5V4GOPNVDLCEDJIL7XGRWC5BRFWA",
        true,
    );
    assert!(res.is_err());
    assert!(res.unwrap_err().to_string().contains("app_id must be > 0"));
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn test_check_allow_sw_param_validation() {
    let ab = ab();
    for res in [
        ab.check_allow_static(
            0,
            "P577PSTDICQ6PQFBR5YMDMJ2YVK7LT5V4GOPNVDLCEDJIL7XGRWC5BRFWA",
        ),
        ab.check_allow_sw_node(
            0,
            "P577PSTDICQ6PQFBR5YMDMJ2YVK7LT5V4GOPNVDLCEDJIL7XGRWC5BRFWA",
        ),
        ab.check_allow_sw_client(
            0,
            "P577PSTDICQ6PQFBR5YMDMJ2YVK7LT5V4GOPNVDLCEDJIL7XGRWC5BRFWA",
        ),
    ] {
        assert!(res.is_err());
        assert!(res.unwrap_err().to_string().contains("app_id must be > 0"));
    }
}
