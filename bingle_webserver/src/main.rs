use bingle_core::api::bingle_api::{BingleApi, OnMessageHandler};
use bingle_core::api::bingle_api_impl::BingleApiImpl;
use bingle_core::engine::BingleAccess;
use bingle_core::util::cli_utils::parse_start_options_from_args;
use bingle_core::util::logging::{BingleFormatter, HandleLayer, LogMode};
use bingle_local::api::bingle_local_api::BingleLocalApi;
use bingle_local::api::bingle_local_api_impl::{BingleApiLocalImpl, LocalApiConfig};
use bingle_webserver::{AppState, start_server};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tracing_subscriber::prelude::*;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut port = 12121;
    let mut address = "127.0.0.1".to_string();
    let mut other_args = Vec::new();
    let mut local_file: Option<PathBuf> = None;
    // Give-up nudge (bingle_notify #11/#17): notify gateway URL activates the nudge; the flag can
    // disable it even when a URL is set. Parsed here (not via StartOptions) since it is a
    // LocalApiConfig concern, not a network-engine one.
    let mut notify_gateway_url: Option<String> = None;
    let mut notify_on_giveup: Option<bool> = None;

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    let mut log_mode = LogMode::Plain;
    while i < args.len() {
        match args[i].as_str() {
            "--log-mode" => {
                if i + 1 < args.len() {
                    let val = args[i + 1].to_ascii_lowercase();
                    log_mode = match val.as_str() {
                        "plain" => LogMode::Plain,
                        "ansi" => LogMode::ANSI,
                        "aws" => LogMode::AWS,
                        "js" => LogMode::JS,
                        _ => LogMode::Plain,
                    };
                    i += 2;
                } else {
                    anyhow::bail!("--log-mode requires a value");
                }
            }
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
            "--notify-gateway-url" => {
                if i + 1 < args.len() {
                    notify_gateway_url = Some(args[i + 1].clone());
                    i += 2;
                } else {
                    anyhow::bail!("--notify-gateway-url requires a <url> value");
                }
            }
            "--notify-on-giveup" => {
                if i + 1 < args.len() {
                    notify_on_giveup = Some(match args[i + 1].to_ascii_lowercase().as_str() {
                        "true" | "1" | "yes" => true,
                        "false" | "0" | "no" => false,
                        other => {
                            anyhow::bail!("--notify-on-giveup expects true|false, got '{}'", other)
                        }
                    });
                    i += 2;
                } else {
                    anyhow::bail!("--notify-on-giveup requires a <true|false> value");
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
    // Initialize logger with selected mode
    let fmt_layer =
        tracing_subscriber::fmt::layer().event_format(BingleFormatter { mode: log_mode });
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,bingle_core=debug"));

    tracing_subscriber::registry()
        .with(filter)
        .with(HandleLayer)
        .with(fmt_layer)
        .init();

    let addr: SocketAddr = format!("{}:{}", address, port).parse()?;

    // Initialize network API
    let api = BingleApiImpl::new(&opts);
    let messages = Arc::new(Mutex::new(Vec::new()));
    let nat_type: Arc<Mutex<String>> = Arc::new(Mutex::new("Unknown".to_string()));

    // Initialize local API if requested
    let mut local_api: Option<Arc<Mutex<Box<dyn BingleLocalApi>>>> = None;
    if let Some(path) = &local_file {
        // Give-up nudge (bingle_notify #11/#17): a `--notify-gateway-url` activates the nudge;
        // `--notify-on-giveup false` disables it even when a URL is set.
        let cfg = LocalApiConfig::with_notify(
            opts.algo_provider_config.clone().unwrap_or_default(),
            opts.app_id.unwrap_or(0),
            opts.asset_id.unwrap_or(0),
            notify_on_giveup,
            notify_gateway_url.clone(),
            None,
        );
        let mut impl_api = BingleApiLocalImpl::new(cfg);
        if path.exists()
            && let Err(e) = impl_api.load(path.to_string_lossy().as_ref())
        {
            tracing::warn!("Failed to load local state from {}: {}", path.display(), e);
        }
        local_api = Some(Arc::new(Mutex::new(Box::new(impl_api))));
    }

    // Setup on-listening handler to update nat_type in shared state
    {
        let nat_type_for_closure = nat_type.clone();
        api.access(|api_mut| {
            let on_listening: Arc<bingle_core::api::bingle_api::OnListeningHandler> =
                Arc::new(move |listening: bool, nt: bingle_core::engine::NatType| {
                    let type_str = if listening {
                        format!("{:?}", nt)
                    } else {
                        "Unknown".to_string()
                    };
                    tracing::info!(
                        "on_listening: listening={} nat_type={}",
                        listening,
                        type_str
                    );
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
        api.access(|api_mut| {
            let on_message: Arc<OnMessageHandler> = Arc::new(move |sender, sender_handle, message| {
                tracing::info!("Received message from {} ({}): {}", sender, sender_handle, message);
                let text = message.get("text")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| message.to_string());
                let cipher_suite = message.get("cipher_suite")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
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
                            cipher_suite,
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
        if let Some(local_arc) = &local_api
            && let Ok(guard) = local_arc.lock()
            && let Ok(status) = guard.keypair_status()
        {
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
    } else {
        // No --local: start API immediately as before
        let api_clone = api.clone();
        let opts_clone = opts.clone();
        api_clone.access(|api_mut| {
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
        start_opts: if api_started {
            None
        } else {
            Some(opts.clone())
        },
        api_started: Arc::new(Mutex::new(api_started)),
        nat_type,
    };

    let res = start_server(addr, state).await;

    tracing::info!("Stopping Bingle API...");
    api.access(|a| a.stop());
    tracing::info!("Stopped.");

    res
}
