// Grouped API tests as submodules of the single all-tests crate

#[path = "bingle_api_impl_integration.rs"]
pub mod bingle_api_impl_integration;

#[path = "bingle_api_impl_unit.rs"]
pub mod bingle_api_impl_unit;

#[path = "pki_generate_pki_from_ops.rs"]
pub mod pki_generate_pki_from_ops;

#[path = "endpoint_identify_integration.rs"]
pub mod endpoint_identify_integration;

#[path = "bingle_api_start_fail.rs"]
pub mod bingle_api_start_fail;

#[path = "bingle_api_relay_check_two_nodes.rs"]
pub mod bingle_api_relay_check_two_nodes;

#[path = "bingle_getters.rs"]
pub mod bingle_getters;
pub mod dtls_via_relay_integration;
pub mod bingle_api_relay_dtls;
#[path = "on_listening_handler.rs"]
pub mod on_listening_handler;

#[path = "network_endpoint_key.rs"]
pub mod network_endpoint_key;

#[path = "bingle_api_start_error.rs"]
pub mod bingle_api_start_error;

#[path = "engine_static_listening_sentinel.rs"]
pub mod engine_static_listening_sentinel;

#[path = "relay_finder_unit.rs"]
pub mod relay_finder_unit;

#[path = "turn_update_listener_relay.rs"]
pub mod turn_update_listener_relay;
