use rust_comms::api::bingle_api_impl::BingleApiImpl;
use rust_comms::api::bingle_api::StartOptions;

// Basic sanity: without engine issuer/app_id configured, list_all_relays should return an empty list.
#[cfg_attr(not(target_os = "ios"), test)]
pub fn list_all_relays_returns_empty_when_unconfigured() {
    let opts = StartOptions { handle: "tester".into(), ..Default::default() };
    let api = BingleApiImpl::new(&opts);
    let relays = api.list_all_relays(false);
    assert!(relays.is_empty());
}

// Ensure the method exists and can be called without panic with include_self toggled.
#[cfg_attr(not(target_os = "ios"), test)]
pub fn list_all_relays_include_self_toggle_no_panic() {
    let opts = StartOptions { handle: "tester".into(), ..Default::default() };
    let api = BingleApiImpl::new(&opts);
    let _ = api.list_all_relays(true);
    let _ = api.list_all_relays(false);
}
