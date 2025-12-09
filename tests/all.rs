// Single top-level integration test crate that groups all tests under tests/* as submodules
// This consolidates all integration tests into one crate to avoid IntelliJ module warnings
// and matches the repository guideline to keep tests in the tests tree.

// Existing grouped messages crate (kept as a submodule of this crate too)
#[path = "messages.rs"]
mod messages;

// Grouped directories
mod api;
mod blockchain;
mod dtls;
mod engine;
mod protocol;
mod relay;
mod stun;
mod cli;
mod util;
mod ddb;
