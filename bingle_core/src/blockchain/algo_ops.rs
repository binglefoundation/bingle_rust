//! Re-export shim: `AlgoOps` and friends now live in the standalone `algo_ops` crate.
//!
//! Kept at `bingle_core::blockchain::algo_ops` so existing consumers compile unchanged while the
//! crate is extracted (issue #161). The Algorand-specific constructor is `AlgoOps::new_for_algorand`.

pub use ::algo_ops::{
    AlgoChainConfig, AlgoOps, AppArg, KeyProvider, address_to_byte_key, byte_key_to_address,
};
