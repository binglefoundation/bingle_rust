//! The key storage surface (issue #385) on a host build: the file store is the default, its
//! settings report `KeyStorageUnavailable`, and the Keychain store is refused off iOS. The Keychain
//! itself is covered by the Swift XCTest on a device and by bingle_local's tests.

use bingle_jsi::api::bingle_jsi_api::BingleJsiApi;
use bingle_jsi::api::bingle_jsi_api_impl::BingleJsiApiImpl;
use bingle_jsi::api::error::BingleJsiError;
use bingle_jsi::api::types::{BingleJsiConfig, KeyStorageBackend};
use bingle_test::temp_file_helpers::project_tmp_file_path;

fn config(path: &str, key_storage: Option<&str>) -> BingleJsiConfig {
    BingleJsiConfig {
        handle: None,
        passphrase: None,
        relay: false,
        static_ip: None,
        stun_servers: None,
        stun_servers_file: None,
        node_file: None,
        log_level: None,
        app_id: None,
        asset_id: None,
        handle_cache_expiry_secs: None,
        debug: false,
        local: Some(path.to_string()),
        notify_gateway_url: None,
        notify_on_giveup: None,
        notify_env: None,
        store_and_forward_send: None,
        store_and_forward_receive: None,
        store_and_forward_poll_interval_secs: None,
        send_pending_messages: None,
        key_storage: key_storage.map(str::to_string),
    }
}

fn state_path() -> String {
    project_tmp_file_path("bingle-jsi-key-storage", ".json")
        .to_string_lossy()
        .into_owned()
}

#[test]
#[cfg(not(target_os = "ios"))]
fn the_file_store_is_the_default_off_ios() {
    let api = BingleJsiApiImpl::init(config(&state_path(), None)).expect("init");
    let status = api.key_storage_status().expect("status");
    assert_eq!(status.backend, KeyStorageBackend::File);
    assert!(!status.sync_choice_pending);
    assert!(!status.reinstall_detected);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn file_store_settings_report_key_storage_unavailable() {
    let api = BingleJsiApiImpl::init(config(&state_path(), Some("file"))).expect("init");
    api.generate_keypair().expect("generate");
    assert!(matches!(
        api.set_device_only(false),
        Err(BingleJsiError::KeyStorageUnavailable { .. })
    ));
    assert!(matches!(
        api.mark_phrase_backed_up(),
        Err(BingleJsiError::KeyStorageUnavailable { .. })
    ));
    assert!(matches!(
        api.restore_stored_key(),
        Err(BingleJsiError::KeyStorageUnavailable { .. })
    ));
    assert!(matches!(
        api.remove_key_from_device(),
        Err(BingleJsiError::KeyStorageUnavailable { .. })
    ));
}

#[test]
#[cfg(not(target_os = "ios"))]
fn the_keychain_store_is_refused_off_ios() {
    assert!(matches!(
        BingleJsiApiImpl::init(config(&state_path(), Some("keychain"))),
        Err(BingleJsiError::InvalidRequest { .. })
    ));
}

#[test]
fn an_unknown_key_storage_value_is_refused() {
    assert!(matches!(
        BingleJsiApiImpl::init(config(&state_path(), Some("cloud"))),
        Err(BingleJsiError::InvalidRequest { .. })
    ));
}

#[test]
fn key_storage_needs_the_local_api() {
    let mut cfg = config(&state_path(), None);
    cfg.local = None;
    cfg.handle = Some("someone".to_string());
    let api = BingleJsiApiImpl::init(cfg).expect("init");
    assert!(matches!(
        api.key_storage_status(),
        Err(BingleJsiError::InvalidRequest { .. })
    ));
}
