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
    simple_logger::init_with_level(log::Level::Info).ok();

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

    let opts = parse_start_options_from_args(other_args).map_err(|e| anyhow::anyhow!(e))?;
    let addr: SocketAddr = format!("{}:{}", address, port).parse()?;

    // Initialize network API
    let api = BingleApiImpl::new(&opts);
    let messages = Arc::new(Mutex::new(Vec::new()));

    // Initialize local API if requested
    let mut local_api: Option<Arc<Mutex<Box<dyn BingleLocalApi>>>> = None;
    if let Some(path) = &local_file {
        let cfg = LocalApiConfig::default();
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

    // Start API
    {
        let api_clone = api.clone();
        let opts_clone = opts.clone();
        api_clone.access_unsafe_for_tests(|api_mut| {
            if let Err(e) = api_mut.start(&opts_clone) {
                log::error!("Failed to start Bingle API: {}", e);
            }
        });
    }

    let state = AppState {
        api: api.clone(),
        messages,
        local_api,
        local_file,
    };

    let res = start_server(addr, state).await;

    log::info!("Stopping Bingle API...");
    api.access_unsafe_for_tests(|a| a.stop());
    log::info!("Stopped.");

    res
}
