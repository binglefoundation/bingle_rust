use std::fs;
use std::path::PathBuf;

use rust_comms::util::config_utils::{parse_start_options_from_args, parse_node_file_with_ids, resolve_app_asset_ids};
use serial_test::serial;

fn write_temp_nodefile(content: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let pid = std::process::id();
    p.push(format!("nodefile-{}-{}.json", pid, now));
    fs::write(&p, content).expect("failed to write temp node file");
    p
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn parse_node_file_with_app_and_asset_ids() {
    let file = write_temp_nodefile(r#"{
        "network": "testnet",
        "client_api_url": "https://api.example",
        "client_api_port": 443,
        "indexer_api_url": "https://idx.example",
        "indexer_api_port": 443,
        "token": null,
        "token_key": null,
        "app_id": 12345,
        "asset_id": 67890
    }"#);

    let (net, cfg, app_id, asset_id) = parse_node_file_with_ids(&file.to_string_lossy()).expect("parse ok");
    assert_eq!(net.as_deref(), Some("testnet"));
    assert_eq!(cfg.client_api_url, "https://api.example");
    assert_eq!(cfg.indexer_api_url, "https://idx.example");
    assert_eq!(app_id, Some(12345));
    assert_eq!(asset_id, Some(67890));
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn resolve_ids_errors_when_node_and_cli_conflict() {
    // node provides app_id, CLI also provides -> error
    let err = resolve_app_asset_ids(Some(1), None, Some(2), Some(3)).unwrap_err();
    assert!(err.contains("--app-id"));
    let err2 = resolve_app_asset_ids(None, Some(1), Some(2), Some(3)).unwrap_err();
    assert!(err2.contains("--asset-id"));
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn node_ids_override_env_vars() {
    // set env vars, but node file should override
    unsafe {
        std::env::set_var("APP_ID", "111");
        std::env::set_var("ASSET_ID", "222");
    }

    let (app, asset) = resolve_app_asset_ids(Some(5), Some(6), None, None).expect("resolve ok");
    assert_eq!(app, 5);
    assert_eq!(asset, 6);
}

#[test]
#[cfg(not(target_os = "ios"))]
#[serial]
pub fn env_ids_used_when_no_node_or_cli() {
    // clear first
    unsafe {
        std::env::remove_var("APP_ID");
        std::env::remove_var("ASSET_ID");
    }
    // expect missing
    assert!(resolve_app_asset_ids(None, None, None, None).is_err());

    // now set env and expect success
    unsafe {
        std::env::set_var("APP_ID", "101");
        std::env::set_var("ASSET_ID", "202");
    }
    let (app, asset) = resolve_app_asset_ids(None, None, None, None).expect("resolve ok");
    assert_eq!(app, 101);
    assert_eq!(asset, 202);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn run_parse_accepts_node_file_flag() {
    let file = write_temp_nodefile(r#"{
        "network": "localnet",
        "client_api_url": "http://localhost",
        "client_api_port": 4001,
        "indexer_api_url": "http://localhost",
        "indexer_api_port": 8980,
        "token": null,
        "token_key": null
    }"#);

    let args = vec![
        "--handle".into(), "tester".into(),
        "--node-file".into(), file.to_string_lossy().to_string(),
    ];
    let opts = parse_start_options_from_args(args).expect("should parse");
    assert_eq!(opts.handle, "tester");
    assert!(opts.algo_provider_config.is_some());
}
