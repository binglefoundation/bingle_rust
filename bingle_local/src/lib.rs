//! A thin local-state layer over `bingle_core` — keypair status, message queue, and contact store.
//!
//! `bingle_local` wraps the peer-to-peer messaging engine in `bingle_core` with the local state a
//! client app needs: the current Algorand keypair and its on-chain status, a queue of outbound
//! messages with retry tracking, and a contact store. The primary entry point is the
//! [`BingleLocalApi`](api::bingle_local_api::BingleLocalApi) trait, implemented by
//! [`BingleApiLocalImpl`](api::bingle_local_api_impl::BingleApiLocalImpl).
//!
//! Registration of a handle on-chain lives in [`api::registration`], and best-effort push
//! notifications to the notify gateway live in [`api::notify`].
#![warn(missing_docs)]

pub mod api;
pub mod module_version;
