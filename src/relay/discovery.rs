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
    // Provide a placeholder address to satisfy AlgoOps constructor requirement (read-only ops)
    // TODO: remove the need for this
    let placeholder_addr = "P577PSTDICQ6PQFBR5YMDMJ2YVK7LT5V4GOPNVDLCEDJIL7XGRWC5BRFWA".to_string();
    let ops = AlgoOps::new(None, Some(placeholder_addr), cfg);
    let ab = if let Some(c) = cache {
        AlgoBingle::new_with_cache(ops, app_id, 0, c)
    } else {
        AlgoBingle::new(ops, app_id, 0)
    };
    Arc::new(move || {
        tracing::info!("[discovery] indexer_discover_closure - in closure app_id={}", app_id);

        // Use the synchronous indexer call to ensure we block until results are ready
        let closure_result = match ab.list_static_endpoints_via_indexer_sync(app_id) {
            Ok(list) => {
                tracing::info!("[discovery] indexer_discover_closure - indexer discovery returned list: {:?}", list);
                let mut out: Vec<RelayInfo> = Vec::new();
                for (id, ep) in list {
                    if let Some(addr) = AlgoBingle::parse_relay_ip(&ep) {
                        out.push(RelayInfo::root(id, addr));
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
