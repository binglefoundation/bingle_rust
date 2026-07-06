// Integration tests: blockchain subset. Requires algokit localnet.
// Run with: cargo test --test integration_blockchain

#[macro_use]
#[path = "util_support/mod.rs"]
pub mod util;

pub mod setup_localnet;

// Granular blockchain user accounts (asset/app/user roles from spec/dapp_endpoints.md)
#[path = "integration/blockchain/blockchain_users.rs"]
pub mod blockchain_users;

#[path = "integration/blockchain/asset_configured_creator_localnet.rs"]
pub mod asset_configured_creator_localnet;

// Blockchain integration tests that hit localnet
#[path = "integration/blockchain/algo_bingle_buy_flow_localnet.rs"]
pub mod algo_bingle_buy_flow_localnet;
#[path = "integration/blockchain/algo_bingle_integration_localnet.rs"]
pub mod algo_bingle_integration_localnet;
#[path = "integration/blockchain/algo_bingle_register_opt_in_app_localnet.rs"]
pub mod algo_bingle_register_opt_in_app_localnet;
#[path = "integration/blockchain/algo_bingle_static_endpoint_integration.rs"]
pub mod algo_bingle_static_endpoint_integration;
#[path = "integration/blockchain/algo_ops_integration_localnet.rs"]
pub mod algo_ops_integration_localnet;
#[path = "integration/blockchain/asset_clawback_creator_localnet.rs"]
pub mod asset_clawback_creator_localnet;
#[path = "integration/blockchain/asset_manager_creator_localnet.rs"]
pub mod asset_manager_creator_localnet;
#[path = "integration/blockchain/deploy_app_and_asset_localnet.rs"]
pub mod deploy_app_and_asset_localnet;
#[path = "integration/blockchain/unique_handle.rs"]
pub mod unique_handle;
#[path = "integration/blockchain/migrate_local_localnet.rs"]
pub mod migrate_local_localnet;

// API integration tests that need localnet
#[path = "integration/api/connection_tests.rs"]
pub mod connection_tests;
#[path = "integration/api/relay_permission_test.rs"]
pub mod relay_permission_test;
#[path = "integration/api/send_message_to_id_integration.rs"]
pub mod send_message_to_id_integration;

// API tests needing localnet
#[path = "api/endpoint_identify_integration.rs"]
pub mod endpoint_identify_integration;

// Blockchain unit-ish tests gated by localnet
#[path = "blockchain/dapp_app_integration.rs"]
pub mod dapp_app_integration;
#[path = "blockchain/algo_bingle/register_collision.rs"]
pub mod register_collision;
#[path = "blockchain/algo_bingle/register_uniqueness.rs"]
pub mod register_uniqueness;

// Engine integration tests needing localnet
#[path = "integration/engine/handle_lookup_localnet.rs"]
pub mod handle_lookup_localnet;
#[path = "integration/engine/route_incoming_sender_auth_localnet.rs"]
pub mod route_incoming_sender_auth_localnet;

// Relay localnet test
#[path = "relay/relay_updater_localnet.rs"]
pub mod relay_updater_localnet;
