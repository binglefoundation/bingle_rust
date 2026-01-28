use std::sync::Arc;

use crate::relay::relay_finder::RelayInfo;

#[cfg(not(target_os = "ios"))]
use crate::blockchain::algo_ops::{AlgoChainConfig, AlgoOps};
#[cfg(not(target_os = "ios"))]
use crate::blockchain::algo_bingle::AlgoBingle;

/// Build a reusable discovery closure that queries the Algorand Indexer for
/// accounts with a static endpoint set in local state for the given app_id.
///
/// The returned closure maps entries to RelayInfo items and preserves the
/// previous behavior of panicking on failures or empty results, ensuring that
/// callers depending on discovery success maintain the same semantics.
#[cfg(not(target_os = "ios"))]
pub fn indexer_discover_closure(
    app_id: u64,
    cfg: Option<AlgoChainConfig>,
) -> Arc<dyn Fn() -> Vec<RelayInfo> + Send + Sync> {
    // Provide a placeholder address to satisfy AlgoOps constructor requirement (read-only ops)
    let placeholder_addr = "P577PSTDICQ6PQFBR5YMDMJ2YVK7LT5V4GOPNVDLCEDJIL7XGRWC5BRFWA".to_string();
    let ops = AlgoOps::new(None, Some(placeholder_addr), cfg);
    let ab = AlgoBingle::new(ops, app_id, 0);
    Arc::new(move || {
        match ab.list_static_endpoints_via_indexer(app_id) {
            Ok(list) => {
                let mut out: Vec<RelayInfo> = Vec::new();
                for (id, ep) in list {
                    if let Some(addr) = AlgoBingle::parse_relay_ip(&ep) {
                        out.push(RelayInfo { id, address: addr });
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
