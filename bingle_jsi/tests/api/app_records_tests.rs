//! The app's own records through bingle_jsi (issue #471): saved before the setter returns, read
//! back by a fresh instance, refused with `InvalidRequest` when invalid, and refused without a
//! local API. Storage and sealing are covered by bingle_local's tests.

use bingle_jsi::api::bingle_jsi_api::BingleJsiApi;
use bingle_jsi::api::bingle_jsi_api_impl::BingleJsiApiImpl;
use bingle_jsi::api::error::BingleJsiError;
use bingle_jsi::api::types::BingleJsiConfig;
use bingle_test::temp_file_helpers::project_tmp_file_path;

use crate::api::message_queue_integration_tests::mock_api;

fn config(path: &str) -> BingleJsiConfig {
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
        key_storage: None,
        encrypt_state: None,
        enable_global_block: None,
        ip_family: None,
    }
}

/// A state file path, removed afterwards.
struct StatePath(String);

impl StatePath {
    fn new() -> Self {
        Self(
            project_tmp_file_path("bingle-jsi-app-records", ".json")
                .to_string_lossy()
                .into_owned(),
        )
    }
}

impl Drop for StatePath {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
fn a_record_is_on_disk_when_the_setter_returns() {
    let path = StatePath::new();
    let api = BingleJsiApiImpl::init(config(&path.0)).expect("init");
    api.generate_keypair().expect("generate");

    api.set_app_record("read_state".into(), r#"{"since":1}"#.into())
        .expect("set");

    let fresh = BingleJsiApiImpl::init(config(&path.0)).expect("init again");
    assert_eq!(
        fresh
            .app_record("read_state".into())
            .expect("get")
            .as_deref(),
        Some(r#"{"since":1}"#)
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
fn removing_a_record_is_saved_too() {
    let path = StatePath::new();
    let api = BingleJsiApiImpl::init(config(&path.0)).expect("init");
    api.generate_keypair().expect("generate");
    api.set_app_record("report_log".into(), "[]".into())
        .expect("set");

    api.remove_app_record("report_log".into()).expect("remove");

    let fresh = BingleJsiApiImpl::init(config(&path.0)).expect("init again");
    assert_eq!(fresh.app_record("report_log".into()).expect("get"), None);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn an_invalid_name_or_value_is_an_invalid_request() {
    let path = StatePath::new();
    let api = BingleJsiApiImpl::init(config(&path.0)).expect("init");

    assert!(matches!(
        api.set_app_record("Read State".into(), "{}".into()),
        Err(BingleJsiError::InvalidRequest { .. })
    ));
    assert!(matches!(
        api.set_app_record("big".into(), "x".repeat(256 * 1024 + 1)),
        Err(BingleJsiError::InvalidRequest { .. })
    ));
    assert!(matches!(
        api.app_record(String::new()),
        Err(BingleJsiError::InvalidRequest { .. })
    ));
    assert!(matches!(
        api.remove_app_record("a/b".into()),
        Err(BingleJsiError::InvalidRequest { .. })
    ));
}

#[test]
fn records_need_the_local_api() {
    let api = BingleJsiApiImpl::init_for_tests(mock_api(vec![], 0), None);
    assert!(matches!(
        api.app_record("read_state".into()),
        Err(BingleJsiError::InvalidRequest { .. })
    ));
    assert!(matches!(
        api.set_app_record("read_state".into(), "{}".into()),
        Err(BingleJsiError::InvalidRequest { .. })
    ));
}
