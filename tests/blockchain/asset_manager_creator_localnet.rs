use rust_comms::algo_ops::AlgoChainConfig;

#[path = "../setup_localnet.rs"]
mod setup_localnet;
#[macro_use]
#[path = "../test_util.rs"]
mod test_util;
use test_util::{localnet_config, ops_from_mnemonic, ADDRESS_SPEND, PASSPHRASE_SPEND};

#[test]
fn asset_creation_sets_manager_to_creator() {
    skip_if_no_localnet!();

    // Ensure funding for the creator test account
    let cfg: AlgoChainConfig = localnet_config();
    setup_localnet::ensure_localnet_accounts_funded(&cfg, &[ADDRESS_SPEND])
        .expect("Failed to ensure localnet test account funded; install algokit and start localnet");

    // Ops for creator
    let creator = ops_from_mnemonic(ADDRESS_SPEND, PASSPHRASE_SPEND, cfg.clone());

    // Create a tiny ASA
    let asset_id = creator
        .create_asset("MGRCHK", 10)
        .expect("create_asset call")
        .expect("asset id");

    // Query on-chain asset info and assert params.manager equals creator address
    let algod = {
        let url = format!("{}:{}", cfg.client_api_url, cfg.client_api_port);
        let token = cfg.token.clone().unwrap_or_default();
        algonaut::algod::v2::Algod::new(&url, &token).expect("algod client")
    };

    let info = {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().expect("rt");
        rt.block_on(async { algod.asset_information(asset_id).await }).expect("asset_information ok")
    };

    let v = serde_json::to_value(&info).expect("json");
    // manager can appear under params.manager or params.manager-address
    let mgr = v
        .get("params").and_then(|p| p.get("manager").and_then(|x| x.as_str())
            .or_else(|| p.get("manager-address").or_else(|| p.get("manager_address")).and_then(|x| x.as_str())))
        .expect("manager field present");

    assert_eq!(mgr, ADDRESS_SPEND, "asset manager should be the creator address");
}
