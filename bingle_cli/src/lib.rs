//! `bingle_cli` is the command-line client for Bingle: register a handle on Algorand, run a
//! node/relay, and send and receive messages.
//!
//! The supported interface of this crate is the **command line itself** — see `bingle_cli --help`
//! and the developer guide (`DEVELOPER.md`) in the repository. It is not a library API.
//!
//! The binary keeps its command dispatch and process-exiting logic in `main.rs`. This library
//! target exists only so the pure argument parsers can be unit tested from the test tree (per
//! CLAUDE.md, tests are kept out of the source files); its modules are internal and are hidden
//! from this reference.
#[doc(hidden)]
pub mod chat;
#[doc(hidden)]
pub mod chat_receive;
#[doc(hidden)]
pub mod chat_register;
#[doc(hidden)]
pub mod chat_repl;
#[doc(hidden)]
pub mod chat_send;
#[doc(hidden)]
pub mod chat_state;
