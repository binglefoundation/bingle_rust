// Common relay test helpers shared across integration test files.

use rust_comms::blockchain::algo_bingle::AlgoBingle;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

/// Wait until all expected `(account_id, SocketAddr)` pairs are visible via
/// the indexer for the given `app_id`.
///
/// Returns `true` if all entries became visible within `timeout`, `false`
/// otherwise.
pub fn wait_for_relays_visible(
    ab: &AlgoBingle,
    app_id: u64,
    expected: &[(String, SocketAddr)],
    timeout: Duration,
) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if let Ok(found) = ab.list_static_endpoints_via_indexer_sync(app_id) {
            let parsed: Vec<(String, SocketAddr)> = found
                .into_iter()
                .filter_map(|(id, advert_record_csv)| {
                    // Try parsing as compact AdvertRecord first
                    if let Some(advert_record) = rust_comms::ddb::AdvertRecord::deserialize_csv(
                        id.clone(),
                        &advert_record_csv,
                    ) {
                        if let Some(ep) = advert_record.endpoint {
                            use std::convert::TryFrom;
                            if let Ok(addr) = std::net::SocketAddr::try_from(ep) {
                                return Some((id, addr));
                            }
                        }
                    } else {
                        tracing::error!(
                            "Failed to parse AdvertRecord for id: {}, addr_str: {}",
                            id,
                            advert_record_csv
                        );
                    }
                    return None;
                })
                .collect();
            if parsed.len() == expected.len() {
                let mut all_match = true;
                for (exp_id, exp_addr) in expected {
                    if !parsed
                        .iter()
                        .any(|(fid, faddr)| fid == exp_id && faddr == exp_addr)
                    {
                        all_match = false;
                        tracing::info!(
                            "[Test] Relay {} not yet visible with address {} (found: {:?})",
                            exp_id,
                            exp_addr,
                            parsed
                        );
                        break;
                    }
                }
                if all_match {
                    tracing::info!(
                        "[Test] All {} relays visible with correct addresses via list_static_endpoints_via_indexer_sync after {:?}",
                        expected.len(),
                        start.elapsed()
                    );
                    return true;
                }
            }
        }
        std::thread::sleep(Duration::from_millis(1000));
    }
    false
}
