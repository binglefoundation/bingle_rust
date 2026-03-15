use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use bingle_webserver::{start_server, AppState};
use rust_comms::api::bingle_api_impl::BingleApiImpl;
use rust_comms::util::cli_utils::parse_start_options_from_args;
use rust_comms::api::bingle_api::{BingleApi, OnMessageHandler};
use rust_comms::engine::BingleAccessUnsafeForTests;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    simple_logger::init_with_level(log::Level::Info).ok();

    let mut port = 12121;
    let mut address = "127.0.0.1".to_string();
    let mut other_args = Vec::new();

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

    // Initialize API
    let api = BingleApiImpl::new(&opts);
    let messages = Arc::new(Mutex::new(Vec::new()));

    // Setup on-message handler
    {
        let msgs = messages.clone();
        api.access_unsafe_for_tests(|api_mut| {
            let on_message: Arc<OnMessageHandler> = Arc::new(move |sender, sender_handle, message| {
                log::info!("Received message from {} ({}): {}", sender, sender_handle, message);
                let mut m = msgs.lock().unwrap();
                m.push(message);
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
    };

    let res = start_server(addr, state).await;

    log::info!("Stopping Bingle API...");
    api.access_unsafe_for_tests(|a| a.stop());
    log::info!("Stopped.");

    res
}
