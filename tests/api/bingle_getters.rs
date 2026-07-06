use rust_comms::api::bingle_api::{BingleApi, StartOptions};
use rust_comms::api::bingle_api_impl::BingleApiImpl;
use rust_comms::engine::BingleAccessUnsafeForTests;

#[test]
#[cfg(not(target_os = "ios"))]
pub fn getters_default_none() {
    let api = BingleApiImpl::new(&StartOptions::new("".into()));
    // Before start(), no issuer is set so id is None
    assert!(
        api.access_unsafe_for_tests(|a: &mut BingleApiImpl| a.get_my_id())
            .is_none()
    );
    assert!(
        api.access_unsafe_for_tests(|a: &mut BingleApiImpl| a.get_user_id())
            .is_none()
    );
    assert!(
        api.access_unsafe_for_tests(|a: &mut BingleApiImpl| a.get_handle())
            .is_none()
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn getters_after_start() {
    let api = BingleApiImpl::new(&StartOptions::new("".into()));
    // Start with handle only (no passphrase) so id remains None; handle should be available
    let opts = StartOptions {
        handle: "tester".into(),
        ..StartOptions::new("".into())
    };
    // start may succeed with minimal options due to default paths; ignore result if Err
    let _ = api.access_unsafe_for_tests(|a: &mut BingleApiImpl| a.start(&opts));
    // handle must be present
    assert_eq!(
        api.access_unsafe_for_tests(|a: &mut BingleApiImpl| a.get_handle())
            .as_deref(),
        Some("tester")
    );
    // id is still optional without passphrase; just ensure alias doesn't panic
    let my_id = api.access_unsafe_for_tests(|a: &mut BingleApiImpl| a.get_my_id());
    assert_eq!(
        api.access_unsafe_for_tests(|a: &mut BingleApiImpl| a.get_user_id()),
        my_id
    );
}
