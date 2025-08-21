use rust_comms::algo_ops::{AlgoOps, AlgoProviderConfig};
use base64::{engine::general_purpose, Engine as _};
use algonaut::crypto::mnemonic;
use std::env;

// Localnet token from Algorand docs / Algokit localnet
pub const LOCALNET_TOKEN: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

// Provided accounts and mnemonics (mnemonics are used here via algonaut to derive the seed)
#[allow(dead_code)]
pub const PASSPHRASE_10MIL: &str = "provide protect forest couch shaft buyer tenant language almost response chief roast spider scorpion injury they good ecology super east domain thunder shrimp absent output";
#[allow(dead_code)]
pub const ADDRESS_10MIL: &str = "P577PSTDICQ6PQFBR5YMDMJ2YVK7LT5V4GOPNVDLCEDJIL7XGRWC5BRFWA";

pub const ADDRESS_SPEND: &str = "4TKGNGRAUHMQI4EOQ34L2AIDX2VGS4OZNZIOE6BLEQFZUDRSB6RJRBPVRE";
#[allow(dead_code)]
pub const PASSPHRASE_SPEND: &str = "theme term glow reflect essence artefact tired bicycle february demand vacuum tent faculty arch elevator rent already anchor rough cry sketch nurse mom able inquiry";

pub const ADDRESS_RECEIVE: &str = "OO3BIFZDJPGMNXZ74NOVH5KZ5WBL3KCPLPELAF32P7HDCQGQIBID7PJC7A";
#[allow(dead_code)]
pub const PASSPHRASE_RECEIVE: &str = "earth idle country misery matrix wolf tired cabin craft roof quantum comfort answer praise second scout title napkin crop trial industry glue kid absorb midnight";

pub fn localnet_config() -> AlgoProviderConfig {
    AlgoProviderConfig {
        client_api_url: "http://localhost".to_string(),
        client_api_port: 4001,
        indexer_api_url: "http://localhost".to_string(),
        indexer_api_port: 8980,
        token: Some(LOCALNET_TOKEN.to_string()),
        token_key: Some("X-Algo-API-Token".to_string()),
    }
}

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

pub fn ops_from_mnemonic(addr: &str, mnem: &str, cfg: AlgoProviderConfig) -> AlgoOps {
    let key32 = mnemonic::to_key(mnem).expect("mnemonic to key");
    let pass = format!("b64:{}", general_purpose::STANDARD.encode(&key32));
    AlgoOps::new(Some(pass), Some(addr.to_string()), Some(cfg))
}
