use crate::api::{BingleLocalApi, Contact, ContactSource, Keypair, Message};
use rust_comms::blockchain::algo_ops::{AlgoChainConfig, AlgoOps};
use rust_comms::blockchain::algo_bingle::AlgoBingle;
use std::sync::Mutex;

/// Configuration for the local API implementation.
/// Includes the blockchain provider configuration and required ids.
#[derive(Debug, Clone)]
pub struct LocalApiConfig {
    pub algo_config: AlgoChainConfig,
    pub app_id: u64,
    pub asset_id: u64,
}

impl Default for LocalApiConfig {
    fn default() -> Self {
        Self { algo_config: AlgoChainConfig::default(), app_id: 0, asset_id: 0 }
    }
}

/// Basic local implementation stub. For now it only supports keypair generation.
pub struct BingleApiLocalImpl {
    keypair: Mutex<Option<Keypair>>, // interior mutability to allow &self methods to ensure keypair exists
    algo_ops: Mutex<Option<AlgoOps>>, // cache constructed AlgoOps for current keypair
    config: LocalApiConfig,
}

impl BingleApiLocalImpl {
    pub fn new(config: LocalApiConfig) -> Self {
        Self { keypair: Mutex::new(None), algo_ops: Mutex::new(None), config }
    }
}

impl BingleLocalApi for BingleApiLocalImpl {
    fn generate_keypair(&mut self) -> Result<Keypair, String> {
        let (id, passphrase) = rust_comms::blockchain::algo_ops::AlgoOps::generate_keypair();
        let kp = Keypair { id, passphrase };
        if let Ok(mut guard) = self.keypair.lock() {
            *guard = Some(kp.clone());
        }
        // Invalidate cached AlgoOps since keypair changed
        if let Ok(mut ops_guard) = self.algo_ops.lock() {
            *ops_guard = None;
        }
        Ok(kp)
    }

    fn register_keypair(&self, handle: String) -> Result<bool, String> {
        // Validate config
        let app_id = self.config.app_id;
        let asset_id = self.config.asset_id;
        if app_id == 0 { return Err("app_id not set in config".to_string()); }
        if asset_id == 0 { return Err("asset_id not set in config".to_string()); }

        // Ensure we have blockchain ops bound to current keypair
        let ops = self.get_algo_ops()?;

        // Execute on-chain steps
        ops.opt_in_app(app_id).map_err(|e| e.to_string())?;
        ops.opt_in_to_asset(asset_id).map_err(|e| e.to_string())?;

        // Create AlgoBingle helper and perform buy + register
        let bgl = AlgoBingle::new(ops.clone(), app_id, asset_id);
        // Determine current price and buy 1 unit
        let price = bgl.get_bingle_price(app_id).map_err(|e| e.to_string())?;
        let _tx1 = bgl.buy_bingle(app_id, asset_id, price).map_err(|e| e.to_string())?;
        let _tx2 = bgl.register(app_id, asset_id, &handle, 1).map_err(|e| e.to_string())?;
        Ok(true)
    }

    fn get_algo_ops(&self) -> Result<rust_comms::blockchain::algo_ops::AlgoOps, String> {
        // 1) Return cached instance if available
        {
            let guard = self.algo_ops.lock().map_err(|_| "mutex poisoned".to_string())?;
            if let Some(ops) = guard.as_ref() {
                return Ok(ops.clone());
            }
        }

        // 2) No cached instance; require an existing keypair (do NOT generate here)
        let pass = {
            let guard = self.keypair.lock().map_err(|_| "mutex poisoned".to_string())?;
            guard
                .as_ref()
                .map(|k| k.passphrase.clone())
                .ok_or_else(|| "no keypair".to_string())?
        };

        // 3) Construct and cache AlgoOps bound to this passphrase
        let ops = AlgoOps::new(Some(pass), None, Some(self.config.algo_config.clone()));
        let mut cache_guard = self.algo_ops.lock().map_err(|_| "mutex poisoned".to_string())?;
        *cache_guard = Some(ops.clone());
        Ok(ops)
    }

    fn add_contact(&mut self, _handle: String, _id: String, _source: ContactSource) -> Result<(), String> {
        Err("not implemented".to_string())
    }

    fn block_contact(&mut self, _id: String) -> Result<(), String> { Err("not implemented".to_string()) }

    fn remove_contact(&mut self, _id: String) -> Result<(), String> { Err("not implemented".to_string()) }

    fn is_blocked(&self, _id: &str) -> Result<bool, String> { Err("not implemented".to_string()) }

    fn get_contacts(&self) -> Result<Vec<Contact>, String> { Err("not implemented".to_string()) }

    fn add_message(
        &mut self,
        _sender_handle: String,
        _recipient_handles: Vec<String>,
        _timestamp: i64,
        _text: String,
    ) -> Result<(), String> {
        Err("not implemented".to_string())
    }

    fn get_messages(&self) -> Result<Vec<Message>, String> { Err("not implemented".to_string()) }
}
