// Grouped blockchain tests

#[path = "algo_ops_test.rs"]
mod algo_ops_test;

#[path = "asset_ops_test.rs"]
mod asset_ops_test;

#[path = "algo_change_reserve_unit.rs"]
mod algo_change_reserve_unit;

#[path = "dapp_app_integration.rs"]
mod dapp_app_integration;

#[path = "algo_ops_more_test.rs"]
mod algo_ops_more_test;

// Nested subdir tests
#[path = "algo_bingle/get_bingle_price.rs"]
mod get_bingle_price;

#[path = "algo_bingle_unit.rs"]
mod algo_bingle_unit;

#[path = "algo_ops_address_derivation_test.rs"]
mod algo_ops_address_derivation_test;

#[path = "algo_ops_reserve_helpers.rs"]
mod algo_ops_reserve_helpers;

// Localnet-heavy tests are intentionally excluded from the consolidated crate to keep CI green:
// - algo_ops_integration_localnet.rs
// - algo_bingle_integration_localnet.rs
// - algo_bingle_buy_flow_localnet.rs
// - algo_bingle_register_opt_in_app_localnet.rs
// - algo_bingle_static_endpoint_integration.rs
// - asset_manager_creator_localnet.rs
// - asset_clawback_creator_localnet.rs
