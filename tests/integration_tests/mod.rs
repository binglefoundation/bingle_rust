// All integration tests requiring external resources (localnet blockchain or internet).
// Referenced from integration_all.rs.

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
#[path = "../integration/blockchain/unique_handle.rs"]
pub mod unique_handle;
// API integration tests that need localnet
#[path = "../integration/api/send_message_to_id_integration.rs"]
pub mod send_message_to_id_integration;
#[path = "../integration/api/connection_tests.rs"]
pub mod connection_tests;
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
// Engine integration tests needing localnet
#[path = "../integration/engine/handle_lookup_localnet.rs"]
pub mod handle_lookup_localnet;
#[path = "../integration/engine/route_incoming_sender_auth_localnet.rs"]
pub mod route_incoming_sender_auth_localnet;
// Relay localnet test
#[path = "../relay/relay_updater_localnet.rs"]
pub mod relay_updater_localnet;
// Internet tests: live STUN over real internet UDP
#[path = "../stun/stun_live_udp_mux.rs"]
pub mod stun_live_udp_mux;
