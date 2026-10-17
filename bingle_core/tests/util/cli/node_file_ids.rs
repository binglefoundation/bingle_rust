use std::path::PathBuf;

use crate::util::test_util::write_project_tmp_file;
use bingle_core::util::cli_utils::parse_start_options_from_args;
use bingle_core::util::config_utils::{
    check_node_and_cli_ids, load_config_and_resolve_ids, parse_node_file_with_ids,
    resolve_app_asset_ids,
};
use serial_test::serial;

fn write_temp_nodefile(content: &str) -> PathBuf {
    write_project_tmp_file("nodefile", ".json", content)
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn parse_node_file_with_app_and_asset_ids() {
    let file = write_temp_nodefile(
        r#"{
        "network": "testnet",
        "client_api_url": "https://api.example",
        "client_api_port": 443,
        "indexer_api_url": "https://idx.example",
        "indexer_api_port": 443,
        "token": null,
        "token_key": null,
        "app_id": 12345,
        "asset_id": 67890
    }"#,
    );

    let (net, cfg, app_id, asset_id) =
        parse_node_file_with_ids(&file.to_string_lossy()).expect("parse ok");
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
#[serial]
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
pub fn load_config_and_resolve_ids_takes_ids_from_node_file() {
    // A node file carrying both ids: config and ids come from it; no CLI ids supplied.
    let file = write_temp_nodefile(
        r#"{
        "network": "testnet",
        "client_api_url": "https://api.example",
        "client_api_port": 443,
        "indexer_api_url": "https://idx.example",
        "indexer_api_port": 443,
        "token": null,
        "token_key": null,
        "app_id": 12345,
        "asset_id": 67890
    }"#,
    );

    let (cfg, app_id, asset_id) =
        load_config_and_resolve_ids(Some(&file.to_string_lossy()), None, None).expect("resolve ok");
    assert_eq!(cfg.client_api_url, "https://api.example");
    assert_eq!(app_id, 12345);
    assert_eq!(asset_id, 67890);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn load_config_and_resolve_ids_takes_ids_from_flags_without_node_file() {
    // No node file: the default config is used and the ids come from the CLI flags. (Deterministic
    // regardless of APP_ID/ASSET_ID env, since a supplied CLI id wins over the env fallback.)
    let (_cfg, app_id, asset_id) =
        load_config_and_resolve_ids(None, Some(42), Some(43)).expect("resolve ok");
    assert_eq!(app_id, 42);
    assert_eq!(asset_id, 43);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn load_config_and_resolve_ids_errors_when_node_file_and_flag_conflict() {
    // The node file carries both ids; also passing either CLI id is the "remove one" conflict.
    let file = write_temp_nodefile(
        r#"{
        "network": "testnet",
        "client_api_url": "https://api.example",
        "client_api_port": 443,
        "indexer_api_url": "https://idx.example",
        "indexer_api_port": 443,
        "token": null,
        "token_key": null,
        "app_id": 12345,
        "asset_id": 67890
    }"#,
    );
    let path = file.to_string_lossy();

    let app_err = load_config_and_resolve_ids(Some(&path), Some(999), None).unwrap_err();
    assert!(app_err.contains("--app-id"), "got: {app_err}");
    let asset_err = load_config_and_resolve_ids(Some(&path), None, Some(999)).unwrap_err();
    assert!(asset_err.contains("--asset-id"), "got: {asset_err}");
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn load_config_and_resolve_ids_propagates_node_file_parse_error() {
    // A missing node file surfaces the parse error rather than resolving.
    let err = load_config_and_resolve_ids(Some("/no/such/node_file.json"), None, None).unwrap_err();
    assert!(!err.is_empty());
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn run_parse_accepts_node_file_flag() {
    let file = write_temp_nodefile(
        r#"{
        "network": "localnet",
        "client_api_url": "http://localhost",
        "client_api_port": 4001,
        "indexer_api_url": "http://localhost",
        "indexer_api_port": 8980,
        "token": null,
        "token_key": null
    }"#,
    );

    let args = vec![
        "--handle".into(),
        "tester".into(),
        "--node-file".into(),
        file.to_string_lossy().to_string(),
    ];
    let opts = parse_start_options_from_args(args).expect("should parse");
    assert_eq!(opts.handle, "tester");
    assert!(opts.algo_provider_config.is_some());
}

/// A node file naming app 12345 and asset 67890, for the #315 conflict tests.
fn node_file_with_ids() -> String {
    write_temp_nodefile(
        r#"{
        "network": "testnet",
        "client_api_url": "https://api.example",
        "client_api_port": 443,
        "indexer_api_url": "https://idx.example",
        "indexer_api_port": 443,
        "token": null,
        "token_key": null,
        "app_id": 12345,
        "asset_id": 67890
    }"#,
    )
    .to_string_lossy()
    .to_string()
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn resolve_ids_accepts_equal_node_and_cli_values() {
    // the same id in both places is not a conflict (#315)
    let (app, asset) =
        resolve_app_asset_ids(Some(7), Some(8), Some(7), Some(8)).expect("equal values agree");
    assert_eq!((app, asset), (7, 8));
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn id_conflict_names_both_values() {
    let err = check_node_and_cli_ids(Some(12345), None, Some(999), None).unwrap_err();
    assert!(
        err.contains("--app-id 999") && err.contains("app_id 12345"),
        "got: {err}"
    );
    let err = check_node_and_cli_ids(None, Some(67890), None, Some(1)).unwrap_err();
    assert!(
        err.contains("--asset-id 1") && err.contains("asset_id 67890"),
        "got: {err}"
    );
    assert!(check_node_and_cli_ids(Some(1), Some(2), None, None).is_ok());
    assert!(check_node_and_cli_ids(None, None, Some(1), Some(2)).is_ok());
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn start_options_report_a_node_file_and_app_id_conflict() {
    // #315: the conflict used to be dropped, leaving the start options with no app id at all
    let args = vec![
        "alice".to_string(),
        "--app-id".to_string(),
        "999".to_string(),
        "--node-file".to_string(),
        node_file_with_ids(),
    ];
    let err = parse_start_options_from_args(args).unwrap_err();
    assert!(
        err.contains("--app-id 999") && err.contains("app_id 12345"),
        "got: {err}"
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn start_options_accept_an_app_id_matching_the_node_file() {
    let args = vec![
        "alice".to_string(),
        "--node-file".to_string(),
        node_file_with_ids(),
        "--app-id".to_string(),
        "12345".to_string(),
    ];
    let opts = parse_start_options_from_args(args).expect("matching ids agree");
    assert_eq!(opts.app_id, Some(12345));
    assert_eq!(opts.asset_id, Some(67890));
}

#[test]
#[cfg(not(target_os = "ios"))]
#[serial]
pub fn start_options_leave_ids_unset_when_none_given() {
    // with no id anywhere the start options stay flexible, as before #315
    unsafe {
        std::env::remove_var("APP_ID");
        std::env::remove_var("ASSET_ID");
    }
    let opts = parse_start_options_from_args(vec!["alice".to_string()]).expect("parse ok");
    assert_eq!(opts.app_id, None);
    assert_eq!(opts.asset_id, None);
}
