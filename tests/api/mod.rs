// Grouped API tests as submodules of the single all-tests crate

#[path = "bingle_api_impl_integration.rs"]
pub mod bingle_api_impl_integration;

#[path = "bingle_api_impl_unit.rs"]
pub mod bingle_api_impl_unit;

#[path = "pki_generate_pki_from_ops.rs"]
pub mod pki_generate_pki_from_ops;


#[path = "bingle_api_start_fail.rs"]
pub mod bingle_api_start_fail;

#[path = "bingle_api_relay_check_two_nodes.rs"]
pub mod bingle_api_relay_check_two_nodes;

#[path = "bingle_getters.rs"]
pub mod bingle_getters;

#[path = "bingle_api_impl/list_all_relays.rs"]
pub mod list_all_relays;

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

#[path = "bingle_api_handle_tests.rs"]
pub mod bingle_api_handle_tests;

#[path = "handle_cache_tests.rs"]
pub mod handle_cache_tests;

#[path = "handle_cache_reverse_lookup.rs"]
pub mod handle_cache_reverse_lookup;

#[path = "handle_reverse_lookup_blockchain_fallback.rs"]
pub mod handle_reverse_lookup_blockchain_fallback;

#[path = "send_over_dtls_guards.rs"]
pub mod send_over_dtls_guards;

#[path = "self_relay_detection.rs"]
pub mod self_relay_detection;

#[path = "ripple_message_unit.rs"]
pub mod ripple_message_unit;
