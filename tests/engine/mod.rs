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
