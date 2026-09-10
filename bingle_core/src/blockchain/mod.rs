//! Bingle's Algorand integration: app/asset operations
//! ([`AlgoBingle`](crate::blockchain::algo_bingle::AlgoBingle)) such as handle registration and
//! lookup, built on the generic [`AlgoOps`](algo_ops::AlgoOps) helpers from the external
//! `algo_ops` crate.

/// Bingle's Algorand application and asset operations,
/// [`AlgoBingle`](crate::blockchain::algo_bingle::AlgoBingle).
pub mod algo_bingle;

/// The shared Sidewinder node-endpoint record codec
/// ([`SidewinderEndpointRecord`](crate::blockchain::sidewinder_endpoint::SidewinderEndpointRecord)),
/// byte-compatible with Sidewinder `sw-membership`'s `EndpointRecord` (issue #237).
pub mod sidewinder_endpoint;
