use rust_comms::algo_ops::{AlgoChainConfig, AlgoOps};
use std::env;

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
    // Otherwise, probe localnet health/status quickly
    let cfg = localnet_config();
    let url = format!("{}:{}", cfg.client_api_url, cfg.client_api_port);
    let token = cfg.token.clone().unwrap_or_default();
    match algonaut::algod::v2::Algod::new(&url, &token) {
        Ok(client) => {
            let rt = tokio::runtime::Builder::new_current_thread().enable_all().build();
            if let Ok(rt) = rt {
                if rt.block_on(client.health()).is_ok() { return true; }
                if rt.block_on(client.status()).is_ok() { return true; }
            }
            false
        }
        Err(_) => false,
    }
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
