// Localnet-specific test modules (require algokit localnet running).
// Referenced from localnet_all.rs.

// Blockchain integration tests that hit localnet
#[path = "../integration/blockchain/algo_bingle_integration_localnet.rs"]
pub mod algo_bingle_integration_localnet;
#[path = "../integration/blockchain/algo_ops_integration_localnet.rs"]
pub mod algo_ops_integration_localnet;
#[path = "../integration/blockchain/algo_bingle_buy_flow_localnet.rs"]
pub mod algo_bingle_buy_flow_localnet;
#[path = "../integration/blockchain/algo_bingle_register_opt_in_app_localnet.rs"]
pub mod algo_bingle_register_opt_in_app_localnet;
#[path = "../integration/blockchain/algo_bingle_static_endpoint_integration.rs"]
pub mod algo_bingle_static_endpoint_integration;
#[path = "../integration/blockchain/asset_manager_creator_localnet.rs"]
pub mod asset_manager_creator_localnet;
#[path = "../integration/blockchain/asset_clawback_creator_localnet.rs"]
pub mod asset_clawback_creator_localnet;

// API integration tests that need localnet
#[path = "../integration/api/send_message_to_id_integration.rs"]
pub mod send_message_to_id_integration;

// API tests needing localnet
#[path = "../api/endpoint_identify_integration.rs"]
pub mod endpoint_identify_integration;

// Blockchain unit-ish tests gated by localnet
#[path = "../blockchain/algo_bingle/register_collision.rs"]
pub mod register_collision;
#[path = "../blockchain/algo_bingle/register_uniqueness.rs"]
pub mod register_uniqueness;
#[path = "../blockchain/dapp_app_integration.rs"]
pub mod dapp_app_integration;

// Relay localnet test
#[path = "../relay/relay_updater_localnet.rs"]
pub mod relay_updater_localnet;
