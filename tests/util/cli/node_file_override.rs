use std::fs;
use std::path::PathBuf;

use rust_comms::util::cli_utils::parse_start_options_from_args;

fn write_temp_nodefile(content: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    // Use system time and pid to reduce collision risk without extra deps
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
pub fn parses_node_file_and_populates_start_options() {
    let file = write_temp_nodefile(r#"{
        "network": "testnet",
        "client_api_url": "https://testnet-api.example",
        "client_api_port": 443,
        "indexer_api_url": "https://testnet-idx.example",
        "indexer_api_port": 443,
        "token": null,
        "token_key": "X-API-Key"
    }"#);

    let args = vec![
        "--handle".into(), "user1".into(),
        "--node-file".into(), file.to_string_lossy().to_string(),
    ];

    let opts = parse_start_options_from_args(args).expect("should parse args");
    assert_eq!(opts.handle, "user1");

    // StartOptions should carry the provider config parsed from the file (per-instance, no globals)
    let cfg = opts.algo_provider_config.as_ref().expect("algo_provider_config present");
    assert_eq!(cfg.client_api_url, "https://testnet-api.example");
    assert_eq!(cfg.client_api_port, 443);
    assert_eq!(cfg.indexer_api_url, "https://testnet-idx.example");
    assert_eq!(cfg.indexer_api_port, 443);
    assert_eq!(cfg.token, None);
    assert_eq!(cfg.token_key.as_deref(), Some("X-API-Key"));
    assert_eq!(opts.algo_network.as_deref(), Some("testnet"));
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn defaults_to_localnet_when_node_file_not_provided() {
    let args = vec!["--handle".into(), "user2".into()];
    let opts = parse_start_options_from_args(args).expect("should parse args");
    assert_eq!(opts.handle, "user2");

    // No node-file specified; StartOptions should not carry a provider config
    assert!(opts.algo_provider_config.is_none());
}