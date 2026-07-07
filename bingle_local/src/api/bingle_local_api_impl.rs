use crate::api::{
    BingleLocalApi, Contact, ContactSource, Keypair, KeypairStatus, Message, REQUIRED_ALGO,
};
use bingle_core::api::bingle_api::BingleError;
use bingle_core::blockchain::algo_bingle::AlgoBingle;
use bingle_core::blockchain::algo_ops::{AlgoChainConfig, AlgoOps};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;

/// Configuration for the local API implementation.
/// Includes the blockchain provider configuration and required ids.
#[derive(Debug, Clone, Default)]
pub struct LocalApiConfig {
    pub algo_config: AlgoChainConfig,
    pub app_id: u64,
    pub asset_id: u64,
}

/// Decide the keypair status from resolved on-chain facts.
///
/// ACTIVE requires both the Bingle$ asset and a registered handle. When the
/// account holds the asset but has no handle, we fall through to the balance
/// check (FUNDED / UNFUNDED). `balance_algos` is only consulted when the
/// account is not ACTIVE.
pub fn keypair_status_from_facts(
    algorand_id: String,
    has_asset: bool,
    handle: Option<String>,
    balance_algos: f64,
) -> KeypairStatus {
    if has_asset && handle.is_some() {
        KeypairStatus {
            status: "ACTIVE".to_string(),
            id: Some(algorand_id),
            handle,
            required_algo: None,
        }
    } else if balance_algos >= REQUIRED_ALGO {
        KeypairStatus {
            status: "FUNDED".to_string(),
            id: Some(algorand_id),
            handle: None,
            required_algo: None,
        }
    } else {
        KeypairStatus {
            status: "UNFUNDED".to_string(),
            id: Some(algorand_id),
            handle: None,
            required_algo: Some(REQUIRED_ALGO),
        }
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
    fn generate_keypair(&mut self) -> Result<Keypair, BingleError> {
        tracing::info!("[BingleLocalApi] Generating new keypair");
        let (id, passphrase) = AlgoOps::generate_keypair();
        let kp = Keypair { id, passphrase };
        tracing::info!("[BingleLocalApi] Generated keypair with id: {}", kp.id);
        if let Ok(mut guard) = self.keypair.lock() {
            *guard = Some(kp.clone());
        }
        // Invalidate cached AlgoOps since keypair changed
        if let Ok(mut ops_guard) = self.algo_ops.lock() {
            *ops_guard = None;
        }
        Ok(kp)
    }

    fn register_keypair(&self, handle: String) -> Result<bool, BingleError> {
        tracing::info!(
            "[BingleLocalApi] Registering keypair with handle: {}",
            handle
        );
        // Validate config
        let app_id = self.config.app_id;
        let asset_id = self.config.asset_id;
        if app_id == 0 {
            return Err(BingleError::Other("app_id not set in config".to_string()));
        }
        if asset_id == 0 {
            return Err(BingleError::Other("asset_id not set in config".to_string()));
        }

        // Ensure we have blockchain ops bound to current keypair
        let ops = match self.get_algo_ops() {
            Ok(o) => o,
            Err(e) => {
                tracing::error!("[register_keypair] Failed to get AlgoOps: {}", e);
                return Err(e);
            }
        };

        // Execute on-chain steps
        if let Err(e) = ops.opt_in_app(app_id) {
            tracing::error!(
                "[register_keypair] Failed to opt in to app {}: {}",
                app_id,
                e
            );
            return Err(BingleError::from_anyhow(e));
        }
        if let Err(e) = ops.opt_in_to_asset(asset_id) {
            tracing::error!(
                "[register_keypair] Failed to opt in to asset {}: {}",
                asset_id,
                e
            );
            return Err(BingleError::from_anyhow(e));
        }

        // Create AlgoBingle helper and perform buy + register
        let bgl = AlgoBingle::new(ops.clone(), app_id, asset_id);
        // Determine current price and buy 1 unit
        let price = match bgl.get_bingle_price(app_id) {
            Ok(p) => p,
            Err(e) => {
                tracing::error!(
                    "[register_keypair] Failed to get Bingle price for app {}: {}",
                    app_id,
                    e
                );
                return Err(BingleError::from_anyhow(e));
            }
        };
        match bgl.buy_bingle(app_id, asset_id, price) {
            Ok(tx) => {
                let _ = tx;
            }
            Err(e) => {
                tracing::error!(
                    "[register_keypair] Failed to buy Bingle (app={}, asset={}, price={}): {}",
                    app_id,
                    asset_id,
                    price,
                    e
                );
                return Err(BingleError::from_anyhow(e));
            }
        }
        match bgl.register(app_id, asset_id, &handle, 1) {
            Ok(tx) => {
                let _ = tx;
            }
            Err(e) => {
                // This is a user action, they need to choose an unused handle probably
                tracing::info!(
                    "[register_keypair] Failed to register handle '{}' (app={}, asset={}): {}",
                    handle,
                    app_id,
                    asset_id,
                    e
                );
                return Err(BingleError::from_anyhow(e));
            }
        }
        tracing::info!(
            "[BingleLocalApi] Keypair registered successfully with handle: {}",
            handle
        );
        Ok(true)
    }

    fn ensure_local_migrated(&self) -> Result<Option<String>, BingleError> {
        let app_id = self.config.app_id;
        if app_id == 0 { return Err(BingleError::Other("app_id not set in config".to_string())); }
        let ops = self.get_algo_ops()?;
        let asset_id = self.config.asset_id;
        let bgl = AlgoBingle::new(ops, app_id, asset_id);
        match bgl.ensure_local_migrated(app_id) {
            Ok(Some(txid)) => {
                tracing::info!("[BingleLocalApi] migrated local state to app {} (tx {})", app_id, txid);
                Ok(Some(txid))
            }
            Ok(None) => {
                tracing::info!("[BingleLocalApi] no local-state migration needed for app {}", app_id);
                Ok(None)
            }
            Err(e) => {
                tracing::error!("[BingleLocalApi] local-state migration failed for app {}: {}", app_id, e);
                Err(BingleError::from_anyhow(e))
            }
        }
    }

    fn get_algo_ops(&self) -> Result<AlgoOps, BingleError> {
        // 1) Return cached instance if available
        {
            let guard = match self.algo_ops.lock() {
                Ok(g) => g,
                Err(e) => {
                    let msg = format!("mutex poisoned: {}", e);
                    tracing::error!("[get_algo_ops] Failed to lock algo_ops: {}", msg);
                    return Err(BingleError::Other(msg));
                }
            };
            if let Some(ops) = guard.as_ref() {
                tracing::debug!("[BingleLocalApi] get_algo_ops: returning cached instance");
                return Ok(ops.clone());
            }
        }

        // 2) No cached instance; require an existing keypair (do NOT generate here)
        let pass = {
            let guard = match self.keypair.lock() {
                Ok(g) => g,
                Err(e) => {
                    let msg = format!("mutex poisoned: {}", e);
                    tracing::error!("[get_algo_ops] Failed to lock keypair: {}", msg);
                    return Err(BingleError::Other(msg));
                }
            };
            match guard.as_ref().map(|k| k.passphrase.clone()) {
                Some(p) => p,
                None => {
                    tracing::error!("[get_algo_ops] No keypair available");
                    return Err(BingleError::Other("no keypair".to_string()));
                }
            }
        };

        // 3) Construct and cache AlgoOps bound to this passphrase
        tracing::debug!(
            "[BingleLocalApi] get_algo_ops: constructing new AlgoOps with config: \
            client_api_url={}, client_api_port={}, indexer_api_url={}, indexer_api_port={}, \
            token={}, token_key={}, app_id={:?}, asset_id={:?}",
            self.config.algo_config.client_api_url,
            self.config.algo_config.client_api_port,
            self.config.algo_config.indexer_api_url,
            self.config.algo_config.indexer_api_port,
            self.config.algo_config.token.as_deref().unwrap_or("<none>"),
            self.config
                .algo_config
                .token_key
                .as_deref()
                .unwrap_or("<none>"),
            self.config.algo_config.app_id,
            self.config.algo_config.asset_id,
        );
        let ops = AlgoOps::new(Some(pass), None, Some(self.config.algo_config.clone()));
        let mut cache_guard = match self.algo_ops.lock() {
            Ok(g) => g,
            Err(e) => {
                let msg = format!("mutex poisoned: {}", e);
                tracing::error!(
                    "[get_algo_ops] Failed to lock algo_ops for caching: {}",
                    msg
                );
                return Err(BingleError::Other(msg));
            }
        };
        *cache_guard = Some(ops.clone());
        Ok(ops)
    }

    fn add_contact(
        &mut self,
        handle: String,
        id: String,
        source: ContactSource,
    ) -> Result<(), BingleError> {
        tracing::info!(
            "[BingleLocalApi] Adding contact: handle={}, id={}, source={:?}",
            handle,
            id,
            source
        );
        // Validate inputs
        if handle.trim().is_empty() {
            return Err(BingleError::Other("handle cannot be empty".to_string()));
        }
        if id.trim().is_empty() {
            return Err(BingleError::Other("id cannot be empty".to_string()));
        }

        let mut map = match self.contacts.lock() {
            Ok(g) => g,
            Err(e) => {
                let msg = format!("mutex poisoned: {}", e);
                tracing::error!("[add_contact] Failed to lock contacts: {}", msg);
                return Err(BingleError::Other(msg));
            }
        };
        if map.contains_key(&id) {
            return Err(BingleError::Other("contact already exists".to_string()));
        }
        map.insert(id, (handle, source, false));
        Ok(())
    }

    fn block_contact(&mut self, id: String) -> Result<(), BingleError> {
        tracing::info!("[BingleLocalApi] Blocking contact: id={}", id);
        if id.trim().is_empty() {
            return Err(BingleError::Other("id cannot be empty".to_string()));
        }
        let mut map = match self.contacts.lock() {
            Ok(g) => g,
            Err(e) => {
                let msg = format!("mutex poisoned: {}", e);
                tracing::error!("[block_contact] Failed to lock contacts: {}", msg);
                return Err(BingleError::Other(msg));
            }
        };
        match map.get_mut(&id) {
            Some((_h, _s, blocked)) => {
                *blocked = true;
                Ok(())
            }
            None => Err(BingleError::Other("contact not found".to_string())),
        }
    }

    fn remove_contact(&mut self, id: String) -> Result<(), BingleError> {
        tracing::info!("[BingleLocalApi] Removing contact: id={}", id);
        if id.trim().is_empty() {
            return Err(BingleError::Other("id cannot be empty".to_string()));
        }
        let mut map = match self.contacts.lock() {
            Ok(g) => g,
            Err(e) => {
                let msg = format!("mutex poisoned: {}", e);
                tracing::error!("[remove_contact] Failed to lock contacts: {}", msg);
                return Err(BingleError::Other(msg));
            }
        };
        if map.remove(&id).is_some() {
            Ok(())
        } else {
            Err(BingleError::Other("contact not found".to_string()))
        }
    }

    fn is_blocked(&self, id: &str) -> Result<bool, BingleError> {
        if id.trim().is_empty() {
            return Err(BingleError::Other("id cannot be empty".to_string()));
        }
        let map = match self.contacts.lock() {
            Ok(g) => g,
            Err(e) => {
                let msg = format!("mutex poisoned: {}", e);
                tracing::error!("[is_blocked] Failed to lock contacts: {}", msg);
                return Err(BingleError::Other(msg));
            }
        };
        Ok(map.get(id).map(|(_, _, b)| *b).unwrap_or(false))
    }

    fn get_contacts(&self) -> Result<Vec<Contact>, BingleError> {
        let map = match self.contacts.lock() {
            Ok(g) => g,
            Err(e) => {
                let msg = format!("mutex poisoned: {}", e);
                tracing::error!("[get_contacts] Failed to lock contacts: {}", msg);
                return Err(BingleError::Other(msg));
            }
        };
        let mut out: Vec<Contact> = Vec::new();
        for (id, (handle, _source, blocked)) in map.iter() {
            if !*blocked {
                out.push(Contact {
                    handle: handle.clone(),
                    id: id.clone(),
                    fields: HashMap::new(),
                });
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
        cipher_suite: Option<String>,
    ) -> Result<(), BingleError> {
        tracing::debug!(
            "[BingleLocalApi] Adding message from: {} to: {:?}",
            sender_handle,
            recipient_handles
        );
        // Basic input validation
        if sender_handle.trim().is_empty() {
            return Err(BingleError::Other(
                "sender_handle cannot be empty".to_string(),
            ));
        }
        if recipient_handles.is_empty() {
            return Err(BingleError::Other(
                "recipient_handles cannot be empty".to_string(),
            ));
        }
        if recipient_handles.iter().any(|h| h.trim().is_empty()) {
            return Err(BingleError::Other(
                "recipient_handles cannot contain empty handles".to_string(),
            ));
        }
        if text.trim().is_empty() {
            return Err(BingleError::Other("text cannot be empty".to_string()));
        }

        let msg = Message {
            sender_handle,
            recipient_handles,
            timestamp,
            text,
            cipher_suite,
            progress: Some(1.0),
            failure_reason: None,
        };
        let mut guard = match self.messages.lock() {
            Ok(g) => g,
            Err(e) => {
                let msg = format!("mutex poisoned: {}", e);
                tracing::error!("[add_message] Failed to lock messages: {}", msg);
                return Err(BingleError::Other(msg));
            }
        };
        guard.push(msg);
        Ok(())
    }

    fn queue_message(
        &mut self,
        recipient_handles: Vec<String>,
        text: String,
    ) -> Result<(), BingleError> {
        tracing::debug!(
            "[BingleLocalApi] Queuing message to: {:?}",
            recipient_handles
        );
        let status = self.keypair_status()?;
        let sender_handle = status.handle.ok_or_else(|| {
            BingleError::Other("No handle registered for current keypair".to_string())
        })?;

        if recipient_handles.is_empty() {
            return Err(BingleError::Other(
                "recipient_handles cannot be empty".to_string(),
            ));
        }
        if recipient_handles.iter().any(|h| h.trim().is_empty()) {
            return Err(BingleError::Other(
                "recipient_handles cannot contain empty handles".to_string(),
            ));
        }
        if text.trim().is_empty() {
            return Err(BingleError::Other("text cannot be empty".to_string()));
        }

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| BingleError::Other(e.to_string()))?
            .as_millis() as i64;

        let msg = Message {
            sender_handle,
            recipient_handles,
            timestamp,
            text,
            cipher_suite: None,
            progress: Some(0.0),
            failure_reason: None,
        };

        let mut guard = match self.messages.lock() {
            Ok(g) => g,
            Err(e) => {
                let msg = format!("mutex poisoned: {}", e);
                tracing::error!("[queue_message] Failed to lock messages: {}", msg);
                return Err(BingleError::Other(msg));
            }
        };
        guard.push(msg);
        Ok(())
    }

    fn update_message_status(
        &mut self,
        timestamp: i64,
        progress: f32,
        failure_reason: Option<String>,
    ) -> Result<(), BingleError> {
        let mut guard = match self.messages.lock() {
            Ok(g) => g,
            Err(e) => {
                let msg = format!("mutex poisoned: {}", e);
                tracing::error!("[update_message_status] Failed to lock messages: {}", msg);
                return Err(BingleError::Other(msg));
            }
        };

        if let Some(msg) = guard.iter_mut().find(|m| m.timestamp == timestamp) {
            msg.progress = Some(progress);
            if failure_reason.is_some() || progress >= 1.0 {
                msg.failure_reason = failure_reason;
            }
            Ok(())
        } else {
            Err(BingleError::Other(format!(
                "Message with timestamp {} not found",
                timestamp
            )))
        }
    }

    fn get_pending_messages(&self) -> Result<Vec<Message>, BingleError> {
        let guard = match self.messages.lock() {
            Ok(g) => g,
            Err(e) => {
                let msg = format!("mutex poisoned: {}", e);
                tracing::error!("[get_pending_messages] Failed to lock messages: {}", msg);
                return Err(BingleError::Other(msg));
            }
        };
        Ok(guard
            .iter()
            .filter(|m| m.progress.is_some_and(|p| p < 1.0))
            .cloned()
            .collect())
    }

    fn get_messages(&self) -> Result<Vec<Message>, BingleError> {
        let guard = match self.messages.lock() {
            Ok(g) => g,
            Err(e) => {
                let msg = format!("mutex poisoned: {}", e);
                tracing::error!("[get_messages] Failed to lock messages: {}", msg);
                return Err(BingleError::Other(msg));
            }
        };
        Ok(guard.clone())
    }

    fn save(&self, path: &str) -> Result<(), BingleError> {
        tracing::info!("[BingleLocalApi] Saving state to: {}", path);
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
            let g = match self.keypair.lock() {
                Ok(g) => g,
                Err(e) => {
                    let msg = format!("mutex poisoned: {}", e);
                    tracing::error!("[save] Failed to lock keypair: {}", msg);
                    return Err(BingleError::Other(msg));
                }
            };
            g.clone()
        };
        let contacts_vec: Vec<ContactEntry> = {
            let map = match self.contacts.lock() {
                Ok(g) => g,
                Err(e) => {
                    let msg = format!("mutex poisoned: {}", e);
                    tracing::error!("[save] Failed to lock contacts: {}", msg);
                    return Err(BingleError::Other(msg));
                }
            };
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
            let g = match self.messages.lock() {
                Ok(g) => g,
                Err(e) => {
                    let msg = format!("mutex poisoned: {}", e);
                    tracing::error!("[save] Failed to lock messages: {}", msg);
                    return Err(BingleError::Other(msg));
                }
            };
            g.clone()
        };

        let state = LocalState {
            keypair,
            contacts: contacts_vec,
            messages,
        };

        // Ensure parent directory exists
        if let Some(parent) = std::path::Path::new(path).parent()
            && !parent.as_os_str().is_empty()
            && !parent.is_dir()
            && let Err(e) = std::fs::create_dir_all(parent)
        {
            let msg = format!(
                "Failed to create parent directory '{}' for '{}': {}",
                parent.display(),
                path,
                e
            );
            tracing::error!("[save] {}", msg);
            return Err(BingleError::Other(msg));
        }

        let file = match std::fs::File::create(path) {
            Ok(f) => f,
            Err(e) => {
                let msg = e.to_string();
                tracing::error!("[save] Failed to create file '{}': {}", path, msg);
                return Err(BingleError::Other(msg));
            }
        };
        if let Err(e) = serde_json::to_writer_pretty(file, &state) {
            let msg = e.to_string();
            tracing::error!("[save] Failed to write JSON to '{}': {}", path, msg);
            return Err(BingleError::Other(msg));
        }
        tracing::info!("[BingleLocalApi] State saved successfully to: {}", path);
        Ok(())
    }

    fn load(&mut self, path: &str) -> Result<(), BingleError> {
        tracing::info!("[BingleLocalApi] Loading state from: {}", path);
        #[derive(Debug, Clone, Serialize, Deserialize)]
        struct ContactEntry {
            id: String,
            handle: String,
            source: ContactSource,
            is_blocked: bool,
        }
        #[derive(Debug, Clone, Serialize, Deserialize)]
        struct LocalState {
            #[serde(default)]
            keypair: Option<Keypair>,
            #[serde(default)]
            contacts: Vec<ContactEntry>,
            #[serde(default)]
            messages: Vec<Message>,
        }

        let file = match std::fs::File::open(path) {
            Ok(f) => f,
            Err(e) => {
                let msg = e.to_string();
                tracing::error!("[load] Failed to open file '{}': {}", path, msg);
                return Err(BingleError::Other(msg));
            }
        };
        let state: LocalState = match serde_json::from_reader(file) {
            Ok(s) => s,
            Err(e) => {
                let msg = e.to_string();
                tracing::error!("[load] Failed to parse JSON from '{}': {}", path, msg);
                return Err(BingleError::Other(msg));
            }
        };

        // Replace current state under locks
        if let Ok(mut k) = self.keypair.lock() {
            *k = state.keypair.clone();
        } else {
            tracing::error!("[load] Failed to lock keypair: mutex poisoned");
            return Err(BingleError::Other("mutex poisoned".to_string()));
        }
        // Invalidate cached AlgoOps since keypair may have changed
        if let Ok(mut ops_guard) = self.algo_ops.lock() {
            *ops_guard = None;
        } else {
            tracing::error!("[load] Failed to lock algo_ops: mutex poisoned");
            return Err(BingleError::Other("mutex poisoned".to_string()));
        }
        if let Ok(mut map) = self.contacts.lock() {
            map.clear();
            for ce in state.contacts.into_iter() {
                map.insert(ce.id, (ce.handle, ce.source, ce.is_blocked));
            }
        } else {
            tracing::error!("[load] Failed to lock contacts: mutex poisoned");
            return Err(BingleError::Other("mutex poisoned".to_string()));
        }
        if let Ok(mut msgs) = self.messages.lock() {
            *msgs = state.messages;
        } else {
            tracing::error!("[load] Failed to lock messages: mutex poisoned");
            return Err(BingleError::Other("mutex poisoned".to_string()));
        }

        tracing::info!("[BingleLocalApi] State loaded successfully from: {}", path);
        Ok(())
    }

    fn keypair_status(&self) -> Result<KeypairStatus, BingleError> {
        tracing::info!("[BingleLocalApi][keypair_status] Checking keypair status");
        // 1) Check if keypair exists
        let kp = {
            let guard = match self.keypair.lock() {
                Ok(g) => g,
                Err(e) => {
                    let msg = format!("mutex poisoned: {}", e);
                    tracing::error!(
                        "[BingleLocalApi][keypair_status] Failed to lock keypair: {}",
                        msg
                    );
                    return Err(BingleError::Other(msg));
                }
            };
            match guard.as_ref() {
                Some(k) => k.clone(),
                None => {
                    tracing::info!(
                        "[BingleLocalApi][keypair_status] Keypair status: None (no keypair)"
                    );
                    return Ok(KeypairStatus {
                        status: "None".to_string(),
                        id: None,
                        handle: None,
                        required_algo: None,
                    });
                }
            }
        };

        let algorand_id = kp.id.clone();

        // Superseded-app gate: if the configured app has been marked superseded (a newer app
        // replaced it), the client must upgrade. Report UPGRADE_REQUIRED and short-circuit
        // ahead of the funding/registration checks — the whole app is retired regardless of
        // this account's funding or handle. Best-effort: a read error falls through to the
        // normal status rather than blocking the user.
        let configured_app_id = self.config.app_id;
        if configured_app_id > 0 {
            match self.get_algo_ops() {
                Ok(ops) => {
                    let bgl = AlgoBingle::new(ops, configured_app_id, self.config.asset_id);
                    match bgl.successor_app(configured_app_id) {
                        Ok(Some(successor)) => {
                            tracing::info!("[BingleLocalApi][keypair_status] app {} superseded by {}; UPGRADE_REQUIRED", configured_app_id, successor);
                            return Ok(KeypairStatus {
                                status: "UPGRADE_REQUIRED".to_string(),
                                id: Some(algorand_id),
                                handle: None,
                                required_algo: None,
                            });
                        }
                        Ok(None) => {}
                        Err(e) => tracing::warn!("[BingleLocalApi][keypair_status] successor check failed (continuing): {}", e),
                    }
                }
                Err(e) => tracing::warn!("[BingleLocalApi][keypair_status] get_algo_ops for successor check failed (continuing): {}", e),
            }
        }

        // 2) Get AlgoOps for blockchain queries
        let ops = match self.get_algo_ops() {
            Ok(o) => o,
            Err(e) => {
                tracing::error!(
                    "[BingleLocalApi][keypair_status] Failed to get AlgoOps: {}",
                    e
                );
                return Err(e);
            }
        };

        // 3) Check if the account has opted in to the Bingle$ asset
        let asset_id = self.config.asset_id;
        let has_asset = if asset_id > 0 {
            match ops.is_account_opted_in_to_asset(&algorand_id, asset_id) {
                Ok(v) => v,
                Err(e) => {
                    tracing::error!(
                        "[BingleLocalApi][keypair_status] Failed to check asset opt-in for {} (asset {}): {}",
                        algorand_id,
                        asset_id,
                        e
                    );
                    return Err(BingleError::from_anyhow(e));
                }
            }
        } else {
            false
        };

        // If the account holds the Bingle$ asset, look up its handle from on-chain local state
        let handle = if has_asset {
            let app_id = self.config.app_id;
            if app_id > 0 {
                let local_state = match ops.local_state_for_account(app_id, &algorand_id) {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::error!(
                            "[BingleLocalApi][keypair_status] Failed to get local state for {} (app {}): {}",
                            algorand_id,
                            app_id,
                            e
                        );
                        return Err(BingleError::from_anyhow(e));
                    }
                };
                local_state.and_then(|entries| {
                    entries
                        .into_iter()
                        .find(|(k, _)| k == "Handle")
                        .map(|(_, v)| v)
                })
            } else {
                None
            }
        } else {
            None
        };

        // ACTIVE requires both the Bingle$ asset and a registered handle. If the
        // account holds the asset but has no Handle entry, fall through to the
        // balance check rather than reporting ACTIVE.
        if has_asset && handle.is_none() {
            tracing::debug!(
                "[BingleLocalApi][keypair_status] Account {} holds Bingle$ asset but no Handle entry found; falling through to balance check",
                algorand_id
            );
        }

        // Balance is only needed when the account is not ACTIVE; skip the query otherwise.
        let balance_algos = if has_asset && handle.is_some() {
            0.0
        } else {
            let balance = match ops.account_balance() {
                Ok(b) => b,
                Err(e) => {
                    tracing::error!(
                        "[BingleLocalApi][keypair_status] Failed to get account balance: {}",
                        e
                    );
                    return Err(BingleError::from_anyhow(e));
                }
            };
            let balance_algos = balance.unwrap_or(0.0);
            tracing::info!(
                "[BingleLocalApi][keypair_status] Balance: {} ALGOs (raw: {:?})",
                balance_algos,
                balance
            );
            balance_algos
        };

        Ok(keypair_status_from_facts(
            algorand_id,
            has_asset,
            handle,
            balance_algos,
        ))
    }

    fn get_keypair(&self) -> Result<Option<Keypair>, BingleError> {
        let guard = match self.keypair.lock() {
            Ok(g) => g,
            Err(e) => {
                let msg = format!("mutex poisoned: {}", e);
                tracing::error!("[get_keypair] Failed to lock keypair: {}", msg);
                return Err(BingleError::Other(msg));
            }
        };
        Ok(guard.clone())
    }
}
