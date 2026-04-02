use crate::api::{BingleLocalApi, Contact, ContactSource, Keypair, Message};
use rust_comms::blockchain::algo_ops::{AlgoChainConfig, AlgoOps};
use rust_comms::blockchain::algo_bingle::AlgoBingle;
use std::collections::HashMap;
use std::sync::Mutex;
use serde::{Deserialize, Serialize};

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
    // Contacts storage: id => (handle, source, is_blocked)
    contacts: Mutex<HashMap<String, (String, ContactSource, bool)>>,
    // Messages storage: append-only log of messages
    messages: Mutex<Vec<Message>>,
}

impl BingleApiLocalImpl {
    pub fn new(config: LocalApiConfig) -> Self {
        Self {
            keypair: Mutex::new(None),
            algo_ops: Mutex::new(None),
            config,
            contacts: Mutex::new(HashMap::new()),
            messages: Mutex::new(Vec::new()),
        }
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

    fn add_contact(&mut self, handle: String, id: String, source: ContactSource) -> Result<(), String> {
        // Validate inputs
        if handle.trim().is_empty() { return Err("handle cannot be empty".to_string()); }
        if id.trim().is_empty() { return Err("id cannot be empty".to_string()); }

        let mut map = self.contacts.lock().map_err(|_| "mutex poisoned".to_string())?;
        if map.contains_key(&id) {
            return Err("contact already exists".to_string());
        }
        map.insert(id, (handle, source, false));
        Ok(())
    }

    fn block_contact(&mut self, id: String) -> Result<(), String> {
        if id.trim().is_empty() { return Err("id cannot be empty".to_string()); }
        let mut map = self.contacts.lock().map_err(|_| "mutex poisoned".to_string())?;
        match map.get_mut(&id) {
            Some((_h, _s, blocked)) => { *blocked = true; Ok(()) }
            None => Err("contact not found".to_string()),
        }
    }

    fn remove_contact(&mut self, id: String) -> Result<(), String> {
        if id.trim().is_empty() { return Err("id cannot be empty".to_string()); }
        let mut map = self.contacts.lock().map_err(|_| "mutex poisoned".to_string())?;
        if map.remove(&id).is_some() { Ok(()) } else { Err("contact not found".to_string()) }
    }

    fn is_blocked(&self, id: &str) -> Result<bool, String> {
        if id.trim().is_empty() { return Err("id cannot be empty".to_string()); }
        let map = self.contacts.lock().map_err(|_| "mutex poisoned".to_string())?;
        Ok(map.get(id).map(|(_, _, b)| *b).unwrap_or(false))
    }

    fn get_contacts(&self) -> Result<Vec<Contact>, String> {
        let map = self.contacts.lock().map_err(|_| "mutex poisoned".to_string())?;
        let mut out: Vec<Contact> = Vec::new();
        for (id, (handle, _source, blocked)) in map.iter() {
            if !*blocked {
                out.push(Contact { handle: handle.clone(), id: id.clone(), fields: HashMap::new() });
            }
        }
        Ok(out)
    }

    fn add_message(
        &mut self,
        sender_handle: String,
        recipient_handles: Vec<String>,
        timestamp: i64,
        text: String,
    ) -> Result<(), String> {
        // Basic input validation
        if sender_handle.trim().is_empty() { return Err("sender_handle cannot be empty".to_string()); }
        if recipient_handles.is_empty() { return Err("recipient_handles cannot be empty".to_string()); }
        if recipient_handles.iter().any(|h| h.trim().is_empty()) { return Err("recipient_handles cannot contain empty handles".to_string()); }
        if text.trim().is_empty() { return Err("text cannot be empty".to_string()); }

        let msg = Message { sender_handle, recipient_handles, timestamp, text };
        let mut guard = self.messages.lock().map_err(|_| "mutex poisoned".to_string())?;
        guard.push(msg);
        Ok(())
    }

    fn get_messages(&self) -> Result<Vec<Message>, String> {
        let guard = self.messages.lock().map_err(|_| "mutex poisoned".to_string())?;
        Ok(guard.clone())
    }

    fn save(&self, path: &str) -> Result<(), String> {
        // Build serializable snapshot
        #[derive(Debug, Clone, Serialize, Deserialize)]
        struct ContactEntry {
            id: String,
            handle: String,
            source: ContactSource,
            is_blocked: bool,
        }
        #[derive(Debug, Clone, Serialize, Deserialize)]
        struct LocalState {
            keypair: Option<Keypair>,
            contacts: Vec<ContactEntry>,
            messages: Vec<Message>,
        }

        // Snapshot under locks (avoid holding multiple locks longer than needed)
        let keypair = {
            let g = self.keypair.lock().map_err(|_| "mutex poisoned".to_string())?;
            g.clone()
        };
        let contacts_vec: Vec<ContactEntry> = {
            let map = self.contacts.lock().map_err(|_| "mutex poisoned".to_string())?;
            map.iter()
                .map(|(id, (handle, source, blocked))| ContactEntry {
                    id: id.clone(),
                    handle: handle.clone(),
                    source: source.clone(),
                    is_blocked: *blocked,
                })
                .collect()
        };
        let messages = {
            let g = self.messages.lock().map_err(|_| "mutex poisoned".to_string())?;
            g.clone()
        };

        let state = LocalState { keypair, contacts: contacts_vec, messages };

        // Ensure parent directory exists
        if let Some(parent) = std::path::Path::new(path).parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
        }

        let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
        serde_json::to_writer_pretty(file, &state).map_err(|e| e.to_string())
    }

    fn load(&mut self, path: &str) -> Result<(), String> {
        #[derive(Debug, Clone, Serialize, Deserialize)]
        struct ContactEntry {
            id: String,
            handle: String,
            source: ContactSource,
            is_blocked: bool,
        }
        #[derive(Debug, Clone, Serialize, Deserialize)]
        struct LocalState {
            keypair: Option<Keypair>,
            contacts: Vec<ContactEntry>,
            messages: Vec<Message>,
        }

        let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
        let state: LocalState = serde_json::from_reader(file).map_err(|e| e.to_string())?;

        // Replace current state under locks
        if let Ok(mut k) = self.keypair.lock() {
            *k = state.keypair.clone();
        } else {
            return Err("mutex poisoned".to_string());
        }
        // Invalidate cached AlgoOps since keypair may have changed
        if let Ok(mut ops_guard) = self.algo_ops.lock() {
            *ops_guard = None;
        } else {
            return Err("mutex poisoned".to_string());
        }
        if let Ok(mut map) = self.contacts.lock() {
            map.clear();
            for ce in state.contacts.into_iter() {
                map.insert(ce.id, (ce.handle, ce.source, ce.is_blocked));
            }
        } else {
            return Err("mutex poisoned".to_string());
        }
        if let Ok(mut msgs) = self.messages.lock() {
            *msgs = state.messages;
        } else {
            return Err("mutex poisoned".to_string());
        }

        Ok(())
    }
}
