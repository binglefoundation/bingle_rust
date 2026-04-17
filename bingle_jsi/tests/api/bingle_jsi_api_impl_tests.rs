use bingle_jsi::api::bingle_jsi_api::BingleJsiApi;
use bingle_jsi::api::bingle_jsi_api_impl::BingleJsiApiImpl;
use bingle_jsi::api::error::BingleJsiError;
use bingle_jsi::api::types::{ContactSource, KeypairStatus, NatType};

// ── init tests ───────────────────────────────────────────────────────

#[test]
fn init_with_handle_succeeds() {
    let args = vec!["--handle".to_string(), "testuser".to_string()];
    let api = BingleJsiApiImpl::init(args);
    assert!(api.is_ok(), "init should succeed with --handle: {:?}", api.err());
}

#[test]
fn init_with_local_and_no_handle_succeeds() {
    let tmp = std::env::temp_dir().join("bingle_jsi_test_init_local.json");
    let args = vec!["--local".to_string(), tmp.to_string_lossy().to_string()];
    let api = BingleJsiApiImpl::init(args);
    assert!(api.is_ok(), "init with --local and no handle should succeed: {:?}", api.err());
    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn init_with_no_args_fails() {
    let args: Vec<String> = vec![];
    let result = BingleJsiApiImpl::init(args);
    assert!(result.is_err());
    match result {
        Err(BingleJsiError::InvalidRequest { .. }) => {}
        other => panic!("Expected InvalidRequest, got {:?}", other.err()),
    }
}

#[test]
fn init_local_missing_value_fails() {
    let args = vec!["--local".to_string()];
    let result = BingleJsiApiImpl::init(args);
    assert!(result.is_err());
    match result {
        Err(BingleJsiError::InvalidRequest { .. }) => {}
        other => panic!("Expected InvalidRequest, got {:?}", other.err()),
    }
}

// ── version test ─────────────────────────────────────────────────────

#[test]
fn version_returns_valid_info() {
    let args = vec!["--handle".to_string(), "testuser".to_string()];
    let api = BingleJsiApiImpl::init(args).expect("init should succeed");
    let info = api.version().expect("version should succeed");
    assert!(!info.version.is_empty());
    assert!(!info.build_timestamp.is_empty());
    assert!(!info.build_number.is_empty());
}

// ── queued test ──────────────────────────────────────────────────────

#[test]
fn queued_returns_empty_initially() {
    let args = vec!["--handle".to_string(), "testuser".to_string()];
    let api = BingleJsiApiImpl::init(args).expect("init should succeed");
    let messages = api.queued().expect("queued should succeed");
    assert!(messages.is_empty());
}

// ── NAT type test ────────────────────────────────────────────────────

#[test]
fn get_nat_type_returns_unknown_initially() {
    let args = vec!["--handle".to_string(), "testuser".to_string()];
    let api = BingleJsiApiImpl::init(args).expect("init should succeed");
    let nat = api.get_nat_type().expect("get_nat_type should succeed");
    assert_eq!(nat.nat_type, NatType::Unknown);
}

// ── local API guard tests (no --local) ───────────────────────────────

#[test]
fn local_methods_fail_without_local_flag() {
    let args = vec!["--handle".to_string(), "testuser".to_string()];
    let api = BingleJsiApiImpl::init(args).expect("init should succeed");

    let result = api.generate_keypair();
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), BingleJsiError::InvalidRequest { .. }));

    let result = api.get_contacts();
    assert!(result.is_err());

    let result = api.get_messages();
    assert!(result.is_err());

    let result = api.keypair_status();
    assert!(result.is_err());

    let result = api.is_blocked("someid".to_string());
    assert!(result.is_err());
}

// ── local API tests (with --local) ───────────────────────────────────

fn init_with_local() -> std::sync::Arc<BingleJsiApiImpl> {
    let tmp = std::env::temp_dir().join(format!(
        "bingle_jsi_test_{}.json",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let args = vec!["--local".to_string(), tmp.to_string_lossy().to_string()];
    BingleJsiApiImpl::init(args).expect("init with --local should succeed")
}

#[test]
fn keypair_status_returns_none_initially() {
    let api = init_with_local();
    let status = api.keypair_status().expect("keypair_status should succeed");
    assert_eq!(status.status, KeypairStatus::None);
    assert!(status.id.is_none());
    assert!(status.handle.is_none());
}

#[test]
fn generate_keypair_succeeds() {
    let api = init_with_local();
    let kp = api.generate_keypair().expect("generate_keypair should succeed");
    assert!(!kp.id.is_empty());
    assert!(!kp.passphrase.is_empty());
}

#[test]
fn generate_keypair_changes_status_to_unfunded() {
    let api = init_with_local();
    let _kp = api.generate_keypair().expect("generate_keypair should succeed");
    let status = api.keypair_status().expect("keypair_status should succeed");
    assert_eq!(status.status, KeypairStatus::Unfunded);
    assert!(status.id.is_some());
}

#[test]
fn add_and_get_contacts() {
    let api = init_with_local();
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
    let api = init_with_local();
    api.add_contact(
        "bob".to_string(),
        "BOB_ID".to_string(),
        ContactSource::Manual,
    )
    .expect("add_contact should succeed");

    api.block_contact("BOB_ID".to_string())
        .expect("block_contact should succeed");

    let blocked = api.is_blocked("BOB_ID".to_string()).expect("is_blocked should succeed");
    assert!(blocked);

    let contacts = api.get_contacts().expect("get_contacts should succeed");
    assert!(contacts.is_empty());
}

#[test]
fn remove_contact_removes_without_blocking() {
    let api = init_with_local();
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

    let blocked = api.is_blocked("CAROL_ID".to_string()).expect("is_blocked should succeed");
    assert!(!blocked);
}

#[test]
fn add_and_get_messages() {
    let api = init_with_local();
    api.add_message(
        "alice".to_string(),
        vec!["bob".to_string()],
        1000,
        "Hello Bob".to_string(),
    )
    .expect("add_message should succeed");

    let messages = api.get_messages().expect("get_messages should succeed");
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].sender_handle, "alice");
    assert_eq!(messages[0].recipient_handles, vec!["bob".to_string()]);
    assert_eq!(messages[0].timestamp, 1000);
    assert_eq!(messages[0].text, "Hello Bob");
}

#[test]
fn save_and_load_round_trip() {
    let tmp = std::env::temp_dir().join(format!(
        "bingle_jsi_save_test_{}.json",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let tmp_str = tmp.to_string_lossy().to_string();

    let api = init_with_local();
    api.add_contact(
        "dave".to_string(),
        "DAVE_ID".to_string(),
        ContactSource::Manual,
    )
    .expect("add_contact should succeed");

    api.save(tmp_str.clone()).expect("save should succeed");

    // Create a fresh instance with --local and load the saved state
    let api2 = init_with_local();
    api2.load(tmp_str.clone()).expect("load should succeed");

    let contacts = api2.get_contacts().expect("get_contacts should succeed");
    assert_eq!(contacts.len(), 1);
    assert_eq!(contacts[0].handle, "dave");

    let _ = std::fs::remove_file(&tmp);
}
