//! Re-export shim: the `BlockChainOps` / `AssetOps` traits now live in the standalone
//! `blockchain_ops` crate, and their Algorand impls (plus `AlgoOps::new_for_algorand`) live in
//! `algo_ops`.
//!
//! Kept at `bingle_core::blockchain::blockchain_ops` so existing consumers compile unchanged
//! (issue #161).

pub use ::blockchain_ops::{AssetOps, BlockChainOps};
