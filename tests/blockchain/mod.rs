// Grouped blockchain tests

#[path = "algo_ops_test.rs"]
pub mod algo_ops_test;

#[path = "asset_ops_test.rs"]
pub mod asset_ops_test;

#[path = "algo_change_reserve_unit.rs"]
pub mod algo_change_reserve_unit;

#[path = "dapp_app_integration.rs"]
pub mod dapp_app_integration;

#[path = "algo_ops_more_test.rs"]
pub mod algo_ops_more_test;

// Nested subdir tests
#[path = "algo_bingle/get_bingle_price.rs"]
pub mod get_bingle_price;

#[path = "algo_bingle_unit.rs"]
pub mod algo_bingle_unit;

#[path = "algo_ops_address_derivation_test.rs"]
pub mod algo_ops_address_derivation_test;

#[path = "algo_ops_reserve_helpers.rs"]
pub mod algo_ops_reserve_helpers;

