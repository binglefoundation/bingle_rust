use rust_comms::algo_ops::{AlgoOps, AlgoProviderConfig, AppArg};
use base64::{engine::general_purpose, Engine as _};
use algonaut::crypto::mnemonic;
use std::env;
use std::fs;

#[path = "setup_localnet.rs"]
mod setup_localnet;

// Localnet token from Algorand docs / Algokit localnet
const LOCALNET_TOKEN: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

// Provided accounts and mnemonics (mnemonics are used here via algonaut to derive the seed)
const ADDRESS_SPEND: &str = "4TKGNGRAUHMQI4EOQ34L2AIDX2VGS4OZNZIOE6BLEQFZUDRSB6RJRBPVRE";
const PASSPHRASE_SPEND: &str = "theme term glow reflect essence artefact tired bicycle february demand vacuum tent faculty arch elevator rent already anchor rough cry sketch nurse mom able inquiry";

fn localnet_config() -> AlgoProviderConfig {
    AlgoProviderConfig {
        client_api_url: "http://localhost".to_string(),
        client_api_port: 4001,
        indexer_api_url: "http://localhost".to_string(),
        indexer_api_port: 8980,
        token: Some(LOCALNET_TOKEN.to_string()),
        token_key: Some("X-Algo-API-Token".to_string()),
    }
}

fn should_run_localnet() -> bool {
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

fn ops_from_mnemonic(addr: &str, mnem: &str, cfg: AlgoProviderConfig) -> AlgoOps {
    let key32 = mnemonic::to_key(mnem).expect("mnemonic to key");
    let pass = format!("b64:{}", general_purpose::STANDARD.encode(&key32));
    AlgoOps::new(Some(pass), Some(addr.to_string()), Some(cfg))
}


#[test]
fn deploy_call_validate_and_delete_teal_app() {
    if !should_run_localnet() {
        eprintln!("SKIP: localnet not available (set RUST_COMMS_RUN_LOCALNET=true to force)");
        return;
    }
    let cfg = localnet_config();
    // Ensure creator account funded
    setup_localnet::ensure_localnet_accounts_funded(&cfg, &[ADDRESS_SPEND])
        .expect("Failed to ensure localnet test accounts funded; install algokit and start localnet");

    let ops = ops_from_mnemonic(ADDRESS_SPEND, PASSPHRASE_SPEND, cfg.clone());

    // Read TEAL sources (both sets present; no fallback needed)
    let approval_src = fs::read_to_string("tests/dapp/mini_approval.teal").expect("read approval teal");
    let clear_src = fs::read_to_string("tests/dapp/mini_clear_state.teal").expect("read clear teal");

    // Compile via algod developer API
    let approval_prog = ops.compile_teal(&approval_src).expect("compile approval teal");
    let clear_prog = ops.compile_teal(&clear_src).expect("compile clear teal");

    // Deploy
    let app_id = ops
        .deploy_app(&approval_prog, &clear_prog, None)
        .expect("deploy app call")
        .expect("created app id");

    // Call app via AlgoOps and validate initial behavior x*2+1
    let x: u64 = 15;
    let (_tx_id, logs) = ops
        .call_app(app_id, ADDRESS_SPEND, None, Some("fn(uint64)uint64"), &[AppArg::Uint(x)])
        .expect("call app");
    assert!(!logs.is_empty(), "app call should emit at least one log");
    let log_bytes = &logs[0];
    assert!(log_bytes.len() >= 12, "expected selector(4)+u64(8) in log");
    let ret_bytes = &log_bytes[4..12];
    let mut eight = [0u8; 8];
    eight.copy_from_slice(ret_bytes);
    let ret = u64::from_be_bytes(eight);
    let expected = 2u64 * x + 1u64;
    assert_eq!(ret, expected, "unexpected return value from fn");

    // Update the app to the mini2 implementation (x*3 - 20)
    let approval2_src = fs::read_to_string("tests/dapp/mini2_approval.teal").expect("read mini2 approval teal");
    let clear2_src = fs::read_to_string("tests/dapp/mini2_clear_state.teal").expect("read mini2 clear teal");
    let approval2_prog = ops.compile_teal(&approval2_src).expect("compile mini2 approval");
    let clear2_prog = ops.compile_teal(&clear2_src).expect("compile mini2 clear");

    ops.update_app(app_id, &approval2_prog, &clear2_prog, None).expect("update app");

    // Call again and validate the new behavior using AlgoOps::call_app
    let (_tx2, logs2) = ops
        .call_app(app_id, ADDRESS_SPEND, None, Some("fn(uint64)uint64"), &[AppArg::Uint(x)])
        .expect("call app after update");
    assert!(!logs2.is_empty(), "app call after update should emit at least one log");
    let log_bytes2 = &logs2[0];
    assert!(log_bytes2.len() >= 12, "expected selector(4)+u64(8) in log after update");
    let ret_bytes2 = &log_bytes2[4..12];
    let mut eight2 = [0u8; 8];
    eight2.copy_from_slice(ret_bytes2);
    let ret2 = u64::from_be_bytes(eight2);
    let expected2 = 3u64 * x - 20u64;
    assert_eq!(ret2, expected2, "unexpected return value from fn after update");

    // Delete app: approval allows creator to delete; require success
    ops.delete_app(app_id).expect("delete app");
}
