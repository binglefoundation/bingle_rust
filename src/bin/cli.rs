use std::sync::Arc;
use std::sync::mpsc::{channel, Sender};

use rust_comms::api::bingle_api::{BingleApi, OnConnectHandler, OnMessageHandler};
use rust_comms::api::bingle_api_impl::BingleApiImpl;
use rust_comms::util::cli_utils::parse_start_options_from_args;

fn main() {
    // Parse CLI args into StartOptions
    let args: Vec<String> = std::env::args().skip(1).collect();
    let debug = args.iter().any(|a| a == "--debug");
    let opts = match parse_start_options_from_args(args.clone()) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("Error: {}\nUsage: cli [--handle <handle>|<handle>] [--passphrase <text>] [--relay] [--static-ip <ip:port>] [--stun-servers <list>] [--stun-servers-file <file>] [--node-file <file>] [--debug]", e);
            std::process::exit(2);
        }
    };

    if debug {
        println!("Debug mode enabled");
        println!("Parsed StartOptions: {:?}", opts);
    }

    // Initialize API
    let mut api = BingleApiImpl::new();

    // Install handlers that print args
    let on_message: Arc<OnMessageHandler> = Arc::new(move |sender, sender_handle, message| {
        println!("on_message: sender={} sender_handle={} message={}", sender, sender_handle, message);
    });
    api.set_on_message(Some(on_message));

    let on_connect: Arc<OnConnectHandler> = Arc::new(move |sender, sender_handle| {
        println!("on_connect: sender={} sender_handle={}", sender, sender_handle);
    });
    api.set_on_connect(Some(on_connect));

    // Start API
    if let Err(e) = api.start(opts) {
        eprintln!("Failed to start: {}", e);
        std::process::exit(1);
    }

    // Install Ctrl-C handler
    let (tx, rx) = channel::<()>();
    install_ctrlc_handler(tx);
    println!("Started. Press Ctrl-C to stop.");

    // Wait until Ctrl-C
    let _ = rx.recv();

    // Stop API
    api.stop();
    println!("Stopped.");
}

fn install_ctrlc_handler(tx: Sender<()>) {
    if let Err(e) = ctrlc::set_handler(move || {
        let _ = tx.send(());
    }) {
        eprintln!("Failed to install Ctrl-C handler: {}", e);
    }
}
