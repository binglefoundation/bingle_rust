use rust_comms::util::cli_utils::parse_start_options_from_args;
use std::fs;
use std::path::PathBuf;

fn write_temp_nodefile(content: &str, suffix: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let pid = std::process::id();
    p.push(format!("nodefile-{}-{}.{}", pid, now, suffix));
    fs::write(&p, content).expect("failed to write temp node file");
    p
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn parses_node_file_with_null_token_fields() {
    let file = write_temp_nodefile(
        r#"{
        "client_api_url": "https://api.example",
        "client_api_port": 443,
        "indexer_api_url": "https://idx.example",
        "indexer_api_port": 443,
        "token": null,
        "token_key": null
    }"#,
        "json",
    );

    let args = vec![
        "--handle".into(),
        "user-null".into(),
        "--node-file".into(),
        file.to_string_lossy().to_string(),
    ];

    let opts = parse_start_options_from_args(args).expect("should parse args");
    assert_eq!(opts.handle, "user-null");
    let cfg = opts.algo_provider_config.as_ref().expect("config present");
    assert_eq!(cfg.client_api_url, "https://api.example");
    assert_eq!(cfg.indexer_api_url, "https://idx.example");
    assert_eq!(cfg.token, None);
    assert_eq!(cfg.token_key, None);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn parses_node_file_with_missing_token_fields() {
    // token and token_key completely omitted
    let file = write_temp_nodefile(
        r#"{
        "network": "mainnet",
        "client_api_url": "https://api2.example",
        "client_api_port": 443,
        "indexer_api_url": "https://idx2.example",
        "indexer_api_port": 443
    }"#,
        "json",
    );

    let args = vec![
        "--handle".into(),
        "user-missing".into(),
        "--node-file".into(),
        file.to_string_lossy().to_string(),
    ];

    let opts = parse_start_options_from_args(args).expect("should parse args");
    assert_eq!(opts.handle, "user-missing");
    let cfg = opts.algo_provider_config.as_ref().expect("config present");
    assert_eq!(cfg.client_api_url, "https://api2.example");
    assert_eq!(cfg.indexer_api_url, "https://idx2.example");
    assert!(cfg.token.is_none());
    assert!(cfg.token_key.is_none());
    assert_eq!(opts.algo_network.as_deref(), Some("mainnet"));
}
