//! A thin wrapper over the Sidewinder Mailbox (first-in first-out, FIFO) operations the
//! store-and-forward path uses (store-and-forward epic #200, foundation story #213).
//!
//! The `sidewinder_ops` client is a generic transaction client — it submits a signed
//! [`TransactionRequest`] (an operation type plus its arguments) and polls the result. The Mailbox
//! is a node-side *application* configured over the FIFO primitive: `post(recipient, message)`
//! appends to the recipient's queue (keyed by the recipient address in `arg[0]`, callable by any
//! enrolled sender) and `pop()` removes and returns the head of the *caller's own* queue (keyed by
//! the authenticated sender). This wrapper turns those two operations into plain method calls,
//! packing the arguments, submitting through the client, and polling the transaction to finality.
//!
//! The operation *type numbers* (`post_type` / `pop_type`) are assigned by the node's
//! `application.yaml`, not fixed by the client crate, so they live in [`MailboxConfig`] and default
//! to the tier-1 Mailbox binding ([`MAILBOX_POST_TYPE`] / [`MAILBOX_POP_TYPE`]). No post-on-fail or
//! read-on-reconnect wiring lives here — those are their own stories (#214 / #215); this story is
//! just the client, the connection config, and the two Mailbox operations.

use algo_ops::AlgoOps;
use bingle_core::api::bingle_api::BingleError;
use sidewinder_ops::{
    AppArg, DiscoveredNode, DiscoveryConfig, PendingTransaction, SidewinderClient,
    SidewinderConfig, SidewinderError, SidewinderErrorKind, SidewinderOps, Stage, SuggestedParams,
    TransactionRequest, resolve_nodes,
};
use std::time::{Duration, Instant};

/// The transaction type the tier-1 Mailbox configuration binds `post` (`FIFO.append`) to. The node's
/// `application.yaml` is the source of truth; this is the default when a caller does not override it.
pub const MAILBOX_POST_TYPE: u32 = 1;
/// The transaction type the tier-1 Mailbox configuration binds `pop` (`FIFO.remove_head`) to.
pub const MAILBOX_POP_TYPE: u32 = 2;

/// Default time to wait for a submitted transaction to reach the `final` stage before giving up.
/// Anchoring to the parent chain dominates this. LocalNet finalises in a round or two; a network
/// whose anchor batching is slower (e.g. TestNet's `v0_0_3` profile, K=64 rounds ≈ minutes) should
/// raise it via [`MailboxConfig::finality_timeout`].
pub const DEFAULT_FINALITY_TIMEOUT: Duration = Duration::from_secs(120);
/// The long-poll window handed to each `watch` call while polling for finality.
const WATCH_WAIT_SECS: u64 = 5;
/// Pause between `watch` retries when the read node does not yet know a just-submitted transaction
/// (a not-found is propagation lag, not a failure), so we do not busy-loop.
const NOT_FOUND_BACKOFF: Duration = Duration::from_millis(500);

/// How the Mailbox reaches its Sidewinder node.
///
/// Two transports (Sidewinder authenticated-access epic #240 / consumer story #244):
/// - [`Bearer`](MailboxConnection::Bearer): the v0.0.2 way — a static base URL plus the fixed shared
///   client token (Sidewinder #164), plaintext. Kept for backwards-compat and local/dev testing, and
///   selected whenever `SIDEWINDER_NODE_URL` + `SIDEWINDER_TOKEN` are supplied (an explicit override).
/// - [`Discovered`](MailboxConnection::Discovered): find the node endpoint on-chain from the Bingle
///   DApp application id, then connect over identity-pinned mutual Transport Layer Security (mTLS) —
///   no bearer token; the enrolled account that signs Mailbox transactions is also the mTLS identity.
///
/// There is deliberately no `Default`: a connection is deployment-specific.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MailboxConnection {
    /// Static base URL + fixed shared bearer token (plaintext), the v0.0.2 transport.
    Bearer {
        /// Base URL of the Sidewinder node, for example `http://localhost:9101`.
        base_url: String,
        /// Bearer token sent on every authenticated node endpoint.
        token: String,
    },
    /// On-chain endpoint discovery + identity-pinned mutual TLS, keyed by the Bingle DApp app id.
    Discovered {
        /// The Bingle DApp application id whose opted-in cluster-node accounts publish endpoints.
        app_id: u64,
    },
}

impl MailboxConnection {
    /// Select the connection mode from a call site's optional URL/token override and the Bingle app id.
    ///
    /// Deterministic and logged (story #244, deliverable 2):
    /// - `base_url` **and** `token` both set → [`Bearer`](MailboxConnection::Bearer) (plaintext
    ///   override, exactly as v0.0.2), because the Bingle app id is always present in normal operation
    ///   and so cannot itself be the selector;
    /// - else a non-zero `app_id` → [`Discovered`](MailboxConnection::Discovered) (discovery + mTLS);
    /// - else `None` (store-and-forward stays unconfigured — today's behaviour).
    ///
    /// Blank strings and a `0` app id are treated as absent (a blank environment value must not
    /// half-configure the Mailbox).
    pub fn select(
        base_url: Option<String>,
        token: Option<String>,
        app_id: Option<u64>,
    ) -> Option<Self> {
        let base_url = base_url.filter(|s| !s.trim().is_empty());
        let token = token.filter(|s| !s.trim().is_empty());
        if let (Some(base_url), Some(token)) = (base_url, token) {
            tracing::info!(
                "sidewinder mailbox: bearer transport (SIDEWINDER_NODE_URL/SIDEWINDER_TOKEN override)"
            );
            return Some(Self::Bearer { base_url, token });
        }
        match app_id.filter(|id| *id != 0) {
            Some(app_id) => {
                tracing::info!(
                    "sidewinder mailbox: on-chain discovery + mutual TLS for Bingle app {app_id}"
                );
                Some(Self::Discovered { app_id })
            }
            None => None,
        }
    }
}

/// How to reach a recipient's Sidewinder Mailbox: the [`MailboxConnection`] plus the operation-type
/// numbers `post` and `pop` are bound to in the node's application configuration.
///
/// There is deliberately no `Default`: a connection is deployment-specific.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MailboxConfig {
    /// How to reach the node: bearer (plaintext) or on-chain discovery + mutual TLS.
    pub connection: MailboxConnection,
    /// Transaction type bound to the Mailbox `post` operation (`FIFO.append`).
    pub post_type: u32,
    /// Transaction type bound to the Mailbox `pop` operation (`FIFO.remove_head`).
    pub pop_type: u32,
    /// How long to wait for a submitted transaction to reach `final` before giving up. Defaults to
    /// [`DEFAULT_FINALITY_TIMEOUT`]; raise it for a network whose anchor batching is slower than
    /// LocalNet (e.g. TestNet).
    pub finality_timeout: Duration,
}

impl MailboxConfig {
    /// Build a **bearer** (plaintext) config from a node URL and bearer token, using the default
    /// tier-1 Mailbox operation types ([`MAILBOX_POST_TYPE`] / [`MAILBOX_POP_TYPE`]) and
    /// [`DEFAULT_FINALITY_TIMEOUT`]. For discovery + mutual TLS use [`discovered`](Self::discovered).
    /// Set the fields on the returned value to override the types or the finality timeout.
    pub fn new(base_url: impl Into<String>, token: impl Into<String>) -> Self {
        Self::from_connection(MailboxConnection::Bearer {
            base_url: base_url.into(),
            token: token.into(),
        })
    }

    /// Build a **discovery + mutual-TLS** config for the given Bingle DApp application id, with the
    /// default operation types and finality timeout.
    pub fn discovered(app_id: u64) -> Self {
        Self::from_connection(MailboxConnection::Discovered { app_id })
    }

    /// Wrap a [`MailboxConnection`] with the default operation types and finality timeout.
    pub fn from_connection(connection: MailboxConnection) -> Self {
        Self {
            connection,
            post_type: MAILBOX_POST_TYPE,
            pop_type: MAILBOX_POP_TYPE,
            finality_timeout: DEFAULT_FINALITY_TIMEOUT,
        }
    }

    /// Map a caller's optional node URL and token to a **bearer** config: `Some` only when *both* are
    /// supplied (blank = absent), `None` otherwise. Prefer [`select`](Self::select), which also enables
    /// the on-chain discovery path from the Bingle app id; this remains for bearer-only call sites and
    /// tests.
    pub fn from_parts(base_url: Option<String>, token: Option<String>) -> Option<Self> {
        let base_url = base_url.filter(|s| !s.trim().is_empty())?;
        let token = token.filter(|s| !s.trim().is_empty())?;
        Some(Self::new(base_url, token))
    }

    /// Select the Mailbox connection from a call site's optional URL/token override and the Bingle app
    /// id (see [`MailboxConnection::select`]). `None` means store-and-forward stays unconfigured.
    /// Shared by the JSI, webserver, and CLI call sites so the selection lives — and is tested — in one
    /// place.
    pub fn select(
        base_url: Option<String>,
        token: Option<String>,
        app_id: Option<u64>,
    ) -> Option<Self> {
        MailboxConnection::select(base_url, token, app_id).map(Self::from_connection)
    }
}

/// Whether store-and-forward posting should run for a send that failed direct delivery: the send
/// gate is on ([`store_and_forward_send`](crate::api::bingle_local_api_impl::LocalApiConfig::store_and_forward_send))
/// *and* a Sidewinder node is configured. Pure, so the post-on-delivery-fail gate (#214) is
/// unit-testable without a node.
pub fn should_forward_send(send_gate: bool, sidewinder_configured: bool) -> bool {
    send_gate && sidewinder_configured
}

/// The recipients of a message (identified by its `timestamp`) that have not yet been posted to a
/// Mailbox, given the set of `(timestamp, handle)` pairs already forwarded. This is the
/// per-recipient idempotency filter for post-on-delivery-fail (#214): a recipient already posted is
/// skipped, so a retry or restart re-posts only recipients whose post has not yet succeeded. Pure,
/// so the once-per-recipient guarantee is unit-tested without a node.
pub fn pending_forward_recipients(
    timestamp: i64,
    recipient_handles: &[String],
    forwarded: &std::collections::HashSet<(i64, String)>,
) -> Vec<String> {
    recipient_handles
        .iter()
        .filter(|handle| !forwarded.contains(&(timestamp, (*handle).clone())))
        .cloned()
        .collect()
}

/// A client for one recipient-addressable Sidewinder Mailbox, bound to an enrolled parent-chain
/// account (the [`AlgoOps`] handle signs every transaction it submits, and — on the discovery
/// transport — is also the mutual-TLS client identity, so one account serves both roles).
pub struct Mailbox {
    /// The enrolled parent-chain account. Retained so the discovery transport can reconnect to a
    /// failed-over node and re-resolve without threading the handle back in.
    algo: AlgoOps,
    /// The live node connection (bearer client, or a discovered-node client with its failover set).
    transport: Transport,
    post_type: u32,
    pop_type: u32,
    finality_timeout: Duration,
}

/// The live node connection: either the static bearer client, or a discovered-node client that
/// retains the resolved node set so a dead node can be failed over to (and re-resolved) cheaply.
enum Transport {
    /// Plaintext + bearer token: a single static endpoint, no failover.
    Bearer(SidewinderClient),
    /// Identity-pinned mutual TLS to a discovered node, with the resolved set held for failover.
    Discovered(DiscoveredTransport),
}

/// The discovery transport's state: the parameters to re-resolve with, the reachable node set, and
/// the client bound to the currently active node.
struct DiscoveredTransport {
    /// Discovery parameters (Bingle app id + membership schema), reused to re-resolve on exhaustion.
    discovery: DiscoveryConfig,
    /// Reachable resolved nodes (those advertising an endpoint), in discovery order.
    nodes: Vec<DiscoveredNode>,
    /// Index into [`nodes`](DiscoveredTransport::nodes) of the currently connected node.
    active: usize,
    /// Client bound (identity-pinned mutual TLS) to `nodes[active]`.
    client: SidewinderClient,
}

impl Mailbox {
    /// Build a Mailbox client from an enrolled [`AlgoOps`] handle and connection config.
    ///
    /// For a [`Bearer`](MailboxConnection::Bearer) connection this fails cleanly (a surfaced
    /// [`BingleError`], never a panic) when the endpoint or token is missing. For a
    /// [`Discovered`](MailboxConnection::Discovered) connection it resolves the app's cluster nodes
    /// on-chain and connects to the first reachable one over identity-pinned mutual TLS, retaining the
    /// rest for failover; a discovery/connect failure is a surfaced (retryable) error.
    pub fn new(algo: AlgoOps, config: MailboxConfig) -> Result<Self, BingleError> {
        let transport = match config.connection {
            MailboxConnection::Bearer { base_url, token } => {
                if base_url.trim().is_empty() {
                    return Err(BingleError::Other(
                        "sidewinder mailbox: node base URL is empty".to_string(),
                    ));
                }
                if token.trim().is_empty() {
                    return Err(BingleError::Other(
                        "sidewinder mailbox: node bearer token is empty".to_string(),
                    ));
                }
                Transport::Bearer(SidewinderClient::from_algo_ops(
                    algo.clone(),
                    SidewinderConfig::new(base_url, token),
                ))
            }
            MailboxConnection::Discovered { app_id } => connect_discovered(&algo, app_id)?,
        };
        Ok(Self {
            algo,
            transport,
            post_type: config.post_type,
            pop_type: config.pop_type,
            finality_timeout: config.finality_timeout,
        })
    }

    /// Post `message` to `recipient`'s Mailbox (`FIFO.append`), waiting for the transaction to
    /// finalise. `recipient` is the recipient's Algorand address string, packed as the queue key in
    /// `arg[0]`; the message bytes are `arg[1]`.
    pub fn post(&mut self, recipient: &str, message: &[u8]) -> Result<(), BingleError> {
        let params = self.params_with_failover("post")?;
        let request = build_post_request(self.post_type, recipient, message, &params);
        self.submit_and_finalize("post", request)?;
        // A successful `FIFO.append` returns an empty result; there is nothing to hand back.
        Ok(())
    }

    /// Pop the head message from the caller's own Mailbox (`FIFO.remove_head`), waiting for the
    /// transaction to finalise. Returns the message bytes, or `None` when the Mailbox is empty (the
    /// operation returns an empty result once the queue is drained).
    ///
    /// The returned bytes are the value exactly as it was posted; interpreting them (opening the
    /// sealed store-and-forward envelope) is the read-on-reconnect story (#215), not this wrapper.
    pub fn pop(&mut self) -> Result<Option<Vec<u8>>, BingleError> {
        let params = self.params_with_failover("pop")?;
        let request = build_pop_request(self.pop_type, &params);
        let finalized = self.submit_and_finalize("pop", request)?;
        Ok(match finalized.result {
            Some(bytes) if !bytes.is_empty() => Some(bytes),
            _ => None,
        })
    }

    /// The client bound to the currently active node.
    fn current_client(&self) -> &SidewinderClient {
        match &self.transport {
            Transport::Bearer(client) => client,
            Transport::Discovered(d) => &d.client,
        }
    }

    /// Whether this transport can fail over to another node (only the discovery transport can).
    fn can_failover(&self) -> bool {
        matches!(self.transport, Transport::Discovered(_))
    }

    /// Fetch suggested params — the first node round-trip of any operation, and thus the reachability
    /// probe. On the discovery transport a transport failure fails over to the next resolved node, and
    /// re-resolves the set from chain once on exhaustion (story #244, deliverable 3). Failover is
    /// confined to this probe, **before** any transaction is submitted, so a finality timeout partway
    /// through an operation never re-submits on a second node (which could double-post).
    fn params_with_failover(&mut self, operation: &str) -> Result<SuggestedParams, BingleError> {
        let mut reresolved = false;
        loop {
            match self
                .current_client()
                .params()
                .map_err(|e| map_error("params", e))
            {
                Ok(params) => return Ok(params),
                Err(e) if is_transport_error(&e) && self.can_failover() => {
                    tracing::warn!(
                        "[mailbox {operation}] node unreachable ({e}); trying another discovered node"
                    );
                    if !self.advance_or_reresolve(&mut reresolved)? {
                        return Err(e);
                    }
                }
                Err(e) => return Err(e),
            }
        }
    }

    /// Advance to the next reachable discovered node; on exhaustion, re-resolve the set from chain
    /// exactly once (tracked by `reresolved`) and start over. Returns `true` when a fresh node was
    /// connected (retry the probe), `false` when the set is exhausted and re-resolution has already
    /// been tried. A no-op returning `false` on the bearer transport. Reconnect/re-resolve failures
    /// surface as errors.
    fn advance_or_reresolve(&mut self, reresolved: &mut bool) -> Result<bool, BingleError> {
        let algo = self.algo.clone();
        let Transport::Discovered(d) = &mut self.transport else {
            return Ok(false);
        };
        if let Some(next) = next_node_index(d.active, d.nodes.len()) {
            d.active = next;
            d.client =
                SidewinderClient::connect(algo, &d.nodes[next]).map_err(map_connect_error)?;
            return Ok(true);
        }
        if *reresolved {
            return Ok(false);
        }
        *reresolved = true;
        tracing::info!("[mailbox] all discovered nodes exhausted; re-resolving from chain");
        let reachable = resolve_reachable(&algo, &d.discovery)?;
        d.nodes = reachable;
        d.active = 0;
        d.client = SidewinderClient::connect(algo, &d.nodes[0]).map_err(map_connect_error)?;
        Ok(true)
    }

    /// Submit `request`, then poll it to the `final` stage, returning the finalised transaction.
    /// A transaction that reaches `failed`, or does not finalise within the configured finality
    /// timeout, is a surfaced error. Runs against the currently active node (see
    /// [`params_with_failover`](Self::params_with_failover), which already selected a reachable one).
    fn submit_and_finalize(
        &self,
        operation: &str,
        request: TransactionRequest,
    ) -> Result<PendingTransaction, BingleError> {
        let txid = self
            .current_client()
            .submit_transaction(&request)
            .map_err(|e| map_error(operation, e))?;
        // Log the transaction id at submit so it can be tracked on the node while it finalises.
        tracing::info!("[mailbox {operation}] submitted tx {txid}");
        self.poll_to_final(operation, &txid)
    }

    /// Poll `txid` until it reaches `final` (or `failed`), long-polling each request. A read node may
    /// not know a just-submitted transaction yet, so a not-found is tolerated as propagation lag
    /// until the deadline; any other error, a `failed` stage, or missing finality is an error.
    fn poll_to_final(
        &self,
        operation: &str,
        txid: &str,
    ) -> Result<PendingTransaction, BingleError> {
        let deadline = Instant::now() + self.finality_timeout;
        // The most recent `stage` seen, so a timeout reports how far the transaction actually got
        // (e.g. stuck at `Pending` vs. reaching `Verified` but never anchored).
        let mut last_stage: Option<String> = None;
        loop {
            match self.current_client().watch(txid, false, WATCH_WAIT_SECS) {
                Ok(pending) => {
                    last_stage = Some(format!("{:?}", pending.stage));
                    tracing::debug!("[mailbox {operation}] {txid} stage={:?}", pending.stage);
                    if pending.stage == Stage::Final {
                        if let Some(error) = pending.error {
                            return Err(BingleError::Other(format!(
                                "sidewinder {operation} transaction {txid} finalised with error: {error:?}"
                            )));
                        }
                        return Ok(pending);
                    }
                    if pending.stage == Stage::Failed {
                        tracing::warn!("[mailbox {operation}] {txid} FAILED: {:?}", pending.error);
                        return Err(BingleError::Other(format!(
                            "sidewinder {operation} transaction {txid} failed: {:?}",
                            pending.error
                        )));
                    }
                }
                Err(e) => {
                    if !is_not_found(&e) {
                        return Err(map_error(operation, e));
                    }
                    std::thread::sleep(NOT_FOUND_BACKOFF);
                }
            }
            if Instant::now() >= deadline {
                return Err(BingleError::Retryable(format!(
                    "sidewinder {operation} transaction {txid} did not finalise within {:?} \
                     (last stage: {})",
                    self.finality_timeout,
                    last_stage.as_deref().unwrap_or("unknown")
                )));
            }
        }
    }
}

/// Build the `post(recipient, message)` transaction: `arg[0]` is the recipient address string bytes
/// (the queue key), `arg[1]` is the message. A unique note keeps two otherwise-identical posts from
/// colliding on the content-address transaction identifier.
#[doc(hidden)]
pub fn build_post_request(
    post_type: u32,
    recipient: &str,
    message: &[u8],
    params: &sidewinder_ops::SuggestedParams,
) -> TransactionRequest {
    TransactionRequest {
        txn_type: post_type,
        args: vec![
            AppArg::Bytes(recipient.as_bytes().to_vec()),
            AppArg::Bytes(message.to_vec()),
        ],
        max_fee: params.min_fee,
        first_valid: params.last_round,
        last_valid: params.last_round + params.max_validity_window,
        instance: params.instance_id.clone(),
        note: Some(AlgoOps::unique_note()),
        group: None,
    }
}

/// Build the `pop()` transaction: no arguments (the queue key is the authenticated sender). A unique
/// note keeps repeated pops distinct on the content address.
#[doc(hidden)]
pub fn build_pop_request(
    pop_type: u32,
    params: &sidewinder_ops::SuggestedParams,
) -> TransactionRequest {
    TransactionRequest {
        txn_type: pop_type,
        args: vec![],
        max_fee: params.min_fee,
        first_valid: params.last_round,
        last_valid: params.last_round + params.max_validity_window,
        instance: params.instance_id.clone(),
        note: Some(AlgoOps::unique_note()),
        group: None,
    }
}

/// Connect the discovery transport: resolve the app's reachable cluster nodes on-chain and connect to
/// the first over identity-pinned mutual TLS, retaining the rest for failover.
fn connect_discovered(algo: &AlgoOps, app_id: u64) -> Result<Transport, BingleError> {
    let discovery = DiscoveryConfig::bingle(app_id);
    let nodes = resolve_reachable(algo, &discovery)?;
    // `nodes` is non-empty (resolve_reachable errors otherwise), so index 0 is present.
    let client = SidewinderClient::connect(algo.clone(), &nodes[0]).map_err(map_connect_error)?;
    Ok(Transport::Discovered(DiscoveredTransport {
        discovery,
        nodes,
        active: 0,
        client,
    }))
}

/// Resolve the app's cluster nodes and keep only those advertising a reachable endpoint (a peer-only
/// node holds `allow_sw_node` but publishes no [`EndpointRecord`](sidewinder_ops::EndpointRecord), and
/// [`DiscoveredNode::base_url`] is then `None`). Errors (retryable) when the scan fails or no node has
/// published an endpoint — so a gate that is on but has nowhere to reach is reported, not silent.
fn resolve_reachable(
    algo: &AlgoOps,
    discovery: &DiscoveryConfig,
) -> Result<Vec<DiscoveredNode>, BingleError> {
    let nodes = resolve_nodes(algo, discovery).map_err(|e| {
        BingleError::Retryable(format!(
            "sidewinder discovery for app {} failed: {e}",
            discovery.app_id
        ))
    })?;
    let reachable: Vec<DiscoveredNode> = nodes
        .into_iter()
        .filter(|node| node.base_url().is_some())
        .collect();
    if reachable.is_empty() {
        return Err(BingleError::Retryable(format!(
            "no permitted cluster node of Bingle app {} has published a reachable endpoint",
            discovery.app_id
        )));
    }
    Ok(reachable)
}

/// The next node index to try after `active` in a set of `count` nodes, or `None` when the set is
/// exhausted. Pure, so the failover cursor is unit-tested without a node.
#[doc(hidden)]
pub fn next_node_index(active: usize, count: usize) -> Option<usize> {
    let next = active + 1;
    (next < count).then_some(next)
}

/// Whether an error should trigger failover to another node: a transport/reachability failure, which
/// [`map_error`] classifies as [`BingleError::Retryable`].
fn is_transport_error(error: &BingleError) -> bool {
    matches!(error, BingleError::Retryable(_))
}

/// Map a mutual-TLS connect failure to a retryable [`BingleError`] so failover/re-resolve can proceed.
fn map_connect_error(error: anyhow::Error) -> BingleError {
    BingleError::Retryable(format!("sidewinder mutual-TLS connect failed: {error}"))
}

/// Whether an error from the client is a Sidewinder not-found — the propagation-lag case tolerated
/// while polling for finality.
fn is_not_found(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<SidewinderError>()
        .is_some_and(|se| se.kind == SidewinderErrorKind::NotFound)
}

/// Map a client error to a [`BingleError`], classifying unreachable/transient causes as retryable so
/// the store-and-forward path can distinguish "try again" from a persistent failure.
fn map_error(operation: &str, error: anyhow::Error) -> BingleError {
    if let Some(se) = error.downcast_ref::<SidewinderError>() {
        if matches!(
            se.kind,
            SidewinderErrorKind::HostUnreachable | SidewinderErrorKind::TransientFailure
        ) {
            return BingleError::Retryable(format!("sidewinder {operation}: {se}"));
        }
    }
    BingleError::Other(format!("sidewinder {operation} failed: {error}"))
}
