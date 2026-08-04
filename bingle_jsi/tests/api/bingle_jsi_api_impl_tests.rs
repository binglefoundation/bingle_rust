use std::sync::{Arc, Mutex};

use bingle_jsi::api::bingle_jsi_api::BingleJsiApi;
use bingle_jsi::api::bingle_jsi_api_impl::BingleJsiApiImpl;
use bingle_jsi::api::callback::{ListeningCallback, MessageCallback};
use bingle_jsi::api::error::BingleJsiError;
use bingle_jsi::api::types::{
    BingleJsiConfig, BingleMessage, ContactSource, KeypairStatus, NatType,
};
use bingle_local::api::bingle_local_api::BingleLocalApi;
use bingle_local::api::bingle_local_api_impl::BingleApiLocalImpl;
use bingle_local::api::notify::{AlertPoster, AlertRequest};
use bingle_test::temp_file_helpers::project_tmp_file_path;

/// Throwaway test account (the committed cross-impl vector's key) used to give the give-up nudge a
/// signing identity without a live blockchain read.
const TEST_MNEMONIC: &str = "square flat curtain negative three april hobby culture unit fit drip bronze cactus stage vault pluck captain nation pond pizza grief domain coin abstract path";

/// Test poster: records every `/alert` handed to it instead of hitting a live gateway.
#[derive(Default)]
struct RecordingPoster {
    calls: Mutex<Vec<(String, AlertRequest)>>,
}

impl RecordingPoster {
    fn calls(&self) -> Vec<(String, AlertRequest)> {
        self.calls.lock().expect("calls lock").clone()
    }
}

impl AlertPoster for RecordingPoster {
    fn post_alert(&self, gateway_url: &str, body: AlertRequest) {
        self.calls
            .lock()
            .expect("calls lock")
            .push((gateway_url.to_string(), body));
    }
}

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
        notify_gateway_url: None,
        notify_on_giveup: None,
        notify_env: None,
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
        notify_gateway_url: None,
        notify_on_giveup: None,
        notify_env: None,
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
        notify_gateway_url: None,
        notify_on_giveup: None,
        notify_env: None,
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
fn init_with_local_and_notify_gateway_succeeds() {
    // The notify config surface (bingle_notify #17) is accepted end-to-end through init and
    // threaded into the local API's LocalApiConfig via LocalApiConfig::with_notify.
    let tmp = project_tmp_file_path("bingle-jsi-test-init-notify", ".json");
    let mut config = config_with_local(&tmp.to_string_lossy());
    config.notify_gateway_url = Some("https://gw.example".to_string());
    config.notify_on_giveup = Some(true);
    let api = BingleJsiApiImpl::init(config);
    assert!(
        api.is_ok(),
        "init with a notify gateway url should succeed: {:?}",
        api.err()
    );
    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn init_with_notify_disabled_succeeds() {
    // An explicit notify_on_giveup=false is accepted even with a gateway URL set.
    let tmp = project_tmp_file_path("bingle-jsi-test-init-notify-off", ".json");
    let mut config = config_with_local(&tmp.to_string_lossy());
    config.notify_gateway_url = Some("https://gw.example".to_string());
    config.notify_on_giveup = Some(false);
    let api = BingleJsiApiImpl::init(config);
    assert!(
        api.is_ok(),
        "init with the nudge disabled should succeed: {:?}",
        api.err()
    );
    let _ = std::fs::remove_file(&tmp);
}

/// End-to-end: a `notify_gateway_url` supplied through `BingleJsiConfig` reaches the local API that
/// `init` builds, so a message give-up driven through the JSI trait fires exactly one signed
/// `/alert` to that gateway (bingle_notify #17). A recording poster is injected into the
/// JSI-created local API via the `as_any_mut` downcast seam to observe the nudge deterministically.
#[test]
#[cfg(not(target_os = "ios"))]
fn end_to_end_giveup_via_jsi_fires_alert_to_configured_gateway() {
    let tmp = project_tmp_file_path("bingle-jsi-test-e2e-notify", ".json");
    let mut config = config_with_local(&tmp.to_string_lossy());
    config.notify_gateway_url = Some("https://gw.example".to_string());
    config.notify_on_giveup = Some(true);
    let api = BingleJsiApiImpl::init(config).expect("init should succeed");

    // Reach the concrete local API that init wired the notify config into, inject a recording
    // poster, and give it a signing identity (keypair + local handle) with no blockchain read.
    let recording = Arc::new(RecordingPoster::default());
    {
        let local = api.local_api_for_tests().expect("local api present");
        let mut guard = local.lock().expect("local api lock");
        let concrete = guard
            .as_any_mut()
            .downcast_mut::<BingleApiLocalImpl>()
            .expect("local api is BingleApiLocalImpl");
        concrete.set_alert_poster(recording.clone());
        concrete
            .import_keypair(TEST_MNEMONIC.to_string())
            .expect("import test keypair");
        concrete.seed_own_handle_for_tests("alice".to_string());
        concrete
            .add_message("alice".into(), vec!["bob".into()], 7, "hi".into(), None)
            .expect("add message");
    }

    // Drive an unreachable (transient) failure through the real JSI call site the app uses — the
    // recipient is offline and we keep retrying, so this is what nudges (bingle_notify #11).
    api.update_message_status(7, 0.0, Some("Retryable: offline".to_string()))
        .expect("update_message_status");

    let calls = recording.calls();
    assert_eq!(
        calls.len(),
        1,
        "an unreachable send via JSI must fire exactly one alert to the configured gateway"
    );
    let (gateway_url, req) = &calls[0];
    assert_eq!(gateway_url, "https://gw.example");
    assert_eq!(req.iss, "alice");
    assert_eq!(req.audience, "bob");
    assert!(!req.sig.is_empty(), "the alert must carry a signature");

    let _ = std::fs::remove_file(&tmp);
}

/// End-to-end: with `notify_on_giveup: false`, a give-up driven through the JSI trait fires no
/// alert even though a gateway URL is configured.
#[test]
#[cfg(not(target_os = "ios"))]
fn end_to_end_giveup_via_jsi_is_silent_when_disabled() {
    let tmp = project_tmp_file_path("bingle-jsi-test-e2e-notify-off", ".json");
    let mut config = config_with_local(&tmp.to_string_lossy());
    config.notify_gateway_url = Some("https://gw.example".to_string());
    config.notify_on_giveup = Some(false);
    let api = BingleJsiApiImpl::init(config).expect("init should succeed");

    let recording = Arc::new(RecordingPoster::default());
    {
        let local = api.local_api_for_tests().expect("local api present");
        let mut guard = local.lock().expect("local api lock");
        let concrete = guard
            .as_any_mut()
            .downcast_mut::<BingleApiLocalImpl>()
            .expect("local api is BingleApiLocalImpl");
        concrete.set_alert_poster(recording.clone());
        concrete
            .import_keypair(TEST_MNEMONIC.to_string())
            .expect("import test keypair");
        concrete.seed_own_handle_for_tests("alice".to_string());
        concrete
            .add_message("alice".into(), vec!["bob".into()], 7, "hi".into(), None)
            .expect("add message");
    }

    api.update_message_status(7, 1.0, Some("permanent".to_string()))
        .expect("update_message_status");

    assert!(
        recording.calls().is_empty(),
        "the nudge must stay silent when disabled"
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
fn import_keypair_adopts_account() {
    let api = init_with_local_helper();
    // Generate to obtain a valid mnemonic, then import it and confirm the same account is adopted.
    let generated = api
        .generate_keypair()
        .expect("generate_keypair should succeed");
    let imported = api
        .import_keypair(generated.passphrase.clone())
        .expect("import_keypair should succeed for a valid mnemonic");
    assert_eq!(imported.id, generated.id);
    assert_eq!(imported.passphrase, generated.passphrase);
}

#[test]
fn import_keypair_rejects_invalid_passphrase() {
    let api = init_with_local_helper();
    let result = api.import_keypair("not a valid mnemonic".to_string());
    assert!(result.is_err(), "invalid mnemonic must not import");
}

#[test]
fn sign_notify_envelope_matches_the_committed_parity_vector() {
    // Full-stack parity check: import the test-vector account, then sign the register envelope
    // through the JSI primitive. Ed25519 is deterministic, so the signature must be byte-for-byte
    // identical to the committed vector (bingle_notify/test/fixtures/verify-vector.json) — which is
    // exactly what the notify gateway's algosdk.verifyBytes accepts.
    let api = init_with_local_helper();
    let mnemonic = "square flat curtain negative three april hobby culture unit fit drip bronze cactus stage vault pluck captain nation pond pizza grief domain coin abstract path";
    let imported = api
        .import_keypair(mnemonic.to_string())
        .expect("import_keypair should succeed for the vector mnemonic");
    assert_eq!(
        imported.id,
        "JH2CPATVR25EJ4B2CQD7P5436MHXJOU2MZGIWPFPAB3JBNVX3CGQDLDENU"
    );

    let sig = api
        .sign_notify_envelope(
            "register".to_string(),
            "alice".to_string(),
            "".to_string(),
            "device-token-abc".to_string(),
            "sandbox".to_string(),
            "BBBBBBBBBBBBBBBBBBBBBB".to_string(),
            1893456000,
        )
        .expect("sign_notify_envelope should succeed with an imported keypair");
    assert_eq!(
        sig,
        "x/Xuu41OFj2kEqfos74PuFcrXrjfWW14ys8lYxyX+e9F7D8Q0iUwE+82ayIRPXFN4IfzwNDK+flzHRvkwxfwBA=="
    );
}

#[test]
fn sign_notify_envelope_fails_without_a_keypair() {
    // With no keypair set there is no signing key, so the primitive must error rather than
    // fabricate a signature.
    let api = init_with_local_helper();
    let result = api.sign_notify_envelope(
        "register".to_string(),
        "alice".to_string(),
        "".to_string(),
        "device-token-abc".to_string(),
        "sandbox".to_string(),
        "BBBBBBBBBBBBBBBBBBBBBB".to_string(),
        1893456000,
    );
    assert!(result.is_err(), "signing without a keypair must fail");
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
        notify_gateway_url: None,
        notify_on_giveup: None,
        notify_env: None,
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
    let api = init_with_local_helper(); // not started
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
fn start_ok_when_already_started() {
    let api = BingleJsiApiImpl::init(config_with_handle("testuser")).expect("init should succeed");
    assert!(api.is_started());
    let result = api.start();
    assert!(result.is_ok());
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

#[test]
fn transient_send_failures_keep_messages_pending() {
    use bingle_jsi::api::bingle_jsi_api_impl::is_permanent_send_failure;
    // Connectivity-related failures are NOT permanent -> keep the message pending for retry and
    // nudge the recipient (#31, bingle_notify #11).
    assert!(!is_permanent_send_failure("Retryable: relay timed out"));
    assert!(!is_permanent_send_failure("Send returned false"));
    assert!(!is_permanent_send_failure("no available relay"));
    assert!(!is_permanent_send_failure("Other: no relay for id"));
    assert!(!is_permanent_send_failure("host unreachable"));
    assert!(!is_permanent_send_failure("NoConnection"));
    // Infra blips are retryable too (previously mis-stopped by the old allowlist).
    assert!(!is_permanent_send_failure("DDB lookup failed"));
    assert!(!is_permanent_send_failure("Failed to send request"));
    // A genuine permanent failure (invalid recipient/message) -> terminally failed, no nudge.
    assert!(is_permanent_send_failure("recipient handle is invalid"));
    assert!(is_permanent_send_failure("account not opted in"));
}

#[test]
fn pending_failure_reason_is_human_readable() {
    use bingle_jsi::api::bingle_jsi_api_impl::pending_failure_reason;
    // A transient (connectivity) failure yields a concise, retry-aware message that does not
    // leak the raw internal error (issue #43).
    let transient =
        pending_failure_reason("Retryable: no ACK_COMPLETE received after 3 retries", true);
    assert_eq!(transient, "Recipient unreachable — will keep retrying");
    assert!(
        !transient.contains("ACK_COMPLETE"),
        "must not leak the raw error"
    );
    // A permanent failure surfaces the underlying error so it is actionable.
    let permanent = pending_failure_reason("recipient handle is invalid", false);
    assert!(permanent.contains("recipient handle is invalid"));
}
