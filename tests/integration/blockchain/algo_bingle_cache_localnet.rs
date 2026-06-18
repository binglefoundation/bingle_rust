use std::sync::{Arc, Mutex};
use rust_comms::blockchain::algo_bingle::{AlgoBingle, AccountsCache, QueryMode};
use rust_comms::blockchain::algo_ops::AlgoChainConfig;
use serial_test::serial;

use crate::setup_localnet;
use crate::util::test_util;
use test_util::{localnet_config, ops_from_mnemonic, ADDRESS_SPEND, PASSPHRASE_SPEND};
use crate::util::test_util::init_test_logging;

#[test]
#[serial]
pub fn test_indexer_cache_force_full_and_cache_only() {
    unsafe { std::env::set_var("BINGLE_ALGO_DEBUG", "true"); }
    init_test_logging();
    
    test_util::assert_localnet_available();
    let cfg: AlgoChainConfig = localnet_config();
    setup_localnet::ensure_localnet_accounts_funded(&cfg, &[ADDRESS_SPEND])
        .expect("localnet funded");
    
    let ops = ops_from_mnemonic(ADDRESS_SPEND, PASSPHRASE_SPEND, cfg.clone());
    let (app_id, asset_id) = test_util::deploy_bingle_app_and_asset(&ops, "BINGLE", 1000);
    
    // Opt-in sender to app so there's at least one account
    ops.opt_in_app(app_id).expect("opt-in app");
    
    let cache = Arc::new(Mutex::new(AccountsCache::default()));
    let ab = AlgoBingle::new_with_cache(ops.clone(), app_id, asset_id, cache.clone());
    
    // 1. ForceFull should populate cache (with retries for Indexer lag)
    let mut count = 0;
    for _ in 0..15 {
        count = 0;
        ab.indexer_query_opted_in_accounts_sync(app_id, QueryMode::ForceFull, |_| {
            count += 1;
            Ok(())
        }).expect("ForceFull success");
        if count >= 1 { break; }
        std::thread::sleep(std::time::Duration::from_millis(1000));
    }
    
    assert!(count >= 1, "Should have found at least the sender account");
    {
        let c = cache.lock().unwrap();
        assert_eq!(c.accounts.len(), count as usize, "Cache should match found account count");
        assert!(c.last_round > 0, "last_round should be set");
        assert!(c.accounts.contains_key(ADDRESS_SPEND), "Cache should contain the opted-in account");
    }
    
    let last_round_first = { cache.lock().unwrap().last_round };
    
    // 2. CacheOnly should use cache and NOT hit indexer (implicitly verified by it working without errors or by checking it returns same count)
    let mut count2 = 0;
    ab.indexer_query_opted_in_accounts_sync(app_id, QueryMode::CacheOnly, |_| {
        count2 += 1;
        Ok(())
    }).expect("CacheOnly success");
    
    assert_eq!(count2, count, "CacheOnly should return same number of accounts as ForceFull");
    assert_eq!(cache.lock().unwrap().last_round, last_round_first, "CacheOnly should not change last_round");
}

#[test]
#[serial]
pub fn test_indexer_cache_refresh_incremental() {
    unsafe {
        std::env::set_var("BINGLE_ALGO_DEBUG", "true");
    }
    init_test_logging();

    test_util::assert_localnet_available();
    let cfg: AlgoChainConfig = localnet_config();
    setup_localnet::ensure_localnet_accounts_funded(&cfg, &[ADDRESS_SPEND]).expect("localnet funded");

    let ops = ops_from_mnemonic(ADDRESS_SPEND, PASSPHRASE_SPEND, cfg.clone());
    let (app_id, asset_id) = test_util::deploy_bingle_app_and_asset(&ops, "BINGLE_REF", 1000);

    let cache = Arc::new(Mutex::new(AccountsCache::default()));
    let ab = AlgoBingle::new_with_cache(ops.clone(), app_id, asset_id, cache.clone());

    // 1. Initial Refresh should do a full scan (because last_round is 0)
    let mut count = 0;
    ab.indexer_query_opted_in_accounts_sync(app_id, QueryMode::Refresh, |_| {
        count += 1;
        Ok(())
    })
    .expect("Refresh success");
    assert_eq!(count, 0, "Initially no accounts opted in");

    let round_after_initial = { cache.lock().unwrap().last_round };
    assert!(round_after_initial > 0);

    // 2. Opt-in an account
    ops.opt_in_app(app_id).expect("opt-in app");

    // Wait for indexer to catch up with the transaction
    std::thread::sleep(std::time::Duration::from_millis(2000));

    // 3. Refresh should find the new account incrementally
    let mut count2 = 0;
    for _ in 0..15 {
        count2 = 0;
        ab.indexer_query_opted_in_accounts_sync(app_id, QueryMode::Refresh, |_| {
            count2 += 1;
            Ok(())
        })
        .expect("Refresh success");
        if count2 == 1 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1000));
    }

    assert_eq!(count2, 1, "Should have found 1 account after opt-in");
    {
        let c = cache.lock().unwrap();
        assert!(
            c.last_round > round_after_initial,
            "last_round should have advanced"
        );
        assert!(
            c.accounts.contains_key(ADDRESS_SPEND),
            "Cache should contain the newly opted-in account"
        );
    }

    // 4. Opt-out (Clear State)
    ops.clear_state_app(app_id).expect("clear state app");

    // Wait for indexer
    std::thread::sleep(std::time::Duration::from_millis(2000));

    // 5. Refresh should remove the account incrementally
    let mut count3 = 0;
    for _ in 0..15 {
        count3 = 0;
        ab.indexer_query_opted_in_accounts_sync(app_id, QueryMode::Refresh, |_| {
            count3 += 1;
            Ok(())
        })
        .expect("Refresh success");
        if count3 == 0 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1000));
    }

    assert_eq!(count3, 0, "Should have 0 accounts after opt-out");
    assert_eq!(
        cache.lock().unwrap().accounts.len(),
        0,
        "Cache should be empty"
    );
}
