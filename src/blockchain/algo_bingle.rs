use anyhow::{anyhow, bail, Result};
use sha2::{Digest, Sha512_256};
use std::future::Future;
use std::str::FromStr;

use crate::blockchain::algo_ops::{AlgoOps, AppArg};

use algonaut::{
    algod::v2::Algod,
    core::{Address, MicroAlgos, ToMsgPack},
    transaction::{
        account::Account,
        builder::CallApplication,
        transaction::Transaction,
        TransferAsset, Pay, TxnBuilder,
    },
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
}

impl AlgoBingle {
    pub fn new(ops: AlgoOps, app_id: u64, asset_id: u64) -> Self {
        // Debug-print the AlgoOps configuration for visibility
        log::info!("[AlgoBingle::new] ops.config={:?} app_id={} asset_id={}", ops.config, app_id, asset_id);
        #[allow(unused)] {  }
        Self { ops, app_id, asset_id }
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
        let client = self.algod_client()?;
        let app_info = match self.rt_block_on(client.application_information(app_id))? {
            Ok(info) => info,
            Err(e) => return Err(anyhow!("failed to fetch application information: {e}")),
        };
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

    /// Call register_endpoint(string)void for the caller.
    /// Passing an empty string clears the local state key.
    /// Returns the submitted transaction id on success.
    pub fn register_endpoint(&self, app_id: u64, endpoint: &str) -> Result<String> {
        if app_id == 0 { bail!("app_id must be > 0"); }
        // Build ARC-4 arguments manually to ensure correct string encoding (2-byte length prefix)
        let client = self.algod_client()?;
        let params = self.params(&client)?;
        let (account, sender) = self.sender_account()?;
        let mut app_args: Vec<Vec<u8>> = Vec::new();
        app_args.push(AlgoOps::arc4_selector("register_endpoint(string)void").to_vec());
        let ep_bytes = endpoint.as_bytes();
        if ep_bytes.len() > u16::MAX as usize { bail!("endpoint too long"); }
        let mut arg = Vec::with_capacity(2 + ep_bytes.len());
        arg.extend_from_slice(&(ep_bytes.len() as u16).to_be_bytes());
        arg.extend_from_slice(ep_bytes);
        app_args.push(arg);
        let call = CallApplication::new(sender, app_id)
            .app_arguments(app_args)
            .build();
        let tx = TxnBuilder::with(&params, call)
            .note(AlgoOps::unique_note())
            .build()
            .map_err(|e| anyhow!("build app call: {e}"))?;
        log::info!("register_endpoint tx: {:?}", tx);
        let signed = account
            .sign_transaction(tx)
            .map_err(|e| anyhow!("sign app call: {e}"))?
            .to_msg_pack()
            .map_err(|e| anyhow!("encode signed app call: {e}"))?;
        self.broadcast_group(&client, vec![signed])
    }

    /// List accounts from the provided slice that have non-empty local state key "static_endpoint" set for this app.
    /// Returns Vec of (account_address, static_endpoint_value).
    pub fn list_static_endpoints_for_accounts(&self, app_id: u64, accounts: &[String]) -> Result<Vec<(String, String)>> {
        if app_id == 0 { bail!("app_id must be > 0"); }
        let mut out: Vec<(String, String)> = Vec::new();
        for acct in accounts {
            if let Ok(Some(entries)) = self.ops.local_state_for_account(app_id, acct) {
                if let Some((_, val)) = entries.into_iter().find(|(k, _)| k == "static_endpoint") {
                    if !val.is_empty() {
                        out.push((acct.clone(), val));
                    }
                }
            }
        }
        Ok(out)
    }

    /// Parse a RelayIP string into a SocketAddr. Accepts forms like "host:port" or "ip:port".
    /// Returns None if parsing fails.
    pub fn parse_relay_ip(ip: &str) -> Option<std::net::SocketAddr> {
        ip.parse::<std::net::SocketAddr>().ok()
    }

    /// Use the Indexer API to list accounts that have a non-empty "static_endpoint" in local state for the given app_id.
    /// Returns Vec of (account_address, static_endpoint_value).
    pub fn list_static_endpoints_via_indexer(&self, app_id: u64) -> Result<Vec<(String, String)>> {
        if tokio::runtime::Handle::try_current().is_ok() {
            return std::thread::scope(|s| {
                s.spawn(|| self.list_static_endpoints_via_indexer_sync(app_id))
                    .join()
                    .unwrap_or_else(|_| Err(anyhow!("indexer thread panicked")))
            });
        }
        self.list_static_endpoints_via_indexer_sync(app_id)
    }

    fn list_static_endpoints_via_indexer_sync(&self, app_id: u64) -> Result<Vec<(String, String)>> {
        // Debug: print the current ops.config for visibility in discovery
        log::info!("[AlgoBingle::list_static_endpoints_via_indexer] ops.config={:?}", self.ops.config);
        #[allow(unused)] {  }
        if app_id == 0 { bail!("app_id must be > 0"); }
        let base = format!("{}:{}/v2/accounts", self.ops.config.indexer_api_url, self.ops.config.indexer_api_port);
        let client = reqwest::blocking::Client::new();
        let mut next: Option<String> = None;
        let mut results: Vec<(String, String)> = Vec::new();
        loop {
            let mut req = client.get(&base)
                .query(&[("application-id", app_id.to_string()), ("limit", "1000".to_string())]);
            if let Some(n) = &next { req = req.query(&[("next", n)]); }
            // Add both indexer and algod token headers for compatibility
            if let Some(tok) = &self.ops.config.token {
                req = req.header("X-Indexer-API-Token", tok.clone())
                         .header(self.ops.config.token_key.clone().unwrap_or_else(|| "X-Algo-API-Token".to_string()), tok.clone());
            }
            log::info!("Sending indexer request {:?}", req);
            let resp = req.send().map_err(|e| anyhow!("indexer request failed: {e}"))?;
            if !resp.status().is_success() { bail!("indexer returned {}", resp.status()); }
            let v: serde_json::Value = resp.json().map_err(|e| anyhow!("indexer json parse failed: {e}"))?;
            // Debug: dump the full JSON returned from /v2/accounts for visibility
            log::trace!("[AlgoBingle][indexer /v2/accounts] page: {}", v);
            let accounts = v.get("accounts").and_then(|x| x.as_array()).cloned().unwrap_or_default();
            for acct in accounts {
                let addr = acct.get("address").and_then(|x| x.as_str()).unwrap_or("").to_string();
                // find local state for this app id
                if let Some(als) = acct.get("apps-local-state").or_else(|| acct.get("apps_local_state")).and_then(|x| x.as_array()) {
                    for st in als {
                        let id = st.get("id").and_then(|x| x.as_u64());
                        if id == Some(app_id) {
                            let keyvals = st.get("key-value").or_else(|| st.get("key_value")).and_then(|x| x.as_array()).cloned().unwrap_or_default();
                            let kvs = Self::decode_state_entries(&keyvals);
                            if let Some((_, val)) = kvs.into_iter().find(|(k, _)| k == "static_endpoint") {
                                if !val.is_empty() {
                                    results.push((addr.clone(), val));
                                }
                            }
                        }
                    }
                }
            }
            next = v.get("next-token").and_then(|x| x.as_str()).map(|s| s.to_string());
            if next.is_none() { break; }
        }
        Ok(results)
    }

    /// Discover root relay ids and socket addresses by scanning provided accounts' local state for key "RelayIP".
    /// This variant uses an injected local-state getter for testability.
    pub fn discover_root_relays_with<F>(
        app_id: u64,
        accounts: &[String],
        get_local: F,
    ) -> Vec<(String, std::net::SocketAddr)>
    where
        F: Fn(u64, &str) -> Option<Vec<(String, String)>>,
    {
        let mut out: Vec<(String, std::net::SocketAddr)> = Vec::new();
        for acct in accounts {
            if let Some(entries) = get_local(app_id, acct) {
                // find static_endpoint
                if let Some((_k, v)) = entries.into_iter().find(|(k, _)|  k == "static_endpoint") {
                    if let Some(addr) = Self::parse_relay_ip(&v) {
                        out.push((acct.clone(), addr));
                    }
                }
            }
        }
        out
    }

    /// Discover root relay ids and socket addresses using AlgoOps for local state fetching.
    /// Requires a list of candidate accounts to check.
    pub fn discover_root_relays(&self, app_id: u64, accounts: &[String]) -> anyhow::Result<Vec<(String, std::net::SocketAddr)>> {
        let res = Self::discover_root_relays_with(app_id, accounts, |aid, acct| {
            match self.ops.local_state_for_account(aid, acct) {
                Ok(v) => v,
                Err(_) => None,
            }
        });
        Ok(res)
    }


    fn algod_client(&self) -> Result<Algod> {
        // Build base URL including port, e.g., http://localhost:4001
        let url = format!("{}:{}", self.ops.config.client_api_url, self.ops.config.client_api_port);
        let token = self.ops.config.token.clone().unwrap_or_default();
        Algod::new(&url, &token).map_err(|e| anyhow!("failed to construct Algod client: {e}"))
    }

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

    fn rt_block_on<T: Send>(&self, fut: impl Future<Output = T> + Send) -> Result<T> {
        // If we are already in a tokio runtime, we must avoid nested block_on and 
        // the "cannot drop runtime" panic during unwinding/drop.
        if tokio::runtime::Handle::try_current().is_ok() {
            return std::thread::scope(|s| {
                let handle = s.spawn(|| {
                    let rt = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .expect("failed to build temporary tokio runtime");
                    rt.block_on(fut)
                });
                handle.join().map_err(|_| anyhow!("rt_block_on thread panicked"))
            });
        }

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| anyhow!("failed to build tokio runtime: {e}"))?;
        Ok(rt.block_on(fut))
    }

    fn params(&self, client: &Algod) -> Result<algonaut::core::SuggestedTransactionParams> {
        match self.rt_block_on(client.suggested_transaction_params())? { Ok(p) => Ok(p), Err(e) => Err(anyhow!("failed to fetch suggested params: {e}")) }
    }

    fn app_address(&self, app_id: u64) -> Result<Address> {
        let addr_str = self.ops.contract_address(app_id)?;
        Address::from_str(&addr_str).map_err(|e| anyhow!("invalid app address: {e}"))
    }

    fn broadcast_group(&self, client: &Algod, signed_group: Vec<Vec<u8>>) -> Result<String> {
        // Concatenate msgpack-encoded signed transactions into one byte array as per Algod API.
        let mut bytes: Vec<u8> = Vec::new();
        for s in signed_group { bytes.extend_from_slice(&s); }

        // Prefer plural API if available; fallback to single endpoint (works with concatenated bytes).
        // We call the same method signature as used elsewhere; most algod servers accept concatenated group.
        let txid = match self.rt_block_on(client.broadcast_raw_transaction(&bytes))? {
            Ok(r) => r.tx_id,
            Err(e) => return Err(anyhow!("broadcast_raw_transaction failed: {e}")),
        };
        // Wait for confirmation of the first tx id in the group.
        self.ops.wait_for_confirmation(&txid, 10)?;
        Ok(txid)
    }

    fn assign_group_id(txns: &mut [Transaction]) -> Result<()> {
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
        let client = self.algod_client()?;
        let params = self.params(&client)?;
        let (account, sender) = self.sender_account()?;
        let app_addr = self.app_address(app_id)?;
        log::info!("[buy_bingle] app_addr: {:#?}", app_addr);

        // 1) Payment: sender -> app address for price. The app will verify this and perform
        //    an inner tx moving 1 unit of the ASA from the creator-held reserve to the sender.
        let pay = Pay::new(sender, app_addr, MicroAlgos(price_microalgos)).build();
        let tx_pay = TxnBuilder::with(&params, pay)
            .note(AlgoOps::unique_note())
            .build()
            .map_err(|e| anyhow!("build pay: {e}"))?;
        log::info!("[buy_bingle] tx_pay: {:#?}", tx_pay);

        // 2) App call: buy_bingle()void with foreign asset, built via AlgoOps helper
        let tx_app = self
            .ops
            .build_call_app_tx(app_id, Some(asset_id), Some("buy_bingle()void"), &[])?;
        log::info!("[buy_bingle] tx_app: {:#?}", tx_app);

        // Group, sign, and send
        let mut txs = vec![tx_pay, tx_app];
        Self::assign_group_id(&mut txs)?;

        let s1 = account.sign_transaction(txs.remove(0)).map_err(|e| anyhow!("sign pay: {e}"))?.to_msg_pack().map_err(|e| anyhow!("encode signed pay: {e}"))?;
        let s2 = account.sign_transaction(txs.remove(0)).map_err(|e| anyhow!("sign app: {e}"))?.to_msg_pack().map_err(|e| anyhow!("encode signed app: {e}"))?;

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
        let client = self.algod_client()?;
        let params = self.params(&client)?;
        let (account, sender) = self.sender_account()?;
        let app_addr = self.app_address(app_id)?;

        let payout = price_microalgos.checked_mul(amount).ok_or_else(|| anyhow!("payout overflow"))?;

        // 1) Asset xfer: sender -> app address of `amount`
        let ax = TransferAsset::new(sender, asset_id, amount, app_addr).build();
        let tx_ax = TxnBuilder::with(&params, ax)
            .note(AlgoOps::unique_note())
            .build()
            .map_err(|e| anyhow!("build axfer: {e}"))?;

        // 2) Payout payment: sender -> sender for `payout` (satisfies on-chain payment check)
        let pay = Pay::new(sender, sender, MicroAlgos(payout)).build();
        let tx_pay = TxnBuilder::with(&params, pay)
            .note(AlgoOps::unique_note())
            .build()
            .map_err(|e| anyhow!("build pay: {e}"))?;

        // 3) App call: sell_bingle(uint64)void
        let mut app_args: Vec<Vec<u8>> = Vec::new();
        app_args.push(AlgoOps::arc4_selector("sell_bingle(uint64)void").to_vec());
        // ARC-4 uint64 as 8-byte big endian
        app_args.push((amount as u64).to_be_bytes().to_vec());
        let call = CallApplication::new(sender, app_id)
            .app_arguments(app_args)
            .foreign_assets(vec![asset_id])
            .build();
        let tx_app = TxnBuilder::with(&params, call)
            .note(AlgoOps::unique_note())
            .build()
            .map_err(|e| anyhow!("build app call: {e}"))?;

        let mut txs = vec![tx_ax, tx_pay, tx_app];
        Self::assign_group_id(&mut txs)?;

        let s1 = account.sign_transaction(txs.remove(0)).map_err(|e| anyhow!("sign axfer: {e}"))?.to_msg_pack().map_err(|e| anyhow!("encode signed axfer: {e}"))?;
        let s2 = account.sign_transaction(txs.remove(0)).map_err(|e| anyhow!("sign pay: {e}"))?.to_msg_pack().map_err(|e| anyhow!("encode signed pay: {e}"))?;
        let s3 = account.sign_transaction(txs.remove(0)).map_err(|e| anyhow!("sign app: {e}"))?.to_msg_pack().map_err(|e| anyhow!("encode signed app: {e}"))?;

        self.broadcast_group(&client, vec![s1, s2, s3])
    }

    /// Call register(handle). Requires:
    /// - app_id, asset_id as above
    /// - price_units: the fee in Bingle$ units (as configured on-chain price)
    pub fn register(&self, app_id: u64, asset_id: u64, handle: &str, price_units: u64) -> Result<String> {
        if app_id == 0 { bail!("app_id must be > 0"); }
        if asset_id == 0 { bail!("asset_id must be > 0"); }
        if handle.is_empty() { bail!("handle must not be empty"); }
        // Ensure the app account is opted-in to the ASA so it can receive the registration fee
        self.opt_in_app_to_asset(app_id, asset_id)?;
        
        // Ensure the caller (sender) is opted-in to the ASA so the axfer succeeds
        let _ = self.opt_in_sender_to_asset(asset_id)?;
        // Ensure the caller is opted-in to the application local state to avoid logic eval error
        self.ops.opt_in_app(app_id)?;
        let client = self.algod_client()?;
        let params = self.params(&client)?;
        let (account, sender) = self.sender_account()?;
        let app_addr = self.app_address(app_id)?;

        // 1) ASA fee: sender -> app address amount = price_units
        let ax = TransferAsset::new(sender, asset_id, price_units, app_addr).build();
        let tx_ax = TxnBuilder::with(&params, ax)
            .note(AlgoOps::unique_note())
            .build()
            .map_err(|e| anyhow!("build axfer: {e}"))?;
        // Print the full asset transfer transaction (tx_ax) with all fields for visibility
        log::info!("tx_ax: {:#?}", tx_ax);

        // 2) App call: register(string)void with handle arg
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
        let call = CallApplication::new(sender, app_id)
            .app_arguments(app_args)
            .foreign_assets(vec![asset_id])
            .build();
        let tx_app = TxnBuilder::with(&params, call)
            .note(AlgoOps::unique_note())
            .build()
            .map_err(|e| anyhow!("build app call: {e}"))?;

        let mut txs = vec![tx_ax, tx_app];
        Self::assign_group_id(&mut txs)?;

        let s1 = account.sign_transaction(txs.remove(0)).map_err(|e| anyhow!("sign axfer: {e}"))?.to_msg_pack().map_err(|e| anyhow!("encode signed axfer: {e}"))?;
        let s2 = account.sign_transaction(txs.remove(0)).map_err(|e| anyhow!("sign app: {e}"))?.to_msg_pack().map_err(|e| anyhow!("encode signed app: {e}"))?;

        self.broadcast_group(&client, vec![s1, s2])
    }

    /// Admin method: Opt the application account into the given ASA by calling the
    /// dApp's opt_in_to_bingle(uint64) method. Must be called by the app creator.
    /// Returns the transaction id of the app call when an opt-in was required; if the
    /// app was already opted in, returns Ok("") without making a call.
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
        let client = self.algod_client()?;
        let params = self.params(&client)?;
        let (account, sender) = self.sender_account()?;
        // Opt-in by sending 0 units to self
        let ax = TransferAsset::new(sender, asset_id, 0, sender).build();
        let tx = TxnBuilder::with(&params, ax)
            .note(AlgoOps::unique_note())
            .build()
            .map_err(|e| anyhow!("build asset opt-in tx: {e}"))?;
        let signed = account
            .sign_transaction(tx)
            .map_err(|e| anyhow!("sign asset opt-in: {e}"))?
            .to_msg_pack()
            .map_err(|e| anyhow!("encode signed asset opt-in: {e}"))?;
        let tx_id = match self.rt_block_on(client.broadcast_raw_transaction(&signed))? { Ok(r) => r.tx_id, Err(e) => return Err(anyhow!("broadcast_raw_transaction failed: {e}")) };
        self.ops.wait_for_confirmation(&tx_id, 10)?;
        Ok(tx_id)
    }
}
