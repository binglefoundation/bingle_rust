use std::fs;
use std::io::Write;
use std::sync::Arc;
use std::sync::mpsc::{channel, Sender};

use rust_comms::api::bingle_api::{BingleApi, OnConnectHandler, OnMessageHandler};
use rust_comms::api::bingle_api_impl::BingleApiImpl;
use rust_comms::util::cli_utils::{parse_start_options_from_args, parse_algos_decimal_to_microalgos, parse_node_file_with_ids, resolve_app_asset_ids};
use rust_comms::blockchain::algo_ops::{AlgoOps, AlgoProviderConfig};
use rust_comms::blockchain::algo_bingle::AlgoBingle;

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
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
        "--help" | "-h" => print_usage_and_exit(0),
        other => {
            eprintln!("Unknown subcommand: {}", other);
            std::process::exit(2);
        }
    }
}

fn print_usage_and_exit(code: i32) {
    let usage = "Usage: bingle_cli <run|register|buybingle|sellbingle> [options]\n  bingle_cli run [--handle <handle>|<handle>] [--passphrase <text>] [--relay] [--static-ip <ip:port>] [--stun-servers <list>] [--stun-servers-file <file>] [--node-file <file>]\n  bingle_cli register --handle <handle> --passphrase <text> --app-id <id> --asset-id <id> --price-units <n> [--node-file <file>]\n  bingle_cli buybingle <price_algos> --passphrase <text> --app-id <id> --asset-id <id> [--node-file <file>]\n  bingle_cli sellbingle <amount_units> <price_algos> --passphrase <text> --app-id <id> --asset-id <id> [--node-file <file>]";
    if code == 0 { println!("{}", usage); } else { eprintln!("{}", usage); }
    std::process::exit(code);
}

fn cmd_run(args: Vec<String>) {
    // Support subcommand help
    if args.len() == 1 && (args[0] == "--help" || args[0] == "-h") {
        eprintln!("Usage: bingle_cli run [--handle <handle>|<handle>] [--passphrase <text>] [--relay] [--static-ip <ip:port>] [--stun-servers <list>] [--stun-servers-file <file>] [--node-file <file>]");
        std::process::exit(0);
    }

    // Parse CLI args into StartOptions
    let opts = match parse_start_options_from_args(args.clone()) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("Error: {}\nUsage: bingle_cli run [--handle <handle>|<handle>] [--passphrase <text>] [--relay] [--static-ip <ip:port>] [--stun-servers <list>] [--stun-servers-file <file>] [--node-file <file>]", e);
            std::process::exit(2);
        }
    };

    // If a static IP was provided, attempt to register it on-chain for discovery BEFORE starting the protocol
    if let Some(static_addr) = opts.static_ip {
        // Resolve app_id from StartOptions or APP_ID env
        eprintln!("Registering static IP {} for on-chain discovery, opts={:?}", static_addr, opts);
        let app_id_opt = opts.app_id.or_else(|| std::env::var("APP_ID").ok().and_then(|s| s.parse::<u64>().ok()));
        match app_id_opt {
            Some(app_id) => {
                // Require a passphrase to sign the transaction
                if opts.algo_passphrase.is_none() {
                    eprintln!("--static-ip was provided but no Algorand passphrase (--passphrase) was set; skipping on-chain register_endpoint");
                } else {
                    let ops = AlgoOps::new(opts.algo_passphrase.clone(), None, opts.algo_provider_config.clone());
                    let bingle = AlgoBingle::new(ops);
                    match bingle.register_endpoint(app_id, &static_addr.to_string()) {
                        Ok(txid) => {
                            println!("Registered static endpoint {} for app_id {} (tx: {})", static_addr, app_id, txid);
                        }
                        Err(e) => {
                            eprintln!("Failed to register static endpoint '{}': {}", static_addr, e);
                        }
                    }
                }
            }
            None => {
                eprintln!("--static-ip was provided but app_id is missing; set --node-file with app_id or APP_ID env to enable on-chain register_endpoint");
            }
        }
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
            eprintln!("Usage: bingle_cli register --handle <handle> --passphrase <text> --app-id <id> --asset-id <id> --price-units <n> [--node-file <file>]");
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
            s => {
                eprintln!("Unknown option for register: {}", s);
                std::process::exit(2);
            }
        }
    }

    let handle = match handle { Some(h) => h, None => { eprintln!("register requires --handle <handle>"); std::process::exit(2); } };
    let passphrase = match passphrase { Some(p) => p, None => { eprintln!("register requires --passphrase <text>"); std::process::exit(2); } };
    let price_units = match price_units { Some(v) => v, None => { eprintln!("register requires --price-units <n>"); std::process::exit(2); } };

    // Load node file (may also contain app_id/asset_id) and build config
    let (cfg, node_app_id, node_asset_id) : (AlgoProviderConfig, Option<u64>, Option<u64>) = match node_file {
        Some(path) => match parse_node_file_with_ids(&path) {
            Ok((_net, cfg, nid_app, nid_asset)) => (cfg, nid_app, nid_asset),
            Err(e) => { eprintln!("{}", e); std::process::exit(2); }
        },
        None => (AlgoProviderConfig::default(), None, None),
    };

    // Resolve IDs with precedence: node file > CLI > env; error if node+CLI both set
    let (app_id, asset_id) = match resolve_app_asset_ids(node_app_id, node_asset_id, app_id, asset_id) {
        Ok(v) => v,
        Err(e) => { eprintln!("{}", e); std::process::exit(2); }
    };

    // Build AlgoOps with provided passphrase; address is derived immediately in AlgoOps::new
    let ops = AlgoOps::new(Some(passphrase.clone()), None, Some(cfg));
    let address = match ops.address.as_ref() {
        Some(a) => a.clone(),
        None => {
            eprintln!("Invalid passphrase: unable to derive address. Provide a valid Algorand mnemonic or supported secret format.");
            std::process::exit(2);
        }
    };

    // Ensure the account is funded
    let bal_algos = match ops.account_balance() {
        Ok(Some(b)) => b,
        Ok(None) => {
            eprintln!("Account {} not found or balance unavailable. Please ensure the account exists and is funded.", address);
            std::process::exit(2);
        }
        Err(e) => {
            eprintln!("Failed to query account balance: {}", e);
            std::process::exit(2);
        }
    };
    if bal_algos <= 0.0 {
        eprintln!("Account {} has zero balance. Please fund it and retry.", address);
        std::process::exit(2);
    }
    println!("Using funded account {} (balance {:.6} ALGO)", address, bal_algos);

    // Register the handle on-chain
    let bingle = AlgoBingle::new(ops.clone());
    match bingle.register(app_id, asset_id, &handle, price_units) {
        Ok(txid) => {
            println!("Successfully registered handle '{}' for {} (tx: {})", handle, address, txid);
        }
        Err(e) => {
            eprintln!("Failed to register handle: {}", e);
            std::process::exit(1);
        }
    }
}

fn cmd_buybingle(args: Vec<String>) {
    // Usage help
    if args.is_empty() || args.iter().any(|a| a == "--help" || a == "-h") {
        eprintln!("Usage: bingle_cli buybingle <price_algos> --passphrase <text> --app-id <id> --asset-id <id> [--node-file <file>]");
        std::process::exit(if args.is_empty() { 2 } else { 0 });
    }

    let mut it = args.into_iter();
    // First positional: price in ALGOs (decimal), convert to microAlgos
    let price_str = it.next().expect("checked non-empty");
    let price_micro = match parse_algos_decimal_to_microalgos(&price_str) { Ok(v) => v, Err(e) => { eprintln!("Invalid <price_algos> '{}': {}", price_str, e); std::process::exit(2); } };

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
            other => { eprintln!("Unknown option for buybingle: {}", other); std::process::exit(2); }
        }
    }

    let passphrase = match passphrase { Some(p) => p, None => { eprintln!("buybingle requires --passphrase <text>"); std::process::exit(2); } };

    // Load node file (may also contain app_id/asset_id)
    let (cfg, node_app_id, node_asset_id) : (AlgoProviderConfig, Option<u64>, Option<u64>) = match node_file {
        Some(path) => match parse_node_file_with_ids(&path) { Ok((_net, cfg, nid_app, nid_asset)) => (cfg, nid_app, nid_asset), Err(e) => { eprintln!("{}", e); std::process::exit(2); } },
        None => (AlgoProviderConfig::default(), None, None),
    };

    // Resolve IDs with precedence
    let (app_id, asset_id) = match resolve_app_asset_ids(node_app_id, node_asset_id, app_id, asset_id) { Ok(v) => v, Err(e) => { eprintln!("{}", e); std::process::exit(2); } };

    // Ops
    let ops = AlgoOps::new(Some(passphrase.clone()), None, Some(cfg));
    let address = ops.address.as_ref().cloned().unwrap_or_else(|| { eprintln!("Invalid passphrase: unable to derive address."); std::process::exit(2); });
    let bal_algos = ops.account_balance().ok().flatten().unwrap_or(0.0);
    if bal_algos <= 0.0 { eprintln!("Account {} has zero/unavailable balance. Please fund it and retry.", address); std::process::exit(2); }
    println!("Using account {} (balance {:.6} ALGO)", address, bal_algos);

    let bingle = AlgoBingle::new(ops.clone());
    match bingle.buy_bingle(app_id, asset_id, price_micro) {
        Ok(txid) => { println!("buybingle submitted (tx: {})", txid); }
        Err(e) => { eprintln!("buybingle failed: {}", e); std::process::exit(1); }
    }
}

fn cmd_sellbingle(args: Vec<String>) {
    // Usage help
    if args.is_empty() || args.iter().any(|a| a == "--help" || a == "-h") {
        eprintln!("Usage: bingle_cli sellbingle <amount_units> <price_algos> --passphrase <text> --app-id <id> --asset-id <id> [--node-file <file>]");
        std::process::exit(if args.is_empty() { 2 } else { 0 });
    }

    let mut it = args.into_iter();
    let amount_str = it.next().expect("checked non-empty");
    let amount_units = match amount_str.parse::<u64>() { Ok(v) => v, Err(e) => { eprintln!("Invalid <amount_units> '{}': {}", amount_str, e); std::process::exit(2); } };
    let price_str = match it.next() { Some(s) => s, None => { eprintln!("sellbingle requires <price_algos> positional argument"); std::process::exit(2); } };
    let price_micro = match parse_algos_decimal_to_microalgos(&price_str) { Ok(v) => v, Err(e) => { eprintln!("Invalid <price_algos> '{}': {}", price_str, e); std::process::exit(2); } };

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
            other => { eprintln!("Unknown option for sellbingle: {}", other); std::process::exit(2); }
        }
    }

    let passphrase = match passphrase { Some(p) => p, None => { eprintln!("sellbingle requires --passphrase <text>"); std::process::exit(2); } };

    // Load node file (may also contain app_id/asset_id)
    let (cfg, node_app_id, node_asset_id) : (AlgoProviderConfig, Option<u64>, Option<u64>) = match node_file {
        Some(path) => match parse_node_file_with_ids(&path) { Ok((_net, cfg, nid_app, nid_asset)) => (cfg, nid_app, nid_asset), Err(e) => { eprintln!("{}", e); std::process::exit(2); } },
        None => (AlgoProviderConfig::default(), None, None),
    };

    // Resolve IDs with precedence
    let (app_id, asset_id) = match resolve_app_asset_ids(node_app_id, node_asset_id, app_id, asset_id) { Ok(v) => v, Err(e) => { eprintln!("{}", e); std::process::exit(2); } };

    // Ops
    let ops = AlgoOps::new(Some(passphrase.clone()), None, Some(cfg));
    let address = ops.address.as_ref().cloned().unwrap_or_else(|| { eprintln!("Invalid passphrase: unable to derive address."); std::process::exit(2); });
    let bal_algos = ops.account_balance().ok().flatten().unwrap_or(0.0);
    if bal_algos <= 0.0 { eprintln!("Account {} has zero/unavailable balance. Please fund it and retry.", address); std::process::exit(2); }
    println!("Using account {} (balance {:.6} ALGO)", address, bal_algos);

    let bingle = AlgoBingle::new(ops.clone());
    match bingle.sell_bingle(app_id, asset_id, amount_units, price_micro) {
        Ok(txid) => { println!("sellbingle submitted (tx: {})", txid); }
        Err(e) => { eprintln!("sellbingle failed: {}", e); std::process::exit(1); }
    }
}

fn req_value(it: &mut impl Iterator<Item = String>, name: &str) -> String {
    match it.next() {
        Some(v) => v,
        None => {
            eprintln!("{} requires a value", name);
            std::process::exit(2);
        }
    }
}

fn parse_u64(v: String, name: &str) -> u64 {
    match v.parse::<u64>() {
        Ok(n) => n,
        Err(e) => { eprintln!("Invalid value for {} '{}': {}", name, v, e); std::process::exit(2); }
    }
}

#[allow(dead_code)]
fn write_text_file(path: &str, content: &str) -> std::io::Result<()> {
    let mut f = fs::File::create(path)?;
    f.write_all(content.as_bytes())
}

fn install_ctrlc_handler(tx: Sender<()>) {
    if let Err(e) = ctrlc::set_handler(move || {
        let _ = tx.send(());
    }) {
        eprintln!("Failed to install Ctrl-C handler: {}", e);
    }
}

