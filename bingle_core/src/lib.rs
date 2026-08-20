//! `bingle_core` is the reference implementation of the Bingle peer-to-peer messaging engine and
//! its Algorand integration.
//!
//! Bingle is a decentralized, end-to-end encrypted messaging protocol with no central server:
//! key management and handle registration run on the Algorand blockchain, while messaging runs
//! over datagram transport layer security (DTLS) with STUN-based network address translation
//! (NAT) traversal and relay fallback.
//!
//! # Primary entry points
//!
//! - [`api::bingle_api::BingleApi`] — the messaging engine trait, implemented by
//!   [`api::bingle_api_impl::BingleApiImpl`]; configure a start with
//!   [`api::bingle_api::StartOptions`].
//! - [`AlgoOps`] — generic Algorand operation helpers.
//! - [`AlgoBingle`] — Bingle's application and asset operations, such as handle registration and
//!   lookup.
//!
//! # Example
//!
//! Construct the messaging engine from start options:
//!
//! ```no_run
//! use bingle_core::api::bingle_api::StartOptions;
//! use bingle_core::api::bingle_api_impl::BingleApiImpl;
//!
//! let options = StartOptions::new("alice".to_string());
//! let api = BingleApiImpl::new(&options);
//! ```
//!
//! The supported public interface is documented in `docs/api-surface.md`; modules marked
//! `#[doc(hidden)]` remain `pub` only for in-workspace use and are not part of that interface.
#![warn(missing_docs)]

/// Shared utilities: logging, version metadata, and configuration helpers.
#[macro_use]
pub mod util;
/// The Bingle messaging engine API.
pub mod api;
/// Algorand integration: [`AlgoOps`] and [`AlgoBingle`].
pub mod blockchain;
#[doc(hidden)]
pub mod ddb;
#[doc(hidden)]
pub mod engine;
/// Version metadata accessor for `bingle_core`.
pub mod module_version;
#[doc(hidden)]
pub mod relay;
#[doc(hidden)]
pub mod stun;
// Logging theme constants — internal catalog, not a supported API.
#[doc(hidden)]
pub mod themes;

// Modules reached only by bingle_core's own test tree (`tests/`) and nothing else in the
// workspace. They are `pub` only under the `test-hooks` feature — which this crate's own test
// build turns on via the self dev-dependency — and `pub(crate)` in a normal/release build and for
// any downstream crate, so they are physically absent from the external surface (issue #180). The
// test-only items inside them are gated the same way to keep release builds warning-free. The
// `test-hooks` arm carries `#[doc(hidden)]` so these modules never appear in the reference and are
// exempt from the crate-level `missing_docs` lint (docs.rs builds with default features anyway).
#[cfg(feature = "test-hooks")]
#[doc(hidden)]
pub mod distributed_mutex;
#[cfg(not(feature = "test-hooks"))]
pub(crate) mod distributed_mutex;
#[cfg(feature = "test-hooks")]
#[doc(hidden)]
pub mod dtls;
#[cfg(not(feature = "test-hooks"))]
pub(crate) mod dtls;
#[cfg(feature = "test-hooks")]
#[doc(hidden)]
pub mod messages;
#[cfg(not(feature = "test-hooks"))]
pub(crate) mod messages;
#[cfg(feature = "test-hooks")]
#[doc(hidden)]
pub mod packet_transport;
#[cfg(not(feature = "test-hooks"))]
pub(crate) mod packet_transport;
#[cfg(feature = "test-hooks")]
#[doc(hidden)]
pub mod protocol;
#[cfg(not(feature = "test-hooks"))]
pub(crate) mod protocol;
#[cfg(feature = "test-hooks")]
#[doc(hidden)]
pub mod turn;
#[cfg(not(feature = "test-hooks"))]
pub(crate) mod turn;

// Export the primary types so external users can import them directly from the crate root.
// (The canonical module paths are `blockchain::algo_ops` / `blockchain::algo_bingle`.)
pub use crate::blockchain::algo_bingle::AlgoBingle;
pub use algo_ops::AlgoOps;
