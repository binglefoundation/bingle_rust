use std::sync::{Arc, Mutex};

use bingle_jsi::api::bingle_jsi_api::BingleJsiApi;
use bingle_jsi::api::bingle_jsi_api_impl::BingleJsiApiImpl;
use bingle_jsi::api::callback::{ListeningCallback, MessageCallback};
use bingle_jsi::api::error::BingleJsiError;
use bingle_jsi::api::types::{
    BingleJsiConfig, BingleMessage, ContactSource, KeypairStatus, NatType,
};
use bingle_test::temp_file_helpers::project_tmp_file_path;

/// Helper: build a minimal config with only `handle` set.
fn config_with_handle(handle: &str) -> BingleJsiConfig {
    BingleJsiConfig {
        handle: Some(handle.to_string()),
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
        local: None,
    }
}

/// Helper: build a config with only `local` set (no handle).
fn config_with_local(path: &str) -> BingleJsiConfig {
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
    }
}

/// Helper: build an empty config (no handle, no local).
fn empty_config() -> BingleJsiConfig {
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
        local: None,
    }
}

// ── init tests ───────────────────────────────────────────────────────

#[test]
fn init_with_handle_succeeds() {
    let api = BingleJsiApiImpl::init(config_with_handle("testuser"));
    assert!(
        api.is_ok(),
        "init should succeed with handle: {:?}",
        api.err()
    );
}

#[test]
fn create_bingle_api_returns_trait_object() {
    let api = bingle_jsi::create_bingle_api(config_with_handle("testuser"));
    assert!(
        api.is_ok(),
        "create_bingle_api should succeed: {:?}",
        api.err()
    );
    let api = api.unwrap();
    let info = api
        .version()
        .expect("version should succeed on trait object");
    assert!(!info.version.is_empty());
}

#[test]
fn init_with_local_and_no_handle_succeeds() {
    let tmp = project_tmp_file_path("bingle-jsi-test-init-local", ".json");
    let api = BingleJsiApiImpl::init(config_with_local(&tmp.to_string_lossy()));
    assert!(
        api.is_ok(),
        "init with local and no handle should succeed: {:?}",
        api.err()
    );
    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn init_with_no_handle_and_no_local_fails() {
    let result = BingleJsiApiImpl::init(empty_config());
    assert!(result.is_err());
    match result {
        Err(BingleJsiError::InvalidRequest { .. }) => {}
        other => panic!("Expected InvalidRequest, got {:?}", other.err()),
    }
}

// ── version test ─────────────────────────────────────────────────────

#[test]
fn version_returns_valid_info() {
    let api = BingleJsiApiImpl::init(config_with_handle("testuser")).expect("init should succeed");
    let info = api.version().expect("version should succeed");
    assert!(!info.version.is_empty());
    assert!(!info.build_timestamp.is_empty());
    assert!(!info.build_number.is_empty());
}

// ── queued test ──────────────────────────────────────────────────────

#[test]
fn queued_returns_empty_initially() {
    let api = BingleJsiApiImpl::init(config_with_handle("testuser")).expect("init should succeed");
    let messages = api.queued().expect("queued should succeed");
    assert!(messages.is_empty());
}

// ── NAT type test ────────────────────────────────────────────────────

#[test]
fn get_nat_type_returns_unknown_initially() {
    let api = BingleJsiApiImpl::init(config_with_handle("testuser")).expect("init should succeed");
    let nat = api.get_nat_type().expect("get_nat_type should succeed");
    assert_eq!(nat.nat_type, NatType::Unknown);
}

// ── local API guard tests (no local) ─────────────────────────────────

#[test]
fn local_methods_fail_without_local_flag() {
    let api = BingleJsiApiImpl::init(config_with_handle("testuser")).expect("init should succeed");

    let result = api.generate_keypair();
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        BingleJsiError::InvalidRequest { .. }
    ));

    let result = api.get_contacts();
    assert!(result.is_err());

    let result = api.get_messages();
    assert!(result.is_err());

    let result = api.keypair_status();
    assert!(result.is_err());

    let result = api.is_blocked("someid".to_string());
    assert!(result.is_err());
}

// ── local API tests (with local) ─────────────────────────────────────

fn init_with_local_helper() -> std::sync::Arc<BingleJsiApiImpl> {
    let tmp = project_tmp_file_path("bingle-jsi-test", ".json");
    BingleJsiApiImpl::init(config_with_local(&tmp.to_string_lossy()))
        .expect("init with local should succeed")
}

#[test]
fn keypair_status_returns_none_initially() {
    let api = init_with_local_helper();
    let status = api.keypair_status().expect("keypair_status should succeed");
    assert_eq!(status.status, KeypairStatus::None);
    assert!(status.id.is_none());
    assert!(status.handle.is_none());
}

#[test]
fn generate_keypair_succeeds() {
    let api = init_with_local_helper();
    let kp = api
        .generate_keypair()
        .expect("generate_keypair should succeed");
    assert!(!kp.id.is_empty());
    assert!(!kp.passphrase.is_empty());
}

#[test]
#[ignore] // needs localnet
fn generate_keypair_changes_status_to_unfunded() {
    let api = init_with_local_helper();
    let _kp = api
        .generate_keypair()
        .expect("generate_keypair should succeed");
    let status = api.keypair_status().expect("keypair_status should succeed");
    assert_eq!(status.status, KeypairStatus::Unfunded);
    assert!(status.id.is_some());
}

#[test]
fn add_and_get_contacts() {
    let api = init_with_local_helper();
    api.add_contact(
        "alice".to_string(),
        "ALICE_ID".to_string(),
        ContactSource::Manual,
    )
    .expect("add_contact should succeed");

    let contacts = api.get_contacts().expect("get_contacts should succeed");
    assert_eq!(contacts.len(), 1);
    assert_eq!(contacts[0].handle, "alice");
    assert_eq!(contacts[0].id, "ALICE_ID");
}

#[test]
fn block_contact_hides_from_contacts() {
    let api = init_with_local_helper();
    api.add_contact(
        "bob".to_string(),
        "BOB_ID".to_string(),
        ContactSource::Manual,
    )
    .expect("add_contact should succeed");

    api.block_contact("BOB_ID".to_string())
        .expect("block_contact should succeed");

    let blocked = api
        .is_blocked("BOB_ID".to_string())
        .expect("is_blocked should succeed");
    assert!(blocked);

    let contacts = api.get_contacts().expect("get_contacts should succeed");
    assert!(contacts.is_empty());
}

#[test]
fn remove_contact_removes_without_blocking() {
    let api = init_with_local_helper();
    api.add_contact(
        "carol".to_string(),
        "CAROL_ID".to_string(),
        ContactSource::Received,
    )
    .expect("add_contact should succeed");

    api.remove_contact("CAROL_ID".to_string())
        .expect("remove_contact should succeed");

    let contacts = api.get_contacts().expect("get_contacts should succeed");
    assert!(contacts.is_empty());

    let blocked = api
        .is_blocked("CAROL_ID".to_string())
        .expect("is_blocked should succeed");
    assert!(!blocked);
}

#[test]
fn add_and_get_messages() {
    let api = init_with_local_helper();
    api.add_message(
        "alice".to_string(),
        vec!["bob".to_string()],
        1000,
        "Hello Bob".to_string(),
        Some("TLS_AES_256_GCM_SHA384".to_string()),
    )
    .expect("add_message should succeed");

    let messages = api.get_messages().expect("get_messages should succeed");
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].sender_handle, "alice");
    assert_eq!(messages[0].recipient_handles, vec!["bob".to_string()]);
    assert_eq!(messages[0].timestamp, 1000);
    assert_eq!(messages[0].text, "Hello Bob");
    let cs = messages[0]
        .cipher_suite
        .as_ref()
        .expect("cipher_suite should be Some");
    assert_eq!(cs, "TLS_AES_256_GCM_SHA384");
}

#[test]
fn save_and_load_round_trip() {
    let tmp_save = project_tmp_file_path("bingle-jsi-save-test", ".json");
    let api = init_with_local_helper();
    api.add_contact(
        "dave".to_string(),
        "DAVE_ID".to_string(),
        ContactSource::Manual,
    )
    .expect("add_contact should succeed");

    api.save(tmp_save.to_string_lossy().to_string())
        .expect("save should succeed");

    // Load into a new instance
    let api2 = init_with_local_helper();
    api2.load(tmp_save.to_string_lossy().to_string())
        .expect("load should succeed");

    let contacts = api2.get_contacts().expect("get_contacts should succeed");
    assert_eq!(contacts.len(), 1);
    assert_eq!(contacts[0].handle, "dave");
    assert_eq!(contacts[0].id, "DAVE_ID");

    let _ = std::fs::remove_file(&tmp_save);
}

#[test]
fn init_with_optional_fields() {
    let config = BingleJsiConfig {
        handle: Some("testuser".to_string()),
        passphrase: None,
        relay: false,
        static_ip: None,
        stun_servers: None,
        stun_servers_file: None,
        node_file: None,
        log_level: Some("debug".to_string()),
        app_id: Some(12345),
        asset_id: Some(67890),
        handle_cache_expiry_secs: Some(300),
        debug: true,
        local: None,
    };
    let api = BingleJsiApiImpl::init(config);
    assert!(
        api.is_ok(),
        "init with optional fields should succeed: {:?}",
        api.err()
    );
}

// ── start / is_started tests ─────────────────────────────────────────

#[test]
fn is_started_true_after_init_without_local() {
    let api = BingleJsiApiImpl::init(config_with_handle("testuser")).expect("init should succeed");
    assert!(
        api.is_started(),
        "engine should be started when no local mode"
    );
}

#[test]
fn is_started_false_after_init_with_local() {
    let api = init_with_local_helper();
    assert!(
        !api.is_started(),
        "engine should not be started in local mode without funded keypair"
    );
}

#[test]
fn start_fails_without_local_api() {
    let api = BingleJsiApiImpl::init(config_with_handle("testuser")).expect("init should succeed");
    let result = api.start();
    assert!(
        result.is_err(),
        "start should fail when already started or no local API"
    );
}

#[test]
fn start_fails_when_keypair_none() {
    let api = init_with_local_helper();
    let result = api.start();
    assert!(result.is_err());
    match result {
        Err(BingleJsiError::InvalidRequest { reason }) => {
            assert!(
                reason.contains("FUNDED"),
                "error should mention FUNDED: {}",
                reason
            );
        }
        other => panic!("Expected InvalidRequest, got {:?}", other),
    }
}

#[test]
#[ignore] // Need localnet
fn start_fails_when_keypair_unfunded() {
    let api = init_with_local_helper();
    let _kp = api
        .generate_keypair()
        .expect("generate_keypair should succeed");
    let status = api.keypair_status().expect("keypair_status should succeed");
    assert_eq!(status.status, KeypairStatus::Unfunded);

    let result = api.start();
    assert!(result.is_err());
    match result {
        Err(BingleJsiError::InvalidRequest { reason }) => {
            assert!(
                reason.contains("FUNDED"),
                "error should mention FUNDED: {}",
                reason
            );
        }
        other => panic!("Expected InvalidRequest, got {:?}", other),
    }
    assert!(!api.is_started());
}

#[test]
fn start_fails_when_already_started() {
    let api = BingleJsiApiImpl::init(config_with_handle("testuser")).expect("init should succeed");
    assert!(api.is_started());
    let result = api.start();
    assert!(result.is_err());
    match result {
        Err(BingleJsiError::InvalidRequest { reason }) => {
            assert!(
                reason.contains("already started"),
                "error should mention already started: {}",
                reason
            );
        }
        other => panic!(
            "Expected InvalidRequest about already started, got {:?}",
            other
        ),
    }
}

// ── set_message_callback tests ───────────────────────────────────────

/// Test callback implementation that records received messages.
struct RecordingCallback {
    received: Arc<Mutex<Vec<(String, String, BingleMessage)>>>,
}

impl MessageCallback for RecordingCallback {
    fn on_message(&self, sender_id: String, sender_handle: String, message: BingleMessage) {
        let mut guard = self.received.lock().unwrap();
        guard.push((sender_id, sender_handle, message));
    }
}

#[test]
fn set_message_callback_succeeds() {
    let api = BingleJsiApiImpl::init(config_with_handle("testuser")).expect("init should succeed");
    let received: Arc<Mutex<Vec<(String, String, BingleMessage)>>> =
        Arc::new(Mutex::new(Vec::new()));
    let cb = RecordingCallback {
        received: received.clone(),
    };
    // Should not panic
    api.set_message_callback(Box::new(cb));
}

#[test]
fn set_message_callback_replaces_previous() {
    let api = BingleJsiApiImpl::init(config_with_handle("testuser")).expect("init should succeed");

    let received1: Arc<Mutex<Vec<(String, String, BingleMessage)>>> =
        Arc::new(Mutex::new(Vec::new()));
    let cb1 = RecordingCallback {
        received: received1.clone(),
    };
    api.set_message_callback(Box::new(cb1));

    let received2: Arc<Mutex<Vec<(String, String, BingleMessage)>>> =
        Arc::new(Mutex::new(Vec::new()));
    let cb2 = RecordingCallback {
        received: received2.clone(),
    };
    // Replacing should not panic
    api.set_message_callback(Box::new(cb2));
}

// ── set_listening_callback tests ─────────────────────────────────────

/// Test callback implementation that records listening state changes.
struct RecordingListeningCallback {
    events: Arc<Mutex<Vec<(bool, String)>>>,
}

impl ListeningCallback for RecordingListeningCallback {
    fn on_listening(&self, listening: bool, nat_type: String) {
        let mut guard = self.events.lock().unwrap();
        guard.push((listening, nat_type));
    }
}

#[test]
fn set_listening_callback_succeeds() {
    let api = BingleJsiApiImpl::init(config_with_handle("testuser")).expect("init should succeed");
    let events: Arc<Mutex<Vec<(bool, String)>>> = Arc::new(Mutex::new(Vec::new()));
    let cb = RecordingListeningCallback {
        events: events.clone(),
    };
    // Should not panic
    api.set_listening_callback(Box::new(cb));
}

#[test]
fn set_listening_callback_replaces_previous() {
    let api = BingleJsiApiImpl::init(config_with_handle("testuser")).expect("init should succeed");

    let events1: Arc<Mutex<Vec<(bool, String)>>> = Arc::new(Mutex::new(Vec::new()));
    let cb1 = RecordingListeningCallback {
        events: events1.clone(),
    };
    api.set_listening_callback(Box::new(cb1));

    let events2: Arc<Mutex<Vec<(bool, String)>>> = Arc::new(Mutex::new(Vec::new()));
    let cb2 = RecordingListeningCallback {
        events: events2.clone(),
    };
    // Replacing should not panic
    api.set_listening_callback(Box::new(cb2));
}
