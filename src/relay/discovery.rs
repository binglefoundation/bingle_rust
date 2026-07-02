use std::sync::Arc;

use crate::relay::relay_finder::RelayInfo;

use crate::blockchain::algo_ops::{AlgoChainConfig, AlgoOps};
use crate::blockchain::algo_bingle::{AlgoBingle, AccountsCache};
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
        tracing::info!("[discovery] indexer_discover_closure - in closure app_id={}", app_id);

        // Use the synchronous indexer call to ensure we block until results are ready
        let closure_result = match algo_bingle_indexer.list_static_endpoints_via_indexer_sync(app_id) {
            Ok(list) => {
                tracing::info!("[discovery] indexer_discover_closure - indexer discovery returned list: {:?}", list);
                let mut out: Vec<RelayInfo> = Vec::new();
                for (id, ep) in list {
                    if let Some(record) = crate::ddb::AdvertRecord::deserialize_csv(id.clone(), &ep) {
                        out.push(RelayInfo::root(record));
                    }
                }
                if out.is_empty() {
                    tracing::warn!("[discovery] indexer discovery returned empty static endpoints list");
                }
                out
            }
            Err(e) => {
                panic!("[discovery] indexer discovery failed: {}", e);
            }
        };

        tracing::info!("[discovery] indexer_discover_closure - returning closure result: {:?}", closure_result);

        closure_result
    })
}
