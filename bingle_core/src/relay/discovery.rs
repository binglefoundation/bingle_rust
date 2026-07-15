use std::sync::Arc;

use crate::relay::relay_finder::RelayInfo;

use crate::blockchain::algo_bingle::{AccountsCache, AlgoBingle};
use crate::blockchain::algo_ops::{AlgoChainConfig, AlgoOps};
use std::sync::Mutex;

/// Build a reusable discovery closure that queries the Algorand Indexer for
/// accounts with a static endpoint set in local state for the given app_id.
///
/// The returned closure maps entries to RelayInfo items and preserves the
/// previous behavior of panicking on failures or empty results, ensuring that
/// callers depending on discovery success maintain the same semantics.
pub fn indexer_discover_closure(
    app_id: u64,
    cfg: Option<AlgoChainConfig>,
    cache: Option<Arc<Mutex<AccountsCache>>>,
) -> Arc<dyn Fn() -> Vec<RelayInfo> + Send + Sync> {
    let indexer = AlgoOps::new_indexer(cfg);
    let algo_bingle_indexer = if let Some(c) = cache {
        AlgoBingle::new_with_cache(indexer, app_id, 0, c)
    } else {
        AlgoBingle::new(indexer, app_id, 0)
    };

    Arc::new(move || {
        tracing::info!(
            "[discovery] indexer_discover_closure - in closure app_id={}",
            app_id
        );

        // Use the synchronous indexer call to ensure we block until results are ready
        let closure_result = match algo_bingle_indexer
            .list_static_endpoints_via_indexer_sync(app_id)
        {
            Ok(list) => {
                tracing::info!(
                    "[discovery] indexer_discover_closure - indexer discovery returned list: {:?}",
                    list
                );
                let mut out: Vec<RelayInfo> = Vec::new();
                for (id, ep) in list {
                    if let Some(record) = crate::ddb::AdvertRecord::deserialize_csv(id.clone(), &ep)
                    {
                        out.push(RelayInfo::root(record));
                    }
                }
                if out.is_empty() {
                    tracing::warn!(
                        "[discovery] indexer discovery returned empty static endpoints list"
                    );
                }
                out
            }
            Err(e) => {
                // Do NOT panic here: this closure runs synchronously on whatever thread drives
                // discovery — including the single-threaded pending-message drain loop
                // (send_message_to_id → list_root_relays → discover_roots). A panic there
                // unwinds and silently kills the drain thread, so no message is ever sent again
                // (even after connectivity returns). When the indexer is unreachable, return an
                // empty list: the relay finder then falls back to cached relays / DDB lookup, the
                // send fails transiently, and the message stays pending to retry once the indexer
                // recovers (bingle_rust #31/#43).
                tracing::warn!(
                    "[discovery] indexer discovery failed: {}; returning empty relay list",
                    e
                );
                Vec::new()
            }
        };

        tracing::info!(
            "[discovery] indexer_discover_closure - returning closure result: {:?}",
            closure_result
        );

        closure_result
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blockchain::algo_ops::AlgoChainConfig;

    /// Regression: the discovery closure used to `panic!` when the indexer query failed,
    /// which unwound and killed the pending-message drain thread (no sends ever again after
    /// an indexer outage). It must instead return an empty list so the caller retries.
    #[test]
    #[cfg(not(target_os = "ios"))]
    fn discover_closure_returns_empty_not_panic_on_indexer_error() {
        // Point the indexer at a dead local port so the query fails fast (connection refused).
        let cfg = AlgoChainConfig {
            client_api_url: "http://127.0.0.1".to_string(),
            client_api_port: 1,
            indexer_api_url: "http://127.0.0.1".to_string(),
            indexer_api_port: 1,
            token: None,
            token_key: None,
            app_id: Some(1),
            asset_id: None,
        };

        let discover = indexer_discover_closure(1, Some(cfg), None);
        // Must not panic; an unreachable indexer yields an empty relay list.
        let relays = discover();
        assert!(
            relays.is_empty(),
            "unreachable indexer should yield an empty relay list, got {:?}",
            relays
        );
    }
}
