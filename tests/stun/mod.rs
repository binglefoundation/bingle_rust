// Grouped STUN tests

#[path = "endpoint_finder_tests.rs"]
mod endpoint_finder_tests;

#[path = "endpoint_finder_impl_send_handler.rs"]
mod endpoint_finder_impl_send_handler;

#[path = "stun_live_udp_mux.rs"]
mod stun_live_udp_mux;

#[path = "simple_server_consistent.rs"]
mod simple_server_consistent;
