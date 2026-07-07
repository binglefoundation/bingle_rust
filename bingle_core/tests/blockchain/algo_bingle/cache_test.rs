use algonaut::model::indexer::Account;
use bingle_core::blockchain::algo_bingle::{AccountsCache, AlgoBingle, QueryMode};
use bingle_core::blockchain::algo_ops::AlgoOps;
use std::sync::{Arc, Mutex};

#[test]
pub fn test_cache_only_mode() {
    let cache = Arc::new(Mutex::new(AccountsCache::default()));
    {
        let mut c = cache.lock().unwrap();
        // Create a dummy account
        let mut acct = Account::default();
        acct.address = "ADDR1".to_string();
        c.accounts.insert("ADDR1".to_string(), acct);
    }

    // Placeholder AlgoOps - using dummy address
    let ops = AlgoOps::new(
        None,
        Some("P577PSTDICQ6PQFBR5YMDMJ2YVK7LT5V4GOPNVDLCEDJIL7XGRWC5BRFWA".to_string()),
        None,
    );
    let ab = AlgoBingle::new_with_cache(ops, 123, 0, cache.clone());

    let mut count = 0;
    ab.indexer_query_opted_in_accounts_sync(123, QueryMode::CacheOnly, None, |acct| {
        count += 1;
        assert_eq!(acct.get("address").and_then(|a| a.as_str()), Some("ADDR1"));
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
        let mut acct = Account::default();
        acct.address = "ADDR1".to_string();
        c.accounts.insert("ADDR1".to_string(), acct);
        c.last_updated = now - 30; // updated 30s ago
        c.last_round = 100;
    }

    let ops = AlgoOps::new(
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
