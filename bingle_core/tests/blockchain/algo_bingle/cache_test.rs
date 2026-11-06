use algo_ops::{AlgoOps, ScannedAccount};
use bingle_core::blockchain::algo_bingle::{AccountsCache, AlgoBingle, QueryMode};
use std::sync::{Arc, Mutex};

// Build a cache entry for `address` with no local state (the decode callback under test here only
// looks at the address). The cache stores each opted-in account as a `(address, ScannedAccount)`.
fn cached_account(address: &str) -> (String, ScannedAccount) {
    (
        address.to_string(),
        ScannedAccount {
            address: address.to_string(),
            local_state: Vec::new(),
        },
    )
}

#[test]
pub fn test_cache_only_mode() {
    let cache = Arc::new(Mutex::new(AccountsCache::default()));
    {
        let mut c = cache.lock().unwrap();
        c.entries.push(cached_account("ADDR1"));
    }

    // Placeholder AlgoOps - using dummy address
    let ops = AlgoOps::new_for_algorand(
        None,
        Some("P577PSTDICQ6PQFBR5YMDMJ2YVK7LT5V4GOPNVDLCEDJIL7XGRWC5BRFWA".to_string()),
        None,
    );
    let ab = AlgoBingle::new_with_cache(ops, 123, 0, cache.clone());

    let mut count = 0;
    ab.indexer_query_opted_in_accounts_sync(123, QueryMode::CacheOnly, None, |acct| {
        count += 1;
        assert_eq!(acct.address, "ADDR1");
        Ok(())
    })
    .unwrap();

    assert_eq!(count, 1, "Should have returned 1 account from cache");
}

#[test]
pub fn test_cache_lifetime_fallback() {
    let cache = Arc::new(Mutex::new(AccountsCache::default()));
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    {
        let mut c = cache.lock().unwrap();
        c.entries.push(cached_account("ADDR1"));
        c.last_updated = now - 30; // updated 30s ago
        c.last_round = 100;
    }

    let ops = AlgoOps::new_for_algorand(
        None,
        Some("P577PSTDICQ6PQFBR5YMDMJ2YVK7LT5V4GOPNVDLCEDJIL7XGRWC5BRFWA".to_string()),
        None,
    );
    let ab = AlgoBingle::new_with_cache(ops, 123, 0, cache.clone());

    // Lifetime is 60s, so 30s ago is "fresh"
    let mut count = 0;
    ab.indexer_query_opted_in_accounts_sync(123, QueryMode::Refresh, Some(60), |_| {
        count += 1;
        Ok(())
    })
    .unwrap();

    assert_eq!(
        count, 1,
        "Should have used cache instead of hitting network (which would fail anyway without real indexer)"
    );
}

// AlgoOps whose indexer refuses connections, so any network refresh fails fast.
fn unreachable_indexer_ops() -> AlgoOps {
    let config = algo_ops::AlgoChainConfig {
        client_api_url: "http://127.0.0.1".to_string(),
        client_api_port: 1,
        indexer_api_url: "http://127.0.0.1".to_string(),
        indexer_api_port: 1,
        app_id: Some(123),
        ..Default::default()
    };
    AlgoOps::new_for_algorand(
        None,
        Some("P577PSTDICQ6PQFBR5YMDMJ2YVK7LT5V4GOPNVDLCEDJIL7XGRWC5BRFWA".to_string()),
        Some(config),
    )
}

#[test]
pub fn test_failed_refresh_falls_back_to_the_cached_accounts() {
    // A cache filled by an earlier scan, now stale, so Refresh goes to the (unreachable) indexer.
    let cache = Arc::new(Mutex::new(AccountsCache::default()));
    {
        let mut c = cache.lock().unwrap();
        c.entries.push(cached_account("ADDR1"));
        c.entries.push(cached_account("ADDR2"));
        c.last_updated = 1;
        c.last_round = 100;
    }
    let ab = AlgoBingle::new_with_cache(unreachable_indexer_ops(), 123, 0, cache.clone());

    let mut seen = Vec::new();
    ab.indexer_query_opted_in_accounts_sync(123, QueryMode::Refresh, Some(30), |acct| {
        seen.push(acct.address.clone());
        Ok(())
    })
    .expect("a failed refresh with a filled cache uses the cached accounts (#380)");

    assert_eq!(seen, vec!["ADDR1".to_string(), "ADDR2".to_string()]);
}

#[test]
pub fn test_failed_refresh_with_a_never_filled_cache_is_an_error() {
    let cache = Arc::new(Mutex::new(AccountsCache::default()));
    let ab = AlgoBingle::new_with_cache(unreachable_indexer_ops(), 123, 0, cache.clone());

    let mut count = 0;
    let result = ab.indexer_query_opted_in_accounts_sync(123, QueryMode::Refresh, Some(30), |_| {
        count += 1;
        Ok(())
    });

    assert!(
        result.is_err(),
        "a first scan that fails must still be visible"
    );
    assert_eq!(count, 0);
}

#[test]
pub fn test_failed_refresh_without_a_shared_cache_is_an_error() {
    // No shared cache: an ephemeral one backs the scan, so there's nothing to fall back to.
    let ab = AlgoBingle::new(unreachable_indexer_ops(), 123, 0);
    let result = ab.indexer_query_opted_in_accounts_sync(123, QueryMode::Refresh, None, |_| Ok(()));
    assert!(result.is_err());
}
