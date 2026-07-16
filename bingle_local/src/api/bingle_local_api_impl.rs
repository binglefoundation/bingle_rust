use crate::api::{
    BingleLocalApi, ChainRegistrationOps, Contact, ContactSource, Keypair, KeypairStatus, Message,
    REQUIRED_ALGO, run_registration,
};
use bingle_core::api::bingle_api::BingleError;
use bingle_core::blockchain::algo_bingle::AlgoBingle;
use bingle_core::blockchain::algo_ops::{AlgoChainConfig, AlgoOps};
use bingle_core::blockchain::error::AlgoErrorKind;
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
///
/// `required_target_algos` is the balance considered adequate to register: the
/// live cost from [`required_funding_algos`] when it can be read, or `REQUIRED_ALGO`
/// as a fallback. For UNFUNDED, `required_algo` is the **top-up** to reach that
/// target (`required_target_algos - balance_algos`), so a semi-funded account is
/// only asked for the shortfall (issue #15, A3/A3b).
pub fn keypair_status_from_facts(
    algorand_id: String,
    has_asset: bool,
    handle: Option<String>,
    balance_algos: f64,
    required_target_algos: f64,
) -> KeypairStatus {
    if has_asset && handle.is_some() {
        KeypairStatus {
            status: "ACTIVE".to_string(),
            id: Some(algorand_id),
            handle,
            required_algo: None,
            stale: false,
        }
    } else if balance_algos >= required_target_algos {
        KeypairStatus {
            status: "FUNDED".to_string(),
            id: Some(algorand_id),
            handle: None,
            required_algo: None,
            stale: false,
        }
    } else {
        // Ask only for the shortfall to reach the target. Clamped at 0 defensively; in this
        // branch balance_algos < required_target_algos so the difference is already positive.
        let top_up = (required_target_algos - balance_algos).max(0.0);
        KeypairStatus {
            status: "UNFUNDED".to_string(),
            id: Some(algorand_id),
            handle: None,
            required_algo: Some(top_up),
            stale: false,
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
    // Cache of the account's registered handle, set whenever a status resolves one. Lets
    // offline operations (queue_message) obtain the sender handle without a live blockchain
    // read once the account is registered (issue #18, A1).
    own_handle: Mutex<Option<String>>,
    // Last successfully computed status, returned when a later read finds the blockchain
    // unreachable so an already-known account stays usable during an outage (issue #18, A2).
    last_status: Mutex<Option<KeypairStatus>>,
    // Cached result of the last network_available() probe with the time it was taken, so the
    // send hot-path does not hit the Algorand node on every message (issue #31).
    last_network_check: Mutex<Option<(bool, std::time::Instant)>>,
}

/// How long a `network_available` probe result is reused before re-probing.
const NETWORK_CHECK_TTL: std::time::Duration = std::time::Duration::from_secs(5);

impl BingleApiLocalImpl {
    pub fn new(config: LocalApiConfig) -> Self {
        Self {
            keypair: Mutex::new(None),
            algo_ops: Mutex::new(None),
            config,
            contacts: Mutex::new(HashMap::new()),
            messages: Mutex::new(Vec::new()),
            own_handle: Mutex::new(None),
            last_status: Mutex::new(None),
            last_network_check: Mutex::new(None),
        }
    }

    /// On a blockchain-unreachable error, return the last-known status so an already-known
    /// account stays usable during a transient outage (issue #18, A2); otherwise propagate the
    /// error (e.g. a genuine first run with no cache still reports NoBlockchain upstream).
    ///
    /// `context` describes the read that failed. A host-unreachable outage is expected and handled
    /// here, so it logs at debug to avoid flooding the log while the node is unreachable (a UI that
    /// polls keypair_status would otherwise emit an error per read); any other error logs at error.
    fn fallback_status(
        &self,
        context: &str,
        err: BingleError,
    ) -> Result<KeypairStatus, BingleError> {
        if algo_host_unreachable(&err) {
            tracing::debug!(
                "[BingleLocalApi][keypair_status] {} (blockchain unreachable): {}",
                context,
                err
            );
        } else {
            tracing::error!("[BingleLocalApi][keypair_status] {}: {}", context, err);
        }
        let cached = self.last_status.lock().ok().and_then(|g| g.clone());
        match status_or_last_known(err, cached) {
            // Ok is only produced by the fallback (unreachable + a cached status present), and
            // status_or_last_known has already flagged it stale.
            Ok(status) => {
                tracing::warn!(
                    "[BingleLocalApi][keypair_status] blockchain unreachable; returning last-known status '{}'",
                    status.status
                );
                Ok(status)
            }
            Err(e) => Err(e),
        }
    }

    /// Record a freshly resolved status so later calls can survive a blockchain outage: caches
    /// the whole status (A2) and, when a handle is present, the registered handle (A1).
    fn cache_status(&self, status: &KeypairStatus) {
        if let Some(handle) = status.handle.as_ref()
            && let Ok(mut guard) = self.own_handle.lock()
        {
            *guard = Some(handle.clone());
        }
        if let Ok(mut guard) = self.last_status.lock() {
            *guard = Some(status.clone());
        }
    }

    /// The account's registered handle for outbound messages. Uses the cache populated by
    /// keypair_status so queuing a send needs no live blockchain read once the account is
    /// registered (issue #18, A1); falls back to a fresh status read only when nothing is cached.
    fn sender_handle(&self) -> Result<String, BingleError> {
        let cached = self.own_handle.lock().ok().and_then(|g| g.clone());
        resolve_sender_handle(cached, || self.keypair_status())
    }
}

/// Resolve the sender handle for an outbound message (issue #18, A1).
///
/// Returns the cached handle without invoking `fetch_status` when one is known, so queuing a
/// send never needs a live blockchain read once the account is registered. Only a genuine first
/// run (no cache) falls back to a fresh status read.
pub fn resolve_sender_handle<F>(
    cached_handle: Option<String>,
    fetch_status: F,
) -> Result<String, BingleError>
where
    F: FnOnce() -> Result<KeypairStatus, BingleError>,
{
    if let Some(handle) = cached_handle {
        return Ok(handle);
    }
    let status = fetch_status()?;
    status
        .handle
        .ok_or_else(|| BingleError::Other("No handle registered for current keypair".to_string()))
}

/// Whether a `BingleError` indicates the blockchain host was unreachable (a transient network
/// outage) as opposed to a genuine failure. Used to decide when to fall back to cached state.
pub fn algo_host_unreachable(err: &BingleError) -> bool {
    matches!(err, BingleError::Algo(ae) if ae.kind == AlgoErrorKind::HostUnreachable)
}

/// Decide a status result under a possible blockchain outage (issue #18, A2).
///
/// Returns the `cached` status only when the failure is a host-unreachable outage and a cached
/// status exists; any other error (or a genuine first run with no cache) propagates so callers
/// still see NoBlockchain. The returned cached status is flagged `stale` so the UI can show it as
/// unavailable rather than a freshly confirmed read (issue #31).
pub fn status_or_last_known(
    err: BingleError,
    cached: Option<KeypairStatus>,
) -> Result<KeypairStatus, BingleError> {
    if algo_host_unreachable(&err)
        && let Some(mut cached) = cached
    {
        cached.stale = true;
        return Ok(cached);
    }
    Err(err)
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
        // A brand-new keypair is not yet ACTIVE: clear the memoized ACTIVE handle/status so
        // keypair_status re-resolves from chain until this account registers.
        if let Ok(mut g) = self.own_handle.lock() {
            *g = None;
        }
        if let Ok(mut g) = self.last_status.lock() {
            *g = None;
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

        // Drive the on-chain steps through the registration seam so the sequence is
        // unit-testable (issue #15, A4). Behaviour is unchanged from the previous inline
        // opt-in/buy/register flow.
        let chain_ops = ChainRegistrationOps::new(ops, app_id, asset_id);
        run_registration(&chain_ops, &handle)?;
        tracing::info!(
            "[BingleLocalApi] Keypair registered successfully with handle: {}",
            handle
        );
        // The account is now ACTIVE with this handle, permanently registered to the id. Memoize
        // it so keypair_status serves ACTIVE with no further blockchain read.
        if let Ok(mut g) = self.own_handle.lock() {
            *g = Some(handle.clone());
        }
        Ok(true)
    }

    fn ensure_local_migrated(&self) -> Result<Option<String>, BingleError> {
        let app_id = self.config.app_id;
        if app_id == 0 {
            return Err(BingleError::Other("app_id not set in config".to_string()));
        }
        let ops = self.get_algo_ops()?;
        let asset_id = self.config.asset_id;
        let bgl = AlgoBingle::new(ops, app_id, asset_id);
        match bgl.ensure_local_migrated(app_id) {
            Ok(Some(txid)) => {
                tracing::info!(
                    "[BingleLocalApi] migrated local state to app {} (tx {})",
                    app_id,
                    txid
                );
                Ok(Some(txid))
            }
            Ok(None) => {
                tracing::info!(
                    "[BingleLocalApi] no local-state migration needed for app {}",
                    app_id
                );
                Ok(None)
            }
            Err(e) => {
                tracing::error!(
                    "[BingleLocalApi] local-state migration failed for app {}: {}",
                    app_id,
                    e
                );
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
                    // Expected, not configured yet
                    tracing::info!("[get_algo_ops] No keypair available");
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
        // Use the cached registered handle so queuing works offline once the account is
        // registered; only a genuine first run (empty cache) needs a live status read.
        let sender_handle = self.sender_handle()?;

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
            // The registered handle, memoized once ACTIVE so status survives restart with no
            // blockchain read (issue #18/#31). Only set when the account is ACTIVE.
            #[serde(default)]
            own_handle: Option<String>,
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

        let own_handle = self.own_handle.lock().ok().and_then(|g| g.clone());

        let state = LocalState {
            keypair,
            contacts: contacts_vec,
            messages,
            own_handle,
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
            #[serde(default)]
            own_handle: Option<String>,
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
        // Restore the memoized ACTIVE handle so keypair_status serves ACTIVE with no blockchain
        // read after a restart. The in-memory last_status cache does not persist; drop it so the
        // reloaded account re-resolves cleanly.
        if let Ok(mut oh) = self.own_handle.lock() {
            *oh = state.own_handle;
        }
        if let Ok(mut ls) = self.last_status.lock() {
            *ls = None;
        }

        tracing::info!("[BingleLocalApi] State loaded successfully from: {}", path);
        Ok(())
    }

    fn network_available(&self, force_recheck: bool) -> Result<bool, BingleError> {
        // Serve a recent cached result unless a fresh probe is explicitly requested.
        if !force_recheck
            && let Ok(guard) = self.last_network_check.lock()
            && let Some((available, at)) = *guard
            && at.elapsed() < NETWORK_CHECK_TTL
        {
            return Ok(available);
        }

        // Probe the node with an account_balance read (the same check start uses). Without a
        // keypair (or if ops can't be built) there is nothing to send/register, so report the
        // network as unavailable rather than erroring — the caller should not attempt to send.
        let ops = match self.get_algo_ops() {
            Ok(ops) => ops,
            Err(e) => {
                tracing::debug!(
                    "[BingleLocalApi][network_available] no algo ops ({}); reporting unavailable",
                    e
                );
                if let Ok(mut guard) = self.last_network_check.lock() {
                    *guard = Some((false, std::time::Instant::now()));
                }
                return Ok(false);
            }
        };
        let available = match ops.account_balance() {
            // The node responded (even a non-network error means it was reachable).
            Ok(_) => true,
            Err(e) => !algo_host_unreachable(&BingleError::from_anyhow(e)),
        };

        if let Ok(mut guard) = self.last_network_check.lock() {
            *guard = Some((available, std::time::Instant::now()));
        }
        tracing::debug!(
            "[BingleLocalApi][network_available] available={} (force_recheck={})",
            available,
            force_recheck
        );
        Ok(available)
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
                        stale: false,
                    });
                }
            }
        };

        let algorand_id = kp.id.clone();

        // Once the account is ACTIVE the handle is permanently registered to this address, so there
        // is nothing left to verify on-chain. Serve the memoized handle with no blockchain read:
        // this eliminates the steady keypair-status polling once registered and stops a node
        // outage from blocking or delaying status (issue #18/#31). The memo is set on registration
        // / the first live ACTIVE read and persisted across restarts; it is cleared when the
        // keypair changes.
        if let Some(handle) = self.own_handle.lock().ok().and_then(|g| g.clone()) {
            tracing::debug!(
                "[BingleLocalApi][keypair_status] ACTIVE (memoized handle '{}'); skipping blockchain",
                handle
            );
            return Ok(KeypairStatus {
                status: "ACTIVE".to_string(),
                id: Some(algorand_id),
                handle: Some(handle),
                required_algo: None,
                stale: false,
            });
        }

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
                            tracing::info!(
                                "[BingleLocalApi][keypair_status] app {} superseded by {}; UPGRADE_REQUIRED",
                                configured_app_id,
                                successor
                            );
                            return Ok(KeypairStatus {
                                status: "UPGRADE_REQUIRED".to_string(),
                                id: Some(algorand_id),
                                handle: None,
                                required_algo: None,
                                stale: false,
                            });
                        }
                        Ok(None) => {}
                        Err(e) => {
                            // Best-effort: a read failure never blocks status. A host-unreachable
                            // outage is expected and logs at debug so a polling UI does not flood
                            // the log; any other failure logs at warn.
                            let e = BingleError::from_anyhow(e);
                            if algo_host_unreachable(&e) {
                                tracing::debug!(
                                    "[BingleLocalApi][keypair_status] successor check unreachable (continuing): {}",
                                    e
                                );
                            } else {
                                tracing::warn!(
                                    "[BingleLocalApi][keypair_status] successor check failed (continuing): {}",
                                    e
                                );
                            }
                        }
                    }
                }
                Err(e) => tracing::warn!(
                    "[BingleLocalApi][keypair_status] get_algo_ops for successor check failed (continuing): {}",
                    e
                ),
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
                    return self.fallback_status(
                        &format!(
                            "Failed to check asset opt-in for {} (asset {})",
                            algorand_id, asset_id
                        ),
                        BingleError::from_anyhow(e),
                    );
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
                        return self.fallback_status(
                            &format!(
                                "Failed to get local state for {} (app {})",
                                algorand_id, app_id
                            ),
                            BingleError::from_anyhow(e),
                        );
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
                    return self.fallback_status(
                        "Failed to get account balance",
                        BingleError::from_anyhow(e),
                    );
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

        // Derive the adequate-funding target from live cost (AlgoBingle::required_funding reads
        // the Bingle$ price + the app's local-schema MBR + fees), falling back to the flat
        // REQUIRED_ALGO if the chain read fails so a transient hiccup never blocks the status
        // (issue #15, A3b). Only needed when not ACTIVE; skip the read otherwise.
        let required_target_algos = if has_asset && handle.is_some() {
            REQUIRED_ALGO
        } else {
            let bgl = AlgoBingle::new(ops.clone(), self.config.app_id, self.config.asset_id);
            match bgl.required_funding() {
                Ok(target) => {
                    tracing::info!(
                        "[BingleLocalApi][keypair_status] derived required funding {} ALGO",
                        target
                    );
                    target
                }
                Err(e) => {
                    tracing::warn!(
                        "[BingleLocalApi][keypair_status] could not derive required funding ({}); falling back to REQUIRED_ALGO {}",
                        e,
                        REQUIRED_ALGO
                    );
                    REQUIRED_ALGO
                }
            }
        };

        let status = keypair_status_from_facts(
            algorand_id,
            has_asset,
            handle,
            balance_algos,
            required_target_algos,
        );
        self.cache_status(&status);
        Ok(status)
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
