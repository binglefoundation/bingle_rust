use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use bingle_webserver::{start_server, AppState};
use rust_comms::api::bingle_api_impl::BingleApiImpl;
use rust_comms::util::cli_utils::parse_start_options_from_args;
use rust_comms::api::bingle_api::{BingleApi, OnMessageHandler};
use rust_comms::engine::BingleAccessUnsafeForTests;
use std::path::PathBuf;
use bingle_local::api::bingle_local_api_impl::{BingleApiLocalImpl, LocalApiConfig};
use bingle_local::api::bingle_local_api::BingleLocalApi;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new("info,rust_comms=debug"))
        .init();

    let mut port = 12121;
    let mut address = "127.0.0.1".to_string();
    let mut other_args = Vec::new();
    let mut local_file: Option<PathBuf> = None;

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--port" | "-p" => {
                if i + 1 < args.len() {
                    port = args[i + 1].parse()?;
                    i += 2;
                } else {
                    anyhow::bail!("--port requires a value");
                }
            }
            "--local" => {
                if i + 1 < args.len() {
                    local_file = Some(PathBuf::from(&args[i + 1]));
                    i += 2;
                } else {
                    anyhow::bail!("--local requires a <file> value");
                }
            }
            "--address" | "-a" => {
                if i + 1 < args.len() {
                    address = args[i + 1].clone();
                    i += 2;
                } else {
                    anyhow::bail!("--address requires a value");
                }
            }
            _ => {
                other_args.push(args[i].clone());
                i += 1;
            }
        }
    }

    // When --local is active, handle is set by the caller (e.g. via registerKeypair),
    // so we don't require it on the command line.
    let opts = match parse_start_options_from_args(other_args.clone()) {
        Ok(o) => o,
        Err(e) if local_file.is_some() && e.contains("Missing handle") => {
            other_args.push("--handle".to_string());
            other_args.push(String::new());
            parse_start_options_from_args(other_args).map_err(|e| anyhow::anyhow!(e))?
        }
        Err(e) => return Err(anyhow::anyhow!(e)),
    };
    let addr: SocketAddr = format!("{}:{}", address, port).parse()?;

    // Initialize network API
    let api = BingleApiImpl::new(&opts);
    let messages = Arc::new(Mutex::new(Vec::new()));
    let nat_type: Arc<Mutex<String>> = Arc::new(Mutex::new("Unknown".to_string()));

    // Initialize local API if requested
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

    // Setup on-listening handler to update nat_type in shared state
    {
        let nat_type_for_closure = nat_type.clone();
        api.access_unsafe_for_tests(|api_mut| {
            let on_listening: Arc<rust_comms::api::bingle_api::OnListeningHandler> = Arc::new(move |listening: bool, nt: rust_comms::engine::NatType| {
                let type_str = if listening { format!("{:?}", nt) } else { "Unknown".to_string() };
                tracing::info!("on_listening: listening={} nat_type={}", listening, type_str);
                if let Ok(mut guard) = nat_type_for_closure.lock() {
                    *guard = type_str;
                }
            });
            api_mut.set_on_listening(Some(on_listening));
        });
    }

    // Setup on-message handler
    {
        let msgs = messages.clone();
        let local_api_for_closure = local_api.clone();
        let local_file_for_closure = local_file.clone();
        let api_for_handle = api.clone();
        api.access_unsafe_for_tests(|api_mut| {
            let on_message: Arc<OnMessageHandler> = Arc::new(move |sender, sender_handle, message| {
                tracing::info!("Received message from {} ({}): {}", sender, sender_handle, message);
                let text = message.get("text")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| message.to_string());
                let mut m = msgs.lock().unwrap();
                m.push(message);
                // Store message in local API buffer so it is accessible via getMessages
                if let Some(local_arc) = &local_api_for_closure {
                    if let Ok(mut guard) = local_arc.lock() {
                        let timestamp = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_millis() as i64)
                            .unwrap_or(0);
                        let recipient = match api_for_handle.get_handle() {
                            Some(h) => h,
                            None => {
                                tracing::error!("[on_message] get_handle returned None; not saving message to local API");
                                return;
                            }
                        };
                        if let Err(e) = guard.add_message(
                            sender_handle.clone(),
                            vec![recipient],
                            timestamp,
                            text,
                        ) {
                            tracing::warn!("[on_message] failed to add message to local API: {}", e);
                        }
                        if let Some(path) = &local_file_for_closure {
                            let _ = guard.save(path.to_string_lossy().as_ref());
                        }
                    }
                    else {
                        tracing::warn!("[on_message] local_api would not lock; not saving local state");
                    }
                }
                else {
                    tracing::warn!("[on_message] local_api is None; not saving local state");
                }
            });
            api_mut.set_on_message(Some(on_message));
        });
    }

    // Determine whether to start API immediately or defer until keypair is ACTIVE
    let mut api_started = false;
    if local_file.is_some() {
        // When --local is active, only start if keypair_status is already ACTIVE
        if let Some(local_arc) = &local_api {
            if let Ok(guard) = local_arc.lock() {
                if let Ok(status) = guard.keypair_status() {
                    if status.status == "ACTIVE" {
                        let api_clone = api.clone();
                        let mut opts_clone = opts.clone();
                        // Update handle and algo_passphrase from the local API's
                        // generated keypair and registered handle before starting.
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
                        tracing::info!("Bingle API start deferred (keypair status: {})", status.status);
                    }
                }
            }
        }
    } else {
        // No --local: start API immediately as before
        let api_clone = api.clone();
        let opts_clone = opts.clone();
        api_clone.access_unsafe_for_tests(|api_mut| {
            if let Err(e) = api_mut.start(&opts_clone) {
                tracing::error!("Failed to start Bingle API: {}", e);
            }
        });
        api_started = true;
    }

    let state = AppState {
        api: api.clone(),
        messages,
        local_api,
        local_file,
        start_opts: if api_started { None } else { Some(opts.clone()) },
        api_started: Arc::new(Mutex::new(api_started)),
        nat_type,
    };

    let res = start_server(addr, state).await;

    tracing::info!("Stopping Bingle API...");
    api.access_unsafe_for_tests(|a| a.stop());
    tracing::info!("Stopped.");

    res
}
