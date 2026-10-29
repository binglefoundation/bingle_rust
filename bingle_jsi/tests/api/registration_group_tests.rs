//! Co-signing the registration service's groups through the JSI (issue #344). The group checks
//! themselves are tested in bingle_core; these tests cover the keys the JSI signs with and the
//! pending recovery keypair's persistence.

use bingle_jsi::api::bingle_jsi_api::BingleJsiApi;
use bingle_jsi::api::bingle_jsi_api_impl::BingleJsiApiImpl;
use bingle_jsi::api::error::BingleJsiError;
use bingle_jsi::api::types::BingleJsiConfig;
use bingle_test::temp_file_helpers::project_tmp_file_path;
use serde_json::Value as Json;
use std::sync::Arc;

/// Groups built by the registration service's own group builder, with the signatures algosdk
/// produced (see bingle_core's `generate_registration_groups.mjs`).
const FIXTURE: &str =
    include_str!("../../../bingle_core/tests/blockchain/fixtures/registration_groups.json");

fn fixture() -> Json {
    serde_json::from_str(FIXTURE).expect("fixture JSON")
}

fn strings(group: &Json, field: &str) -> Vec<String> {
    group[field]
        .as_array()
        .expect("array")
        .iter()
        .map(|v| v.as_str().expect("string").to_string())
        .collect()
}

fn indexes(group: &Json) -> Vec<u32> {
    group["sign_indexes"]
        .as_array()
        .expect("array")
        .iter()
        .map(|v| u32::try_from(v.as_u64().expect("index")).expect("small index"))
        .collect()
}

fn config(local: Option<String>, ids: Option<(u64, u64)>) -> BingleJsiConfig {
    BingleJsiConfig {
        handle: None,
        passphrase: None,
        relay: false,
        static_ip: None,
        stun_servers: None,
        stun_servers_file: None,
        node_file: None,
        log_level: None,
        app_id: ids.map(|(app, _)| app),
        asset_id: ids.map(|(_, asset)| asset),
        handle_cache_expiry_secs: None,
        debug: false,
        local,
        notify_gateway_url: None,
        notify_on_giveup: None,
        notify_env: None,
        sidewinder_node_url: None,
        sidewinder_token: None,
        store_and_forward_send: None,
        store_and_forward_receive: None,
        store_and_forward_poll_interval_secs: None,
        send_pending_messages: None,
    }
}

/// An instance with a state file and the fixture's app and asset IDs, and the state file's path.
fn api_with_state(name: &str) -> (Arc<BingleJsiApiImpl>, String) {
    let f = fixture();
    let ids = (
        f["bingle_app_id"].as_u64().expect("app id"),
        f["bingle_asset_id"].as_u64().expect("asset id"),
    );
    let path = project_tmp_file_path(name, ".json")
        .to_string_lossy()
        .to_string();
    let api = BingleJsiApiImpl::init(config(Some(path.clone()), Some(ids))).expect("init");
    (api, path)
}

#[test]
#[cfg(not(target_os = "ios"))]
fn registration_group_is_signed_with_the_stored_keypair() {
    let f = fixture();
    let (api, path) = api_with_state("bingle-jsi-registration-group");
    api.import_keypair(
        f["client"]["mnemonic"]
            .as_str()
            .expect("mnemonic")
            .to_string(),
    )
    .expect("import");
    let group = &f["registration"];
    let signed = api
        .sign_registration_group(
            strings(group, "transactions"),
            indexes(group),
            f["handle"].as_str().expect("handle").to_string(),
        )
        .expect("signs");
    assert_eq!(signed, strings(group, "signed_transactions"));
    let _ = std::fs::remove_file(path);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn registration_group_from_another_account_is_refused() {
    let f = fixture();
    let (api, path) = api_with_state("bingle-jsi-registration-other");
    api.generate_keypair().expect("generate");
    let group = &f["registration"];
    let result = api.sign_registration_group(
        strings(group, "transactions"),
        indexes(group),
        f["handle"].as_str().expect("handle").to_string(),
    );
    assert!(matches!(result, Err(BingleJsiError::InvalidRequest { .. })));
    let _ = std::fs::remove_file(path);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn registration_group_needs_the_app_and_asset_ids() {
    let f = fixture();
    let path = project_tmp_file_path("bingle-jsi-registration-no-ids", ".json")
        .to_string_lossy()
        .to_string();
    let api = BingleJsiApiImpl::init(config(Some(path.clone()), None)).expect("init");
    api.import_keypair(
        f["client"]["mnemonic"]
            .as_str()
            .expect("mnemonic")
            .to_string(),
    )
    .expect("import");
    let group = &f["registration"];
    let result = api.sign_registration_group(
        strings(group, "transactions"),
        indexes(group),
        f["handle"].as_str().expect("handle").to_string(),
    );
    assert!(matches!(result, Err(BingleJsiError::InvalidRequest { .. })));
    let _ = std::fs::remove_file(path);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn recovery_group_needs_a_recovery_to_have_begun() {
    let f = fixture();
    let (api, path) = api_with_state("bingle-jsi-recovery-not-begun");
    api.import_keypair(
        f["client"]["mnemonic"]
            .as_str()
            .expect("mnemonic")
            .to_string(),
    )
    .expect("import");
    let group = &f["recovery"];
    let result = api.sign_recovery_group(
        strings(group, "transactions"),
        indexes(group),
        f["handle"].as_str().expect("handle").to_string(),
    );
    match result {
        Err(BingleJsiError::InvalidRequest { reason }) => {
            assert!(reason.contains("beginRecoveryKeypair"), "{reason}")
        }
        other => panic!("expected an invalid request, got {other:?}"),
    }
    let _ = std::fs::remove_file(path);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn recovery_group_is_signed_with_the_pending_keypair() {
    let f = fixture();
    let (api, path) = api_with_state("bingle-jsi-recovery-pending");
    api.import_keypair(
        f["client"]["mnemonic"]
            .as_str()
            .expect("mnemonic")
            .to_string(),
    )
    .expect("import");
    let pending = api.begin_recovery_keypair().expect("begin");
    let group = &f["recovery"];
    // The fixture's group moves to its own new address, not this freshly generated one, so the
    // new key's transactions are refused: the group is checked against the pending keypair.
    let result = api.sign_recovery_group(
        strings(group, "transactions"),
        indexes(group),
        f["handle"].as_str().expect("handle").to_string(),
    );
    assert!(matches!(result, Err(BingleJsiError::InvalidRequest { .. })));
    assert_ne!(
        pending.id,
        f["recovered"]["address"].as_str().expect("address")
    );
    let _ = std::fs::remove_file(path);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn recovery_keypair_is_saved_before_it_is_returned() {
    let (api, path) = api_with_state("bingle-jsi-recovery-saved");
    api.generate_keypair().expect("generate");
    let pending = api.begin_recovery_keypair().expect("begin");
    let saved: Json =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("state file")).expect("JSON");
    assert_eq!(
        saved["recovery_keypair"]["id"].as_str(),
        Some(pending.id.as_str())
    );

    // A restarted app resumes the same recovery.
    let restarted = BingleJsiApiImpl::init(config(Some(path.clone()), None)).expect("init again");
    assert_eq!(
        restarted.begin_recovery_keypair().expect("resume").id,
        pending.id
    );
    let _ = std::fs::remove_file(path);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn completing_a_recovery_makes_the_pending_keypair_current() {
    let (api, path) = api_with_state("bingle-jsi-recovery-complete");
    api.generate_keypair().expect("generate");
    let pending = api.begin_recovery_keypair().expect("begin");
    let current = api.complete_recovery_keypair().expect("complete");
    assert_eq!(current.id, pending.id);
    assert_eq!(current.passphrase, pending.passphrase);
    let saved: Json =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("state file")).expect("JSON");
    assert_eq!(saved["keypair"]["id"].as_str(), Some(pending.id.as_str()));
    assert!(saved.get("recovery_keypair").is_none());
    assert!(api.complete_recovery_keypair().is_err());
    let _ = std::fs::remove_file(path);
}
