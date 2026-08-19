// Grouped blockchain tests
//
// Note: the pure AlgoOps unit tests (address derivation, sign-notify envelope,
// reserve helpers, account_balance, generate_keypair, asset_holding,
// set_asset_clawback, node errors, retry logic, change-reserve param checks)
// were moved to the standalone `algo_ops` crate's test suite. The
// bingle_test-backend AlgoOps trait tests (algo_ops_test, asset_ops_test,
// algo_ops_more_test) and all AlgoBingle tests remain here.

#[path = "algo_ops_test.rs"]
pub mod algo_ops_test;

#[path = "asset_ops_test.rs"]
pub mod asset_ops_test;

#[path = "algo_ops_more_test.rs"]
pub mod algo_ops_more_test;

// Nested subdir tests
#[path = "algo_bingle/get_bingle_price.rs"]
pub mod get_bingle_price;

#[path = "algo_bingle/handle_lookup.rs"]
pub mod handle_lookup;

#[path = "algo_bingle/cache_test.rs"]
pub mod cache_test;

#[path = "algo_bingle/set_allow_relay_test.rs"]
pub mod set_allow_relay_test;

#[path = "algo_bingle/check_allow_relay_test.rs"]
pub mod check_allow_relay_test;

#[path = "algo_bingle/required_funding.rs"]
pub mod required_funding;

#[path = "algo_bingle_unit.rs"]
pub mod algo_bingle_unit;

#[path = "static_endpoint_guard.rs"]
pub mod static_endpoint_guard;

#[path = "blockchain_ops_trait.rs"]
pub mod blockchain_ops_trait;
