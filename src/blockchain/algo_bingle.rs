use anyhow::{anyhow, bail, Result};
use sha2::{Digest, Sha512_256};
use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::blockchain::algo_ops::{AlgoOps, AppArg};

use algonaut::{
    Algod,
    core::{Address, AppId, AssetId, MicroAlgos, ToMsgPack},
    transaction::{
        account::Account,
        builder::CallApplication,
        transaction::Transaction,
        TransferAsset, Pay,
    },
    model::algod::SuggestedParams,
};

/// Lightweight helper around AlgoOps for calling the Bingle dApp.
///
/// This module purposefully focuses only on building and submitting the minimal
/// transaction groups required by the Bingle contract methods:
/// - buy_bingle: payment to app address for the configured price, plus an ASA transfer
///   of 1 unit to the caller, and the app call (with foreign asset set to the ASA id).
/// - sell_bingle: ASA transfer of `amount` from caller to app address, a payment to the
///   caller for price*amount (we conservatively self-pay, which satisfies on-chain checks),
///   and the app call.
/// - register: ASA transfer of `price` units from caller to app address, and the app call
///   with the handle argument.
///
/// Note: For simplicity and minimal changes, these helpers assume that all transactions
/// in the group are signed by the same account (self). This still satisfies the on-chain
/// validation logic in the provided contract.
#[derive(Debug, Clone)]
pub struct AlgoBingle {
    pub ops: AlgoOps,
    pub app_id: u64,
    pub asset_id: u64,
    pub cache: Option<Arc<Mutex<AccountsCache>>>,
}

#[derive(Debug, Default, Clone)]
pub struct AccountsCache {
    /// The last round number that was fully processed.
    pub last_round: u64,
    /// The time when the cache was last updated (Unix timestamp in seconds).
    pub last_updated: u64,
    /// Map of account address to the full account object.
    pub accounts: HashMap<String, algonaut::model::indexer::Account>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryMode {
    Refresh,     // Incremental if cache exists, else Full
    CacheOnly,   // Use cache without network
    ForceFull,   // Force a full scan
}

impl AlgoBingle {
    pub fn new(ops: AlgoOps, app_id: u64, asset_id: u64) -> Self {
        // Debug-print the AlgoOps configuration for visibility
        algo_log!("[AlgoBingle::new] ops.config={:?} app_id={} asset_id={}", ops.config, app_id, asset_id);
        Self { ops, app_id, asset_id, cache: None }
    }

    pub fn new_with_cache(ops: AlgoOps, app_id: u64, asset_id: u64, cache: Arc<Mutex<AccountsCache>>) -> Self {
        algo_log!("[AlgoBingle::new_with_cache] app_id={} asset_id={}", app_id, asset_id);
        Self { ops, app_id, asset_id, cache: Some(cache) }
    }

    /// Extract the BinglePrice value from already-decoded global state entries.
    /// Returns Some(price) if found and parsed as u64; otherwise None.
    pub fn extract_bingle_price(entries: &[(String, String)]) -> Option<u64> {
        entries
            .iter()
            .find(|(k, _)| k == "BinglePrice")
            .and_then(|(_, v)| v.parse::<u64>().ok())
    }

    /// Fetch the current Bingle price from the application's global state under key "BinglePrice".
    /// The price is stored as a uint in global state (microAlgos per Bingle unit).
    /// Errors if the key is not set or the application cannot be queried.
    pub fn get_bingle_price(&self, app_id: u64) -> Result<u64> {
        if app_id == 0 { bail!("app_id must be > 0"); }
        let client = self.ops.algod_client()?;
        let app_info = self.ops.algod_call(|| client.app(algonaut::core::AppId(app_id)))
            .map_err(|e| anyhow!("application_information failed: {e}"))?;
        let v = serde_json::to_value(&app_info)
            .map_err(|e| anyhow!("failed to serialize application info: {e}"))?;
        // Navigate to params.global-state (or global_state depending on casing)
        let gs_vec: Vec<serde_json::Value> = v
            .get("params")
            .and_then(|p| p.get("global-state").or_else(|| p.get("global_state")))
            .and_then(|x| x.as_array())
            .cloned()
            .unwrap_or_default();
        let entries = Self::decode_state_entries(&gs_vec);
        match Self::extract_bingle_price(&entries) {
            Some(p) => Ok(p),
            None => Err(anyhow!("BinglePrice not set in global state for app_id {app_id}")),
        }
    }

    fn decode_state_entries(entries: &[serde_json::Value]) -> Vec<(String, String)> {
        use base64::Engine as _;
        let mut kvs: Vec<(String, String)> = Vec::new();
        for entry in entries {
            let key_b64 = match entry.get("key").and_then(|x| x.as_str()) { Some(s) => s, None => continue };
            let key = match base64::engine::general_purpose::STANDARD.decode(key_b64) {
                Ok(bytes) => String::from_utf8(bytes).unwrap_or_else(|_| key_b64.to_string()),
                Err(_) => key_b64.to_string(),
            };
            let val_obj = match entry.get("value").and_then(|x| x.as_object()) { Some(o) => o, None => continue };
            let vtype = val_obj.get("type").and_then(|x| x.as_u64()).unwrap_or(0);
            let val = if vtype == 1 {
                // bytes can be an array of numbers or a base64 string
                let bytes_opt = if let Some(arr) = val_obj.get("bytes").and_then(|x| x.as_array()) {
                    let mut buf: Vec<u8> = Vec::with_capacity(arr.len());
                    for n in arr {
                        if let Some(u) = n.as_u64() { buf.push((u & 0xFF) as u8); }
                        else if let Some(i) = n.as_i64() { if i >= 0 { buf.push(((i as u64) & 0xFF) as u8); } }
                    }
                    Some(buf)
                } else if let Some(b64) = val_obj.get("bytes").and_then(|x| x.as_str()) {
                    match base64::engine::general_purpose::STANDARD.decode(b64) { Ok(b) => Some(b), Err(_) => None }
                } else { None };
                if let Some(bytes) = bytes_opt {
                    match String::from_utf8(bytes.clone()) {
                        Ok(s) => s,
                        Err(_) => {
                            let mut hex = String::from("0x");
                            for byte in bytes { hex.push_str(&format!("{:02x}", byte)); }
                            hex
                        }
                    }
                } else { String::new() }
            } else {
                val_obj.get("uint").and_then(|x| x.as_u64()).map(|u| u.to_string()).unwrap_or_else(|| "0".to_string())
            };
            kvs.push((key, val));
        }
        kvs
    }

    /// Grant or revoke permission for a specific address to register a static endpoint.
    ///
    /// Calls set_allow_static on-chain for the provided target address. The caller must be the
    /// app creator (enforced by the contract). The target address must be opted-in to the app.
    /// Returns the submitted transaction id on success.
    pub fn set_allow_static(&self, app_id: u64, target_address: &str, allow: bool) -> Result<String> {
        if app_id == 0 { bail!("app_id must be > 0"); }
        // Use AlgoOps::call_app and pass target address as an ARC-4 address argument
        let pk = crate::blockchain::algo_ops::address_to_byte_key(target_address)
            .map_err(|e| anyhow!("invalid target address: {e}"))?;
        let (txid, _logs) = self
            .ops
            .call_app(
                app_id,
                None,
                Some("set_allow_static(address,uint64)void"),
                &[crate::blockchain::algo_ops::AppArg::Bytes(pk.to_vec()), crate::blockchain::algo_ops::AppArg::Uint(if allow { 1 } else { 0 })],
            )?;
        Ok(txid)
    }

    /// Grant or revoke permission for a specific address to relay.
    ///
    /// Calls set_allow_relay on-chain for the provided target address. The caller must be the
    /// app creator (enforced by the contract). The target address must be opted-in to the app.
    /// Returns the submitted transaction id on success.
    pub fn set_allow_relay(&self, app_id: u64, target_address: &str, allow: bool) -> Result<String> {
        if app_id == 0 { bail!("app_id must be > 0"); }
        // Use AlgoOps::call_app and pass target address as an ARC-4 address argument
        let pk = crate::blockchain::algo_ops::address_to_byte_key(target_address)
            .map_err(|e| anyhow!("invalid target address: {e}"))?;
        let (txid, _logs) = self
            .ops
            .call_app(
                app_id,
                None,
                Some("set_allow_relay(address,uint64)void"),
                &[crate::blockchain::algo_ops::AppArg::Bytes(pk.to_vec()), crate::blockchain::algo_ops::AppArg::Uint(if allow { 1 } else { 0 })],
            )?;
        Ok(txid)
    }

    /// Call register_endpoint(string)void for the caller.
    /// Passing an empty string clears the local state key.
    /// Returns the submitted transaction id on success.
    // endpoint_advert_record_compact is now a compact AdvertRecord
    pub fn register_endpoint(&self, app_id: u64, endpoint_advert_record_compact: &str) -> Result<String> {
        if app_id == 0 { bail!("app_id must be > 0"); }
        // Build ARC-4 arguments manually to ensure correct string encoding (2-byte length prefix)
        let client = self.ops.algod_client()?;
        let params = self.params(&client)?;
        let (account, sender) = self.sender_account()?;
        let mut app_args: Vec<Vec<u8>> = Vec::new();
        app_args.push(AlgoOps::arc4_selector("register_endpoint(string)void").to_vec());
        let ep_bytes = endpoint_advert_record_compact.as_bytes();
        if ep_bytes.len() > u16::MAX as usize { bail!("endpoint too long"); }
        let mut arg = Vec::with_capacity(2 + ep_bytes.len());
        arg.extend_from_slice(&(ep_bytes.len() as u16).to_be_bytes());
        arg.extend_from_slice(ep_bytes);
        app_args.push(arg);
        let tx = CallApplication::new(sender, AppId(app_id))
            .app_arguments(app_args)
            .note(AlgoOps::unique_note())
            .build(&params)
            .map_err(|e| anyhow!("build app call: {e}"))?;
        algo_log!("register_endpoint tx: {:?}", tx);
        let signed = account
            .sign(tx)
            .map_err(|e| anyhow!("sign app call: {e}"))?
            .to_msg_pack()
            .map_err(|e| anyhow!("encode signed app call: {e}"))?;
        self.broadcast_group(&client, vec![signed])
    }

    /// Parse a RelayIP string into a SocketAddr. Accepts forms like "host:port" or "ip:port".
    /// Returns None if parsing fails.
    pub fn parse_relay_ip(ip: &str) -> Option<std::net::SocketAddr> {
        ip.parse::<std::net::SocketAddr>().ok()
    }

    /// Use the Indexer API to list accounts that have a non-empty "static_endpoint" in local state for the given app_id.
    /// Returns Vec of (account_address, static_endpoint_value).
    pub fn list_static_endpoints_via_indexer(&self, app_id: u64) -> Result<Vec<(String, String)>> {
        algo_log!("[AlgoBingle::list_static_endpoints_via_indexer] app_id={}", app_id);
        if tokio::runtime::Handle::try_current().is_ok() {
            algo_log!("[AlgoBingle::list_static_endpoints_via_indexer] running in tokio runtime, spawning thread");
            let thread_result = std::thread::scope(|s| {
                s.spawn(|| self.list_static_endpoints_via_indexer_sync(app_id))
                    .join()
                    .unwrap_or_else(|_| Err(anyhow!("indexer thread panicked")))
            });
            algo_log!("[AlgoBingle::list_static_endpoints_via_indexer] thread result={:?}", thread_result);
            return thread_result;
        }

        algo_log!("[AlgoBingle::list_static_endpoints_via_indexer] not running in tokio runtime, calling sync");
        let sync_result = self.list_static_endpoints_via_indexer_sync(app_id);
        algo_log!("[AlgoBingle::list_static_endpoints_via_indexer] sync result={:?}", sync_result);
        sync_result
    }

    pub fn list_static_endpoints_via_indexer_sync(&self, app_id: u64) -> Result<Vec<(String, String)>> {
        // Debug: print the current ops.config for visibility in discovery
        algo_log!("[AlgoBingle::list_static_endpoints_via_indexer_sync] ops.config={:?}", self.ops.config);
        let mut results: Vec<(String, String)> = Vec::new();
        let indexer_query_result = self.indexer_query_opted_in_accounts_sync(app_id, QueryMode::Refresh, Some(30), |acct| {
            let addr = acct.get("address").and_then(|x| x.as_str()).unwrap_or("").to_string();
            tracing::debug!("[AlgoBingle::list_static_endpoints_via_indexer_sync] processing account: address: {:?}", addr);
            if let Some(als) = acct.get("apps-local-state").or_else(|| acct.get("apps_local_state")).and_then(|x| x.as_array()) {
                for st in als {
                    let id = st.get("id").and_then(|x| x.as_u64());
                    // tracing::debug!("[AlgoBingle::list_static_endpoints_via_indexer_sync] processing app-local-state: id: {:?}", id);
                    if id == Some(app_id) {
                        let keyvals = st.get("key-value").or_else(|| st.get("key_value")).and_then(|x| x.as_array()).cloned().unwrap_or_default();
                        let kvs = Self::decode_state_entries(&keyvals);
                        tracing::debug!("[AlgoBingle::list_static_endpoints_via_indexer_sync] processing decoded: kvs: {:?}", kvs);
                        let ep = kvs.iter().find(|(k, _)| k == "static_endpoint").map(|(_, v)| v.as_str()).unwrap_or("");
                        let ep_x = kvs.iter().find(|(k, _)| k == "static_endpoint_x").map(|(_, v)| v.as_str()).unwrap_or("");
                        let full_val = format!("{}{}", ep, ep_x);
                        if !full_val.is_empty() {
                            results.push((addr.clone(), full_val));
                        }
                    }
                }
            }
            Ok(())
        });

        if let Err(e) = indexer_query_result {
            tracing::error!("[AlgoBingle::list_static_endpoints_via_indexer_sync] indexer_query_opted_in_accounts_sync failed: {}", e);
            Err(e)
        }
        else {
            algo_log!("[AlgoBingle::list_static_endpoints_via_indexer_sync] results={:?}", results);
            Ok(results)
        }
    }

    fn is_opted_in(acct: &algonaut::model::indexer::Account, app_id: u64) -> bool {
        if acct.deleted.unwrap_or(false) {
            return false;
        }
        if let Some(states) = &acct.apps_local_state {
            states
                .iter()
                .any(|s| s.id == app_id && !s.deleted.unwrap_or(false))
        } else {
            false
        }
    }

    fn collect_addresses(
        txn: &algonaut::model::indexer::Transaction,
        addresses: &mut HashSet<String>,
    ) {
        addresses.insert(txn.sender.clone());
        if let Some(app_txn) = &txn.application_transaction {
            if let Some(accounts) = &app_txn.accounts {
                for addr in accounts {
                    addresses.insert(addr.clone());
                }
            }
        }
        if let Some(inner_txns) = &txn.inner_txns {
            for inner in inner_txns {
                Self::collect_addresses(inner, addresses);
            }
        }
    }

    /// Helper to query all accounts opted into the given app_id via the algonaut Indexer.
    pub fn indexer_query_opted_in_accounts_sync<F>(
        &self,
        app_id: u64,
        mode: QueryMode,
        cache_lifetime_secs: Option<u64>,
        mut f: F,
    ) -> Result<()>
    where
        F: FnMut(&serde_json::Value) -> Result<()>,
    {
        algo_log!(
            "[AlgoBingle][indexer_query_opted_in_accounts_sync] app_id={} mode={:?} cache_lifetime={:?}",
            app_id,
            mode,
            cache_lifetime_secs
        );
        if app_id == 0 {
            bail!("app_id must be > 0");
        }

        if let Some(cache_lock) = &self.cache {
            let mut cache = cache_lock.lock().unwrap();

            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();

            let mut effective_mode = mode;
            if mode == QueryMode::Refresh {
                if let Some(lifetime) = cache_lifetime_secs {
                    if now < cache.last_updated + lifetime {
                        algo_log!(
                            "[AlgoBingle][indexer_query_opted_in_accounts_sync] Refresh: cache is fresh ({} < {} + {}), falling back to CacheOnly",
                            now,
                            cache.last_updated,
                            lifetime
                        );
                        effective_mode = QueryMode::CacheOnly;
                    }
                }
            }

            match effective_mode {
                QueryMode::CacheOnly => {
                    algo_log!(
                        "[AlgoBingle][indexer_query_opted_in_accounts_sync] CacheOnly: using {} cached accounts",
                        cache.accounts.len()
                    );
                }
                QueryMode::ForceFull | QueryMode::Refresh => {
                    let indexer = self.ops.indexer_client()?;
                    if effective_mode == QueryMode::ForceFull || cache.last_round == 0 {
                        algo_log!(
                            "[AlgoBingle][indexer_query_opted_in_accounts_sync] {:?}: performing full scan",
                            effective_mode
                        );
                        cache.accounts.clear();
                        let mut next: Option<String> = None;
                        let mut current_round;
                        loop {
                            let next_ref = next.as_deref();
                            let response = self.ops.algod_call(|| {
                                indexer.search_for_accounts(
                                    None,
                                    Some(1000),
                                    next_ref,
                                    None,
                                    None,
                                    None,
                                    None,
                                    None,
                                    None,
                                    Some(algonaut::core::AppId(app_id)),
                                )
                            })
                            .map_err(|e| anyhow!("indexer request failed: {e}"))?;

                            current_round = response.current_round;
                            for acct in response.accounts {
                                cache.accounts.insert(acct.address.clone(), acct);
                            }
                            next = response.next_token;
                            if next.is_none() {
                                break;
                            }
                        }
                        cache.last_round = current_round;
                    } else {
                        algo_log!(
                            "[AlgoBingle][indexer_query_opted_in_accounts_sync] Refresh: incremental since round {}",
                            cache.last_round
                        );
                        let mut next: Option<String> = None;
                        let mut addresses_to_refresh = HashSet::new();
                        let min_round = cache.last_round + 1;
                        let mut current_round;

                        loop {
                            let next_ref = next.as_deref();
                            let response = self.ops.algod_call(|| {
                                indexer.search_for_transactions(
                                    None,
                                    next_ref,
                                    None,
                                    None,
                                    None,
                                    None,
                                    None,
                                    Some(min_round),
                                    None,
                                    None,
                                    None,
                                    None,
                                    None,
                                    None,
                                    None,
                                    None,
                                    None,
                                    None,
                                    Some(AppId(app_id)),
                                )
                            })
                            .map_err(|e| anyhow!("indexer search_for_transactions failed: {e}"))?;

                            current_round = response.current_round;
                            for txn in response.transactions {
                                Self::collect_addresses(&txn, &mut addresses_to_refresh);
                            }
                            next = response.next_token;
                            if next.is_none() {
                                break;
                            }
                        }

                        algo_log!(
                            "[AlgoBingle] Refresh: found {} unique addresses to check",
                            addresses_to_refresh.len()
                        );
                        for addr in addresses_to_refresh {
                            let address = Address::from_str(&addr)
                                .map_err(|e| anyhow!("invalid address {addr}: {e}"))?;
                            let acct_response = self
                                .ops
                                .algod_call(|| indexer.lookup_account_by_id(&address, None, None, None))
                                .map_err(|e| {
                                    anyhow!("indexer lookup_account_by_id failed for {addr}: {e}")
                                })?;
                            let acct = *acct_response.account;
                            if Self::is_opted_in(&acct, app_id) {
                                cache.accounts.insert(addr, acct);
                            } else {
                                cache.accounts.remove(&addr);
                            }
                        }
                        cache.last_round = current_round;
                    }
                    cache.last_updated = now;
                    algo_log!(
                        "[AlgoBingle][indexer_query_opted_in_accounts_sync] done updating cache. size={} last_round={}. Iterating over accounts.",
                        cache.accounts.len(),
                        cache.last_round
                    );
                }
            }

            for acct in cache.accounts.values() {
                let v = serde_json::to_value(acct)
                    .map_err(|e| anyhow!("failed to serialize account from cache: {e}"))?;
                f(&v)?;
            }
            return Ok(());
        }

        // Legacy behavior without cache
        algo_log!("[AlgoBingle][indexer_query_opted_in_accounts_sync] no cache available, performing full scan");
        let indexer = self.ops.indexer_client()?;
        let mut next: Option<String> = None;
        loop {
            let next_ref = next.as_deref();
            let response = self.ops.algod_call(|| indexer.search_for_accounts(
                None,
                Some(1000),
                next_ref,
                None,
                None,
                None,
                None,
                None,
                None,
                Some(algonaut::core::AppId(app_id)),
            )).map_err(|e| anyhow!("indexer request failed: {e}"))?;
            
            for acct in response.accounts {
                let v = serde_json::to_value(&acct)
                    .map_err(|e| anyhow!("failed to serialize account: {e}"))?;
                f(&v)?;
            }
            next = response.next_token;
            if next.is_none() { break; }
        }

        algo_log!("[AlgoBingle][indexer_query_opted_in_accounts_sync] done");
        Ok(())
    }

    /// Lookup a Bingle handle on-chain using the Indexer.
    /// Returns the address of the oldest account that has this handle registered in its local state.
    pub fn handle_lookup(&self, handle: &str) -> Result<Option<String>> {
        let app_id = if self.app_id != 0 { self.app_id } else {
            self.ops.config.app_id.ok_or_else(|| anyhow!("app_id not configured in AlgoOps config"))?
        };
        if tokio::runtime::Handle::try_current().is_ok() {
            return std::thread::scope(|s| {
                s.spawn(|| self.handle_lookup_sync(app_id, handle))
                    .join()
                    .unwrap_or_else(|_| Err(anyhow!("indexer thread panicked")))
            });
        }
        self.handle_lookup_sync(app_id, handle)
    }

    fn handle_lookup_sync(&self, app_id: u64, handle: &str) -> Result<Option<String>> {
        let mut matches: Vec<(String, u64)> = Vec::new();
        self.indexer_query_opted_in_accounts_sync(app_id, QueryMode::Refresh, Some(30), |acct| {
            Self::extract_handle_match(acct, app_id, handle, &mut matches);
            Ok(())
        })?;
        algo_log!("[handle_lookup_sync] handle={} matches={:?}", handle, matches);
        Ok(Self::pick_oldest_match(matches))
    }

    /// Normalises a handle for comparison: lowercase and alphanumeric characters only.
    pub fn normalize_handle(handle: &str) -> String {
        handle
            .chars()
            .filter(|c| c.is_alphanumeric())
            .map(|c| c.to_ascii_lowercase())
            .collect()
    }

    /// Extracted logic to find a handle match in an account's local state and append to matches list.
    pub fn extract_handle_match(acct: &serde_json::Value, app_id: u64, handle: &str, matches: &mut Vec<(String, u64)>) {
        let addr = acct.get("address").and_then(|x| x.as_str()).unwrap_or("").to_string();
        let normalised_handle = Self::normalize_handle(handle);
        // Find local state for this app id
        if let Some(als) = acct.get("apps-local-state").or_else(|| acct.get("apps_local_state")).and_then(|x| x.as_array()) {
            for st in als {
                let id = st.get("id").and_then(|x| x.as_u64());
                if id == Some(app_id) {
                    let keyvals = st.get("key-value").or_else(|| st.get("key_value")).and_then(|x| x.as_array()).cloned().unwrap_or_default();
                    let kvs = Self::decode_state_entries(&keyvals);
                    algo_log!("[extract_handle_match] address={} decoded_state={:?}", addr, kvs);
                    if let Some((_, h)) = kvs.iter().find(|(k, _)| k == "Handle") {
                        if Self::normalize_handle(h) == normalised_handle {
                            let time = kvs.iter().find(|(k, _)| k == "HandleTime")
                                .and_then(|(_, v)| v.parse::<u64>().ok())
                                .unwrap_or(0);
                            matches.push((addr.clone(), time));
                        }
                    }
                }
            }
        }
    }

    /// Picks the oldest account (smallest HandleTime) from a list of matches.
    pub fn pick_oldest_match(mut matches: Vec<(String, u64)>) -> Option<String> {
        if matches.is_empty() {
            None
        } else {
            matches.sort_by(|a, b| {
                let res = a.1.cmp(&b.1);
                if res == std::cmp::Ordering::Equal {
                    a.0.cmp(&b.0)
                } else {
                    res
                }
            });
            Some(matches[0].0.clone())
        }
    }

    /// Discover root relay ids and socket addresses by scanning provided accounts' local state for key "static_endpoint".
    /// This variant uses an injected local-state getter for testability.
    // pub fn discover_root_relays_with<F>(
    //     app_id: u64,
    //     accounts: &[String],
    //     get_local: F,
    // ) -> Vec<(String, std::net::SocketAddr)>
    // where
    //     F: Fn(u64, &str) -> Option<Vec<(String, String)>>,
    // {
    //     let mut out: Vec<(String, std::net::SocketAddr)> = Vec::new();
    //     for acct in accounts {
    //         if let Some(entries) = get_local(app_id, acct) {
    //             let ep = entries.iter().find(|(k, _)| k == "static_endpoint").map(|(_, v)| v.as_str()).unwrap_or("");
    //             let ep_x = entries.iter().find(|(k, _)| k == "static_endpoint_x").map(|(_, v)| v.as_str()).unwrap_or("");
    //             let v = format!("{}{}", ep, ep_x);
    //             if !v.is_empty() {
    //                 if let Some(addr) = Self::parse_relay_ip(&v) {
    //                     out.push((acct.clone(), addr));
    //                 }
    //             }
    //         }
    //     }
    //     out
    // }

    /// Discover root relay ids and socket addresses using AlgoOps for local state fetching.
    /// Requires a list of candidate accounts to check.
    // pub fn discover_root_relays(&self, app_id: u64, accounts: &[String]) -> anyhow::Result<Vec<(String, std::net::SocketAddr)>> {
    //     let res = Self::discover_root_relays_with(app_id, accounts, |aid, acct| {
    //         match self.ops.local_state_for_account(aid, acct) {
    //             Ok(v) => v,
    //             Err(_) => None,
    //         }
    //     });
    //     Ok(res)
    // }



    fn sender_account(&self) -> Result<(Account, Address)> {
        let sk = self.ops.private_key_bytes()?;
        let seed: [u8; 32] = sk.as_slice().try_into().map_err(|_| anyhow!("Secret key must be 32 bytes"))?;
        let account = Account::from_seed(seed);
        let addr_str = self
            .ops
            .address
            .as_ref()
            .ok_or_else(|| anyhow!("This operation needs an address"))?;
        let addr = Address::from_str(addr_str).map_err(|e| anyhow!("invalid address: {e}"))?;
        Ok((account, addr))
    }


    fn params(&self, client: &Algod) -> Result<SuggestedParams> {
        self.ops.algod_call(|| client.suggested_params())
    }


    fn app_address(&self, app_id: u64) -> Result<Address> {
        let addr_str = self.ops.contract_address(app_id)?;
        Address::from_str(&addr_str).map_err(|e| anyhow!("invalid app address: {e}"))
    }

    pub fn broadcast_group(&self, client: &Algod, signed_group: Vec<Vec<u8>>) -> Result<String> {
        // Concatenate msgpack-encoded signed transactions into one byte array as per Algod API.
        let mut bytes: Vec<u8> = Vec::new();
        for s in signed_group { bytes.extend_from_slice(&s); }

        let txid = self.ops.algod_call(|| client.send_raw(&bytes))
            .map_err(|e| anyhow!("send_raw failed: {e}"))?.tx_id;
        // Wait for confirmation of the first tx id in the group.
        self.ops.wait_for_confirmation(&txid, 10)?;
        Ok(txid)
    }

    pub fn assign_group_id(txns: &mut [Transaction]) -> Result<()> {
        // Try to use algonaut's internal grouping if available via feature gates; otherwise compute manually.
        // We attempt the canonical SDK approach from Python: gid = sha512_256("TG" || msgpack({txlist:[txid...]})).
        // algonaut exposes a helper under transaction::transaction::assign_group_id in recent versions.
        #[allow(unused_imports)]
        use algonaut::transaction::transaction as txmod;
        #[allow(unused_variables)]
        {
            // If algonaut provides assign_group_id, use it.
            #[allow(unused_must_use)]
            {
                // Some versions expose txmod::assign_group_id(&mut [Transaction]) -> Result<()>
                // We'll call it if available; otherwise the below manual fallback compiles.
            }
        }
        // Manual fallback: compute group id like Python SDK.
        // 1) Compute txids = sha512_256("TX" || msgpack(transaction_without_group)) for each txn
        let mut txids: Vec<[u8; 32]> = Vec::with_capacity(txns.len());
        for tx in txns.iter() {
            let raw = tx.to_msg_pack().map_err(|e| anyhow!("failed to msgpack unsigned tx: {e}"))?;
            let mut tv = Vec::with_capacity(2 + raw.len());
            tv.extend_from_slice(b"TX");
            tv.extend_from_slice(&raw);
            let mut ht = Sha512_256::new();
            ht.update(&tv);
            let txid: [u8; 32] = ht.finalize().into();
            txids.push(txid);
        }
        // 2) Build msgpack of { "txlist": [txid1, txid2, ...] } manually (minimal encoder)
        // msgpack map of 1 element => 0x81, key is a str of len 6 => 0xA6 'txlist'
        // array header: len N => 0x90 + N (assuming N<16)
        let n = txids.len();
        if n == 0 { bail!("no transactions to group"); }
        if n > 15 { bail!("too many txns in group (max 15 supported by minimal encoder)"); }
        let mut group_mp: Vec<u8> = Vec::with_capacity(1 + 1 + 6 + 1 + n * 34);
        group_mp.push(0x81); // 1-key map
        group_mp.push(0xA6); // str len 6
        group_mp.extend_from_slice(b"txlist");
        group_mp.push(0x90 + (n as u8)); // fixarray of length n
        for txid in &txids {
            // bin32 with 32 bytes: prefix 0xC4 0x20
            group_mp.push(0xC4);
            group_mp.push(32);
            group_mp.extend_from_slice(txid);
        }
        // 3) gid = sha512_256("TG" || group_mp)
        let mut v = Vec::with_capacity(2 + group_mp.len());
        v.extend_from_slice(b"TG");
        v.extend_from_slice(&group_mp);
        let mut hg = Sha512_256::new();
        hg.update(&v);
        let gid: [u8; 32] = hg.finalize().into();

        // 4) Set group id on each transaction
        for tx in txns.iter_mut() {
            tx.group = Some(algonaut::crypto::HashDigest(gid));
        }
        Ok(())
    }

    /// Call buy_bingle. Requires:
    /// - app_id: the deployed application id
    /// - asset_id: the ASA id for Bingle$
    /// - price_microalgos: the current price configured on-chain (microAlgos)
    pub fn buy_bingle(&self, app_id: u64, asset_id: u64, price_microalgos: u64) -> Result<String> {
        if app_id == 0 { bail!("app_id must be > 0"); }
        if asset_id == 0 { bail!("asset_id must be > 0"); }
        // Ensure the caller (sender) is opted in to receive the asset from the app's inner tx
        let _ = self.opt_in_sender_to_asset(asset_id)?;
        let client = self.ops.algod_client()?;
        let params = self.params(&client)?;
        let (account, sender) = self.sender_account()?;
        let app_addr = self.app_address(app_id)?;
        algo_log!("[buy_bingle] app_addr: {:#?}", app_addr);

        // 1) Payment: sender -> app address for price. The app will verify this and perform
        //    an inner tx moving 1 unit of the ASA from the creator-held reserve to the sender.
        let tx_pay = Pay::new(sender, app_addr, MicroAlgos(price_microalgos))
            .note(AlgoOps::unique_note())
            .build(&params)
            .map_err(|e| anyhow!("build pay: {e}"))?;
        algo_log!("[buy_bingle] tx_pay: {:#?}", tx_pay);

        // 2) App call: buy_bingle()void with foreign asset, built via AlgoOps helper
        let tx_app = self
            .ops
            .build_call_app_tx(app_id, Some(asset_id), Some("buy_bingle()void"), &[])?;
        algo_log!("[buy_bingle] tx_app: {:#?}", tx_app);

        // Group, sign, and send
        let mut txs = vec![tx_pay, tx_app];
        Self::assign_group_id(&mut txs)?;

        let s1 = account.sign(txs.remove(0)).map_err(|e| anyhow!("sign pay: {e}"))?.to_msg_pack().map_err(|e| anyhow!("encode signed pay: {e}"))?;
        let s2 = account.sign(txs.remove(0)).map_err(|e| anyhow!("sign app: {e}"))?.to_msg_pack().map_err(|e| anyhow!("encode signed app: {e}"))?;

        self.broadcast_group(&client, vec![s1, s2])
    }

    /// Call sell_bingle. Requires:
    /// - app_id, asset_id as above
    /// - amount: number of units to sell
    /// - price_microalgos: price of one unit, payout = price * amount
    pub fn sell_bingle(&self, app_id: u64, asset_id: u64, amount: u64, price_microalgos: u64) -> Result<String> {
        if app_id == 0 { bail!("app_id must be > 0"); }
        if asset_id == 0 { bail!("asset_id must be > 0"); }
        if amount == 0 { bail!("amount must be > 0"); }
        // Ensure the app account is opted-in to the ASA so it can receive transfers
        self.opt_in_app_to_asset(app_id, asset_id)?;
        let client = self.ops.algod_client()?;
        let params = self.params(&client)?;
        let (account, sender) = self.sender_account()?;
        let app_addr = self.app_address(app_id)?;

        let payout = price_microalgos.checked_mul(amount).ok_or_else(|| anyhow!("payout overflow"))?;

        // 1) Asset xfer: sender -> app address of `amount`
        let tx_ax = TransferAsset::new(sender, AssetId(asset_id), amount, app_addr)
            .note(AlgoOps::unique_note())
            .build(&params)
            .map_err(|e| anyhow!("build axfer: {e}"))?;

        // 2) Payout payment: sender -> sender for `payout` (satisfies on-chain payment check)
        let tx_pay = Pay::new(sender, sender, MicroAlgos(payout))
            .note(AlgoOps::unique_note())
            .build(&params)
            .map_err(|e| anyhow!("build pay: {e}"))?;

        // 3) App call: sell_bingle(uint64)void
        let mut app_args: Vec<Vec<u8>> = Vec::new();
        app_args.push(AlgoOps::arc4_selector("sell_bingle(uint64)void").to_vec());
        // ARC-4 uint64 as 8-byte big endian
        app_args.push((amount as u64).to_be_bytes().to_vec());
        let tx_app = CallApplication::new(sender, AppId(app_id))
            .app_arguments(app_args)
            .foreign_assets(vec![AssetId(asset_id)])
            .note(AlgoOps::unique_note())
            .build(&params)
            .map_err(|e| anyhow!("build app call: {e}"))?;

        let mut txs = vec![tx_ax, tx_pay, tx_app];
        Self::assign_group_id(&mut txs)?;

        let s1 = account.sign(txs.remove(0)).map_err(|e| anyhow!("sign axfer: {e}"))?.to_msg_pack().map_err(|e| anyhow!("encode signed axfer: {e}"))?;
        let s2 = account.sign(txs.remove(0)).map_err(|e| anyhow!("sign pay: {e}"))?.to_msg_pack().map_err(|e| anyhow!("encode signed pay: {e}"))?;
        let s3 = account.sign(txs.remove(0)).map_err(|e| anyhow!("sign app: {e}"))?.to_msg_pack().map_err(|e| anyhow!("encode signed app: {e}"))?;

        self.broadcast_group(&client, vec![s1, s2, s3])
    }

    /// Call register(handle). Requires:
    /// - app_id, asset_id as above
    /// - price_units: the fee in Bingle$ units (as configured on-chain price)
    pub fn register(&self, app_id: u64, asset_id: u64, handle: &str, price_units: u64) -> Result<String> {
        if app_id == 0 { bail!("app_id must be > 0"); }
        if asset_id == 0 { bail!("asset_id must be > 0"); }
        if handle.is_empty() { bail!("handle must not be empty"); }

        let sender_str = self.ops.address.as_ref().ok_or_else(|| anyhow!("No sender address"))?.to_string();

        // 1. Pre-check: Handle uniqueness via indexer
        if let Some(existing_owner) = self.handle_lookup(handle)? {
            if existing_owner != sender_str {
                bail!("Handle already in use by {}", existing_owner);
            }
        }

        let tx_id = self.register_unchecked(app_id, asset_id, handle, price_units)?;

        // 4. Post-check: Validate that we are the deterministic winner for this handle
        self.verify_registration_winner(handle, &sender_str)?;

        Ok(tx_id)
    }

    /// Internal method to perform the registration on-chain without uniqueness pre-checks.
    /// Used by `register` after pre-checks, or by tests to simulate "hacked" registrations.
    pub fn register_unchecked(&self, app_id: u64, asset_id: u64, handle: &str, price_units: u64) -> Result<String> {
        let client = self.ops.algod_client()?;
        let params = self.params(&client)?;
        let (account, sender) = self.sender_account()?;

        // Ensure the app account is opted-in to the ASA so it can receive the registration fee
        self.opt_in_app_to_asset(app_id, asset_id)?;
        
        // Ensure the caller (sender) is opted-in to the ASA so the axfer succeeds
        let _ = self.opt_in_sender_to_asset(asset_id)?;
        // Ensure the caller is opted-in to the application local state to avoid logic eval error
        self.ops.opt_in_app(app_id)?;
        let app_addr = self.app_address(app_id)?;

        // 2) ASA fee: sender -> app address amount = price_units
        let tx_ax = TransferAsset::new(sender, AssetId(asset_id), price_units, app_addr)
            .note(AlgoOps::unique_note())
            .build(&params)
            .map_err(|e| anyhow!("build axfer: {e}"))?;
        // Print the full asset transfer transaction (tx_ax) with all fields for visibility
        algo_log!("tx_ax: {:#?}", tx_ax);

        // 3) App call: register(string)void with handle arg
        // ARC-4 string encoding: 2-byte big-endian length prefix + UTF-8 bytes
        let mut app_args: Vec<Vec<u8>> = Vec::new();
        app_args.push(AlgoOps::arc4_selector("register(string)void").to_vec());
        let handle_bytes = handle.as_bytes();
        let len = handle_bytes.len();
        if len > u16::MAX as usize { bail!("handle too long"); }
        let mut arg = Vec::with_capacity(2 + len);
        arg.extend_from_slice(&(len as u16).to_be_bytes());
        arg.extend_from_slice(handle_bytes);
        app_args.push(arg);
        let tx_app = CallApplication::new(sender, AppId(app_id))
            .app_arguments(app_args)
            .foreign_assets(vec![AssetId(asset_id)])
            .note(AlgoOps::unique_note())
            .build(&params)
            .map_err(|e| anyhow!("build app call: {e}"))?;

        let mut txs = vec![tx_ax, tx_app];
        Self::assign_group_id(&mut txs)?;

        let s1 = account.sign(txs.remove(0)).map_err(|e| anyhow!("sign axfer: {e}"))?.to_msg_pack().map_err(|e| anyhow!("encode signed axfer: {e}"))?;
        let s2 = account.sign(txs.remove(0)).map_err(|e| anyhow!("sign app: {e}"))?.to_msg_pack().map_err(|e| anyhow!("encode signed app: {e}"))?;

        self.broadcast_group(&client, vec![s1, s2])
    }

    /// Wait up to 15 seconds for indexer to catch up and verify that `expected_winner` is the owner of `handle`.
    pub fn verify_registration_winner(&self, handle: &str, expected_winner: &str) -> Result<()> {
        let mut winner_verified = false;
        for i in 0..15 {
            if i > 0 { std::thread::sleep(std::time::Duration::from_secs(1)); }
            if let Some(winner) = self.handle_lookup(handle)? {
                if winner == expected_winner {
                    winner_verified = true;
                    break;
                } else {
                    bail!("Handle registration verification failed: handle '{}' was taken by {} (expected {})", handle, winner, expected_winner);
                }
            }
        }

        if !winner_verified {
            algo_log!("Warning: handle registration verification timed out for handle '{}'. It may still be pending in indexer.", handle);
        }
        Ok(())
    }

    /// Admin method: Opt the application account into the given ASA by calling the
    /// dApp's opt_in_to_bingle(uint64) method. Must be called by the app creator.
    /// Returns the transaction id of the app call when an opt-in was required; if the
    /// app was already opted in, returns Ok("") without making a call.
    /// TODO: make this generic as called from deploy
    pub fn opt_in_app_to_asset(&self, app_id: u64, asset_id: u64) -> Result<String> {
        if app_id == 0 { bail!("app_id must be > 0"); }
        if asset_id == 0 { bail!("asset_id must be > 0"); }
        // If the app address already holds the asset, nothing to do
        let app_addr_str = self.ops.contract_address(app_id)?;
        if self.ops.is_account_opted_in_to_asset(&app_addr_str, asset_id)? { return Ok(String::new()); }
        // Call the admin method on the contract; this must be signed by the creator
        let (_tx_id, _logs) = self
            .ops
            .call_app(app_id, Some(asset_id), Some("opt_in_to_bingle(uint64)void"), &[AppArg::Uint(asset_id)])?;
        // Return tx id to signal a call occurred; fetch from tuple index 0
        Ok(_tx_id)
    }

    /// Ensure the sender account is opted-in to the given ASA. If already opted-in, returns Ok("").
    /// Otherwise, sends an asset transfer of 0 units from the sender to themselves to perform opt-in.
    pub fn opt_in_sender_to_asset(&self, asset_id: u64) -> Result<String> {
        if asset_id == 0 { bail!("asset_id must be > 0"); }
        let sender_str = self
            .ops
            .address
            .as_ref()
            .ok_or_else(|| anyhow!("This operation needs an address"))?;
        if self.ops.is_account_opted_in_to_asset(sender_str, asset_id)? {
            return Ok(String::new());
        }
        let client = self.ops.algod_client()?;
        let params = self.params(&client)?;
        let (account, sender) = self.sender_account()?;
        // Opt-in by sending 0 units to self
        let tx = TransferAsset::new(sender, AssetId(asset_id), 0, sender)
            .note(AlgoOps::unique_note())
            .build(&params)
            .map_err(|e| anyhow!("build asset opt-in tx: {e}"))?;
        let signed = account
            .sign(tx)
            .map_err(|e| anyhow!("sign asset opt-in: {e}"))?
            .to_msg_pack()
            .map_err(|e| anyhow!("encode signed asset opt-in: {e}"))?;
        let tx_id = self.ops.algod_call(|| client.send_raw(&signed))
            .map_err(|e| anyhow!("send_raw failed: {e}"))?.tx_id;
        self.ops.wait_for_confirmation(&tx_id, 10)?;
        Ok(tx_id)
    }
}
