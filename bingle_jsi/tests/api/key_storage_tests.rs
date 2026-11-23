//! The key storage surface (issue #385) on a host build: the file store is the default, its
//! settings report `KeyStorageUnavailable`, and the Keychain store is refused off iOS. The Keychain
//! itself is covered by the Swift XCTest on a device and by bingle_local's tests.

use bingle_jsi::api::bingle_jsi_api::BingleJsiApi;
use bingle_jsi::api::bingle_jsi_api_impl::BingleJsiApiImpl;
use bingle_jsi::api::error::BingleJsiError;
use bingle_jsi::api::types::{BingleJsiConfig, ContactSource, KeyStorageBackend, StateFileStatus};
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
        encrypt_state: None,
        enable_global_block: None,
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

#[test]
#[cfg(not(target_os = "ios"))]
fn delete_account_with_no_keypair_deletes_the_state_file() {
    // With no keypair there is nothing to opt out on chain, so this needs no network (issue #100).
    let path = project_tmp_file_path("bingle-jsi-delete-account", ".json")
        .to_string_lossy()
        .into_owned();
    let api = BingleJsiApiImpl::init(config(&path, Some("file"))).expect("init");
    api.add_contact("friend".into(), "FRIEND_ID".into(), ContactSource::Manual)
        .expect("add contact");
    api.save(path.clone()).expect("save");
    assert!(std::path::Path::new(&path).exists());

    api.delete_account().expect("delete");

    assert!(!std::path::Path::new(&path).exists(), "state file deleted");
    assert!(api.get_contacts().expect("contacts").is_empty());
    assert!(!api.is_started());
    api.delete_account().expect("deleting again is safe");
}

#[test]
#[cfg(not(target_os = "ios"))]
fn delete_account_reports_an_unreachable_node_as_no_blockchain_and_changes_nothing() {
    // A node file pointing at a closed local port, so the on-chain opt-out can't reach algod
    // (issue #100).
    let node_file = project_tmp_file_path("bingle-jsi-delete-unreachable-node", ".json");
    std::fs::write(
        &node_file,
        r#"{"network":"localnet","client_api_url":"http://127.0.0.1","client_api_port":9,"indexer_api_url":"http://127.0.0.1","indexer_api_port":9,"token":null,"token_key":null,"app_id":1,"asset_id":2}"#,
    )
    .expect("write node file");
    let path = project_tmp_file_path("bingle-jsi-delete-unreachable", ".json")
        .to_string_lossy()
        .into_owned();
    let mut cfg = config(&path, Some("file"));
    cfg.node_file = Some(node_file.to_string_lossy().into_owned());
    let api = BingleJsiApiImpl::init(cfg).expect("init");
    let kp = api.generate_keypair().expect("generate");
    api.save(path.clone()).expect("save");

    let err = api
        .delete_account()
        .expect_err("the opt-out can't reach the node");

    assert!(
        matches!(err, BingleJsiError::NoBlockchain { .. }),
        "an unreachable node is NoBlockchain: {err:?}"
    );
    let text = std::fs::read_to_string(&path).expect("the state file is kept");
    assert!(text.contains(&kp.id), "the keypair is kept");
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&node_file);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn encrypt_state_with_the_file_store_is_refused() {
    // Sealing needs the Keychain's device secret (issue #459); never fall back to plain JSON.
    let mut cfg = config(&state_path(), Some("file"));
    cfg.encrypt_state = Some(true);
    assert!(matches!(
        BingleJsiApiImpl::init(cfg),
        Err(BingleJsiError::InvalidRequest { .. })
    ));
}

#[test]
#[cfg(not(target_os = "ios"))]
fn state_file_status_reports_a_new_install_and_a_plain_file() {
    let path = project_tmp_file_path("bingle-jsi-state-status", ".json")
        .to_string_lossy()
        .into_owned();
    let api = BingleJsiApiImpl::init(config(&path, Some("file"))).expect("init");
    assert_eq!(api.state_file_status().unwrap(), StateFileStatus::NotLoaded);
    api.generate_keypair().expect("generate");
    api.save(path.clone()).expect("save");

    let reopened = BingleJsiApiImpl::init(config(&path, Some("file"))).expect("init again");
    assert_eq!(
        reopened.state_file_status().unwrap(),
        StateFileStatus::Plain
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn forget_state_key_saves_and_a_plain_file_needs_no_phrase() {
    let path = project_tmp_file_path("bingle-jsi-forget-key", ".json")
        .to_string_lossy()
        .into_owned();
    let api = BingleJsiApiImpl::init(config(&path, Some("file"))).expect("init");
    let kp = api.generate_keypair().expect("generate");

    api.forget_state_key().expect("forget");
    let text = std::fs::read_to_string(&path).expect("saved before forgetting");
    assert!(text.contains(&kp.id));

    assert!(
        api.open_state_with_recovery_phrase(kp.passphrase).is_err(),
        "a plain file isn't opened with the phrase"
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn export_keypair_returns_the_current_keypair_with_the_file_store() {
    let path = state_path();
    let api = BingleJsiApiImpl::init(config(&path, Some("file"))).expect("init");
    let kp = api.generate_keypair().expect("generate");

    let exported = api.export_keypair().expect("export");
    assert_eq!(exported.id, kp.id);
    assert_eq!(exported.passphrase, kp.passphrase);
    let _ = std::fs::remove_file(&path);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn export_keypair_with_no_keypair_is_an_invalid_request() {
    let path = state_path();
    let api = BingleJsiApiImpl::init(config(&path, Some("file"))).expect("init");
    assert!(matches!(
        api.export_keypair(),
        Err(BingleJsiError::InvalidRequest { .. })
    ));
    let _ = std::fs::remove_file(&path);
}
