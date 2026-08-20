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
pub mod distributed_mutex;
#[doc(hidden)]
pub mod dtls;
#[doc(hidden)]
pub mod engine;
#[doc(hidden)]
pub mod messages;
/// Version metadata accessor for `bingle_core`.
pub mod module_version;
#[doc(hidden)]
pub mod packet_transport;
#[doc(hidden)]
pub mod protocol;
#[doc(hidden)]
pub mod relay;
#[doc(hidden)]
pub mod stun;
// Logging theme constants — internal catalog, not a supported API.
#[doc(hidden)]
pub mod themes;
#[doc(hidden)]
pub mod turn;

// Backward-compatible module re-exports
/// Re-export of [`blockchain::algo_ops`] for importing directly from the crate root.
pub use blockchain::algo_ops;

/// Re-export of [`blockchain::algo_bingle`] for importing directly from the crate root.
pub use blockchain::algo_bingle;

// New: export primary types for external users so they can import directly from the crate root
pub use crate::blockchain::algo_ops::AlgoOps;

pub use crate::blockchain::algo_bingle::AlgoBingle;
