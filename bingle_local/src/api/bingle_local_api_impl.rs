//! The concrete [`BingleLocalApi`] implementation [`BingleApiLocalImpl`] and its
//! [`LocalApiConfig`].

use crate::api::notify::envelope::{fresh_nonce, now_secs};
use crate::api::notify::{
    AlertPoster, HttpAlertPoster, HttpRegisterPoster, RegisterPoster, build_register_request,
    encode_apns_token, post_giveup_alerts,
};
use crate::api::sidewinder::MailboxConfig;
use crate::api::{
    BingleLocalApi, ChainRegistrationOps, Contact, ContactSource, Keypair, KeypairStatus, Message,
    REQUIRED_ALGO, run_registration,
};
use algo_ops::error::AlgoErrorKind;
use algo_ops::{AlgoChainConfig, AlgoOps};
use bingle_core::api::bingle_api::{BingleError, SendFailureKind};
use bingle_core::blockchain::algo_bingle::AlgoBingle;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

// Store-and-forward posting (the Mailbox accessor, gate accessors, and the post-on-delivery-fail
// hook, epic #200 / #214). A child module so it still reads this implementation's private state.
mod store_and_forward;

/// Configuration for the local API implementation.
/// Includes the blockchain provider configuration and required ids.
#[derive(Debug, Clone)]
pub struct LocalApiConfig {
    /// Algorand node and indexer connection configuration.
    pub algo_config: AlgoChainConfig,
    /// Id of the Bingle application on-chain.
    pub app_id: u64,
    /// Id of the Bingle$ asset on-chain.
    pub asset_id: u64,
    /// Feature gate for the give-up nudge (bingle_notify #11). When `true` (the current default),
    /// a store-and-retry give-up POSTs a content-free `/alert` to the notify gateway so the
    /// recipient's device wakes and the end-to-end flow can retry while both are online. When
    /// `false`, give-up behaves exactly as before — no nudge, no gateway call.
    pub notify_on_giveup: bool,
    /// Base URL of the notify gateway (the bingle_notify stack's `ApiBaseUrl`). The nudge POSTs to
    /// `{notify_gateway_url}/alert`. When `None`, the nudge is skipped even if `notify_on_giveup`
    /// is set — each caller opts in per instance by providing the URL.
    pub notify_gateway_url: Option<String>,
    /// APNs environment a `/register` token is registered under: `"sandbox"` or `"production"`.
    /// Defaults to [`default_notify_env`] (sandbox); a production build overrides it.
    pub notify_env: String,
    /// Sidewinder Mailbox connection for store-and-forward (epic #200): the node endpoint, bearer
    /// token, and operation-type bindings the offline path posts to and reads from. `None` when the
    /// deployment has no Sidewinder node configured, in which case store-and-forward is unavailable.
    /// Whether it is *used* when available is the separate on/off toggle (#212).
    pub sidewinder: Option<MailboxConfig>,
    /// Send-side store-and-forward gate (epic #200, story #212): when `true`, a give-up on direct
    /// delivery posts the sealed message to the recipient's Sidewinder Mailbox (#214); when `false`
    /// (the default), give-up behaves exactly as today. Independent of the receive gate — a client
    /// may forward without listening, or listen without forwarding. Also requires `sidewinder` to
    /// be configured for the post to have anywhere to go.
    pub store_and_forward_send: bool,
    /// Receive-side store-and-forward gate (epic #200, story #212): when `true`, the client polls its
    /// own Sidewinder Mailbox on reconnect and on a cadence, reading messages forwarded to it (#215);
    /// when `false` (the default), no polling happens. Independent of the send gate. Also requires
    /// `sidewinder` to be configured for there to be a Mailbox to poll.
    pub store_and_forward_receive: bool,
}

/// The APNs environment assumed when a caller does not specify one: `"sandbox"`, matching a dev /
/// Xcode build. A production (TestFlight/App Store) build overrides this via `notify_env`.
pub fn default_notify_env() -> String {
    "sandbox".to_string()
}

impl Default for LocalApiConfig {
    fn default() -> Self {
        // notify_on_giveup defaults to `true` (bingle_notify #11, revisit once proven); the gateway
        // URL defaults to `None`, so a defaulted config makes no gateway call until a URL is set.
        Self {
            algo_config: AlgoChainConfig::default(),
            app_id: 0,
            asset_id: 0,
            notify_on_giveup: true,
            notify_gateway_url: None,
            notify_env: default_notify_env(),
            sidewinder: None,
            // Store-and-forward defaults off: it is a new, unproven path (#214/#215), enabled per
            // build/environment once proven. Each side is independent.
            store_and_forward_send: false,
            store_and_forward_receive: false,
        }
    }
}

impl LocalApiConfig {
    /// Build a config from a caller's configuration surface, applying the give-up-nudge defaults
    /// (bingle_notify #17). `notify_on_giveup` defaults to `true` when the caller passes `None`; the
    /// gateway URL passes through unchanged (`None` ⇒ the nudge stays dormant). Shared by the JSI and
    /// webserver call sites so the mapping lives — and is tested — in one place.
    pub fn with_notify(
        algo_config: AlgoChainConfig,
        app_id: u64,
        asset_id: u64,
        notify_on_giveup: Option<bool>,
        notify_gateway_url: Option<String>,
    ) -> Self {
        Self {
            algo_config,
            app_id,
            asset_id,
            notify_on_giveup: notify_on_giveup.unwrap_or(true),
            notify_gateway_url,
            notify_env: default_notify_env(),
            sidewinder: None,
            store_and_forward_send: false,
            store_and_forward_receive: false,
        }
    }

    /// Attach a Sidewinder Mailbox connection (store-and-forward, epic #200). Chained after
    /// [`with_notify`](Self::with_notify) by a call site whose configuration supplies a node
    /// endpoint and token; left unset (`None`) when no Sidewinder node is configured.
    pub fn with_sidewinder(mut self, sidewinder: Option<MailboxConfig>) -> Self {
        self.sidewinder = sidewinder;
        self
    }

    /// Set the store-and-forward send / receive gates (epic #200, story #212). Chained after
    /// [`with_notify`](Self::with_notify) by a call site whose configuration supplies them; each
    /// argument defaults to `false` (off) when the caller passes `None`, so an `init` or persisted
    /// config without the fields keeps store-and-forward disabled. The two sides are independent —
    /// see [`store_and_forward_send`](Self::store_and_forward_send) /
    /// [`store_and_forward_receive`](Self::store_and_forward_receive).
    pub fn with_store_and_forward(
        mut self,
        store_and_forward_send: Option<bool>,
        store_and_forward_receive: Option<bool>,
    ) -> Self {
        self.store_and_forward_send = store_and_forward_send.unwrap_or(false);
        self.store_and_forward_receive = store_and_forward_receive.unwrap_or(false);
        self
    }
}

/// Decide the keypair status from resolved on-chain facts.
///
/// ACTIVE requires both the Bingle$ asset and a registered handle. When the
/// account holds the asset but has no handle, we fall through to the balance
/// check (FUNDED / UNFUNDED). `balance_algos` is only consulted when the
/// account is not ACTIVE.
///
/// `required_target_algos` is the balance considered adequate to register: the
/// live cost from `required_funding_algos` when it can be read, or `REQUIRED_ALGO`
/// as a fallback. For UNFUNDED, `required_algo` is the **top-up** to reach that
/// target (`required_target_algos - balance_algos`), so a semi-funded account is
/// only asked for the shortfall (issue #15, A3/A3b).
#[doc(hidden)]
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
    // The app id the memoized `own_handle` was established on. The ACTIVE short-circuit only trusts
    // the memo when this matches the configured app: after an app upgrade (same keypair, new
    // app_id) the memo is stale for the new app, so status re-resolves from chain and drives the
    // one-time local migration. `None` (e.g. state written before this field existed) is likewise
    // not trusted, so existing users migrate on upgrade.
    own_handle_app_id: Mutex<Option<u64>>,
    // Session cache: an app id we have confirmed (this process) is not superseded, so the ACTIVE
    // memo path does not re-read the successor pointer on every poll (issue #18/#31).
    live_app_confirmed: Mutex<Option<u64>>,
    // Last successfully computed status, returned when a later read finds the blockchain
    // unreachable so an already-known account stays usable during an outage (issue #18, A2).
    last_status: Mutex<Option<KeypairStatus>>,
    // Cached result of the last network_available() probe with the time it was taken, so the
    // send hot-path does not hit the Algorand node on every message (issue #31).
    last_network_check: Mutex<Option<(bool, std::time::Instant)>>,
    // Best-effort sender for the give-up nudge to the notify gateway (bingle_notify #11). Defaults
    // to the real HTTP poster; a seam so tests can observe the nudge without a live gateway.
    alert_poster: Arc<dyn AlertPoster>,
    // Synchronous sender for the `/register` envelope (bingle_notify #i). Defaults to the real HTTP
    // poster; a seam so tests can observe the registration without a live gateway.
    register_poster: Arc<dyn RegisterPoster>,
    // Message timestamps we have already nudged for, so the unreachable/give-up nudge fires at most
    // once per message even though update_message_status is called on every retry (bingle_notify
    // #11/#17). In-memory only: a restart may re-nudge a still-pending message, which is acceptable
    // (it only re-wakes an offline recipient so the pending retries can land).
    nudged_messages: Mutex<HashSet<i64>>,
    // (message timestamp, recipient handle) pairs already posted to the recipient's Sidewinder
    // Mailbox, so store-and-forward posts each message to each recipient at most once even though
    // update_message_status fires on every retry (store-and-forward epic #200, story #214). Keyed
    // per recipient so a multi-recipient message whose post to one recipient failed retries only the
    // failed recipient without double-posting the others. Persisted (see save/load) so a restart does
    // not re-post an already-forwarded message.
    forwarded_messages: Mutex<HashSet<(i64, String)>>,
}

/// How long a `network_available` probe result is reused before re-probing.
const NETWORK_CHECK_TTL: std::time::Duration = std::time::Duration::from_secs(5);

impl BingleApiLocalImpl {
    /// Create a new local API implementation from `config`.
    ///
    /// Starts with no keypair, an empty contact store and message queue, and the default HTTP
    /// notify posters. Generate or import a keypair before calling the on-chain operations.
    pub fn new(config: LocalApiConfig) -> Self {
        Self {
            keypair: Mutex::new(None),
            algo_ops: Mutex::new(None),
            config,
            contacts: Mutex::new(HashMap::new()),
            messages: Mutex::new(Vec::new()),
            own_handle: Mutex::new(None),
            own_handle_app_id: Mutex::new(None),
            live_app_confirmed: Mutex::new(None),
            last_status: Mutex::new(None),
            last_network_check: Mutex::new(None),
            alert_poster: Arc::new(HttpAlertPoster::new()),
            register_poster: Arc::new(HttpRegisterPoster::new()),
            nudged_messages: Mutex::new(HashSet::new()),
            forwarded_messages: Mutex::new(HashSet::new()),
        }
    }

    /// Override the `/register` sender (bingle_notify #i). Intended for tests, which inject a
    /// recording poster to observe the registration without a live gateway.
    pub fn set_register_poster(&mut self, poster: Arc<dyn RegisterPoster>) {
        self.register_poster = poster;
    }

    /// Override the give-up nudge sender (bingle_notify #11). Intended for tests, which inject a
    /// recording poster to observe the `/alert` without a live gateway.
    pub fn set_alert_poster(&mut self, poster: Arc<dyn AlertPoster>) {
        self.alert_poster = poster;
    }

    /// The account's registered handle as recorded in local state (memoized once the account is
    /// ACTIVE, and restored by [`load`](BingleLocalApi::load)), or `None` if unknown. Reads the
    /// in-memory memo only — no blockchain access. Used by the `bingle_cli chat` state-file bridge
    /// to start the engine from a saved state file without requiring `--handle` on the command line.
    pub fn own_handle(&self) -> Option<String> {
        self.own_handle.lock().ok().and_then(|g| g.clone())
    }

    /// Seed the memoized local handle (tests only). Lets a test exercise the give-up nudge, which
    /// signs the envelope as the local handle, without a live blockchain read to resolve it.
    #[doc(hidden)]
    pub fn seed_own_handle_for_tests(&self, handle: String) {
        if let Ok(mut g) = self.own_handle.lock() {
            *g = Some(handle);
        }
    }

    /// Fire the best-effort notify nudge for recipients a send could not reach — either still
    /// unreachable and retrying, or terminally given up (bingle_notify #11/#17). The caller
    /// (`update_message_status`) dedups so this fires at most once per message.
    ///
    /// Gated on `notify_on_giveup` and a configured `notify_gateway_url`; when either is off this is
    /// a no-op and sending behaves exactly as before. Resolves the local (sender) handle and the
    /// active keypair, then delegates the per-recipient sign-and-post to
    /// [`post_giveup_alerts`](crate::api::notify::post_giveup_alerts). Never blocks or fails
    /// delivery: a missing handle, signing failure, or transport error is logged and swallowed.
    fn notify_giveup(&self, recipient_handles: &[String]) {
        if !self.config.notify_on_giveup {
            return;
        }
        let Some(gateway_url) = self.config.notify_gateway_url.as_deref() else {
            return;
        };
        if recipient_handles.is_empty() {
            return;
        }

        // The envelope issuer is the local (sender) handle. Use the cached handle so this needs no
        // live blockchain read; if none is known we cannot sign a valid envelope, so skip.
        let iss = match self.sender_handle() {
            Ok(h) => h,
            Err(e) => {
                tracing::warn!(
                    "[notify_giveup] no local handle to sign alert ({}); skipping nudge",
                    e
                );
                return;
            }
        };
        // Bind the signer to the active keypair (no network). Without it we cannot sign; skip.
        let ops = match self.get_algo_ops() {
            Ok(o) => o,
            Err(e) => {
                tracing::warn!(
                    "[notify_giveup] no keypair to sign alert ({}); skipping nudge",
                    e
                );
                return;
            }
        };

        post_giveup_alerts(
            self.alert_poster.as_ref(),
            gateway_url,
            &ops,
            &iss,
            recipient_handles,
        );
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
        if let Some(handle) = status.handle.as_ref() {
            if let Ok(mut guard) = self.own_handle.lock() {
                *guard = Some(handle.clone());
            }
            // Scope the memo to the app it was resolved on so it is not trusted after an upgrade.
            if let Ok(mut g) = self.own_handle_app_id.lock() {
                *g = Some(self.config.app_id);
            }
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
#[doc(hidden)]
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
#[doc(hidden)]
pub fn algo_host_unreachable(err: &BingleError) -> bool {
    matches!(err, BingleError::Algo(ae) if ae.kind == AlgoErrorKind::HostUnreachable)
}

/// Decide a status result under a possible blockchain outage (issue #18, A2).
///
/// Returns the `cached` status only when the failure is a host-unreachable outage and a cached
/// status exists; any other error (or a genuine first run with no cache) propagates so callers
/// still see NoBlockchain. The returned cached status is flagged `stale` so the UI can show it as
/// unavailable rather than a freshly confirmed read (issue #31).
#[doc(hidden)]
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
        if let Ok(mut g) = self.own_handle_app_id.lock() {
            *g = None;
        }
        if let Ok(mut g) = self.live_app_confirmed.lock() {
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

    fn import_keypair(&mut self, passphrase: String) -> Result<Keypair, BingleError> {
        tracing::info!("[BingleLocalApi] Importing keypair from passphrase");
        // Validate the mnemonic and derive the account id. The passphrase itself is never logged.
        let id = AlgoOps::address_from_passphrase(&passphrase)
            .map_err(|e| BingleError::Other(e.to_string()))?;
        let kp = Keypair { id, passphrase };
        tracing::info!("[BingleLocalApi] Imported keypair with id: {}", kp.id);
        if let Ok(mut guard) = self.keypair.lock() {
            *guard = Some(kp.clone());
        }
        // The imported account's on-chain state is unknown (it may already be registered), so
        // clear the memoized ACTIVE handle/status and let keypair_status re-resolve from chain.
        if let Ok(mut g) = self.own_handle.lock() {
            *g = None;
        }
        if let Ok(mut g) = self.own_handle_app_id.lock() {
            *g = None;
        }
        if let Ok(mut g) = self.live_app_confirmed.lock() {
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
        // it (scoped to this app) so keypair_status serves ACTIVE with no further blockchain read.
        if let Ok(mut g) = self.own_handle.lock() {
            *g = Some(handle.clone());
        }
        if let Ok(mut g) = self.own_handle_app_id.lock() {
            *g = Some(app_id);
        }
        Ok(true)
    }

    fn register_apns_token(&self, token: Vec<u8>) -> Result<bool, BingleError> {
        // Hex-encode the raw 32-byte token here (never in the Swift bridge). A mis-sized token is
        // rejected before we sign or send, so the 80-byte-token failure cannot recur.
        let token_hex = encode_apns_token(&token)?;

        // A registration needs somewhere to send it.
        let Some(gateway_url) = self.config.notify_gateway_url.as_deref() else {
            return Err(BingleError::Other(
                "notify_gateway_url is not configured; cannot register APNs token".to_string(),
            ));
        };
        let env = self.config.notify_env.clone();

        // The envelope issuer is the local handle; the signer binds to the active keypair (no
        // network). Either missing means we cannot produce a valid envelope.
        let iss = self.sender_handle()?;
        let ops = self.get_algo_ops()?;

        let nonce = fresh_nonce();
        // Short freshness window (the contract allows up to 60s ahead), matching the alert envelope.
        let exp = now_secs() + 45;
        let req = build_register_request(&ops, &iss, &token_hex, &env, &nonce, exp)?;

        tracing::info!(
            "[notify][register] registering APNs token for '{}' (env {}) -> {}",
            iss,
            env,
            gateway_url
        );
        self.register_poster.post_register(gateway_url, req)
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
        let ops =
            AlgoOps::new_for_algorand(Some(pass), None, Some(self.config.algo_config.clone()));
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
            failure_kind: None,
            sent_time: None,
            delivered_time: None,
            signature: None,
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
            failure_kind: None,
            sent_time: None,
            delivered_time: None,
            signature: None,
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
        failure_kind: Option<SendFailureKind>,
    ) -> Result<(), BingleError> {
        let mut guard = match self.messages.lock() {
            Ok(g) => g,
            Err(e) => {
                let msg = format!("mutex poisoned: {}", e);
                tracing::error!("[update_message_status] Failed to lock messages: {}", msg);
                return Err(BingleError::Other(msg));
            }
        };

        // Fire the notify nudge whenever a send reports a failure — the recipient is unreachable and
        // still retrying (progress < 1.0), or the send has terminally given up (progress >= 1.0).
        // Waking an offline recipient with a content-free push lets the pending retries land, so the
        // nudge must fire while the message is still unreachable, not only on give-up (which, for a
        // transient "keep retrying" failure, never happens) — bingle_notify #11/#17. A successful
        // send (progress 1.0, no failure_reason) never nudges. Dedup happens below.
        let failed_recipients = {
            if let Some(msg) = guard.iter_mut().find(|m| m.timestamp == timestamp) {
                msg.progress = Some(progress);
                if failure_reason.is_some() || progress >= 1.0 {
                    msg.failure_reason = failure_reason;
                    // Keep the typed cause in lockstep with the reason: set on failure, cleared on
                    // a successful/terminal send with no reason (issue #99).
                    msg.failure_kind = failure_kind;
                }
                if msg.failure_reason.is_some() {
                    Some((
                        msg.timestamp,
                        msg.recipient_handles.clone(),
                        msg.text.clone(),
                    ))
                } else {
                    None
                }
            } else {
                return Err(BingleError::Other(format!(
                    "Message with timestamp {} not found",
                    timestamp
                )));
            }
        };
        // Release the messages lock before nudging: notify_giveup takes other locks (keypair /
        // algo_ops) and hands off to the poster, which must not run under the messages lock.
        drop(guard);
        if let Some((ts, recipients, text)) = failed_recipients {
            // Nudge at most once per message: HashSet::insert returns true only the first time this
            // timestamp is seen, so repeated retries of the same unreachable message don't re-nudge.
            let first_nudge = match self.nudged_messages.lock() {
                Ok(mut nudged) => nudged.insert(ts),
                Err(e) => {
                    tracing::error!(
                        "[update_message_status] Failed to lock nudged_messages: {}",
                        e
                    );
                    false
                }
            };
            if first_nudge {
                self.notify_giveup(&recipients);
            }
            // Store-and-forward post-on-delivery-fail (#214): post the sealed message to each
            // recipient's Sidewinder Mailbox so it survives until they reconnect. Runs on each failed
            // retry but is idempotent per recipient, so it posts once per recipient and retries only
            // recipients whose post has not yet succeeded. Gated + best-effort; never affects delivery.
            let fully_forwarded = self.forward_message_to_mailbox(ts, &recipients, &text);
            if fully_forwarded {
                // The message is now safely in every recipient's Mailbox, so stop retrying direct
                // Bingle delivery: mark it complete and clear the transient failure. The recipient
                // reads it from the Mailbox on reconnect (#215).
                if let Ok(mut guard) = self.messages.lock() {
                    if let Some(m) = guard.iter_mut().find(|m| m.timestamp == ts) {
                        m.progress = Some(1.0);
                        m.failure_reason = None;
                        m.failure_kind = None;
                    }
                }
            }
        }
        Ok(())
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

    fn poll_mailbox(&self) -> Result<Vec<Message>, BingleError> {
        // The read-on-reconnect logic lives in the `store_and_forward` child module (#215).
        self.poll_mailbox_inner()
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
        struct ForwardedEntry {
            timestamp: i64,
            handle: String,
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
            // The app id `own_handle` was established on. Absent in state written before app-scoped
            // memoization; a missing value means the memo is not trusted (forces a re-resolve, so
            // an upgrading user migrates instead of falsely reading as ACTIVE on the new app).
            #[serde(default)]
            own_handle_app_id: Option<u64>,
            // (message timestamp, recipient handle) pairs already posted to a Sidewinder Mailbox, so
            // store-and-forward does not re-post after a restart (store-and-forward epic #200, #214).
            #[serde(default)]
            forwarded_messages: Vec<ForwardedEntry>,
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
        let own_handle_app_id = self.own_handle_app_id.lock().ok().and_then(|g| *g);
        let forwarded_messages: Vec<ForwardedEntry> = self
            .forwarded_messages
            .lock()
            .map(|g| {
                g.iter()
                    .map(|(timestamp, handle)| ForwardedEntry {
                        timestamp: *timestamp,
                        handle: handle.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default();

        let state = LocalState {
            keypair,
            contacts: contacts_vec,
            messages,
            own_handle,
            own_handle_app_id,
            forwarded_messages,
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
        struct ForwardedEntry {
            timestamp: i64,
            handle: String,
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
            #[serde(default)]
            own_handle_app_id: Option<u64>,
            #[serde(default)]
            forwarded_messages: Vec<ForwardedEntry>,
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
        if let Ok(mut oha) = self.own_handle_app_id.lock() {
            *oha = state.own_handle_app_id;
        }
        // Restore the store-and-forward posted-set so a restart does not re-post an already-forwarded
        // message to the recipient's Mailbox (store-and-forward epic #200, story #214).
        if let Ok(mut fwd) = self.forwarded_messages.lock() {
            *fwd = state
                .forwarded_messages
                .into_iter()
                .map(|e| (e.timestamp, e.handle))
                .collect();
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

        let configured_app_id = self.config.app_id;

        // Serve the memoized ACTIVE handle without a blockchain read — but only when the memo was
        // established for the *currently configured* app. This keeps steady-state polling read-free
        // (issue #18/#31) while staying correct across an app upgrade:
        //   - memo app == configured app: the account is registered here. Confirm once per session
        //     that the app has not been superseded (else UPGRADE_REQUIRED), then serve ACTIVE.
        //   - memo app != configured app, or no recorded app (state written before app-scoped
        //     memoization): the memo is stale for this app, so fall through to the full on-chain
        //     check. That reports non-ACTIVE and lets the caller drive the one-time local migration.
        // When no app is configured (app_id 0, e.g. tests) there is nothing to scope against, so the
        // memo is served as before.
        let memoized_handle = self.own_handle.lock().ok().and_then(|g| g.clone());
        let memo_app_id = self.own_handle_app_id.lock().ok().and_then(|g| *g);
        if let Some(handle) = memoized_handle
            && (configured_app_id == 0 || memo_app_id == Some(configured_app_id))
        {
            // A registered user still configured for a *superseded* app must be told to upgrade.
            // Check the successor pointer once per session and cache the "live" result so later
            // polls stay read-free. A read error is best-effort: serve ACTIVE rather than block.
            let already_live =
                self.live_app_confirmed.lock().ok().and_then(|g| *g) == Some(configured_app_id);
            if configured_app_id > 0
                && !already_live
                && let Ok(ops) = self.get_algo_ops()
            {
                let bgl = AlgoBingle::new(ops, configured_app_id, self.config.asset_id);
                match bgl.successor_app(configured_app_id) {
                    Ok(Some(successor)) => {
                        tracing::info!(
                            "[BingleLocalApi][keypair_status] memoized app {} superseded by {}; UPGRADE_REQUIRED",
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
                    Ok(None) => {
                        if let Ok(mut g) = self.live_app_confirmed.lock() {
                            *g = Some(configured_app_id);
                        }
                    }
                    Err(e) => tracing::debug!(
                        "[BingleLocalApi][keypair_status] memoized-app successor check failed (serving ACTIVE): {}",
                        BingleError::from_anyhow(e)
                    ),
                }
            }
            tracing::debug!(
                "[BingleLocalApi][keypair_status] ACTIVE (memoized handle '{}' for app {}); skipping blockchain",
                handle,
                configured_app_id
            );
            return Ok(KeypairStatus {
                status: "ACTIVE".to_string(),
                id: Some(algorand_id),
                handle: Some(handle),
                required_algo: None,
                stale: false,
            });
        }

        // Superseded-app gate for the non-memoized path: if the configured app has been marked
        // superseded (a newer app replaced it), the client must upgrade. Report UPGRADE_REQUIRED and
        // short-circuit ahead of the funding/registration checks — the whole app is retired
        // regardless of this account's funding or handle. Best-effort: a read error falls through to
        // the normal status rather than blocking the user.
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

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
