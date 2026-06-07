// Grouped engine tests

#[path = "dtls_send_no_lazy_start.rs"]
pub mod dtls_send_no_lazy_start;

#[path = "engine_bingle_dtls_basic.rs"]
pub mod engine_bingle_dtls_basic;

#[path = "engine_connections.rs"]
pub mod engine_connections;

#[path = "turn_relay_integration.rs"]
pub mod turn_relay_integration;

#[path = "turn_relay_forwards_dtls.rs"]
pub mod turn_relay_forwards_dtls;

#[path = "ddb_upsert.rs"]
pub mod ddb_upsert;

#[path = "ddb_client_non_optional.rs"]
pub mod ddb_client_non_optional;

#[path = "seen_endpoints.rs"]
pub mod seen_endpoints;

#[path = "engine_bind_unspecified_ip.rs"]
pub mod engine_bind_unspecified_ip;

#[path = "engine_start.rs"]
pub mod engine_start;

#[path = "relay_roots_ddb.rs"]
pub mod relay_roots_ddb;

#[path = "start_with_addr_listening.rs"]
pub mod start_with_addr_listening;

#[path = "turn_no_public_addr.rs"]
pub mod turn_no_public_addr;

#[path = "set_public_addr.rs"]
pub mod set_public_addr;

#[path = "send_to_peer_guards.rs"]
pub mod send_to_peer_guards;

#[path = "stun_state_engine.rs"]
pub mod stun_state_engine;

#[path = "cipher_suite_injection.rs"]
pub mod cipher_suite_injection;

#[path = "send_status.rs"]
pub mod send_status;

#[path = "no_connection.rs"]
pub mod no_connection;

#[path = "no_connection_retry.rs"]
pub mod no_connection_retry;
