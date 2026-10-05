//! The state behind a [`BingleApiLocalImpl`](super::BingleApiLocalImpl) handle (issue #283).
//!
//! Every clone of the handle shares one `LocalState`, and all of it is interior-mutable, so any
//! clone can read and update the store. As a child module of `bingle_local_api_impl`, its fields are
//! visible to that module and its other children (`store_and_forward`).

use super::LocalApiConfig;
use crate::api::notify::{AlertPoster, HttpAlertPoster, HttpRegisterPoster, RegisterPoster};
use crate::api::{ContactSource, Keypair, KeypairStatus, Message};
use algo_ops::AlgoOps;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, RwLock, RwLockReadGuard};

/// The state shared by every clone of a [`BingleApiLocalImpl`](super::BingleApiLocalImpl).
/// Internal; reached only through the handle.
pub struct LocalState {
    pub(super) keypair: Mutex<Option<Keypair>>, // interior mutability to allow &self methods to ensure keypair exists
    pub(super) algo_ops: Mutex<Option<AlgoOps>>, // cache constructed AlgoOps for current keypair
    // Read per operation; the store-and-forward and notify setters swap fields live (story #242).
    pub(super) config: RwLock<LocalApiConfig>,
    // Contacts storage: id => (handle, source, is_blocked)
    pub(super) contacts: Mutex<HashMap<String, (String, ContactSource, bool)>>,
    // Messages storage: append-only log of messages
    pub(super) messages: Mutex<Vec<Message>>,
    // Cache of the account's registered handle, set whenever a status resolves one. Lets
    // offline operations (queue_message) obtain the sender handle without a live blockchain
    // read once the account is registered (issue #18, A1).
    pub(super) own_handle: Mutex<Option<String>>,
    // The app id the memoized `own_handle` was established on. The ACTIVE short-circuit only trusts
    // the memo when this matches the configured app: after an app upgrade (same keypair, new
    // app_id) the memo is stale for the new app, so status re-resolves from chain and drives the
    // one-time local migration. `None` (e.g. state written before this field existed) is likewise
    // not trusted, so existing users migrate on upgrade.
    pub(super) own_handle_app_id: Mutex<Option<u64>>,
    // Session cache: an app id we have confirmed (this process) is not superseded, so the ACTIVE
    // memo path does not re-read the successor pointer on every poll (issue #18/#31).
    pub(super) live_app_confirmed: Mutex<Option<u64>>,
    // Last successfully computed status, returned when a later read finds the blockchain
    // unreachable so an already-known account stays usable during an outage (issue #18, A2).
    pub(super) last_status: Mutex<Option<KeypairStatus>>,
    // Cached result of the last network_available() probe with the time it was taken, so the
    // send hot-path does not hit the Algorand node on every message (issue #31).
    pub(super) last_network_check: Mutex<Option<(bool, std::time::Instant)>>,
    // Best-effort sender for the give-up nudge to the notify gateway (bingle_notify #11). Defaults
    // to the real HTTP poster; a seam so tests can observe the nudge without a live gateway.
    pub(super) alert_poster: RwLock<Arc<dyn AlertPoster>>,
    // Synchronous sender for the `/register` envelope (bingle_notify #i). Defaults to the real HTTP
    // poster; a seam so tests can observe the registration without a live gateway.
    pub(super) register_poster: RwLock<Arc<dyn RegisterPoster>>,
    // Ids of messages we have already nudged for, so the unreachable/give-up nudge fires at most
    // once per message even though update_message_status is called on every retry (bingle_notify
    // #11/#17). In-memory only: a restart may re-nudge a still-pending message, which is acceptable
    // (it only re-wakes an offline recipient so the pending retries can land).
    pub(super) nudged_messages: Mutex<HashSet<String>>,
    // (message id, recipient handle) pairs already posted to the recipient's Sidewinder
    // Mailbox, so store-and-forward posts each message to each recipient at most once even though
    // update_message_status fires on every retry (store-and-forward epic #200, story #214). Keyed
    // per recipient so a multi-recipient message whose post to one recipient failed retries only the
    // failed recipient without double-posting the others. Persisted (see save/load) so a restart does
    // not re-post an already-forwarded message.
    pub(super) forwarded_messages: Mutex<HashSet<(String, String)>>,
}

impl LocalState {
    /// Fresh state: no keypair, an empty contact store and message queue, and the default HTTP
    /// notify posters.
    pub(super) fn new(config: LocalApiConfig) -> Self {
        Self {
            keypair: Mutex::new(None),
            algo_ops: Mutex::new(None),
            config: RwLock::new(config),
            contacts: Mutex::new(HashMap::new()),
            messages: Mutex::new(Vec::new()),
            own_handle: Mutex::new(None),
            own_handle_app_id: Mutex::new(None),
            live_app_confirmed: Mutex::new(None),
            last_status: Mutex::new(None),
            last_network_check: Mutex::new(None),
            alert_poster: RwLock::new(Arc::new(HttpAlertPoster::new())),
            register_poster: RwLock::new(Arc::new(HttpRegisterPoster::new())),
            nudged_messages: Mutex::new(HashSet::new()),
            forwarded_messages: Mutex::new(HashSet::new()),
        }
    }

    /// The current configuration. Hold the guard only briefly: the store-and-forward and notify
    /// setters take the write lock.
    pub(super) fn config(&self) -> RwLockReadGuard<'_, LocalApiConfig> {
        self.config.read().unwrap_or_else(|e| e.into_inner())
    }

    pub(super) fn alert_poster(&self) -> Arc<dyn AlertPoster> {
        self.alert_poster
            .read()
            .map(|g| Arc::clone(&g))
            .unwrap_or_else(|e| Arc::clone(&e.into_inner()))
    }

    pub(super) fn register_poster(&self) -> Arc<dyn RegisterPoster> {
        self.register_poster
            .read()
            .map(|g| Arc::clone(&g))
            .unwrap_or_else(|e| Arc::clone(&e.into_inner()))
    }
}
