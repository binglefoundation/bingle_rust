//! Re-export shim: `AlgoError` now lives in the standalone `algo_ops` crate.
//!
//! Kept at `bingle_core::blockchain::error` so existing consumers compile unchanged (issue #161).

pub use ::algo_ops::error::*;
