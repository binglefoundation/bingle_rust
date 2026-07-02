// Grouped blockchain tests

#[path = "algo_ops_test.rs"]
pub mod algo_ops_test;

#[path = "asset_ops_test.rs"]
pub mod asset_ops_test;

#[path = "algo_change_reserve_unit.rs"]
pub mod algo_change_reserve_unit;


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


#[path = "algo_bingle_unit.rs"]
pub mod algo_bingle_unit;

#[path = "algo_ops_address_derivation_test.rs"]
pub mod algo_ops_address_derivation_test;

#[path = "algo_ops_reserve_helpers.rs"]
pub mod algo_ops_reserve_helpers;

// New unit covering keypair generation helper
#[path = "generate_keypair.rs"]
pub mod generate_keypair;
#[path = "account_balance_test.rs"]
pub mod account_balance_test;

#[path = "asset_holding_test.rs"]
pub mod asset_holding_test;

#[path = "set_asset_clawback_test.rs"]
pub mod set_asset_clawback_test;

#[path = "test_node_errors.rs"]
pub mod test_node_errors;

#[path = "test_retry_logic.rs"]
pub mod test_retry_logic;

