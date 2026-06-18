use std::sync::{Arc, Mutex};
use rust_comms::blockchain::algo_bingle::{AlgoBingle, AccountsCache, QueryMode};
use rust_comms::blockchain::algo_ops::AlgoOps;
use algonaut::model::indexer::Account;

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
    let ops = AlgoOps::new(None, Some("P577PSTDICQ6PQFBR5YMDMJ2YVK7LT5V4GOPNVDLCEDJIL7XGRWC5BRFWA".to_string()), None);
    let ab = AlgoBingle::new_with_cache(ops, 123, 0, cache.clone());
    
    let mut count = 0;
    ab.indexer_query_opted_in_accounts_sync(123, QueryMode::CacheOnly, |acct| {
        count += 1;
        assert_eq!(acct.get("address").and_then(|a| a.as_str()), Some("ADDR1"));
        Ok(())
    }).unwrap();
    
    assert_eq!(count, 1, "Should have returned 1 account from cache");
}

#[test]
pub fn test_query_no_cache_legacy_behavior() {
    // Ensure it doesn't crash if cache is None, but it will try to hit network if called with Refresh/ForceFull
    let ops = AlgoOps::new(None, Some("P577PSTDICQ6PQFBR5YMDMJ2YVK7LT5V4GOPNVDLCEDJIL7XGRWC5BRFWA".to_string()), None);
    let _ab = AlgoBingle::new(ops, 123, 0);
    
    // We can't easily test ForceFull without network, but we can verify it fails as expected if network unreachable
    // For now, CacheOnly with None cache should just do nothing or error? 
    // Current implementation: if self.cache is None, it falls through to legacy full scan.
}
