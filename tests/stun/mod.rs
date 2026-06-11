// Grouped STUN tests

#[path = "endpoint_finder_tests.rs"]
pub mod endpoint_finder_tests;

#[path = "endpoint_finder_impl_send_handler.rs"]
pub mod endpoint_finder_impl_send_handler;


#[path = "simple_server_consistent.rs"]
pub mod simple_server_consistent;

#[path = "simple_server_inconsistent.rs"]
pub mod simple_server_inconsistent;

#[path = "blocked_detection.rs"]
pub mod blocked_detection;

#[path = "reset_state_resumes_polling.rs"]
pub mod reset_state_resumes_polling;

#[path = "blocked_then_recovery.rs"]
pub mod blocked_then_recovery;
