// Grouped STUN tests

#[path = "endpoint_finder_tests.rs"]
pub mod endpoint_finder_tests;

#[path = "endpoint_finder_impl_send_handler.rs"]
pub mod endpoint_finder_impl_send_handler;

#[path = "stun_live_udp_mux.rs"]
pub mod stun_live_udp_mux;

#[path = "simple_server_consistent.rs"]
pub mod simple_server_consistent;

#[path = "simple_server_inconsistent.rs"]
pub mod simple_server_inconsistent;
