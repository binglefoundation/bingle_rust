use bingle_core::api::bingle_api::{BingleApi, BingleApiInternal, StartOptions};
use bingle_core::api::bingle_api_impl::BingleApiImpl;
use bingle_core::ddb::{AdvertRecord, InetSocketAddress};
use bingle_core::engine::BingleAccessUnsafeForTests;

#[path = "../test_util.rs"]
pub mod test_util;

#[test]
pub fn test_ddb_upsert_record_mandatory_verification() {
    let addr = "127.0.0.1:0".parse().unwrap();
    let opts = StartOptions {
        handle: "test".into(),
        algo_passphrase: Some(test_util::PASSPHRASE_RECEIVE.to_string()),
        static_ip: Some(addr),
        am_relay: false,
        stun_servers: None,
        algo_provider_config: None,
        algo_network: None,
        app_id: None,
        asset_id: None,
        log_level: None,
        handle_cache_expiry: None,
        dangerous_debug: true,
        log_mode: bingle_core::util::logging::LogMode::Plain,
        wait_response_timeout: None,
    };
    let api = BingleApiImpl::new(&opts);
    api.access_unsafe_for_tests(|a| a.start(&opts))
        .expect("start ok");

    let my_id = api
        .access_unsafe_for_tests(|a| a.get_my_id())
        .expect("get_my_id ok");
    let signing_key = api
        .access_unsafe_for_tests(|a| a.get_signing_key())
        .expect("get_signing_key ok");

    // 1. Create a signed record
    let signed_record = AdvertRecord::new(
        my_id.clone(),
        Some(InetSocketAddress {
            host: "127.0.0.1".into(),
            port: 1234,
        }),
        Some(false),
        None,
        None,
        "2025-01-01T00:00:00Z".into(),
        &signing_key,
    );
    assert!(signed_record.verify(), "Signed record should verify");

    // 2. Create an unsigned record with a DIFFERENT ID
    let other_id = "OTHER_ID".to_string();
    let unsigned_record = AdvertRecord::new_unsigned(
        other_id,
        Some(InetSocketAddress {
            host: "127.0.0.1".into(),
            port: 1235,
        }),
        Some(false),
        None,
        None,
        "2025-01-01T00:00:00Z".into(),
    );
    assert!(
        !unsigned_record.verify(),
        "Unsigned record should not verify"
    );

    // 3. Upsert signed record - should succeed
    api.ddb_upsert_record(signed_record.clone());
    assert_eq!(
        api.ddb_backend_size(),
        1,
        "Signed record should be upserted"
    );

    // 4. Upsert unsigned record - should be ignored (currently it works but logs a warning)
    api.ddb_upsert_record(unsigned_record);
    // After my change, it should still be 1
    assert_eq!(
        api.ddb_backend_size(),
        1,
        "Unsigned record should NOT be upserted"
    );

    api.access_unsafe_for_tests(|a| a.stop());
}
