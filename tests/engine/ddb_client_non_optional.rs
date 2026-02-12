#![cfg(not(target_os = "ios"))]

use rust_comms::api::bingle_api::StartOptions;
use rust_comms::api::bingle_api_impl::BingleApiImpl;
use rust_comms::engine::Engine;

#[test]
fn engine_new_has_non_optional_ddb_client() {
    // Engine::new should construct a non-optional DDB client (NullDdbClient if no app_id)
    let eng = Engine::new(&StartOptions::default(), crate::util::mock_bingle_api::mock_api_weak());
    let cli = eng.ddb_client();
    // lookup should return an error (NullDdbClient), not panic or require Option unwraps
    let res = cli.lookup("SOME_ID");
    assert!(res.is_err(), "lookup should error on NullDdbClient, got: {:?}", res);
}

#[test]
fn bingle_api_impl_exposes_non_optional_engine_ddb_client() {
    // BingleApiImpl default (no app_id) should still expose a DDB client through engine helper
    let api = BingleApiImpl::new(&StartOptions::default());
    // The helper calls through to engine.ddb_client().lookup(); ensure it returns an Err, not panic
    let res = api.engine_ddb_lookup_for_tests("SOME_ID");
    assert!(res.is_err(), "engine_ddb_lookup_for_tests should return Err on missing app_id");
}
