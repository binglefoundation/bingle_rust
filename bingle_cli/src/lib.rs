//! Library surface for the `bingle_cli` binary.
//!
//! The binary keeps its command dispatch and process-exiting logic in `main.rs`, but pure argument
//! parsers live here so they can be unit tested from the test tree (per CLAUDE.md, tests are kept
//! out of the source files).
pub mod chat;
pub mod chat_register;
pub mod chat_state;
