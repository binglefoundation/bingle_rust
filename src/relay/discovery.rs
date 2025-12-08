use std::sync::Arc;

use crate::relay::relay_finder::RootRelayInfo;

#[cfg(not(target_os = "ios"))]
use crate::blockchain::algo_ops::{AlgoChainConfig, AlgoOps};
#[cfg(not(target_os = "ios"))]
use crate::blockchain::algo_bingle::AlgoBingle;

/// Build a reusable discovery closure that queries the Algorand Indexer for
/// accounts with a static endpoint set in local state for the given app_id.
///
/// The returned closure maps entries to RootRelayInfo items and preserves the
/// previous behavior of panicking on failures or empty results, ensuring that
/// callers depending on discovery success maintain the same semantics.
#[cfg(not(target_os = "ios"))]
pub fn indexer_discover_closure(
    app_id: u64,
    cfg: Option<AlgoChainConfig>,
) -> Arc<dyn Fn() -> Vec<RootRelayInfo> + Send + Sync> {
    let ops = AlgoOps::new(None, None, cfg);
    let ab = AlgoBingle::new(ops, app_id, 0);
    Arc::new(move || {
        match ab.list_static_endpoints_via_indexer(app_id) {
            Ok(list) => {
                let mut out: Vec<RootRelayInfo> = Vec::new();
                for (id, ep) in list {
                    if let Some(addr) = AlgoBingle::parse_relay_ip(&ep) {
                        out.push(RootRelayInfo { id, address: addr });
                    }
                }
                if out.is_empty() {
                    panic!("[discovery] indexer discovery returned empty static endpoints list");
                } else {
                    out
                }
            }
            Err(e) => {
                panic!("[discovery] indexer discovery failed: {}", e);
            }
        }
    })
}
