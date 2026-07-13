use std::net::{SocketAddr, ToSocketAddrs};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use serde_json::Value as JsonValue;

use crate::api::bingle_jsi_api::BingleJsiApi;
use crate::api::callback::{ListeningCallback, LogCallback, MessageCallback};
use crate::api::error::BingleJsiError;
use crate::api::types::{
    BingleJsiConfig, BingleMessage, Contact, ContactSource, HandleLookupPartialResult,
    InetSocketAddress, Keypair, KeypairStatus, KeypairStatusResponse, Message, NatType,
    NatTypeResponse, NetworkSourceKey, VersionInfo,
};
use bingle_core::api::bingle_api::{BingleApi, BingleApiBoth, BingleError, StartOptions};
use bingle_core::api::bingle_api_impl::BingleApiImpl;
use bingle_core::api::network_endpoint::NetworkEndpoint;
use bingle_core::blockchain::error::AlgoErrorKind;
use bingle_core::engine::BingleAccess;
use bingle_core::util::config_utils::{
    parse_node_file_with_ids, parse_stun_file, parse_stun_list, resolve_app_asset_ids,
};
use bingle_local::api::bingle_local_api::BingleLocalApi;
use bingle_local::api::bingle_local_api_impl::{BingleApiLocalImpl, LocalApiConfig};

/// Concrete implementation of BingleJsiApi backed by BingleApiImpl and BingleApiLocalImpl.
pub struct BingleJsiApiImpl {
    api: Arc<dyn BingleApiBoth>,
    messages: Arc<Mutex<Vec<JsonValue>>>,
    local_api: Option<Arc<Mutex<Box<dyn BingleLocalApi>>>>,
    local_file: Option<PathBuf>,
    nat_type: Arc<Mutex<String>>,
    message_callback: Arc<Mutex<Option<Box<dyn MessageCallback>>>>,
    listening_callback: Arc<Mutex<Option<Box<dyn ListeningCallback>>>>,
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

/// Whether a failed pending-message send looks transient (i.e. connectivity-related) and so the
/// message should stay pending to be retried, rather than being marked permanently failed.
/// Recognises retryable errors, undelivered sends, and no-route/no-relay/unreachable conditions.
pub fn is_transient_send_failure(err: &str) -> bool {
    let e = err.to_ascii_lowercase();
    e.starts_with("retryable:")
        || e.contains("send returned false")
        || e.contains("no available relay")
        || e.contains("no relay")
        || e.contains("unreachable")
        || e.contains("no route")
        || e.contains("noconnection")
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
            let cfg = LocalApiConfig {
                algo_config: opts.algo_provider_config.clone().unwrap_or_default(),
                app_id: opts.app_id.unwrap_or(0),
                asset_id: opts.asset_id.unwrap_or(0),
            };
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
        while *started.lock().unwrap_or_else(|e| e.into_inner()) {
            // Only drain the pending queue when the transport can actually deliver: we must be
            // listening and not in NoConnection (no route). Otherwise leave messages pending so
            // they are retried once connectivity returns, without noisy failed sends (issue #31).
            let transport_up = listening.load(Ordering::SeqCst)
                && nat_type
                    .lock()
                    .map(|g| *g != "NoConnection")
                    .unwrap_or(true);
            if transport_up
                && let Some(ref local_arc) = local_api
            {
                let pending_message_list = match local_arc.lock() {
                    Ok(guard) => guard.get_pending_messages(),
                    Err(_) => {
                        tracing::error!("[BingleJsiApiImpl] local_api lock poisoned");
                        break;
                    }
                };

                if let Ok(messages) = pending_message_list {
                    for msg in messages {
                        tracing::info!(
                            "[BingleJsiApiImpl] Processing pending message: {}",
                            msg.timestamp
                        );
                        let api_clone = api.clone();
                        let local_api_clone = local_arc.clone();
                        let timestamp = msg.timestamp;

                        let progress_callback =
                            Arc::new(move |percent: u8, _status_msg: String| {
                                // Note: progress messages don't come with a good failure reason
                                // we could separate these through the API but TMWFN
                                if let Ok(mut guard) = local_api_clone.lock() {
                                    let _ = guard.update_message_status(
                                        timestamp,
                                        percent as f32 / 100.0,
                                        None,
                                    );
                                }
                            });

                        let mut all_success = true;
                        let mut last_error = None;

                        for handle in &msg.recipient_handles {
                            let payload = serde_json::json!({
                                "text": msg.text,
                            });

                            tracing::info!(
                                "BingleJsiApiImpl][send_message_to_handles] Sending message to handle: {:?}",
                                handle
                            );

                            let res = api_clone.send_message_to_handle(
                                handle,
                                payload,
                                Some(progress_callback.clone()),
                            );
                            match res {
                                Ok(true) => {}
                                Ok(false) => {
                                    all_success = false;
                                    last_error = Some("Send returned false".to_string());
                                }
                                Err(BingleError::Retryable(e)) => {
                                    all_success = false;
                                    last_error = Some(format!("Retryable: {}", e));
                                    break;
                                }
                                Err(e) => {
                                    all_success = false;
                                    last_error = Some(e.to_string());
                                }
                            }
                        }

                        if all_success {
                            if let Ok(mut guard) = local_arc.lock() {
                                let _ = guard.update_message_status(timestamp, 1.0, None);
                            }
                        } else if let Some(err) = last_error {
                            // A transient failure (retryable, undelivered, or no route/relay while
                            // connectivity is flapping) keeps the message pending (progress 0.0) so
                            // it is retried on the next tick once the transport recovers; only a
                            // genuinely permanent failure is marked terminal (progress 1.0). This
                            // stops an offline/relay-down send from being silently dropped (#31).
                            let (progress, level) = if is_transient_send_failure(&err) {
                                (0.0, "transient")
                            } else {
                                (1.0, "permanent")
                            };
                            tracing::debug!(
                                "[BingleJsiApiImpl] pending message {} send failed ({}): {}",
                                timestamp,
                                level,
                                err
                            );
                            if let Ok(mut guard) = local_arc.lock() {
                                let _ =
                                    guard.update_message_status(timestamp, progress, Some(err));
                            }
                        }
                    }
                }
            }
            std::thread::sleep(std::time::Duration::from_secs(5));
        }
        tracing::info!("[BingleJsiApiImpl] Background processing loop stopped");
    }

    pub fn api_for_tests(&self) -> Arc<dyn BingleApiBoth> {
        self.api.clone()
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
        Ok(messages
            .into_iter()
            .map(|m| Message {
                sender_handle: m.sender_handle,
                recipient_handles: m.recipient_handles,
                timestamp: m.timestamp,
                text: m.text,
                cipher_suite: m.cipher_suite,
                progress: m.progress,
                failure_reason: m.failure_reason,
            })
            .collect())
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
        guard
            .update_message_status(timestamp, progress, failure_reason)
            .map_err(bingle_error_to_jsi)?;
        drop(guard);
        save_if_configured(&self.local_api, &self.local_file);
        Ok(())
    }

    fn network_available(&self, force_recheck: bool) -> Result<bool, BingleJsiError> {
        // Transport-level short-circuit (issue #31 addendum): if we are not listening, or the
        // engine reports NoConnection (no route), treat as down without probing the node.
        if !self.listening.load(Ordering::SeqCst) {
            return Ok(false);
        }
        if self
            .nat_type
            .lock()
            .map(|g| *g == "NoConnection")
            .unwrap_or(false)
        {
            return Ok(false);
        }
        // Transport is up: confirm the Algorand node is actually reachable.
        let guard = local_api_guard(&self.local_api)?;
        guard
            .network_available(force_recheck)
            .map_err(bingle_error_to_jsi)
    }

    fn keypair_status(&self) -> Result<KeypairStatusResponse, BingleJsiError> {
        let guard = local_api_guard(&self.local_api)?;
        let status = guard.keypair_status().map_err(bingle_error_to_jsi)?;
        Ok(KeypairStatusResponse {
            status: parse_keypair_status(&status.status),
            id: status.id,
            handle: status.handle,
            required_algo: status.required_algo,
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
}
