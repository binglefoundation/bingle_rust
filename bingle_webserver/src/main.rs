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
    simple_logger::SimpleLogger::new()
        .with_level(log::LevelFilter::Info)
        .with_module_level("rust_comms", log::LevelFilter::Debug)
        .init()
        .ok();

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
                log::warn!("Failed to load local state from {}: {}", path.display(), e);
            }
        }
        local_api = Some(Arc::new(Mutex::new(Box::new(impl_api))));
    }

    // Setup on-message handler
    {
        let msgs = messages.clone();
        let local_api_for_closure = local_api.clone();
        let local_file_for_closure = local_file.clone();
        api.access_unsafe_for_tests(|api_mut| {
            let on_message: Arc<OnMessageHandler> = Arc::new(move |sender, sender_handle, message| {
                log::info!("Received message from {} ({}): {}", sender, sender_handle, message);
                let mut m = msgs.lock().unwrap();
                m.push(message);
                // Save local state after inbound message if enabled
                if let Some(local_arc) = &local_api_for_closure {
                    if let Ok(guard) = local_arc.lock() {
                        if let Some(path) = &local_file_for_closure {
                            let _ = guard.save(path.to_string_lossy().as_ref());
                        }
                    }
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
                                log::error!("Failed to start Bingle API: {}", e);
                            }
                        });
                        api_started = true;
                        log::info!("Bingle API started (keypair is ACTIVE)");
                    } else {
                        log::info!("Bingle API start deferred (keypair status: {})", status.status);
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
                log::error!("Failed to start Bingle API: {}", e);
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
    };

    let res = start_server(addr, state).await;

    log::info!("Stopping Bingle API...");
    api.access_unsafe_for_tests(|a| a.stop());
    log::info!("Stopped.");

    res
}
