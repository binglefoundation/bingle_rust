// Single top-level integration test crate that groups all tests under tests/* as submodules
// This consolidates all integration tests into one crate to avoid IntelliJ module warnings
// and matches the repository guideline to keep tests in the tests tree.

// Existing grouped messages crate (kept as a submodule of this crate too)
#[path = "messages.rs"]
pub mod messages;

// Grouped directories
pub mod api;
pub mod blockchain;
pub mod dtls;
pub mod engine;
pub mod protocol;
pub mod packet_transport;
pub mod relay;
pub mod stun;
pub mod cli;
#[macro_use]
pub mod util;
pub mod ddb;
pub mod turn;
pub mod distributed_mutex;
pub mod integration;
pub mod setup_localnet;
pub mod module_version;
pub mod security;

#[path = "dev_arc4_selector.rs"]
pub mod dev_arc4_selector;
