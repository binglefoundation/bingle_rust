use std::net::{SocketAddr, ToSocketAddrs};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde_json::Value as JsonValue;

use rust_comms::api::bingle_api::{BingleApi, BingleError, StartOptions};
use rust_comms::api::bingle_api_impl::BingleApiImpl;
use rust_comms::api::network_endpoint::NetworkEndpoint;
use rust_comms::blockchain::error::AlgoErrorKind;
use rust_comms::engine::BingleAccessUnsafeForTests;
use rust_comms::util::cli_utils::{parse_stun_list, parse_stun_file, parse_node_file_with_ids, resolve_app_asset_ids};

use bingle_local::api::bingle_local_api::BingleLocalApi;
use bingle_local::api::bingle_local_api_impl::{BingleApiLocalImpl, LocalApiConfig};

use crate::api::callback::{ListeningCallback, LogCallback, MessageCallback};
use crate::api::error::BingleJsiError;
use crate::api::types::{
    BingleJsiConfig, BingleMessage, Contact, ContactSource, InetSocketAddress, Keypair,
    KeypairStatus, KeypairStatusResponse, Message, NatType, NatTypeResponse, NetworkSourceKey,
    VersionInfo,
};
use crate::api::bingle_jsi_api::BingleJsiApi;

/// Concrete implementation of BingleJsiApi backed by BingleApiImpl and BingleApiLocalImpl.
pub struct BingleJsiApiImpl {
    api: Arc<BingleApiImpl>,
    messages: Arc<Mutex<Vec<JsonValue>>>,
    local_api: Option<Arc<Mutex<Box<dyn BingleLocalApi>>>>,
    local_file: Option<PathBuf>,
    nat_type: Arc<Mutex<String>>,
    message_callback: Arc<Mutex<Option<Box<dyn MessageCallback>>>>,
    listening_callback: Arc<Mutex<Option<Box<dyn ListeningCallback>>>>,
    started: Arc<Mutex<bool>>,
    opts: Arc<Mutex<StartOptions>>,
}

/// Convert a JSI NetworkSourceKey to the internal NetworkEndpoint type.
fn nsk_to_endpoint(nsk: &NetworkSourceKey) -> NetworkEndpoint {
    if let Some(relay_id) = &nsk.relay_id {
        let relay_addr = nsk.relay_address.as_ref().and_then(|a| isa_to_socket_addr(a));
        NetworkEndpoint::new_relay(relay_id.clone(), relay_addr, nsk.relay_channel)
    } else if let Some(addr) = nsk.inet_socket_address.as_ref().and_then(|a| isa_to_socket_addr(a)) {
        NetworkEndpoint::new_direct(addr)
    } else {
        NetworkEndpoint::new_unset()
    }
}

/// Convert an InetSocketAddress to a SocketAddr.
fn isa_to_socket_addr(isa: &InetSocketAddress) -> Option<SocketAddr> {
    format!("{}:{}", isa.host, isa.port).to_socket_addrs().ok()?.next()
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
    JsonValue::Object(map)
}

/// Convert a serde_json Value back to a BingleMessage (uniffi record).
fn json_to_message(val: &JsonValue) -> BingleMessage {
    BingleMessage {
        app: val.get("app").and_then(|v| v.as_str()).map(|s| s.to_string()),
        r#type: val.get("type").and_then(|v| v.as_str()).map(|s| s.to_string()),
        tag: val.get("tag").and_then(|v| v.as_str()).map(|s| s.to_string()),
        response_tag: val.get("responseTag").and_then(|v| v.as_str()).map(|s| s.to_string()),
        text: val.get("text").and_then(|v| v.as_str()).map(|s| s.to_string()),
        data: val.get("data").map(|v| v.to_string()),
    }
}

/// Save local state if local_api and local_file are both configured.
fn save_if_configured(
    local_api: &Option<Arc<Mutex<Box<dyn BingleLocalApi>>>>,
    local_file: &Option<PathBuf>,
) {
    if let (Some(local_arc), Some(path)) = (local_api, local_file) {
        if let Ok(guard) = local_arc.lock() {
            let _ = guard.save(path.to_string_lossy().as_ref());
        }
    }
}

/// Parse a keypair status string (from BingleLocalApi) into a KeypairStatus enum.
fn parse_keypair_status(status: &str) -> KeypairStatus {
    match status {
        "UNFUNDED" => KeypairStatus::Unfunded,
        "FUNDED" => KeypairStatus::Funded,
        "ACTIVE" => KeypairStatus::Active,
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

fn bingle_error_to_jsi(e: BingleError) -> BingleJsiError {
    match e {
        BingleError::Algo(ae) if ae.kind == AlgoErrorKind::HostUnreachable => {
            BingleJsiError::NoBlockchain { reason: ae.to_string() }
        }
        _ => BingleJsiError::InternalError { reason: e.to_string() },
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

        let stun_servers: Option<Vec<SocketAddr>> = if let Some(ref file) = config.stun_servers_file {
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
            let (net, cfg, nid_app, nid_asset) =
                parse_node_file_with_ids(node_file).map_err(|e| BingleJsiError::InvalidRequest { reason: e })?;
            algo_network = net;
            algo_provider_config = Some(cfg);
            node_app_id = nid_app;
            node_asset_id = nid_asset;
        }

        let (app_id, asset_id) = match resolve_app_asset_ids(node_app_id, node_asset_id, config.app_id, config.asset_id) {
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
            dangerous_debug: false,
            log_mode: rust_comms::util::logging::LogMode::JS,
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
            if path.exists() {
                if let Err(e) = impl_api.load(path.to_string_lossy().as_ref()) {
                    tracing::warn!("Failed to load local state from {}: {}", path.display(), e);
                }
            }
            local_api = Some(Arc::new(Mutex::new(Box::new(impl_api))));
        }

        let listening_callback: Arc<Mutex<Option<Box<dyn ListeningCallback>>>> =
            Arc::new(Mutex::new(None));

        let message_callback: Arc<Mutex<Option<Box<dyn MessageCallback>>>> =
            Arc::new(Mutex::new(None));

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
            api.access_unsafe_for_tests(|api_mut| {
                let on_listening: Arc<rust_comms::api::bingle_api::OnListeningHandler> =
                    Arc::new(move |listening: bool, nt: rust_comms::engine::NatType| {
                        let type_str = if listening {
                            format!("{:?}", nt)
                        } else {
                            "Unknown".to_string()
                        };
                        tracing::info!("on_listening: listening={} nat_type={}", listening, type_str);
                        if let Ok(mut guard) = nat_type_for_closure.lock() {
                            *guard = type_str.clone();
                        }
                        // Invoke user listening callback if registered
                        if let Ok(guard) = lcb.lock() {
                            if let Some(ref callback) = *guard {
                                callback.on_listening(listening, type_str);
                            }
                        }
                    });
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
            api.access_unsafe_for_tests(|api_mut| {
                let on_message: Arc<rust_comms::api::bingle_api::OnMessageHandler> =
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
                        let mut m = msgs.lock().unwrap();
                        m.push(message.clone());
                        // Store message in local API if configured
                        if let Some(local_arc) = &local_api_for_closure {
                            if let Ok(mut guard) = local_arc.lock() {
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
                                ) {
                                    tracing::warn!("[on_message] failed to add message: {}", e);
                                }
                                if let Some(path) = &local_file_for_closure {
                                    let _ = guard.save(path.to_string_lossy().as_ref());
                                }
                            }
                        }
                    });
                api_mut.set_on_message(Some(on_message));
            });
        }

        // Determine whether to start the API immediately or defer
        let mut api_started = false;
        if local_file.is_some() {
            if let Some(local_arc) = &local_api {
                if let Ok(guard) = local_arc.lock() {
                    if let Ok(status) = guard.keypair_status() {
                        if status.status == "ACTIVE" {
                            let api_clone = api.clone();
                            let mut opts_clone = opts.clone();
                            if let Some(handle) = &status.handle {
                                opts_clone.handle = handle.clone();
                            }
                            if let Ok(Some(kp)) = guard.get_keypair() {
                                opts_clone.algo_passphrase = Some(kp.passphrase);
                            }
                            api_clone.access_unsafe_for_tests(|api_mut| {
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
            }
        } else {
            let api_clone = api.clone();
            let opts_clone = opts.clone();
            api_clone.access_unsafe_for_tests(|api_mut| {
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
}

/// Guard helper: obtain the local API mutex guard or return an error.
fn local_api_guard(
    local_api: &Option<Arc<Mutex<Box<dyn BingleLocalApi>>>>,
) -> Result<std::sync::MutexGuard<'_, Box<dyn BingleLocalApi>>, BingleJsiError> {
    let local_arc = local_api.as_ref().ok_or_else(|| BingleJsiError::InvalidRequest {
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

    fn send_message_to_id(
        &self,
        user_id: String,
        message: BingleMessage,
    ) -> Result<bool, BingleJsiError> {
        let json = message_to_json(&message);
        self.api.send_message_to_id(&user_id, json, None).map_err(bingle_error_to_jsi)
    }

    fn send_message_to_handle(
        &self,
        handle: String,
        message: BingleMessage,
    ) -> Result<bool, BingleJsiError> {
        let json = message_to_json(&message);
        self.api.send_message_to_handle(&handle, json, None).map_err(bingle_error_to_jsi)
    }

    fn send_message_to_network(
        &self,
        network_source_key: NetworkSourceKey,
        user_id: String,
        message: BingleMessage,
    ) -> Result<bool, BingleJsiError> {
        let endpoint = nsk_to_endpoint(&network_source_key);
        let json = message_to_json(&message);
        self.api.send_message_to_network(&endpoint, &user_id, json, None).map_err(bingle_error_to_jsi)
    }

    fn send_message_to_id_with_response(
        &self,
        user_id: String,
        message: BingleMessage,
    ) -> Result<BingleMessage, BingleJsiError> {
        let json = message_to_json(&message);
        match self.api.send_message_to_id_with_response(&user_id, json, None) {
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
        match self.api.send_message_to_handle_with_response(&handle, json, None) {
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
        let guard = self.messages.lock().map_err(|_| BingleJsiError::InternalError {
            reason: "Messages lock poisoned".to_string(),
        })?;
        Ok(guard.iter().map(|v| json_to_message(v)).collect())
    }

    fn version(&self) -> Result<VersionInfo, BingleJsiError> {
        let info = rust_comms::util::version::get_version_info();
        Ok(VersionInfo {
            version: info.version,
            git_sha: info.git_sha,
            build_timestamp: info.build_timestamp,
            build_number: info.build_number,
        })
    }

    fn get_versions(&self) -> Result<std::collections::HashMap<String, VersionInfo>, BingleJsiError> {
        let mut map = std::collections::HashMap::new();

        let base_info = rust_comms::module_version::get_version();
        map.insert("Base".to_string(), VersionInfo {
            version: base_info.version,
            git_sha: base_info.git_sha,
            build_timestamp: base_info.build_timestamp,
            build_number: base_info.build_number,
        });

        let jsi_info = crate::module_version::get_version();
        map.insert("JSI".to_string(), VersionInfo {
            version: jsi_info.version,
            git_sha: jsi_info.git_sha,
            build_timestamp: jsi_info.build_timestamp,
            build_number: jsi_info.build_number,
        });

        Ok(map)
    }

    fn get_nat_type(&self) -> Result<NatTypeResponse, BingleJsiError> {
        let guard = self.nat_type.lock().map_err(|_| BingleJsiError::InternalError {
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
        guard.register_keypair(handle).map_err(|e| BingleJsiError::InternalError {
            reason: e.to_string(),
        })?;
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
        guard
            .block_contact(id)
            .map_err(bingle_error_to_jsi)?;
        drop(guard);
        save_if_configured(&self.local_api, &self.local_file);
        Ok(())
    }

    fn remove_contact(&self, id: String) -> Result<(), BingleJsiError> {
        let mut guard = local_api_guard(&self.local_api)?;
        guard
            .remove_contact(id)
            .map_err(bingle_error_to_jsi)?;
        drop(guard);
        save_if_configured(&self.local_api, &self.local_file);
        Ok(())
    }

    fn is_blocked(&self, id: String) -> Result<bool, BingleJsiError> {
        let guard = local_api_guard(&self.local_api)?;
        guard
            .is_blocked(&id)
            .map_err(bingle_error_to_jsi)
    }

    fn get_contacts(&self) -> Result<Vec<Contact>, BingleJsiError> {
        let guard = local_api_guard(&self.local_api)?;
        let contacts = guard
            .get_contacts()
            .map_err(bingle_error_to_jsi)?;
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
    ) -> Result<(), BingleJsiError> {
        let mut guard = local_api_guard(&self.local_api)?;
        guard
            .add_message(sender_handle, recipient_handles, timestamp, text)
            .map_err(bingle_error_to_jsi)?;
        drop(guard);
        save_if_configured(&self.local_api, &self.local_file);
        Ok(())
    }

    fn get_messages(&self) -> Result<Vec<Message>, BingleJsiError> {
        let guard = local_api_guard(&self.local_api)?;
        let messages = guard
            .get_messages()
            .map_err(bingle_error_to_jsi)?;
        Ok(messages
            .into_iter()
            .map(|m| Message {
                sender_handle: m.sender_handle,
                recipient_handles: m.recipient_handles,
                timestamp: m.timestamp,
                text: m.text,
            })
            .collect())
    }

    fn keypair_status(&self) -> Result<KeypairStatusResponse, BingleJsiError> {
        let guard = local_api_guard(&self.local_api)?;
        let status = guard
            .keypair_status()
            .map_err(bingle_error_to_jsi)?;
        Ok(KeypairStatusResponse {
            status: parse_keypair_status(&status.status),
            id: status.id,
            handle: status.handle,
            required_algo: status.required_algo,
        })
    }

    fn save(&self, path: String) -> Result<(), BingleJsiError> {
        let guard = local_api_guard(&self.local_api)?;
        guard
            .save(&path)
            .map_err(bingle_error_to_jsi)
    }

    fn load(&self, path: String) -> Result<(), BingleJsiError> {
        let mut guard = local_api_guard(&self.local_api)?;
        guard
            .load(&path)
            .map_err(bingle_error_to_jsi)
    }

    fn set_message_callback(&self, callback: Box<dyn MessageCallback>) {
        if let Ok(mut guard) = self.message_callback.lock() {
            *guard = Some(callback);
            tracing::info!("[BingleJsiApiImpl][set_message_callback] Registered message callback");
        }
        else {
            tracing::error!("Failed to lock message_callback");
        }
    }

    fn set_log_callback(&self, callback: Box<dyn LogCallback>) {
        crate::api::log_bridge::set_global_log_callback(callback);
    }

    fn set_listening_callback(&self, callback: Box<dyn ListeningCallback>) {
        if let Ok(mut guard) = self.listening_callback.lock() {
            *guard = Some(callback);
            tracing::info!("[BingleJsiApiImpl][set_listening_callback] Registered listening callback");
        } else {
            tracing::error!("Failed to lock listening_callback");
        }
    }

    fn start(&self) -> Result<(), BingleJsiError> {
        // Check if already started
        {
            let guard = self.started.lock().map_err(|_| BingleJsiError::InternalError {
                reason: "Started flag lock poisoned".to_string(),
            })?;
            if *guard {
                return Err(BingleJsiError::InvalidRequest {
                    reason: "Engine already started".to_string(),
                });
            }
        }

        // Check keypair status is FUNDED or ACTIVE
        let guard = local_api_guard(&self.local_api)?;
        let status = guard
            .keypair_status()
            .map_err(bingle_error_to_jsi)?;
        let kp_status = parse_keypair_status(&status.status);
        if kp_status != KeypairStatus::Funded && kp_status != KeypairStatus::Active {
            return Err(BingleJsiError::InvalidRequest {
                reason: format!(
                    "Cannot start engine: keypair must be FUNDED or ACTIVE, but is {:?}",
                    kp_status
                ),
            });
        }

        // Build opts with handle and passphrase from local API
        let mut opts_clone = self.opts.lock().map_err(|_| BingleJsiError::InternalError {
            reason: "Opts lock poisoned".to_string(),
        })?.clone();
        if let Some(handle) = &status.handle {
            opts_clone.handle = handle.clone();
        }
        if let Ok(Some(kp)) = guard.get_keypair() {
            opts_clone.algo_passphrase = Some(kp.passphrase);
        }
        drop(guard);

        // Start the engine
        let api_clone = self.api.clone();
        let mut start_err = None;
        api_clone.access_unsafe_for_tests(|api_mut| {
            if let Err(e) = api_mut.start(&opts_clone) {
                tracing::error!("Failed to start Bingle API: {}", e);
                start_err = Some(bingle_error_to_jsi(e));
            }
        });

        if let Some(err) = start_err {
            return Err(err);
        }

        // Mark as started
        if let Ok(mut started_guard) = self.started.lock() {
            *started_guard = true;
        }

        // Output INFO with version information
        if let Ok(versions) = self.get_versions() {
            tracing::info!("Bingle JSI started. Versions: {:?}", versions);
        }

        tracing::info!("Bingle engine started");
        Ok(())
    }

    fn is_started(&self) -> bool {
        self.started.lock().map(|g| *g).unwrap_or(false)
    }
}
