use std::fs;
use std::io::Write;
use std::sync::Arc;
use std::sync::mpsc::{channel, Sender};

use rust_comms::api::bingle_api::{BingleApi, BingleError, OnConnectHandler, OnListeningHandler, OnMessageHandler};
use rust_comms::api::bingle_api_impl::BingleApiImpl;
use rust_comms::util::config_utils::{parse_algos_decimal_to_microalgos, parse_node_file_with_ids, resolve_app_asset_ids};
use rust_comms::util::cli_utils::parse_start_options_from_args;
use rust_comms::blockchain::algo_ops::{AlgoChainConfig, AlgoOps};
use rust_comms::blockchain::error::{AlgoError, AlgoErrorKind};
use rust_comms::blockchain::algo_bingle::AlgoBingle;
use rust_comms::engine::BingleAccessUnsafeForTests;
use tracing::warn;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::prelude::*;
use rust_comms::util::logging::{BingleFormatter, HandleLayer, LogMode};
use rust_comms::util::version::VersionsMap;

fn init_logger_from_args(args: &mut Vec<String>) {
    // Parse and strip global logging flags from args, choose the last-specified level if multiple are present
    let mut chosen: Option<LevelFilter> = None;
    let mut chosen_mode: Option<LogMode> = None;
    let mut i = 0usize;
    while i < args.len() {
        let a = args[i].as_str();
        // Support --log-level <level> and --log-level=<level>
        if a == "--log-level" {
            if i + 1 < args.len() {
                let val = args[i + 1].to_ascii_lowercase();
                let lvl = match val.as_str() {
                    "trace" => LevelFilter::TRACE,
                    "debug" => LevelFilter::DEBUG,
                    "info" => LevelFilter::INFO,
                    "warn" | "warning" => LevelFilter::WARN,
                    "error" => LevelFilter::ERROR,
                    _ => LevelFilter::INFO,
                };
                chosen = Some(lvl);
                // Remove flag and its value
                args.remove(i); // remove "--log-level"
                args.remove(i); // remove value now at same index
                continue; // re-check current index
            } else {
                // No value provided; drop the flag and continue
                args.remove(i);
                continue;
            }
        } else if let Some(rest) = a.strip_prefix("--log-level=") {
            let val = rest.to_ascii_lowercase();
            let lvl = match val.as_str() {
                "trace" => LevelFilter::TRACE,
                "debug" => LevelFilter::DEBUG,
                "info" => LevelFilter::INFO,
                "warn" | "warning" => LevelFilter::WARN,
                "error" => LevelFilter::ERROR,
                _ => LevelFilter::INFO,
            };
            chosen = Some(lvl);
            args.remove(i);
            continue;
        } else if a == "--log-mode" {
            if i + 1 < args.len() {
                let val = args[i + 1].to_ascii_lowercase();
                let mode = match val.as_str() {
                    "plain" => LogMode::Plain,
                    "ansi" => LogMode::ANSI,
                    "aws" => LogMode::AWS,
                    "js" => LogMode::JS,
                    _ => LogMode::Plain,
                };
                chosen_mode = Some(mode);
                args.remove(i);
                args.remove(i);
                continue;
            } else {
                args.remove(i);
                continue;
            }
        } else if let Some(rest) = a.strip_prefix("--log-mode=") {
            let val = rest.to_ascii_lowercase();
            let mode = match val.as_str() {
                "plain" => LogMode::Plain,
                "ansi" => LogMode::ANSI,
                "aws" => LogMode::AWS,
                "js" => LogMode::JS,
                _ => LogMode::Plain,
            };
            chosen_mode = Some(mode);
            args.remove(i);
            continue;
        }

        let matched = match a {
            "--log-warn" | "-q" => { chosen = Some(LevelFilter::WARN); true }
            "--log-info" => { chosen = Some(LevelFilter::INFO); true }
            "--log-debug" | "-v" => { chosen = Some(LevelFilter::DEBUG); true }
            "--log-trace" | "--vv" | "-vv" => { chosen = Some(LevelFilter::TRACE); true }
            _ => false,
        };
        if matched {
            args.remove(i);
        } else {
            i += 1;
        }
    }
    let level = chosen.unwrap_or(LevelFilter::INFO);
    let mode = chosen_mode.unwrap_or(LogMode::Plain);
    let fmt_layer = tracing_subscriber::fmt::layer()
        .event_format(BingleFormatter { mode });

    let subscriber = tracing_subscriber::registry()
        .with(level)
        .with(HandleLayer)
        .with(fmt_layer);

    let _ = tracing::subscriber::set_global_default(subscriber);
}

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    // Initialize logger from global flags and strip them from args (default Info)
    init_logger_from_args(&mut args);

    // Build and output VersionsMap
    let mut versions = VersionsMap::new();
    versions.insert("Base".to_string(), rust_comms::module_version::get_version());
    versions.insert("CLI".to_string(), rust_comms::module_version::get_version());
    tracing::info!("Bingle CLI starting. Versions: {:?}", versions);

    if args.is_empty() {
        print_usage_and_exit(2);
    }

    // Support top-level help
    if args.len() == 1 && (args[0] == "--help" || args[0] == "-h") {
        print_usage_and_exit(0);
    }

    let sub = args.remove(0);
    match sub.as_str() {
        "run" => cmd_run(args),
        "register" => cmd_register(args),
        "buybingle" => cmd_buybingle(args),
        "sellbingle" => cmd_sellbingle(args),
        "checkrelays" => cmd_checkrelays(args),
        "--help" | "-h" => print_usage_and_exit(0),
        other => {
            warn!("Unknown subcommand: {}", other);
            std::process::exit(2);
        }
    }
}

fn print_usage_and_exit(code: i32) {
    let usage = "Usage: bingle_cli <run|register|buybingle|sellbingle|checkrelays> [options]\n  Common options (for all commands): --log-warn|-q | --log-info | --log-debug|-v | --log-trace|--vv|-vv | --log-mode <Plain|ANSI|AWS|JS> | --stun-servers <list> | --stun-servers-file <file>\n  bingle_cli run [--handle <handle>|<handle>] [--passphrase <text>] [--relay] [--static-ip <ip:port>] [--stun-servers <list>] [--stun-servers-file <file>] [--node-file <file>] [--app-id <id>] [--asset-id <id>] [--sentinel-file <path>] [--echo] [--log-mode <Plain|ANSI|AWS|JS>]\n  bingle_cli register --handle <handle> --passphrase <text> --app-id <id> --asset-id <id> --price-units <n> [--node-file <file>] [--stun-servers <list>] [--stun-servers-file <file>] [--log-mode <Plain|ANSI|AWS|JS>]\n  bingle_cli buybingle <price_algos> --passphrase <text> --app-id <id> --asset-id <id> [--node-file <file>] [--stun-servers <list>] [--stun-servers-file <file>] [--log-mode <Plain|ANSI|AWS|JS>]\n  bingle_cli sellbingle <amount_units> <price_algos> --passphrase <text> --app-id <id> --asset-id <id> [--node-file <file>] [--stun-servers <list>] [--stun-servers-file <file>] [--log-mode <Plain|ANSI|AWS|JS>]\n  bingle_cli checkrelays --passphrase <text> [--node-file <file>] [--app-id <id>] [--asset-id <id>] [--interval-ms <n>] [--stun-servers <list>] [--stun-servers-file <file>] [--log-mode <Plain|ANSI|AWS|JS>]";
    if code == 0 { tracing::info!("{}", usage); } else { warn!("{}", usage); }
    std::process::exit(code);
}


fn cmd_run(mut args: Vec<String>) {
    // Support subcommand help
    if args.len() == 1 && (args[0] == "--help" || args[0] == "-h") {
        warn!("Usage: bingle_cli run [--handle <handle>|<handle>] [--passphrase <text>] [--relay] [--static-ip <ip:port>] [--stun-servers <list>] [--stun-servers-file <file>] [--node-file <file>] [--sentinel-file <path>] [--echo]");
        std::process::exit(0);
    }

    // Extract and remove optional --sentinel-file <path> and --echo from args before StartOptions parsing
    let mut sentinel_file: Option<String> = None;
    let mut echo_mode = false;
    let mut i = 0usize;
    while i < args.len() {
        if args[i] == "--sentinel-file" {
            if i + 1 >= args.len() {
                warn!("--sentinel-file requires a <path> value");
                std::process::exit(2);
            }
            sentinel_file = Some(args[i + 1].clone());
            // Remove the flag and its value
            args.remove(i); // remove flag
            args.remove(i); // remove value now at same index
            // Do not increment i; continue scanning
            continue;
        }
        if args[i] == "--echo" {
            echo_mode = true;
            args.remove(i);
            continue;
        }
        i += 1;
    }

    // Parse CLI args into StartOptions
    let opts = match parse_start_options_from_args(args.clone()) {
        Ok(o) => o,
        Err(e) => {
            warn!("Error: {}\nUsage: bingle_cli run [--handle <handle>|<handle>] [--passphrase <text>] [--relay] [--static-ip <ip:port>] [--stun-servers <list>] [--stun-servers-file <file>] [--node-file <file>] [--sentinel-file <path>]", e);
            std::process::exit(2);
        }
    };

    // If a static IP was provided, attempt to register it on-chain for discovery BEFORE starting the protocol
    if let Some(static_addr) = opts.static_ip {
        // Resolve app_id from StartOptions or APP_ID env
        warn!("Registering static IP {} for on-chain discovery, opts={:?}", static_addr, opts);
        let app_id_opt = opts.app_id.or_else(|| std::env::var("APP_ID").ok().and_then(|s| s.parse::<u64>().ok()));
        match app_id_opt {
            Some(app_id) => {
                // Require a passphrase to sign the transaction
                if opts.algo_passphrase.is_none() {
                    warn!("--static-ip was provided but no Algorand passphrase (--passphrase) was set; skipping on-chain register_endpoint");
                } else {
                    let ops = AlgoOps::new(opts.algo_passphrase.clone(), None, opts.algo_provider_config.clone());
                    let asset_id_for_ctor = opts.asset_id.or_else(|| opts.algo_provider_config.as_ref().and_then(|c| c.asset_id)).unwrap_or(0);
                                        let bingle = AlgoBingle::new(ops, app_id, asset_id_for_ctor);
                    // TODO: register_endpoint now takes a compact AdvertRecord
                    // create this here (use constructor to sign with id, needs a SigningKey)
                    match bingle.register_endpoint(app_id, &static_addr.to_string()) {
                        Ok(txid) => {
                            tracing::info!("Registered static endpoint {} for app_id {} (tx: {})", static_addr, app_id, txid);
                        }
                        Err(e) => {
                            warn!("Failed to register static endpoint '{}': {}", static_addr, e);
                        }
                    }
                }
            }
            None => {
                warn!("--static-ip was provided but app_id is missing; set --node-file with app_id or APP_ID env to enable on-chain register_endpoint");
            }
        }
    }

    // Initialize API
    let api = BingleApiImpl::new(&opts);

    // Install handlers (requires mutable access to the Arc contents; CLI owns the only strong ref here)
    {
        let api_for_echo = api.clone();
        api.access_unsafe_for_tests(|api_mut| {
            let on_message: Arc<OnMessageHandler> = Arc::new(move |sender, sender_handle, message| {
                tracing::info!("on_message: sender={} sender_handle={} message={}", sender, sender_handle, message);
                if echo_mode {
                    // Check if this is a PlainTextMessage: has "text" field and no non-null "app"/"type"
                    if let Some(text) = message.get("text").and_then(|v| v.as_str()) {
                        let is_plain = message.get("app").map_or(true, |v| v.is_null())
                            && message.get("type").map_or(true, |v| v.is_null());
                        if is_plain {
                            let echo_text = format!("Echo: {}", text);
                            let echo_msg = serde_json::json!({ "text": echo_text });
                            tracing::info!("[echo] Echoing back to {}: {}", sender, echo_msg);
                            let ok = api_for_echo.send_message_to_id(&sender, echo_msg, None);
                            if !matches!(ok, Ok(true)) {
                                tracing::warn!("[echo] Failed to send echo to {}: {:?}", sender, ok);
                            }
                        }
                    }
                }
            });
            api_mut.set_on_message(Some(on_message));

            let on_connect: Arc<OnConnectHandler> = Arc::new(move |sender, sender_handle| {
                tracing::info!("on_connect: sender={} sender_handle={}", sender, sender_handle);
            });
            api_mut.set_on_connect(Some(on_connect));

            // Optional: install OnListening handler to manage a sentinel file
            if let Some(path) = sentinel_file.clone() {
                let p = path.clone();
                let on_listening: Arc<OnListeningHandler> = Arc::new(move |listening: bool, _nat_type: rust_comms::engine::NatType| {
                    if listening {
                        match fs::OpenOptions::new().create(true).write(true).truncate(true).open(&p) {
                            Ok(mut f) => {
                                let _ = writeln!(f, "listening");
                                tracing::info!("Created sentinel file: {}", p);
                            }
                            Err(e) => {
                                tracing::warn!("Failed to create sentinel file '{}': {}", p, e);
                            }
                        }
                    } else {
                        match fs::remove_file(&p) {
                            Ok(()) => tracing::info!("Removed sentinel file: {}", p),
                            Err(e) => tracing::warn!("Failed to remove sentinel file '{}': {}", p, e),
                        }
                    }
                });
                api_mut.set_on_listening(Some(on_listening));
            }
        });
    }

    // Start API
    loop {
        let mut start_res = Ok(());
        api.access_unsafe_for_tests(|api_mut| {
            start_res = api_mut.start(&opts);
        });

        match start_res {
            Ok(_) => break,
            Err(e) => {
                if let BingleError::Algo(ae) = &e {
                    if ae.kind == AlgoErrorKind::HostUnreachable {
                        tracing::error!("Algorand node unreachable: {}. Retrying in 60s...", ae);
                        std::thread::sleep(std::time::Duration::from_secs(60));
                        continue;
                    }
                }
                warn!("Failed to start: {}", e);
                std::process::exit(1);
            }
        }
    }

    // Install Ctrl-C handler
    let (tx, rx) = channel::<()>();
    install_ctrlc_handler(tx);
    tracing::info!("Started. Press Ctrl-C or send SIGTERM to stop.");

    // Wait until Ctrl-C
    let _ = rx.recv();

    tracing::info!("Received shutdown signal. Stopping...");

    // Determine shutdown action and execute if needed
    let app_id_env = std::env::var("APP_ID").ok().and_then(|s| s.parse::<u64>().ok());
    let action = resolve_shutdown_action(&opts, app_id_env);
    match action {
        ShutdownAction::Unregister { app_id, passphrase, algo_provider_config, asset_id } => {
            tracing::info!("[cmd_run] Unregistering static endpoint for app_id={}", app_id);
            let ops = AlgoOps::new(Some(passphrase), None, algo_provider_config.clone());
            let asset_id_for_ctor = asset_id.or_else(|| algo_provider_config.as_ref().and_then(|c| c.asset_id)).unwrap_or(0);
            let bingle = AlgoBingle::new(ops, app_id, asset_id_for_ctor);
            match bingle.register_endpoint(app_id, "") {
                Ok(txid) => {
                    tracing::info!("[cmd_run] Unregistered static endpoint for app_id {} (tx: {})", app_id, txid);
                }
                Err(e) => {
                    tracing::warn!("[cmd_run] Failed to unregister static endpoint for app_id {}: {}", app_id, e);
                }
            }
        }
        ShutdownAction::NoStaticIp => {
            tracing::debug!("[cmd_run] No static IP configured; skipping endpoint unregistration");
        }
        ShutdownAction::NoAppId => {
            tracing::debug!("[cmd_run] No app_id available; skipping endpoint unregistration");
        }
        ShutdownAction::NoPassphrase => {
            tracing::debug!("[cmd_run] No passphrase available; skipping endpoint unregistration");
        }
    }

    // Stop API
    {
        api.access_unsafe_for_tests(|api_mut| {
            api_mut.stop();
        });
    }
    tracing::info!("Stopped.");
}

use rust_comms::api::network_endpoint::NetworkEndpoint;
use serde_json::json;
use std::net::SocketAddr;
use std::time::Instant;
use std::time::Duration;

fn cmd_checkrelays(mut args: Vec<String>) {
    // Support subcommand help
    if args.len() == 1 && (args[0] == "--help" || args[0] == "-h") {
        warn!("Usage: bingle_cli checkrelays --passphrase <text> [--node-file <file>] [--app-id <id>] [--asset-id <id>] [--interval-ms <n>] [--once] [--stun-servers <list>] [--stun-servers-file <file>]\n\
              Notes: by default this command repeats indefinitely, sleeping --interval-ms between runs. Use --once to run a single iteration.");
        std::process::exit(0);
    }

    // Extract optional --interval-ms <n>
    let mut interval_ms: u64 = 5000;
    let mut once: bool = false;
    let mut i = 0usize;
    while i < args.len() {
        if args[i] == "--interval-ms" {
            if i + 1 >= args.len() {
                warn!("--interval-ms requires a value");
                std::process::exit(2);
            }
            match args[i + 1].parse::<u64>() {
                Ok(v) => interval_ms = v,
                Err(e) => { warn!("invalid --interval-ms: {}", e); std::process::exit(2); }
            }
            args.remove(i); // flag
            args.remove(i); // value
            continue;
        } else if args[i] == "--once" {
            once = true;
            args.remove(i);
            continue;
        }
        i += 1;
    }

    // Manually parse only the options needed for checkrelays (no handle required)
    let mut passphrase: Option<String> = None;
    let mut node_file: Option<String> = None;
    let mut cli_app_id: Option<u64> = None;
    let mut cli_asset_id: Option<u64> = None;
    let mut stun_servers: Option<Vec<SocketAddr>> = None;
    let mut dangerous_debug = false;

    // Remaining args after --interval-ms extraction
    let mut it = args.into_iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--passphrase" => {
                let p = it.next().unwrap_or_else(|| {
                    warn!("--passphrase requires a value");
                    std::process::exit(2);
                });
                passphrase = Some(p);
            }
            "--node-file" => {
                let nf = it.next().unwrap_or_else(|| {
                    warn!("--node-file requires a <file> value");
                    std::process::exit(2);
                });
                node_file = Some(nf);
            }
            "--app-id" => {
                let v = it.next().unwrap_or_else(|| {
                    warn!("--app-id requires a value");
                    std::process::exit(2);
                });
                match v.parse::<u64>() {
                    Ok(id) => cli_app_id = Some(id),
                    Err(e) => { warn!("Invalid --app-id '{}': {}", v, e); std::process::exit(2); }
                }
            }
            "--asset-id" => {
                let v = it.next().unwrap_or_else(|| {
                    warn!("--asset-id requires a value");
                    std::process::exit(2);
                });
                match v.parse::<u64>() {
                    Ok(id) => cli_asset_id = Some(id),
                    Err(e) => { warn!("Invalid --asset-id '{}': {}", v, e); std::process::exit(2); }
                }
            }
            "--stun-servers" => {
                let v = it.next().unwrap_or_else(|| {
                    warn!("--stun-servers requires a value");
                    std::process::exit(2);
                });
                match rust_comms::util::config_utils::parse_stun_list(&v) {
                    Ok(list) => stun_servers = Some(list),
                    Err(e) => { warn!("{}", e); std::process::exit(2); }
                }
            }
            "--stun-servers-file" => {
                let v = it.next().unwrap_or_else(|| {
                    warn!("--stun-servers-file requires a <file> value");
                    std::process::exit(2);
                });
                match rust_comms::util::config_utils::parse_stun_file(&v) {
                    Ok(list) => stun_servers = Some(list),
                    Err(e) => { warn!("{}", e); std::process::exit(2); }
                }
            }
            "--dangerous-debug" => {
                dangerous_debug = true;
            }
            s if s.starts_with('-') => {
                warn!("Unknown option: {}", s);
                std::process::exit(2);
            }
            other => {
                // Positional arguments are not expected for checkrelays
                warn!("Unexpected positional argument: {}", other);
                std::process::exit(2);
            }
        }
    }

    // Require passphrase so we can derive our id and access blockchain if needed
    let pass = match passphrase { Some(p) if !p.is_empty() => p, _ => { warn!("--passphrase is required"); std::process::exit(2); } };

    // Load node file config if provided
    let (algo_network, algo_provider_config, node_app_id, node_asset_id) = if let Some(nf) = node_file.clone() {
        match parse_node_file_with_ids(&nf) {
            Ok((net, cfg, a, b)) => (net, Some(cfg), a, b),
            Err(e) => { warn!("{}", e); std::process::exit(2); }
        }
    } else { (None, None, None, None) };

    // Resolve app/asset ids (prefer values from node file or CLI; else environment)
    let (app_id, asset_id) = match resolve_app_asset_ids(node_app_id, node_asset_id, cli_app_id, cli_asset_id) {
        Ok((a, b)) => (a, b),
        Err(e) => { warn!("{}", e); std::process::exit(2); }
    };

    // Build AlgoOps for discovery using same config
    let ops = AlgoOps::new(Some(pass.clone()), None, algo_provider_config.clone());
    let my_address = match &ops.address { Some(a) => a.clone(), None => { warn!("Unable to derive address from passphrase"); std::process::exit(2); } };

    // Construct minimal StartOptions without requiring user-provided handle
    let opts = rust_comms::api::bingle_api::StartOptions {
        handle: my_address.clone(), // synthetic handle: use our address to satisfy Engine
        algo_passphrase: Some(pass.clone()),
        static_ip: None,
        am_relay: false,
        stun_servers,
        algo_provider_config: algo_provider_config.clone(),
        algo_network,
        app_id: Some(app_id),
        asset_id: Some(asset_id),
        log_level: None,
        handle_cache_expiry: None,
        dangerous_debug,
        log_mode: LogMode::Plain,
        wait_response_timeout: None,
    };

    // Create API and start engine minimal
    let api = BingleApiImpl::new(&opts);
    loop {
        let mut start_res = Ok(());
        api.access_unsafe_for_tests(|api_mut| {
            start_res = api_mut.start(&opts);
        });

        match start_res {
            Ok(_) => break,
            Err(e) => {
                if let BingleError::Algo(ae) = &e {
                    if ae.kind == AlgoErrorKind::HostUnreachable {
                        tracing::error!("Algorand node unreachable: {}. Retrying in 60s...", ae);
                        std::thread::sleep(std::time::Duration::from_secs(60));
                        continue;
                    }
                }
                warn!("Failed to start API: {}", e);
                std::process::exit(2);
            }
        }
    }

    // After starting, wait for Engine to reach Registered state to ensure discovery is ready
    let api_both: Arc<dyn rust_comms::api::bingle_api::BingleApiBoth> = api.clone();
    let wait_start = Instant::now();
    let wait_timeout = Duration::from_secs(5);
    while wait_start.elapsed() < wait_timeout {
        let st = api_both.get_state();
        if st == rust_comms::engine::EngineState::Registered {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    // Helper for a single check that returns Ok(duration_ms) or Err(()) using Ping/PingResponse
    let do_check = |relay_id: &str, addr: SocketAddr| -> Result<u128, ()> {
        let nsk = NetworkEndpoint::new_direct(addr);
        // Send a Ping with text and expect a PingResponse with ACK text
        let req = json!({ "app": "ping", "type": "ping", "text": "bingle-cli probe" });
        let start = Instant::now();
        match api.send_message_to_network_with_response(&nsk, &relay_id.to_string(), req, None) {
            Ok(resp) => {
                let app_ok = resp.get("app").and_then(|v| v.as_str()) == Some("ping");
                let type_ok = resp.get("type").and_then(|v| v.as_str()) == Some("response");
                let text_ok = resp.get("text").and_then(|v| v.as_str()).map(|s| s.starts_with("ACK:")).unwrap_or(false);
                if app_ok && type_ok && text_ok { Ok(start.elapsed().as_millis()) } else { Err(()) }
            }
            Err(_) => Err(())
        }
    };

    // Repeat loop (unless --once)
    loop {
        let relays = api.list_all_relays(false);

        if relays.is_empty() {
            tracing::warn!("No relays discovered; nothing to check");
        } else {
            for r in &relays {
                let mut ok: u32 = 0;
                let mut fail: u32 = 0;
                let mut times: Vec<u128> = Vec::new();
                for _ in 0..5 {
                    match do_check(r.id(), r.address()) {
                        Ok(ms) => { ok += 1; times.push(ms); }
                        Err(()) => { fail += 1; }
                    }
                }
                times.sort();
                let p50 = times.get((times.len().saturating_sub(1))/2).copied().unwrap_or(0);
                let p95 = if !times.is_empty() { let idx = ((times.len() as f64)*0.95).ceil() as usize - 1; *times.get(idx.min(times.len()-1)).unwrap_or(&0) } else { 0 };
                let avg = if !times.is_empty() { times.iter().sum::<u128>() as f64 / times.len() as f64 } else { 0.0 };
                let rate = if ok + fail > 0 { fail as f64 / (ok + fail) as f64 } else { 1.0 };
                println!(
                    "relay {} @ {} -> ok={} fail={} fail_rate={:.0}% avg={:.1}ms p50={}ms p95={}ms",
                    r.id(), r.address(), ok, fail, rate*100.0, avg, p50, p95
                );
            }
        }

        if once { break; }
        std::thread::sleep(Duration::from_millis(interval_ms));
    }

    // Stop API before exit
    api.access_unsafe_for_tests(|api_mut| api_mut.stop());
}

fn cmd_register(args: Vec<String>) {
    // Simple manual parsing to keep dependencies minimal
    let mut it = args.into_iter();
    let mut handle: Option<String> = None;
    let mut app_id: Option<u64> = None;
    let mut asset_id: Option<u64> = None;
    let mut price_units: Option<u64> = None;
    let mut node_file: Option<String> = None;
    let mut passphrase: Option<String> = None;

    if let Some(arg) = it.clone().next() {
        if arg == "--help" || arg == "-h" {
            warn!("Usage: bingle_cli register --handle <handle> --passphrase <text> --app-id <id> --asset-id <id> --price-units <n> [--node-file <file>] [--stun-servers <list>] [--stun-servers-file <file>]");
            std::process::exit(0);
        }
    }

    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--handle" => { handle = Some(req_value(&mut it, "--handle")); }
            "--passphrase" => { passphrase = Some(req_value(&mut it, "--passphrase")); }
            "--app-id" => { app_id = Some(parse_u64(req_value(&mut it, "--app-id"), "--app-id")); }
            "--asset-id" => { asset_id = Some(parse_u64(req_value(&mut it, "--asset-id"), "--asset-id")); }
            "--price-units" => { price_units = Some(parse_u64(req_value(&mut it, "--price-units"), "--price-units")); }
            "--node-file" => { node_file = Some(req_value(&mut it, "--node-file")); }
            // Accept common STUN options for consistency across commands; ignored for register
            "--stun-servers" => { let _ = req_value(&mut it, "--stun-servers"); }
            "--stun-servers-file" => { let _ = req_value(&mut it, "--stun-servers-file"); }
            s => {
                warn!("Unknown option for register: {}", s);
                std::process::exit(2);
            }
        }
    }

    let handle = match handle { Some(h) => h, None => { warn!("register requires --handle <handle>"); std::process::exit(2); } };
    let passphrase = match passphrase { Some(p) => p, None => { warn!("register requires --passphrase <text>"); std::process::exit(2); } };
    let price_units = match price_units { Some(v) => v, None => { warn!("register requires --price-units <n>"); std::process::exit(2); } };

    // Load node file (may also contain app_id/asset_id) and build config
    let (cfg, node_app_id, node_asset_id) : (AlgoChainConfig, Option<u64>, Option<u64>) = match node_file {
        Some(path) => match parse_node_file_with_ids(&path) {
            Ok((_net, cfg, nid_app, nid_asset)) => (cfg, nid_app, nid_asset),
            Err(e) => { warn!("{}", e); std::process::exit(2); }
        },
        None => (AlgoChainConfig::default(), None, None),
    };

    // Resolve IDs with precedence: node file > CLI > env; error if node+CLI both set
    let (app_id, asset_id) = match resolve_app_asset_ids(node_app_id, node_asset_id, app_id, asset_id) {
        Ok(v) => v,
        Err(e) => { warn!("{}", e); std::process::exit(2); }
    };

    // Build AlgoOps with provided passphrase; address is derived immediately in AlgoOps::new
    let ops = AlgoOps::new(Some(passphrase.clone()), None, Some(cfg));
    let address = match ops.address.as_ref() {
        Some(a) => a.clone(),
        None => {
            warn!("Invalid passphrase: unable to derive address. Provide a valid Algorand mnemonic or supported secret format.");
            std::process::exit(2);
        }
    };

    // Ensure the account is funded
    let bal_algos = loop {
        match ops.account_balance() {
            Ok(Some(b)) => break b,
            Ok(None) => {
                warn!("Account {} not found or balance unavailable. Please ensure the account exists and is funded.", address);
                std::process::exit(2);
            }
            Err(e) => {
                if let Some(ae) = e.downcast_ref::<AlgoError>() {
                    if ae.kind == AlgoErrorKind::HostUnreachable {
                        tracing::error!("Algorand node unreachable: {}. Retrying in 60s...", ae);
                        std::thread::sleep(std::time::Duration::from_secs(60));
                        continue;
                    }
                }
                warn!("Failed to query account balance: {}", e);
                std::process::exit(2);
            }
        }
    };
    if bal_algos <= 0.0 {
        warn!("Account {} has zero balance. Please fund it and retry.", address);
        std::process::exit(2);
    }
    tracing::info!("Using funded account {} (balance {:.6} ALGO)", address, bal_algos);

    // Register the handle on-chain
    let bingle = AlgoBingle::new(ops.clone(), app_id, asset_id);
    loop {
        match bingle.register(app_id, asset_id, &handle, price_units) {
            Ok(txid) => {
                tracing::info!("Successfully registered handle '{}' for {} (tx: {})", handle, address, txid);
                break;
            }
            Err(e) => {
                if let Some(ae) = e.downcast_ref::<AlgoError>() {
                    if ae.kind == AlgoErrorKind::HostUnreachable {
                        tracing::error!("Algorand node unreachable: {}. Retrying in 60s...", ae);
                        std::thread::sleep(std::time::Duration::from_secs(60));
                        continue;
                    }
                }
                warn!("Failed to register handle: {}", e);
                std::process::exit(1);
            }
        }
    }
}

fn cmd_buybingle(args: Vec<String>) {
    // Usage help
    if args.is_empty() || args.iter().any(|a| a == "--help" || a == "-h") {
        warn!("Usage: bingle_cli buybingle <price_algos> --passphrase <text> --app-id <id> --asset-id <id> [--node-file <file>] [--stun-servers <list>] [--stun-servers-file <file>]");
        std::process::exit(if args.is_empty() { 2 } else { 0 });
    }

    let mut it = args.into_iter();
    // First positional: price in ALGOs (decimal), convert to microAlgos
    let price_str = it.next().expect("checked non-empty");
    let price_micro = match parse_algos_decimal_to_microalgos(&price_str) { Ok(v) => v, Err(e) => { warn!("Invalid <price_algos> '{}': {}", price_str, e); std::process::exit(2); } };

    let mut app_id: Option<u64> = None;
    let mut asset_id: Option<u64> = None;
    let mut node_file: Option<String> = None;
    let mut passphrase: Option<String> = None;

    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--app-id" => { app_id = Some(parse_u64(req_value(&mut it, "--app-id"), "--app-id")); }
            "--asset-id" => { asset_id = Some(parse_u64(req_value(&mut it, "--asset-id"), "--asset-id")); }
            "--node-file" => { node_file = Some(req_value(&mut it, "--node-file")); }
            "--passphrase" => { passphrase = Some(req_value(&mut it, "--passphrase")); }
            // Accept common STUN options for consistency across commands; ignored for buybingle
            "--stun-servers" => { let _ = req_value(&mut it, "--stun-servers"); }
            "--stun-servers-file" => { let _ = req_value(&mut it, "--stun-servers-file"); }
            other => { warn!("Unknown option for buybingle: {}", other); std::process::exit(2); }
        }
    }

    let passphrase = match passphrase { Some(p) => p, None => { warn!("buybingle requires --passphrase <text>"); std::process::exit(2); } };

    // Load node file (may also contain app_id/asset_id)
    let (cfg, node_app_id, node_asset_id) : (AlgoChainConfig, Option<u64>, Option<u64>) = match node_file {
        Some(path) => match parse_node_file_with_ids(&path) { Ok((_net, cfg, nid_app, nid_asset)) => (cfg, nid_app, nid_asset), Err(e) => { warn!("{}", e); std::process::exit(2); } },
        None => (AlgoChainConfig::default(), None, None),
    };

    // Resolve IDs with precedence
    let (app_id, asset_id) = match resolve_app_asset_ids(node_app_id, node_asset_id, app_id, asset_id) { Ok(v) => v, Err(e) => { warn!("{}", e); std::process::exit(2); } };

    // Ops
    let ops = AlgoOps::new(Some(passphrase.clone()), None, Some(cfg));
    let address = ops.address.as_ref().cloned().unwrap_or_else(|| { warn!("Invalid passphrase: unable to derive address."); std::process::exit(2); });
    let bal_algos = loop {
        match ops.account_balance() {
            Ok(Some(b)) => break b,
            Ok(None) => break 0.0,
            Err(e) => {
                if let Some(ae) = e.downcast_ref::<AlgoError>() {
                    if ae.kind == AlgoErrorKind::HostUnreachable {
                        tracing::error!("Algorand node unreachable: {}. Retrying in 60s...", ae);
                        std::thread::sleep(std::time::Duration::from_secs(60));
                        continue;
                    }
                }
                break 0.0;
            }
        }
    };
    if bal_algos <= 0.0 { warn!("Account {} has zero/unavailable balance. Please fund it and retry.", address); std::process::exit(2); }
    tracing::info!("Using account {} (balance {:.6} ALGO)", address, bal_algos);

    let bingle = AlgoBingle::new(ops.clone(), app_id, asset_id);
    loop {
        match bingle.buy_bingle(app_id, asset_id, price_micro) {
            Ok(txid) => {
                tracing::info!("buybingle submitted (tx: {})", txid);
                break;
            }
            Err(e) => {
                if let Some(ae) = e.downcast_ref::<AlgoError>() {
                    if ae.kind == AlgoErrorKind::HostUnreachable {
                        tracing::error!("Algorand node unreachable: {}. Retrying in 60s...", ae);
                        std::thread::sleep(std::time::Duration::from_secs(60));
                        continue;
                    }
                }
                warn!("buybingle failed: {}", e);
                std::process::exit(1);
            }
        }
    }
}

fn cmd_sellbingle(args: Vec<String>) {
    // Usage help
    if args.is_empty() || args.iter().any(|a| a == "--help" || a == "-h") {
        warn!("Usage: bingle_cli sellbingle <amount_units> <price_algos> --passphrase <text> --app-id <id> --asset-id <id> [--node-file <file>] [--stun-servers <list>] [--stun-servers-file <file>]");
        std::process::exit(if args.is_empty() { 2 } else { 0 });
    }

    let mut it = args.into_iter();
    let amount_str = it.next().expect("checked non-empty");
    let amount_units = match amount_str.parse::<u64>() { Ok(v) => v, Err(e) => { warn!("Invalid <amount_units> '{}': {}", amount_str, e); std::process::exit(2); } };
    let price_str = match it.next() { Some(s) => s, None => { warn!("sellbingle requires <price_algos> positional argument"); std::process::exit(2); } };
    let price_micro = match parse_algos_decimal_to_microalgos(&price_str) { Ok(v) => v, Err(e) => { warn!("Invalid <price_algos> '{}': {}", price_str, e); std::process::exit(2); } };

    let mut app_id: Option<u64> = None;
    let mut asset_id: Option<u64> = None;
    let mut node_file: Option<String> = None;
    let mut passphrase: Option<String> = None;

    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--app-id" => { app_id = Some(parse_u64(req_value(&mut it, "--app-id"), "--app-id")); }
            "--asset-id" => { asset_id = Some(parse_u64(req_value(&mut it, "--asset-id"), "--asset-id")); }
            "--node-file" => { node_file = Some(req_value(&mut it, "--node-file")); }
            "--passphrase" => { passphrase = Some(req_value(&mut it, "--passphrase")); }
            // Accept common STUN options for consistency across commands; ignored for sellbingle
            "--stun-servers" => { let _ = req_value(&mut it, "--stun-servers"); }
            "--stun-servers-file" => { let _ = req_value(&mut it, "--stun-servers-file"); }
            other => { warn!("Unknown option for sellbingle: {}", other); std::process::exit(2); }
        }
    }

    let passphrase = match passphrase { Some(p) => p, None => { warn!("sellbingle requires --passphrase <text>"); std::process::exit(2); } };

    // Load node file (may also contain app_id/asset_id)
    let (cfg, node_app_id, node_asset_id) : (AlgoChainConfig, Option<u64>, Option<u64>) = match node_file {
        Some(path) => match parse_node_file_with_ids(&path) { Ok((_net, cfg, nid_app, nid_asset)) => (cfg, nid_app, nid_asset), Err(e) => { warn!("{}", e); std::process::exit(2); } },
        None => (AlgoChainConfig::default(), None, None),
    };

    // Resolve IDs with precedence
    let (app_id, asset_id) = match resolve_app_asset_ids(node_app_id, node_asset_id, app_id, asset_id) { Ok(v) => v, Err(e) => { warn!("{}", e); std::process::exit(2); } };

    // Ops
    let ops = AlgoOps::new(Some(passphrase.clone()), None, Some(cfg));
    let address = ops.address.as_ref().cloned().unwrap_or_else(|| { warn!("Invalid passphrase: unable to derive address."); std::process::exit(2); });
    let bal_algos = loop {
        match ops.account_balance() {
            Ok(Some(b)) => break b,
            Ok(None) => break 0.0,
            Err(e) => {
                if let Some(ae) = e.downcast_ref::<AlgoError>() {
                    if ae.kind == AlgoErrorKind::HostUnreachable {
                        tracing::error!("Algorand node unreachable: {}. Retrying in 60s...", ae);
                        std::thread::sleep(std::time::Duration::from_secs(60));
                        continue;
                    }
                }
                break 0.0;
            }
        }
    };
    if bal_algos <= 0.0 { warn!("Account {} has zero/unavailable balance. Please fund it and retry.", address); std::process::exit(2); }
    tracing::info!("Using account {} (balance {:.6} ALGO)", address, bal_algos);

    let bingle = AlgoBingle::new(ops.clone(), app_id, asset_id);
    loop {
        match bingle.sell_bingle(app_id, asset_id, amount_units, price_micro) {
            Ok(txid) => {
                tracing::info!("sellbingle submitted (tx: {})", txid);
                break;
            }
            Err(e) => {
                if let Some(ae) = e.downcast_ref::<AlgoError>() {
                    if ae.kind == AlgoErrorKind::HostUnreachable {
                        tracing::error!("Algorand node unreachable: {}. Retrying in 60s...", ae);
                        std::thread::sleep(std::time::Duration::from_secs(60));
                        continue;
                    }
                }
                warn!("sellbingle failed: {}", e);
                std::process::exit(1);
            }
        }
    }
}

fn req_value(it: &mut impl Iterator<Item = String>, name: &str) -> String {
    match it.next() {
        Some(v) => v,
        None => {
            warn!("{} requires a value", name);
            std::process::exit(2);
        }
    }
}

fn parse_u64(v: String, name: &str) -> u64 {
    match v.parse::<u64>() {
        Ok(n) => n,
        Err(e) => { warn!("Invalid value for {} '{}': {}", name, v, e); std::process::exit(2); }
    }
}

#[allow(dead_code)]
fn write_text_file(path: &str, content: &str) -> std::io::Result<()> {
    let mut f = fs::File::create(path)?;
    f.write_all(content.as_bytes())
}

/// Describes the action to take on shutdown regarding static endpoint unregistration.
#[derive(Debug, PartialEq, Eq)]
enum ShutdownAction {
    /// No static IP was configured; nothing to unregister.
    NoStaticIp,
    /// Static IP was configured but no app_id is available.
    NoAppId,
    /// Static IP and app_id are available but no passphrase.
    NoPassphrase,
    /// All parameters are available; unregister the endpoint.
    Unregister {
        app_id: u64,
        passphrase: String,
        algo_provider_config: Option<AlgoChainConfig>,
        asset_id: Option<u64>,
    },
}

/// Determine the shutdown action based on the current StartOptions.
/// The app_id_env parameter allows injecting the APP_ID environment variable for testability.
fn resolve_shutdown_action(
    opts: &rust_comms::api::bingle_api::StartOptions,
    app_id_env: Option<u64>,
) -> ShutdownAction {
    if opts.static_ip.is_none() {
        return ShutdownAction::NoStaticIp;
    }
    let app_id_opt = opts.app_id.or(app_id_env);
    match app_id_opt {
        None => ShutdownAction::NoAppId,
        Some(app_id) => {
            match opts.algo_passphrase {
                Some(ref passphrase) => ShutdownAction::Unregister {
                    app_id,
                    passphrase: passphrase.clone(),
                    algo_provider_config: opts.algo_provider_config.clone(),
                    asset_id: opts.asset_id,
                },
                None => ShutdownAction::NoPassphrase,
            }
        }
    }
}

fn install_ctrlc_handler(tx: Sender<()>) {
    if let Err(e) = ctrlc::set_handler(move || {
        tracing::info!("Received shutdown signal (SIGINT/SIGTERM); sending message to main thread to exit");
        let _ = tx.send(());
    }) {
        warn!("Failed to install signal handler: {}", e);
    }
}

