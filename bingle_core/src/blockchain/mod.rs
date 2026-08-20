//! Algorand integration: generic chain helpers ([`AlgoOps`](crate::blockchain::algo_ops::AlgoOps))
//! and Bingle's app/asset operations
//! ([`AlgoBingle`](crate::blockchain::algo_bingle::AlgoBingle)) such as handle registration and
//! lookup.

/// Bingle's Algorand application and asset operations,
/// [`AlgoBingle`](crate::blockchain::algo_bingle::AlgoBingle).
pub mod algo_bingle;
/// Generic Algorand operation helpers, [`AlgoOps`](crate::blockchain::algo_ops::AlgoOps).
pub mod algo_ops;
// Lower-level chain operations behind `AlgoOps`/`AlgoBingle`; not a supported API.
#[doc(hidden)]
pub mod blockchain_ops;
/// Algorand error types, including [`error::AlgoErrorKind`].
pub mod error;
