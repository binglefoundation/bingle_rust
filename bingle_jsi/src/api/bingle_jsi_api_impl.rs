use std::net::{SocketAddr, ToSocketAddrs};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use serde_json::Value as JsonValue;

use crate::api::bingle_jsi_api::BingleJsiApi;
use crate::api::callback::{
    ListeningCallback, LogCallback, MessageCallback, PushRegistrationCallback,
};
use crate::api::error::BingleJsiError;
use crate::api::types::{
    BingleJsiConfig, BingleMessage, Contact, ContactSource, FailureKind, HandleLookupPartialResult,
    InetSocketAddress, Keypair, KeypairStatus, KeypairStatusResponse, Message, NatType,
    NatTypeResponse, NetworkSourceKey, VersionInfo,
};
use algo_ops::error::AlgoErrorKind;
use bingle_core::api::bingle_api::{
    BingleApi, BingleApiBoth, BingleError, SendFailureKind, StartOptions,
};
use bingle_core::api::bingle_api_impl::BingleApiImpl;
use bingle_core::api::network_endpoint::NetworkEndpoint;
use bingle_core::engine::BingleAccess;
use bingle_core::util::config_utils::{
    parse_node_file_with_ids, parse_stun_file, parse_stun_list, resolve_app_asset_ids,
};
use bingle_local::api::MailboxConfig;
use bingle_local::api::bingle_local_api::BingleLocalApi;
use bingle_local::api::bingle_local_api_impl::{BingleApiLocalImpl, LocalApiConfig};
// Shared outbound-send retry policy (issue #82). Re-exported so existing
// `bingle_jsi::api::bingle_jsi_api_impl::{is_transient_send_failure, pending_failure_reason}`
// paths (and tests) keep resolving after the move to bingle_local.
use bingle_local::api::send_retry::{
    RETRY_BACKOFF, SendFailure, classify_send_error, select_sendable_message,
};
pub use bingle_local::api::send_retry::{is_transient_send_failure, pending_failure_reason};

/// Concrete implementation of BingleJsiApi backed by BingleApiImpl and BingleApiLocalImpl.
pub struct BingleJsiApiImpl {
    api: Arc<dyn BingleApiBoth>,
    messages: Arc<Mutex<Vec<JsonValue>>>,
    local_api: Option<Arc<Mutex<Box<dyn BingleLocalApi>>>>,
    local_file: Option<PathBuf>,
    nat_type: Arc<Mutex<String>>,
    message_callback: Arc<Mutex<Option<Box<dyn MessageCallback>>>>,
    listening_callback: Arc<Mutex<Option<Box<dyn ListeningCallback>>>>,
    push_registration_callback: Arc<Mutex<Option<Box<dyn PushRegistrationCallback>>>>,
    listening: Arc<AtomicBool>,
    processing_thread: Arc<Mutex<Option<JoinHandle<()>>>>,
    started: Arc<Mutex<bool>>,
    opts: Arc<Mutex<StartOptions>>,
}

/// Convert a JSI NetworkSourceKey to the internal NetworkEndpoint type.
fn nsk_to_endpoint(nsk: &NetworkSourceKey) -> NetworkEndpoint {
    if let Some(relay_id) = &nsk.relay_id {
        let relay_addr = nsk.relay_address.as_ref().and_then(isa_to_socket_addr);
        NetworkEndpoint::new_relay(relay_id.clone(), relay_addr, nsk.relay_channel)
    } else if let Some(addr) = nsk
        .inet_socket_address
        .as_ref()
        .and_then(isa_to_socket_addr)
    {
        NetworkEndpoint::new_direct(addr)
    } else {
        NetworkEndpoint::new_unset()
    }
}

/// Convert an InetSocketAddress to a SocketAddr.
fn isa_to_socket_addr(isa: &InetSocketAddress) -> Option<SocketAddr> {
    format!("{}:{}", isa.host, isa.port)
        .to_socket_addrs()
        .ok()?
        .next()
}

/// Convert a BingleMessage (uniffi record) to a serde_json Value.
fn message_to_json(msg: &BingleMessage) -> JsonValue {
    let mut map = serde_json::Map::new();
    if let Some(app) = &msg.app {
        map.insert("app".to_string(), JsonValue::String(app.clone()));
    }
    if let Some(t) = &msg.r#type {
        map.insert("type".to_string(), JsonValue::String(t.clone()));
    }
    if let Some(tag) = &msg.tag {
        map.insert("tag".to_string(), JsonValue::String(tag.clone()));
    }
    if let Some(rt) = &msg.response_tag {
        map.insert("responseTag".to_string(), JsonValue::String(rt.clone()));
    }
    if let Some(text) = &msg.text {
        map.insert("text".to_string(), JsonValue::String(text.clone()));
    }
    if let Some(data) = &msg.data {
        if let Ok(parsed) = serde_json::from_str::<JsonValue>(data) {
            map.insert("data".to_string(), parsed);
        } else {
            map.insert("data".to_string(), JsonValue::String(data.clone()));
        }
    }
    if let Some(cipher_suite) = &msg.cipher_suite {
        map.insert(
            "cipherSuite".to_string(),
            JsonValue::String(cipher_suite.clone()),
        );
    }
    JsonValue::Object(map)
}

/// Convert a serde_json Value back to a BingleMessage (uniffi record).
fn json_to_message(val: &JsonValue) -> BingleMessage {
    BingleMessage {
        app: val
            .get("app")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        r#type: val
            .get("type")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        tag: val
            .get("tag")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        response_tag: val
            .get("responseTag")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        text: val
            .get("text")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        data: val.get("data").map(|v| v.to_string()),
        cipher_suite: val
            .get("cipherSuite")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
    }
}

/// Save local state if local_api and local_file are both configured.
fn save_if_configured(
    local_api: &Option<Arc<Mutex<Box<dyn BingleLocalApi>>>>,
    local_file: &Option<PathBuf>,
) {
    if let (Some(local_arc), Some(path)) = (local_api, local_file)
        && let Ok(guard) = local_arc.lock()
    {
        let _ = guard.save(path.to_string_lossy().as_ref());
    }
}

/// Parse a keypair status string (from BingleLocalApi) into a KeypairStatus enum.
fn parse_keypair_status(status: &str) -> KeypairStatus {
    match status {
        "UNFUNDED" => KeypairStatus::Unfunded,
        "FUNDED" => KeypairStatus::Funded,
        "ACTIVE" => KeypairStatus::Active,
        "UPGRADE_REQUIRED" => KeypairStatus::UpgradeRequired,
        _ => KeypairStatus::None,
    }
}

/// Map a stored [`bingle_local::api::Message`] to the FFI [`Message`] surfaced to JS.
///
/// Every field of the local record is carried across, including the store-and-forward fields
/// (`sent_time`, `delivered_time`, `signature`) added in issue #204 — a dedicated test asserts none
/// are dropped, so a future field addition that forgets the bridge fails CI.
#[doc(hidden)]
pub fn local_message_to_jsi(m: bingle_local::api::Message) -> Message {
    Message {
        sender_handle: m.sender_handle,
        recipient_handles: m.recipient_handles,
        timestamp: m.timestamp,
        text: m.text,
        cipher_suite: m.cipher_suite,
        progress: m.progress,
        failure_reason: m.failure_reason,
        failure_kind: m.failure_kind.map(send_failure_kind_to_ffi),
        sent_time: m.sent_time,
        delivered_time: m.delivered_time,
        signature: m.signature,
    }
}

/// Map the core `SendFailureKind` to the FFI-exposed `FailureKind` (issue #99). Same pattern as the
/// `KeypairStatus` mapping: the enum is defined at the FFI boundary and translated here.
fn send_failure_kind_to_ffi(kind: SendFailureKind) -> FailureKind {
    match kind {
        SendFailureKind::HandleNotFound => FailureKind::HandleNotFound,
        SendFailureKind::HandleLookupFailed => FailureKind::HandleLookupFailed,
        SendFailureKind::RecipientNotAdvertised => FailureKind::RecipientNotAdvertised,
        SendFailureKind::InvalidRecipientId => FailureKind::InvalidRecipientId,
        SendFailureKind::NoRelayAvailable => FailureKind::NoRelayAvailable,
        SendFailureKind::RelayAllocationFailed => FailureKind::RelayAllocationFailed,
        SendFailureKind::PeerUnreachable => FailureKind::PeerUnreachable,
        SendFailureKind::NoResponse => FailureKind::NoResponse,
        SendFailureKind::MalformedAdvert => FailureKind::MalformedAdvert,
        SendFailureKind::ProtocolError => FailureKind::ProtocolError,
        SendFailureKind::NotReady => FailureKind::NotReady,
        SendFailureKind::Unknown => FailureKind::Unknown,
    }
}

/// Parse a NAT type string into a NatType enum.
fn parse_nat_type(nat: &str) -> NatType {
    match nat {
        "NoConnection" => NatType::NoConnection,
        "Symmetric" => NatType::Symmetric,
        "Restricted" => NatType::Restricted,
        "FullCone" => NatType::FullCone,
        _ => NatType::Unknown,
    }
}

fn bingle_error_to_jsi(e: BingleError) -> BingleJsiError {
    match e {
        BingleError::Algo(ae) if ae.kind == AlgoErrorKind::HostUnreachable => {
            BingleJsiError::NoBlockchain {
                reason: ae.to_string(),
            }
        }
        BingleError::Retryable(reason) => BingleJsiError::Retryable { reason },
        BingleError::HandleTaken(owner) => BingleJsiError::HandleTaken {
            reason: format!("handle already in use by {}", owner),
        },
        _ => BingleJsiError::InternalError {
            reason: e.to_string(),
        },
    }
}

impl BingleJsiApiImpl {
    /// Initialize the Bingle JSI API from a typed configuration object.
    ///
    /// Each field on `BingleJsiConfig` corresponds to a command-line parameter
    /// of the bingle_webserver. When generated as TypeScript via uniffi, the
    /// config becomes a plain object:
    ///
    /// ```typescript
    /// {
    ///   handle: string | null,
    ///   passphrase: string | null,
    ///   relay: boolean,
    ///   static_ip: string | null,
    ///   stun_servers: string | null,
    ///   stun_servers_file: string | null,
    ///   node_file: string | null,
    ///   log_level: string | null,
    ///   app_id: number | null,
    ///   asset_id: number | null,
    ///   handle_cache_expiry_secs: number | null,
    ///   debug: boolean,
    ///   local: string | null,
    ///   notify_gateway_url: string | null,
    ///   notify_on_giveup: boolean | null,
    /// }
    /// ```
    pub fn init(config: BingleJsiConfig) -> Result<Arc<Self>, BingleJsiError> {
        let local_file: Option<PathBuf> = config.local.map(PathBuf::from);

        // Map BingleJsiConfig directly to StartOptions.
        let handle = match (&config.handle, &local_file) {
            (Some(h), _) => h.clone(),
            (None, Some(_)) => String::new(), // local mode: handle set later via registerKeypair
            (None, None) => {
                return Err(BingleJsiError::InvalidRequest {
                    reason: "Missing handle: provide handle or enable local mode".to_string(),
                });
            }
        };

        let static_ip: Option<SocketAddr> = config
            .static_ip
            .map(|v| {
                v.parse::<SocketAddr>()
                    .map_err(|e| BingleJsiError::InvalidRequest {
                        reason: format!("Invalid static_ip '{}': {}", v, e),
                    })
            })
            .transpose()?;

        let stun_servers: Option<Vec<SocketAddr>> = if let Some(ref file) = config.stun_servers_file
        {
            Some(parse_stun_file(file).map_err(|e| BingleJsiError::InvalidRequest { reason: e })?)
        } else if let Some(ref list) = config.stun_servers {
            Some(parse_stun_list(list).map_err(|e| BingleJsiError::InvalidRequest { reason: e })?)
        } else {
            None
        };

        let mut algo_provider_config = None;
        let mut algo_network = None;
        let mut node_app_id = None;
        let mut node_asset_id = None;
        if let Some(ref node_file) = config.node_file {
            let (net, cfg, nid_app, nid_asset) = parse_node_file_with_ids(node_file)
                .map_err(|e| BingleJsiError::InvalidRequest { reason: e })?;
            algo_network = net;
            algo_provider_config = Some(cfg);
            node_app_id = nid_app;
            node_asset_id = nid_asset;
        }

        let (app_id, asset_id) =
            match resolve_app_asset_ids(node_app_id, node_asset_id, config.app_id, config.asset_id)
            {
                Ok((a, b)) => (Some(a), Some(b)),
                Err(_) => (None, None),
            };

        let handle_cache_expiry = config
            .handle_cache_expiry_secs
            .map(std::time::Duration::from_secs);

        let opts = StartOptions {
            handle,
            algo_passphrase: config.passphrase,
            static_ip,
            am_relay: config.relay,
            stun_servers,
            algo_provider_config,
            algo_network,
            app_id,
            asset_id,
            log_level: config.log_level,
            handle_cache_expiry,
            dangerous_debug: false, // We don't want to enable dangerous debug DTLS features
            log_mode: bingle_core::util::logging::LogMode::JS,
            wait_response_timeout: None, // default to DEFAULT_WAIT_RESPONSE_TIMEOUT
        };

        // Install the callback log bridge (no-op if already installed by a prior init call)
        let log_level = opts.log_level.as_deref().unwrap_or("info");
        let level_filter = match log_level.to_ascii_lowercase().as_str() {
            "trace" => tracing_subscriber::filter::LevelFilter::TRACE,
            "debug" => tracing_subscriber::filter::LevelFilter::DEBUG,
            "info" => tracing_subscriber::filter::LevelFilter::INFO,
            "warn" | "warning" => tracing_subscriber::filter::LevelFilter::WARN,
            "error" => tracing_subscriber::filter::LevelFilter::ERROR,
            _ => tracing_subscriber::filter::LevelFilter::INFO,
        };
        crate::api::log_bridge::install_log_bridge(level_filter);

        let api = BingleApiImpl::new(&opts);
        let messages: Arc<Mutex<Vec<JsonValue>>> = Arc::new(Mutex::new(Vec::new()));
        let nat_type: Arc<Mutex<String>> = Arc::new(Mutex::new("Unknown".to_string()));

        // Initialize local API if --local was provided
        let mut local_api: Option<Arc<Mutex<Box<dyn BingleLocalApi>>>> = None;
        if let Some(path) = &local_file {
            // Give-up nudge (bingle_notify #11/#17): the feature stays on by default; supplying a
            // gateway URL is what activates it. An explicit `notify_on_giveup: false` disables it
            // even when a URL is set.
            let mut cfg = LocalApiConfig::with_notify(
                opts.algo_provider_config.clone().unwrap_or_default(),
                opts.app_id.unwrap_or(0),
                opts.asset_id.unwrap_or(0),
                config.notify_on_giveup,
                config.notify_gateway_url.clone(),
            );
            // Thread the build's APNs environment through for `/register` (defaults to sandbox when
            // the caller leaves it null).
            if let Some(env) = config.notify_env.clone() {
                cfg.notify_env = env;
            }
            // Store-and-forward (epic #200): configure the Sidewinder Mailbox when both the node URL
            // and bearer token are supplied. Either one alone leaves store-and-forward unconfigured.
            cfg = cfg.with_sidewinder(MailboxConfig::from_parts(
                config.sidewinder_node_url.clone(),
                config.sidewinder_token.clone(),
            ));
            // Store-and-forward gates (#212): each side is independent and defaults off when unset.
            cfg = cfg.with_store_and_forward(
                config.store_and_forward_send,
                config.store_and_forward_receive,
            );
            let mut impl_api = BingleApiLocalImpl::new(cfg);
            if path.exists()
                && let Err(e) = impl_api.load(path.to_string_lossy().as_ref())
            {
                tracing::warn!("Failed to load local state from {}: {}", path.display(), e);
            }
            local_api = Some(Arc::new(Mutex::new(Box::new(impl_api))));
        }

        let listening_callback: Arc<Mutex<Option<Box<dyn ListeningCallback>>>> =
            Arc::new(Mutex::new(None));

        let message_callback: Arc<Mutex<Option<Box<dyn MessageCallback>>>> =
            Arc::new(Mutex::new(None));

        let listening = Arc::new(AtomicBool::new(false));
        let processing_thread = Arc::new(Mutex::new(None));
        let started = Arc::new(Mutex::new(false));
        let opts_mutex = Arc::new(Mutex::new(opts.clone()));

        let api_instance = Arc::new(Self {
            api: api.clone(),
            messages: messages.clone(),
            local_api: local_api.clone(),
            local_file: local_file.clone(),
            nat_type: nat_type.clone(),
            message_callback: message_callback.clone(),
            listening_callback: listening_callback.clone(),
            push_registration_callback: Arc::new(Mutex::new(None)),
            listening: listening.clone(),
            processing_thread: processing_thread.clone(),
            started: started.clone(),
            opts: opts_mutex,
        });

        // Output INFO with version information as early as possible
        if let Ok(versions) = api_instance.get_versions() {
            tracing::info!("Bingle JSI initializing. Versions: {:?}", versions);
        }

        // Setup on-listening handler to update nat_type and invoke user callback
        {
            let nat_type_for_closure = nat_type.clone();
            let lcb = listening_callback.clone();
            let listening_atomic = listening.clone();
            api.access(|api_mut| {
                let on_listening: Arc<bingle_core::api::bingle_api::OnListeningHandler> = Arc::new(
                    move |listening_val: bool, nt: bingle_core::engine::NatType| {
                        let type_str = if listening_val {
                            format!("{:?}", nt)
                        } else {
                            "Unknown".to_string()
                        };
                        tracing::info!(
                            "on_listening: listening={} nat_type={}",
                            listening_val,
                            type_str
                        );
                        listening_atomic.store(listening_val, Ordering::SeqCst);
                        if let Ok(mut guard) = nat_type_for_closure.lock() {
                            *guard = type_str.clone();
                        }
                        // Invoke user listening callback if registered
                        if let Ok(guard) = lcb.lock()
                            && let Some(ref callback) = *guard
                        {
                            callback.on_listening(listening_val, type_str);
                        }
                    },
                );
                api_mut.set_on_listening(Some(on_listening));
            });
        }

        // Setup on-message handler to queue received messages
        {
            let msgs = messages.clone();
            let local_api_for_closure = local_api.clone();
            let local_file_for_closure = local_file.clone();
            let api_for_handle = api.clone();
            let cb = message_callback.clone();
            api.access(|api_mut| {
                let on_message: Arc<bingle_core::api::bingle_api::OnMessageHandler> =
                    Arc::new(move |sender, sender_handle, message| {
                        tracing::info!("[BingleJsiApiImpl][init handler] Received message from {}: {}", sender_handle, message);
                        // Invoke user callback if registered
                        if let Ok(guard) = cb.lock() {
                            if let Some(ref callback) = *guard {
                                tracing::info!("[BingleJsiApiImpl][init handler] Invoking user callback for message from {}", sender_handle);
                                let bingle_msg = json_to_message(&message);
                                callback.on_message(
                                    sender.clone(),
                                    sender_handle.clone(),
                                    bingle_msg,
                                );
                            }
                            else {
                                tracing::info!("[BingleJsiApiImpl][init handler] No user callback registered");
                            }
                        }
                        else {
                            tracing::warn!("[BingleJsiApiImpl][init handler] Could not lock callback");
                        }

                        let text = message
                            .get("text")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| message.to_string());
                        let cipher_suite = message
                            .get("cipher_suite")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());
                        let mut m = msgs.lock().unwrap();
                        m.push(message.clone());
                        // Store message in local API if configured
                        if let Some(local_arc) = &local_api_for_closure
                            && let Ok(mut guard) = local_arc.lock() {
                                let timestamp = std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .map(|d| d.as_millis() as i64)
                                    .unwrap_or(0);
                                let recipient = match api_for_handle.get_handle() {
                                    Some(h) => h,
                                    None => {
                                        tracing::error!("[on_message] get_handle returned None");
                                        return;
                                    }
                                };
                                if let Err(e) = guard.add_message(
                                    sender_handle.clone(),
                                    vec![recipient],
                                    timestamp,
                                    text,
                                    cipher_suite,
                                ) {
                                    tracing::warn!("[on_message] failed to add message: {}", e);
                                }
                                if let Some(path) = &local_file_for_closure {
                                    let _ = guard.save(path.to_string_lossy().as_ref());
                                }
                            }
                    });
                api_mut.set_on_message(Some(on_message));
            });
        }

        // Determine whether to start the API immediately or defer
        let mut api_started = false;
        if local_file.is_some() {
            if let Some(local_arc) = &local_api
                && let Ok(guard) = local_arc.lock()
                && let Ok(mut status) = guard.keypair_status()
            {
                // If the configured app has been superseded, do not migrate or start:
                // the client must be upgraded. The UPGRADE_REQUIRED status is surfaced to
                // the UI to prompt the user to update the app.
                if status.status == "UPGRADE_REQUIRED" {
                    tracing::warn!(
                        "Bingle API start blocked: configured app is superseded, upgrade required"
                    );
                } else {
                    // If not yet active on the configured app, attempt a one-time migration
                    // of local state from a blessed predecessor app, then re-check. This
                    // transparently upgrades a user whose registration lives on an older app
                    // instead of prompting them to register again. Best-effort: failures do
                    // not block start.
                    if status.status != "ACTIVE" {
                        match guard.ensure_local_migrated() {
                            Ok(Some(tx)) => {
                                tracing::info!(
                                    "Migrated local state from predecessor app (tx {})",
                                    tx
                                );
                                if let Ok(s2) = guard.keypair_status() {
                                    status = s2;
                                }
                            }
                            Ok(None) => {}
                            // Expect this on a first run with no keypair
                            Err(e) => tracing::info!(
                                "Local-state migration check failed (continuing): {}",
                                e
                            ),
                        }
                    }
                    if status.status == "ACTIVE" {
                        let api_clone = api.clone();
                        let mut opts_clone = opts.clone();
                        if let Some(handle) = &status.handle {
                            opts_clone.handle = handle.clone();
                        }
                        if let Ok(Some(kp)) = guard.get_keypair() {
                            opts_clone.algo_passphrase = Some(kp.passphrase);
                        }
                        api_clone.access(|api_mut| {
                            if let Err(e) = api_mut.start(&opts_clone) {
                                tracing::error!("Failed to start Bingle API: {}", e);
                            }
                        });
                        api_started = true;
                        tracing::info!("Bingle API started (keypair is ACTIVE)");
                    } else {
                        tracing::info!(
                            "Bingle API start deferred (keypair status: {})",
                            status.status
                        );
                    }
                }
            }
        } else {
            let api_clone = api.clone();
            let opts_clone = opts.clone();
            api_clone.access(|api_mut| {
                if let Err(e) = api_mut.start(&opts_clone) {
                    tracing::error!("Failed to start Bingle API: {}", e);
                }
            });
            api_started = true;
        }

        if api_started {
            if let Ok(mut started_guard) = started.lock() {
                *started_guard = true;
            }
        } else {
            tracing::info!("Bingle API not yet started; waiting for keypair to become ACTIVE");
        }

        Ok(api_instance)
    }

    fn run_processing_loop(
        api: Arc<dyn BingleApiBoth>,
        local_api: Option<Arc<Mutex<Box<dyn BingleLocalApi>>>>,
        listening: Arc<AtomicBool>,
        nat_type: Arc<Mutex<String>>,
        started: Arc<Mutex<bool>>,
    ) {
        tracing::info!("[BingleJsiApiImpl] Starting background processing loop");

        // Delivery (relay discovery, DTLS, blockchain lookups) can block or take a long time when
        // connectivity is flaky. To guarantee this loop can never hang, all sending happens on a
        // dedicated worker thread, one message at a time; this scheduler thread only hands work
        // off and reaps results, so it never blocks on a send.
        const SEND_WATCHDOG: std::time::Duration = std::time::Duration::from_secs(45);
        type PendingMsg = bingle_local::api::bingle_local_api::Message;

        let Some(local_api) = local_api else {
            tracing::info!("[BingleJsiApiImpl] no local API; processing loop idle");
            while *started.lock().unwrap_or_else(|e| e.into_inner()) {
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
            tracing::info!("[BingleJsiApiImpl] Background processing loop stopped");
            return;
        };

        // The transport can deliver only when we are listening and not in NoConnection (no route).
        let transport_up = || {
            listening.load(Ordering::SeqCst)
                && nat_type
                    .lock()
                    .map(|g| *g != "NoConnection")
                    .unwrap_or(true)
        };

        // Scheduler -> worker: the pending message to send. Worker -> scheduler: the outcome
        // (timestamp, progress, failure_reason) to persist.
        let (req_tx, req_rx) = std::sync::mpsc::channel::<PendingMsg>();
        // Worker -> scheduler result: (timestamp, progress, failure_reason, failure_kind). The typed
        // kind (issue #99) is carried alongside the human reason so it is persisted on the message.
        let (res_tx, res_rx) =
            std::sync::mpsc::channel::<(i64, f32, Option<String>, Option<SendFailureKind>)>();

        // Dedicated sender worker: owns every send_message_to_handle call. It blocks here (never on
        // the scheduler) if a send is slow; a panic is contained by catch_unwind so it can't die.
        let worker = {
            let api = api.clone();
            let local_api = local_api.clone();
            std::thread::Builder::new()
                .name("bingle-sender".to_string())
                .spawn(move || {
                    while let Ok(msg) = req_rx.recv() {
                        let timestamp = msg.timestamp;
                        let progress_local = local_api.clone();
                        let progress_callback =
                            Arc::new(move |percent: u8, _status_msg: String| {
                                if let Ok(mut guard) = progress_local.lock() {
                                    let _ = guard.update_message_status(
                                        timestamp,
                                        percent as f32 / 100.0,
                                        None,
                                        None,
                                    );
                                }
                            });

                        let mut all_success = true;
                        let mut last_failure: Option<SendFailure> = None;
                        for handle in &msg.recipient_handles {
                            let payload = serde_json::json!({ "text": msg.text });
                            tracing::info!(
                                "BingleJsiApiImpl][send_message_to_handles] Sending message to handle: {:?}",
                                handle
                            );
                            // A panic anywhere in the delivery path must not kill the worker.
                            let res = match std::panic::catch_unwind(
                                std::panic::AssertUnwindSafe(|| {
                                    api.send_message_to_handle(
                                        handle,
                                        payload,
                                        Some(progress_callback.clone()),
                                    )
                                }),
                            ) {
                                Ok(r) => r,
                                Err(_) => {
                                    tracing::error!(
                                        "[BingleJsiApiImpl] send_message_to_handle panicked for {:?}",
                                        handle
                                    );
                                    Err(BingleError::Send {
                                        kind: SendFailureKind::NotReady,
                                        detail: "send panicked".to_string(),
                                    })
                                }
                            };
                            // Classify the typed cause directly from the result (issue #99); no
                            // string parsing. `None` means delivered.
                            if let Some(failure) = classify_send_error(&res) {
                                all_success = false;
                                let retryable = failure.kind.is_retryable();
                                last_failure = Some(failure);
                                // Stop trying further recipients on a transient failure (the
                                // transport is likely down); a permanent one falls through to the
                                // next recipient. Mirrors the pre-#99 behaviour.
                                if retryable {
                                    break;
                                }
                            }
                        }

                        // A transient failure keeps the message pending (progress 0.0) for retry;
                        // only a permanent failure is marked terminal (progress 1.0).
                        let result = if all_success {
                            (timestamp, 1.0_f32, None, None)
                        } else {
                            let failure = last_failure.unwrap_or_else(|| SendFailure {
                                kind: SendFailureKind::Unknown,
                                reason: "unknown send failure".to_string(),
                            });
                            let transient = failure.kind.is_retryable();
                            let (progress, level) = if transient {
                                (0.0_f32, "transient")
                            } else {
                                (1.0_f32, "permanent")
                            };
                            // Surface a concise, human-readable failure_reason plus the typed kind on
                            // the queued message so the app can process the error reliably (issues
                            // #43, #99). Transient failures stay pending (progress 0.0) and keep
                            // retrying; both clear automatically on the next successful send.
                            tracing::debug!(
                                "[BingleJsiApiImpl] pending message {} send failed ({}, {:?}): {}",
                                timestamp,
                                level,
                                failure.kind,
                                failure.reason
                            );
                            (
                                timestamp,
                                progress,
                                Some(failure.reason),
                                Some(failure.kind),
                            )
                        };
                        if res_tx.send(result).is_err() {
                            break; // scheduler gone
                        }
                    }
                    tracing::info!("[BingleJsiApiImpl] sender worker stopped");
                })
                .ok()
        };

        // Scheduler: never blocks on a send. Hands the worker the oldest *eligible* pending message,
        // one at a time, and reaps results. If a send is slow/stuck we simply don't hand off
        // another — so the loop stays responsive and cannot hang; the message retries once the
        // worker frees. `retry_after` holds per-message backoff deadlines so a repeatedly-failing
        // recipient can't block delivery to others (head-of-line blocking).
        let mut in_flight: Option<(i64, std::time::Instant)> = None;
        let mut warned_stuck = false;
        let mut retry_after: std::collections::HashMap<i64, std::time::Instant> =
            std::collections::HashMap::new();
        while *started.lock().unwrap_or_else(|e| e.into_inner()) {
            // Reap a completed result (blocks up to 200ms — responsive, no busy-spin).
            match res_rx.recv_timeout(std::time::Duration::from_millis(200)) {
                Ok((ts, progress, reason, kind)) => {
                    if let Ok(mut guard) = local_api.lock() {
                        let _ = guard.update_message_status(ts, progress, reason, kind);
                    }
                    // progress >= 1.0 is terminal (delivered or permanently failed): no more
                    // retries. Otherwise the message stays pending — back it off so it yields the
                    // head of the queue to other messages before its next attempt.
                    if progress >= 1.0 {
                        retry_after.remove(&ts);
                    } else {
                        retry_after.insert(ts, std::time::Instant::now() + RETRY_BACKOFF);
                    }
                    if in_flight.map(|(t, _)| t == ts).unwrap_or(false) {
                        in_flight = None;
                        warned_stuck = false;
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    tracing::error!("[BingleJsiApiImpl] sender worker gone; stopping loop");
                    break;
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            }

            // Watchdog: note (but tolerate) a send taking a long time. We never start a concurrent
            // send, so a stuck send just defers delivery until it returns (bounded by the send
            // path's own timeouts). The scheduler itself keeps running.
            if let Some((ts, since)) = in_flight {
                if !warned_stuck && since.elapsed() > SEND_WATCHDOG {
                    tracing::warn!(
                        "[BingleJsiApiImpl] pending message {} send exceeded {:?}; deferring further drains until it completes",
                        ts,
                        SEND_WATCHDOG
                    );
                    warned_stuck = true;
                }
            }

            // Hand off the oldest eligible pending message when the worker is free and the
            // transport is up.
            if in_flight.is_none() && transport_up() {
                let pending = match local_api.lock() {
                    Ok(guard) => guard.get_pending_messages().ok().unwrap_or_default(),
                    Err(_) => {
                        tracing::error!("[BingleJsiApiImpl] local_api lock poisoned");
                        Vec::new()
                    }
                };
                // Forget backoff deadlines for messages that are no longer pending (delivered or
                // removed) so the map stays bounded.
                retry_after.retain(|ts, _| pending.iter().any(|m| m.timestamp == *ts));
                let next =
                    select_sendable_message(pending, &retry_after, std::time::Instant::now());
                if let Some(msg) = next {
                    tracing::info!(
                        "[BingleJsiApiImpl] Processing pending message: {}",
                        msg.timestamp
                    );
                    in_flight = Some((msg.timestamp, std::time::Instant::now()));
                    if req_tx.send(msg).is_err() {
                        tracing::error!("[BingleJsiApiImpl] sender worker gone; stopping loop");
                        break;
                    }
                }
            }
        }

        // Shutdown: close the request channel so the worker exits. We don't join — if a send is in
        // progress the worker finishes it and exits on its own; not joining keeps shutdown from
        // blocking on a slow send.
        drop(req_tx);
        drop(worker);
        tracing::info!("[BingleJsiApiImpl] Background processing loop stopped");
    }

    pub fn api_for_tests(&self) -> Arc<dyn BingleApiBoth> {
        self.api.clone()
    }

    /// The local API this instance was initialized with, if any. Lets tests reach the concrete
    /// implementation (via `BingleLocalApi::as_any_mut`) to observe or override behaviour wired up
    /// by `init` — e.g. the give-up nudge sender (bingle_notify #17).
    pub fn local_api_for_tests(&self) -> Option<Arc<Mutex<Box<dyn BingleLocalApi>>>> {
        self.local_api.clone()
    }

    pub fn init_for_tests(
        api: Arc<dyn BingleApiBoth>,
        local_api: Option<Arc<Mutex<Box<dyn BingleLocalApi>>>>,
    ) -> Arc<Self> {
        let messages: Arc<Mutex<Vec<JsonValue>>> = Arc::new(Mutex::new(Vec::new()));
        let nat_type: Arc<Mutex<String>> = Arc::new(Mutex::new("Unknown".to_string()));
        let listening = Arc::new(AtomicBool::new(false));
        let started = Arc::new(Mutex::new(false));
        let mut opts_obj = StartOptions::new("test".to_string());
        opts_obj.dangerous_debug = true;
        let opts = Arc::new(Mutex::new(opts_obj));

        let listening_atomic = listening.clone();
        api.access(|api_mut| {
            api_mut.set_on_listening(Some(Arc::new(move |listening_val, _nat| {
                listening_atomic.store(listening_val, Ordering::SeqCst);
            })));
        });

        Arc::new(Self {
            api,
            messages,
            local_api,
            local_file: None,
            nat_type,
            message_callback: Arc::new(Mutex::new(None)),
            listening_callback: Arc::new(Mutex::new(None)),
            push_registration_callback: Arc::new(Mutex::new(None)),
            listening,
            processing_thread: Arc::new(Mutex::new(None)),
            started,
            opts,
        })
    }

    pub fn set_local_api_for_tests(&self, _local_api: Arc<Mutex<Box<dyn BingleLocalApi>>>) {
        // This is safe because we are just replacing the Arc in the Option.
        // Wait, self.local_api is Option<Arc<Mutex<Box<dyn BingleLocalApi>>>>.
        // It is NOT behind a Mutex itself. So I can't change it if I only have &self.
    }
}

/// Guard helper: obtain the local API mutex guard or return an error.
fn local_api_guard(
    local_api: &Option<Arc<Mutex<Box<dyn BingleLocalApi>>>>,
) -> Result<std::sync::MutexGuard<'_, Box<dyn BingleLocalApi>>, BingleJsiError> {
    let local_arc = local_api
        .as_ref()
        .ok_or_else(|| BingleJsiError::InvalidRequest {
            reason: "Local API not enabled (set 'local' in BingleJsiConfig)".to_string(),
        })?;
    local_arc.lock().map_err(|_| BingleJsiError::InternalError {
        reason: "Local API lock poisoned".to_string(),
    })
}

impl BingleJsiApi for BingleJsiApiImpl {
    fn handle_lookup(&self, handle: String) -> Result<String, BingleJsiError> {
        match self.api.handle_lookup(&handle) {
            Ok(Some(id)) => Ok(id),
            Ok(None) => Err(BingleJsiError::NotFound {
                reason: format!("Handle '{}' not found", handle),
            }),
            Err(e) => Err(bingle_error_to_jsi(e)),
        }
    }

    fn handle_lookup_partial(
        &self,
        handle: String,
    ) -> Result<HandleLookupPartialResult, BingleJsiError> {
        match self.api.handle_lookup_partial(&handle) {
            Ok(Some((id, canonical_handle))) => Ok(HandleLookupPartialResult {
                id,
                canonical_handle,
            }),
            Ok(None) => Err(BingleJsiError::NotFound {
                reason: format!("No handle matching '{}' found", handle),
            }),
            Err(e) => Err(bingle_error_to_jsi(e)),
        }
    }

    fn send_message_to_id(
        &self,
        user_id: String,
        message: BingleMessage,
    ) -> Result<bool, BingleJsiError> {
        let json = message_to_json(&message);
        self.api
            .send_message_to_id(&user_id, json, None)
            .map_err(bingle_error_to_jsi)
    }

    fn send_message_to_handle(
        &self,
        handle: String,
        message: BingleMessage,
    ) -> Result<bool, BingleJsiError> {
        let json = message_to_json(&message);
        self.api
            .send_message_to_handle(&handle, json, None)
            .map_err(bingle_error_to_jsi)
    }

    fn send_message_to_network(
        &self,
        network_source_key: NetworkSourceKey,
        user_id: String,
        message: BingleMessage,
    ) -> Result<bool, BingleJsiError> {
        let endpoint = nsk_to_endpoint(&network_source_key);
        let json = message_to_json(&message);
        self.api
            .send_message_to_network(&endpoint, &user_id, json, None)
            .map_err(bingle_error_to_jsi)
    }

    fn send_message_to_id_with_response(
        &self,
        user_id: String,
        message: BingleMessage,
    ) -> Result<BingleMessage, BingleJsiError> {
        let json = message_to_json(&message);
        match self
            .api
            .send_message_to_id_with_response(&user_id, json, None)
        {
            Ok(resp) => Ok(json_to_message(&resp)),
            Err(e) => Err(bingle_error_to_jsi(e)),
        }
    }

    fn send_message_to_handle_with_response(
        &self,
        handle: String,
        message: BingleMessage,
    ) -> Result<BingleMessage, BingleJsiError> {
        let json = message_to_json(&message);
        match self
            .api
            .send_message_to_handle_with_response(&handle, json, None)
        {
            Ok(resp) => Ok(json_to_message(&resp)),
            Err(e) => Err(bingle_error_to_jsi(e)),
        }
    }

    fn send_message_to_network_with_response(
        &self,
        network_source_key: NetworkSourceKey,
        user_id: String,
        message: BingleMessage,
    ) -> Result<BingleMessage, BingleJsiError> {
        let endpoint = nsk_to_endpoint(&network_source_key);
        let json = message_to_json(&message);
        match self
            .api
            .send_message_to_network_with_response(&endpoint, &user_id, json, None)
        {
            Ok(resp) => Ok(json_to_message(&resp)),
            Err(e) => Err(bingle_error_to_jsi(e)),
        }
    }

    fn queued(&self) -> Result<Vec<BingleMessage>, BingleJsiError> {
        let guard = self
            .messages
            .lock()
            .map_err(|_| BingleJsiError::InternalError {
                reason: "Messages lock poisoned".to_string(),
            })?;
        Ok(guard.iter().map(json_to_message).collect())
    }

    fn version(&self) -> Result<VersionInfo, BingleJsiError> {
        let info = bingle_core::util::version::get_version_info();
        Ok(VersionInfo {
            version: info.version,
            git_sha: info.git_sha,
            build_timestamp: info.build_timestamp,
            build_number: info.build_number,
        })
    }

    fn get_versions(
        &self,
    ) -> Result<std::collections::HashMap<String, VersionInfo>, BingleJsiError> {
        let mut map = std::collections::HashMap::new();

        let base_info = bingle_core::module_version::get_version();
        map.insert(
            "Base".to_string(),
            VersionInfo {
                version: base_info.version,
                git_sha: base_info.git_sha,
                build_timestamp: base_info.build_timestamp,
                build_number: base_info.build_number,
            },
        );

        let jsi_info = crate::module_version::get_version();
        map.insert(
            "JSI".to_string(),
            VersionInfo {
                version: jsi_info.version,
                git_sha: jsi_info.git_sha,
                build_timestamp: jsi_info.build_timestamp,
                build_number: jsi_info.build_number,
            },
        );

        Ok(map)
    }

    fn get_nat_type(&self) -> Result<NatTypeResponse, BingleJsiError> {
        let guard = self
            .nat_type
            .lock()
            .map_err(|_| BingleJsiError::InternalError {
                reason: "NAT type lock poisoned".to_string(),
            })?;
        Ok(NatTypeResponse {
            nat_type: parse_nat_type(&guard),
        })
    }

    fn generate_keypair(&self) -> Result<Keypair, BingleJsiError> {
        let mut guard = local_api_guard(&self.local_api)?;
        let kp = guard.generate_keypair().map_err(bingle_error_to_jsi)?;
        drop(guard);
        save_if_configured(&self.local_api, &self.local_file);
        Ok(Keypair {
            id: kp.id,
            passphrase: kp.passphrase,
        })
    }

    fn import_keypair(&self, passphrase: String) -> Result<Keypair, BingleJsiError> {
        let mut guard = local_api_guard(&self.local_api)?;
        let kp = guard
            .import_keypair(passphrase)
            .map_err(bingle_error_to_jsi)?;
        drop(guard);
        save_if_configured(&self.local_api, &self.local_file);
        Ok(Keypair {
            id: kp.id,
            passphrase: kp.passphrase,
        })
    }

    fn register_keypair(&self, handle: String) -> Result<(), BingleJsiError> {
        let guard = local_api_guard(&self.local_api)?;
        // Route through bingle_error_to_jsi so a duplicate handle surfaces as the typed
        // HandleTaken (issue #15 A1) rather than a generic InternalError, letting the app
        // prompt for a different handle instead of showing a funding/other failure.
        guard
            .register_keypair(handle)
            .map_err(bingle_error_to_jsi)?;
        drop(guard);
        save_if_configured(&self.local_api, &self.local_file);
        Ok(())
    }

    fn sign_notify_envelope(
        &self,
        route: String,
        iss: String,
        audience: String,
        token: String,
        env: String,
        nonce: String,
        exp: i64,
    ) -> Result<String, BingleJsiError> {
        let guard = local_api_guard(&self.local_api)?;
        // get_algo_ops binds to the active keypair (no network); it errors if none is set, which
        // maps to a typed jsi error rather than signing with a bogus key.
        let ops = guard.get_algo_ops().map_err(bingle_error_to_jsi)?;
        drop(guard);
        ops.sign_notify_envelope(&route, &iss, &audience, &token, &env, &nonce, exp)
            .map_err(|e| BingleJsiError::InternalError {
                reason: e.to_string(),
            })
    }

    fn request_push_registration(&self) -> Result<(), BingleJsiError> {
        // Rust cannot call the UIKit registration APIs; it asks the host, whose thin Swift bridge
        // does the platform calls and later returns the token via register_apns_token.
        let guard =
            self.push_registration_callback
                .lock()
                .map_err(|_| BingleJsiError::InternalError {
                    reason: "push_registration_callback lock poisoned".to_string(),
                })?;
        match guard.as_ref() {
            Some(cb) => {
                cb.on_request_registration();
                Ok(())
            }
            None => Err(BingleJsiError::InternalError {
                reason: "no push registration callback set".to_string(),
            }),
        }
    }

    fn register_apns_token(&self, token: Vec<u8>) -> Result<bool, BingleJsiError> {
        let guard = local_api_guard(&self.local_api)?;
        guard
            .register_apns_token(token)
            .map_err(bingle_error_to_jsi)
    }

    fn apns_registration_failed(&self, reason: String) {
        tracing::warn!("[notify][register] iOS push registration failed: {reason}");
    }

    fn add_contact(
        &self,
        handle: String,
        id: String,
        source: ContactSource,
    ) -> Result<(), BingleJsiError> {
        let local_source = match source {
            ContactSource::Manual => bingle_local::api::bingle_local_api::ContactSource::Manual,
            ContactSource::Received => bingle_local::api::bingle_local_api::ContactSource::Received,
        };
        let mut guard = local_api_guard(&self.local_api)?;
        guard
            .add_contact(handle, id, local_source)
            .map_err(bingle_error_to_jsi)?;
        drop(guard);
        save_if_configured(&self.local_api, &self.local_file);
        Ok(())
    }

    fn block_contact(&self, id: String) -> Result<(), BingleJsiError> {
        let mut guard = local_api_guard(&self.local_api)?;
        guard.block_contact(id).map_err(bingle_error_to_jsi)?;
        drop(guard);
        save_if_configured(&self.local_api, &self.local_file);
        Ok(())
    }

    fn remove_contact(&self, id: String) -> Result<(), BingleJsiError> {
        let mut guard = local_api_guard(&self.local_api)?;
        guard.remove_contact(id).map_err(bingle_error_to_jsi)?;
        drop(guard);
        save_if_configured(&self.local_api, &self.local_file);
        Ok(())
    }

    fn is_blocked(&self, id: String) -> Result<bool, BingleJsiError> {
        let guard = local_api_guard(&self.local_api)?;
        guard.is_blocked(&id).map_err(bingle_error_to_jsi)
    }

    fn get_contacts(&self) -> Result<Vec<Contact>, BingleJsiError> {
        let guard = local_api_guard(&self.local_api)?;
        let contacts = guard.get_contacts().map_err(bingle_error_to_jsi)?;
        Ok(contacts
            .into_iter()
            .map(|c| Contact {
                handle: c.handle,
                id: c.id,
                fields: c.fields,
            })
            .collect())
    }

    fn add_message(
        &self,
        sender_handle: String,
        recipient_handles: Vec<String>,
        timestamp: i64,
        text: String,
        cipher_suite: Option<String>,
    ) -> Result<(), BingleJsiError> {
        let mut guard = local_api_guard(&self.local_api)?;
        guard
            .add_message(
                sender_handle,
                recipient_handles,
                timestamp,
                text,
                cipher_suite,
            )
            .map_err(bingle_error_to_jsi)?;
        drop(guard);
        save_if_configured(&self.local_api, &self.local_file);
        Ok(())
    }

    fn get_messages(&self) -> Result<Vec<Message>, BingleJsiError> {
        let guard = local_api_guard(&self.local_api)?;
        let messages = guard.get_messages().map_err(bingle_error_to_jsi)?;
        Ok(messages.into_iter().map(local_message_to_jsi).collect())
    }

    fn queue_message(
        &self,
        recipient_handles: Vec<String>,
        text: String,
    ) -> Result<(), BingleJsiError> {
        let mut guard = local_api_guard(&self.local_api)?;
        guard
            .queue_message(recipient_handles, text)
            .map_err(bingle_error_to_jsi)?;
        drop(guard);
        save_if_configured(&self.local_api, &self.local_file);
        Ok(())
    }

    fn update_message_status(
        &self,
        timestamp: i64,
        progress: f32,
        failure_reason: Option<String>,
    ) -> Result<(), BingleJsiError> {
        let mut guard = local_api_guard(&self.local_api)?;
        // The FFI method is unchanged (issue #99): the app supplies only a reason string, so no
        // typed cause is available on this direct path — the worker path carries the real kind.
        guard
            .update_message_status(timestamp, progress, failure_reason, None)
            .map_err(bingle_error_to_jsi)?;
        drop(guard);
        save_if_configured(&self.local_api, &self.local_file);
        Ok(())
    }

    fn network_available(&self, _force_recheck: bool) -> Result<bool, BingleJsiError> {
        // Availability *for sending* depends only on the P2P transport, not the Algorand node
        // (issue #31). Message delivery uses the STUN-discovered endpoint and DTLS relays; handle
        // lookups are served from cache. So a node outage must not mark the network unavailable for
        // messaging — that would wrongly stop queue draining while messages can still be delivered.
        // We are available when listening and the engine reports a usable route (not NoConnection).
        if !self.listening.load(Ordering::SeqCst) {
            return Ok(false);
        }
        let no_route = self
            .nat_type
            .lock()
            .map(|g| *g == "NoConnection")
            .unwrap_or(false);
        Ok(!no_route)
    }

    fn keypair_status(&self) -> Result<KeypairStatusResponse, BingleJsiError> {
        let guard = local_api_guard(&self.local_api)?;
        let status = guard.keypair_status().map_err(bingle_error_to_jsi)?;
        Ok(KeypairStatusResponse {
            status: parse_keypair_status(&status.status),
            id: status.id,
            handle: status.handle,
            required_algo: status.required_algo,
            stale: status.stale,
        })
    }

    fn save(&self, path: String) -> Result<(), BingleJsiError> {
        let guard = local_api_guard(&self.local_api)?;
        guard.save(&path).map_err(bingle_error_to_jsi)
    }

    fn load(&self, path: String) -> Result<(), BingleJsiError> {
        let mut guard = local_api_guard(&self.local_api)?;
        guard.load(&path).map_err(bingle_error_to_jsi)
    }

    fn set_message_callback(&self, callback: Box<dyn MessageCallback>) {
        if let Ok(mut guard) = self.message_callback.lock() {
            *guard = Some(callback);
            tracing::info!("[BingleJsiApiImpl][set_message_callback] Registered message callback");
        } else {
            tracing::error!("Failed to lock message_callback");
        }
    }

    fn set_push_registration_callback(&self, callback: Box<dyn PushRegistrationCallback>) {
        if let Ok(mut guard) = self.push_registration_callback.lock() {
            *guard = Some(callback);
            tracing::info!(
                "[BingleJsiApiImpl][set_push_registration_callback] Registered push registration callback"
            );
        } else {
            tracing::error!("Failed to lock push_registration_callback");
        }
    }

    fn set_log_callback(&self, callback: Box<dyn LogCallback>) {
        crate::api::log_bridge::set_global_log_callback(callback);
    }

    fn set_listening_callback(&self, callback: Box<dyn ListeningCallback>) {
        if let Ok(mut guard) = self.listening_callback.lock() {
            *guard = Some(callback);
            tracing::info!(
                "[BingleJsiApiImpl][set_listening_callback] Registered listening callback"
            );
        } else {
            tracing::error!("Failed to lock listening_callback");
        }
    }

    fn start(&self) -> Result<(), BingleJsiError> {
        // Check if already started
        let already_started = {
            let guard = self
                .started
                .lock()
                .map_err(|_| BingleJsiError::InternalError {
                    reason: "Started flag lock poisoned".to_string(),
                })?;
            *guard
        };

        if already_started {
            tracing::info!(
                "[BingleJsiApiImpl][start] Engine already started, skipping engine start"
            );
            // This is expectable, do not return here
        } else {
            tracing::info!("[BingleJsiApiImpl][start] Starting engine");

            // Check keypair status is FUNDED or ACTIVE (bypass in debug mode)
            let mut bypass_status = false;
            if let Ok(opts) = self.opts.lock()
                && opts.dangerous_debug
            {
                tracing::info!("[BingleJsiApiImpl][start] Bypassing keypair status check");
                bypass_status = true;
            }

            let guard = local_api_guard(&self.local_api)?;
            let status = guard.keypair_status().map_err(bingle_error_to_jsi)?;
            tracing::info!(
                "[BingleJsiApiImpl][start] Keypair status: {:?}",
                status.status
            );
            let kp_status = parse_keypair_status(&status.status);
            if !bypass_status
                && kp_status != KeypairStatus::Funded
                && kp_status != KeypairStatus::Active
            {
                return Err(BingleJsiError::InvalidRequest {
                    reason: format!(
                        "Cannot start engine: keypair must be FUNDED or ACTIVE, but is {:?}",
                        kp_status
                    ),
                });
            }
            tracing::info!("[BingleJsiApiImpl][start] Keypair status check passed");

            // Build opts with handle and passphrase from local API
            let mut opts_clone = self
                .opts
                .lock()
                .map_err(|_| BingleJsiError::InternalError {
                    reason: "Opts lock poisoned".to_string(),
                })?
                .clone();
            if let Some(handle) = &status.handle {
                opts_clone.handle = handle.clone();
            }
            if let Ok(Some(kp)) = guard.get_keypair() {
                opts_clone.algo_passphrase = Some(kp.passphrase);
            }
            drop(guard);

            tracing::info!(
                "[BingleJsiApiImpl][start] Built opts with handle {} and passphrase from local API",
                opts_clone.handle
            );
            // Start the engine
            let api_clone = self.api.clone();
            let mut start_err = None;
            api_clone.access(|api_mut| {
                if let Err(e) = api_mut.start(&opts_clone) {
                    tracing::error!("Failed to start Bingle API: {}", e);
                    start_err = Some(bingle_error_to_jsi(e));
                }
            });

            if let Some(err) = start_err {
                tracing::info!(
                    "[BingleJsiApiImpl][start] Failed to start Bingle API: {:?}",
                    err
                );
                return Err(err);
            }

            // Mark as started
            if let Ok(mut started_guard) = self.started.lock() {
                *started_guard = true;
            }
        }
        tracing::info!("[BingleJsiApiImpl][start] Bingle API started, will run processing loop");

        // Start processing thread
        let api_inner = self.api.clone();
        let local_inner = self.local_api.clone();
        let listening_inner = self.listening.clone();
        let nat_type_inner = self.nat_type.clone();
        let started_inner = self.started.clone();

        let processing_thread = std::thread::spawn(move || {
            Self::run_processing_loop(
                api_inner,
                local_inner,
                listening_inner,
                nat_type_inner,
                started_inner,
            );
        });

        if let Ok(mut guard) = self.processing_thread.lock() {
            *guard = Some(processing_thread);
        }

        tracing::info!("[BingleJsiApiImpl][start] Bingle API has started processing loop");

        // Output INFO with version information
        if let Ok(versions) = self.get_versions() {
            tracing::info!(
                "BingleJsiApiImpl][start] Bingle JSI started. Versions: {:?}",
                versions
            );
        }

        tracing::info!("BingleJsiApiImpl][start] Bingle engine started");
        Ok(())
    }

    fn stop(&self) -> Result<(), BingleJsiError> {
        // Mark as stopped
        {
            let mut guard = self
                .started
                .lock()
                .map_err(|_| BingleJsiError::InternalError {
                    reason: "Started flag lock poisoned".to_string(),
                })?;
            if !*guard {
                return Ok(()); // Already stopped
            }
            *guard = false;
        }

        // Tell a relay we are leaving so it removes our DDB entry. Best-effort:
        // relies on the transport ACK, and a failure here must not block shutdown.
        match self.api.ddb_signoff() {
            Ok(()) => tracing::info!("Sent DDB signoff"),
            Err(e) => tracing::warn!("DDB signoff failed (continuing shutdown): {}", e),
        }

        // Stop the engine
        self.api.access(|api_mut| {
            api_mut.stop();
        });

        // Join the processing thread
        let mut thread_guard =
            self.processing_thread
                .lock()
                .map_err(|_| BingleJsiError::InternalError {
                    reason: "Processing thread lock poisoned".to_string(),
                })?;
        if let Some(handle) = thread_guard.take() {
            let _ = handle.join();
        }

        tracing::info!("Bingle engine stopped");
        Ok(())
    }

    fn is_started(&self) -> bool {
        self.started.lock().map(|g| *g).unwrap_or(false)
    }

    fn foregrounding(&self) {
        // Refresh the relay registration off the bridge thread: on_foreground may perform a
        // blocking Listen round-trip, so we must not block the app's AppState callback (issue #50).
        tracing::info!("[BingleJsiApiImpl] foregrounding()");
        let api = self.api.clone();
        std::thread::spawn(move || {
            api.with_engine(&mut |e| e.on_foreground());
        });
    }

    fn backgrounding(&self) {
        // Pause the keep-alive off the bridge thread (stopping it joins a worker) (issue #50).
        tracing::info!("[BingleJsiApiImpl] backgrounding()");
        let api = self.api.clone();
        std::thread::spawn(move || {
            api.with_engine(&mut |e| e.on_background());
        });
    }
}

#[cfg(test)]
mod tests {
    use super::select_sendable_message;
    use bingle_local::api::bingle_local_api::Message;
    use std::collections::HashMap;
    use std::time::{Duration, Instant};

    fn msg(timestamp: i64) -> Message {
        Message {
            sender_handle: "me".to_string(),
            recipient_handles: vec!["them".to_string()],
            timestamp,
            text: "hi".to_string(),
            cipher_suite: None,
            progress: Some(0.0),
            failure_reason: None,
            failure_kind: None,
            sent_time: None,
            delivered_time: None,
            signature: None,
        }
    }

    #[test]
    fn picks_oldest_when_no_backoff() {
        let now = Instant::now();
        let retry_after = HashMap::new();
        let chosen = select_sendable_message(vec![msg(30), msg(10), msg(20)], &retry_after, now);
        assert_eq!(chosen.map(|m| m.timestamp), Some(10));
    }

    #[test]
    fn skips_backed_off_head_and_picks_next_eligible() {
        // Regression: the oldest message (ts=10) is backing off, so it must NOT starve
        // the newer, eligible message (ts=20) — that was the head-of-line block.
        let now = Instant::now();
        let mut retry_after = HashMap::new();
        retry_after.insert(10, now + Duration::from_secs(5)); // not yet eligible
        let chosen = select_sendable_message(vec![msg(10), msg(20)], &retry_after, now);
        assert_eq!(chosen.map(|m| m.timestamp), Some(20));
    }

    #[test]
    fn retries_backed_off_message_once_deadline_passes() {
        let now = Instant::now();
        let mut retry_after = HashMap::new();
        retry_after.insert(10, now - Duration::from_secs(1)); // deadline already passed
        let chosen = select_sendable_message(vec![msg(10), msg(20)], &retry_after, now);
        assert_eq!(chosen.map(|m| m.timestamp), Some(10));
    }

    #[test]
    fn returns_none_when_all_backed_off() {
        let now = Instant::now();
        let mut retry_after = HashMap::new();
        retry_after.insert(10, now + Duration::from_secs(5));
        retry_after.insert(20, now + Duration::from_secs(5));
        let chosen = select_sendable_message(vec![msg(10), msg(20)], &retry_after, now);
        assert!(chosen.is_none());
    }
}
