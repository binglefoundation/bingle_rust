// Grouped engine tests

#[path = "endpoint_identify.rs"]
mod endpoint_identify;

#[path = "dtls_send_no_lazy_start.rs"]
mod dtls_send_no_lazy_start;

#[path = "engine_bingle_dtls_basic.rs"]
mod engine_bingle_dtls_basic;

#[path = "engine_connections.rs"]
mod engine_connections;

#[path = "turn_relay_integration.rs"]
mod turn_relay_integration;

#[path = "turn_relay_forwards_dtls.rs"]
mod turn_relay_forwards_dtls;

#[path = "ddb_upsert.rs"]
mod ddb_upsert;

#[path = "ddb_client_non_optional.rs"]
mod ddb_client_non_optional;
