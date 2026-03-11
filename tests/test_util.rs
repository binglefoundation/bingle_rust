use rust_comms::algo_ops::{AlgoChainConfig, AlgoOps, AppArg};
use std::env;
use std::sync::Once;
use log::LevelFilter;
use std::fs;

// Macro to skip localnet-dependent tests with a standard message.
// Usage: skip_if_no_localnet!();
#[allow(unused_macros)]
macro_rules! skip_if_no_localnet {
    () => {
        if !test_util::should_run_localnet() {
            eprintln!("SKIP: localnet not available (set RUST_COMMS_RUN_LOCALNET=true to force)");
            return;
        }
    };
}

// Localnet token from Algorand docs / Algokit localnet
#[allow(dead_code)]
pub const LOCALNET_TOKEN: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

// Provided accounts and mnemonics (mnemonics are used here via algonaut to derive the seed)
#[allow(dead_code)]
pub const PASSPHRASE_10MIL: &str = "provide protect forest couch shaft buyer tenant language almost response chief roast spider scorpion injury they good ecology super east domain thunder shrimp absent output";
#[allow(dead_code)]
pub const ADDRESS_10MIL: &str = "P577PSTDICQ6PQFBR5YMDMJ2YVK7LT5V4GOPNVDLCEDJIL7XGRWC5BRFWA";
#[allow(dead_code)]
pub const ADDRESS_SPEND: &str = "4TKGNGRAUHMQI4EOQ34L2AIDX2VGS4OZNZIOE6BLEQFZUDRSB6RJRBPVRE";
#[allow(dead_code)]
pub const PASSPHRASE_SPEND: &str = "theme term glow reflect essence artefact tired bicycle february demand vacuum tent faculty arch elevator rent already anchor rough cry sketch nurse mom able inquiry";

#[allow(dead_code)]
pub const ADDRESS_RECEIVE: &str = "OO3BIFZDJPGMNXZ74NOVH5KZ5WBL3KCPLPELAF32P7HDCQGQIBID7PJC7A";
#[allow(dead_code)]
pub const PASSPHRASE_RECEIVE: &str = "earth idle country misery matrix wolf tired cabin craft roof quantum comfort answer praise second scout title napkin crop trial industry glue kid absorb midnight";

#[allow(dead_code)]
pub fn localnet_config() -> AlgoChainConfig {
    AlgoChainConfig {
        client_api_url: "http://localhost".to_string(),
        client_api_port: 4001,
        indexer_api_url: "http://localhost".to_string(),
        indexer_api_port: 8980,
        token: Some(LOCALNET_TOKEN.to_string()),
        token_key: Some("X-Algo-API-Token".to_string()),
        app_id: None,
        asset_id: None,
    }
}

#[allow(dead_code)]
pub fn should_run_localnet() -> bool {
    // Allow overriding via env var for CI / IDE Run Configuration
    if let Ok(val) = env::var("RUST_COMMS_RUN_LOCALNET") {
        let v = val.to_ascii_lowercase();
        if v == "1" || v == "true" || v == "yes" { return true; }
        if v == "0" || v == "false" || v == "no" { return false; }
    }
    // Otherwise, probe localnet health/status quickly AND ensure required TEAL artifacts exist
    let cfg = localnet_config();
    let url = format!("{}:{}", cfg.client_api_url, cfg.client_api_port);
    let token = cfg.token.clone().unwrap_or_default();
    let network_ok = match algonaut::algod::v2::Algod::new(&url, &token) {
        Ok(client) => {
            let rt = tokio::runtime::Builder::new_current_thread().enable_all().build();
            if let Ok(rt) = rt {
                if rt.block_on(client.health()).is_ok() { true }
                else if rt.block_on(client.status()).is_ok() { true }
                else { false }
            } else { false }
        }
        Err(_) => false,
    };
    if !network_ok { return false; }

    // Require local TEAL artifacts to be present unless explicitly overridden by env var above
    use std::path::Path;
    let approval = Path::new("dapp/projects/dapp/smart_contracts/artifacts/bingle_dapp/BingleDapp.approval.teal");
    let clear = Path::new("dapp/projects/dapp/smart_contracts/artifacts/bingle_dapp/BingleDapp.clear.teal");
    if !(approval.exists() && clear.exists()) {
        eprintln!("SKIP: localnet detected but TEAL artifacts missing under dapp/.../artifacts/bingle_dapp");
        return false;
    }

    true
}

#[allow(dead_code)]
pub fn ops_from_mnemonic(addr: &str, mnem: &str, cfg: AlgoChainConfig) -> AlgoOps {
    // Pass the mnemonic directly as the passphrase (ASCII string)
    let pass = mnem.to_string();
    AlgoOps::new(Some(pass), Some(addr.to_string()), Some(cfg))
}

// Shared helper for tests: allocate a free UDP port on loopback.
#[allow(dead_code)]
pub fn find_unused_loopback_port() -> u16 {
    use std::net::{IpAddr, Ipv4Addr, UdpSocket};
    let sock = UdpSocket::bind((IpAddr::V4(Ipv4Addr::LOCALHOST), 0)).expect("bind temp socket");
    let port = sock.local_addr().expect("local addr").port();
    drop(sock);
    port
}

// Helper: print current working directory for debugging path issues in tests
#[allow(dead_code)]
pub fn print_cwd_for_debug() {
    match std::env::current_dir() {
        Ok(cwd) => eprintln!("Current working directory: {}", cwd.display()),
        Err(e) => eprintln!("Failed to get current working directory: {}", e),
    }
}

#[allow(dead_code)]
pub fn init_test_logging() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let level = LevelFilter::Debug;
        let _ = env_logger::Builder::new()
            .filter_level(level)
            .format_timestamp_millis()
            .try_init();
        // Panic hook that logs at error! and then defers to default behavior
        let default_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |pi| {
            log::error!("PANIC: {}", pi);
            default_hook(pi);
        }));
    });
}

/// Helper: Deploy the Bingle application to localnet using the TEAL artifacts in the `dapp/` folder.
/// Returns the app_id of the deployed contract.
/// Also sets the BinglePrice to 1 microAlgo as a default convenience.
#[allow(dead_code)]
pub fn deploy_bingle_app(ops: &AlgoOps) -> u64 {
    let approval_path = "dapp/projects/dapp/smart_contracts/artifacts/bingle_dapp/BingleDapp.approval.teal";
    let clear_path = "dapp/projects/dapp/smart_contracts/artifacts/bingle_dapp/BingleDapp.clear.teal";

    let approval_src = fs::read_to_string(approval_path).expect("read approval teal from artifacts");
    let clear_src = fs::read_to_string(clear_path).expect("read clear teal from artifacts");

    let approval_bytes = ops.compile_teal(&approval_src).expect("compile approval teal");
    let clear_bytes = ops.compile_teal(&clear_src).expect("compile clear teal");

    let app_id = ops.deploy_app(&approval_bytes, &clear_bytes, None)
        .expect("deploy app call")
        .expect("failed to get app_id after deployment");

    // Default: set Bingle price to 1 microAlgo to allow registration/buy flows to work
    let _ = ops.call_app(app_id, None, Some("set_bingle_price(uint64)void"), &[AppArg::Uint(1)])
        .expect("set_bingle_price(1) call");

    app_id
}

/// Helper: Deploy the Bingle application AND create a corresponding Bingle$ ASA.
/// Sets the app's price to 1 and configures the ASA reserve/clawback to the app address.
/// Returns (app_id, asset_id).
#[allow(dead_code)]
pub fn deploy_bingle_app_and_asset(ops: &AlgoOps, asset_name: &str, total_units: u64) -> (u64, u64) {
    let app_id = deploy_bingle_app(ops);

    // Create ASA with reserve/clawback set to the application address
    let asset_id = ops.create_asset_with_reserve_app(asset_name, total_units, app_id)
        .expect("create_asset_with_reserve_app call")
        .expect("failed to get asset_id after creation");

    // Opt the app account into the ASA so it can receive/send it
    let ab = rust_comms::blockchain::algo_bingle::AlgoBingle::new(ops.clone(), app_id, asset_id);
    let _ = ab.opt_in_app_to_asset(app_id, asset_id).expect("opt_in_app_to_asset call");

    // Also ensure clawback is explicitly set to app if not already covered by create_asset_with_reserve_app
    ops.set_asset_clawback_to_app(app_id, asset_id).expect("set_asset_clawback_to_app call");

    (app_id, asset_id)
}