use rust_comms::api::bingle_api::StartOptions;
use rust_comms::api::bingle_api_impl::BingleApiImpl;

#[test]
#[cfg(not(target_os = "ios"))]
fn test_dangerous_debug_allowed_in_debug_build() {
    // This test runs in debug mode (cfg(debug_assertions) is true)
    // so BingleApiImpl should NOT panic when dangerous_debug is true.
    let mut opts = StartOptions::new("test_handle".into());
    opts.dangerous_debug = true;
    
    // Should not panic
    let _api = BingleApiImpl::new(&opts);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn test_dangerous_debug_false_always_allowed() {
    let mut opts = StartOptions::new("test_handle".into());
    opts.dangerous_debug = false;
    
    // Should not panic regardless of build type
    let _api = BingleApiImpl::new(&opts);
}
