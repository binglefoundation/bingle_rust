// tests/blockchain/algo_bingle/register_collision.rs
use rust_comms::blockchain::algo_bingle::AlgoBingle;
use rust_comms::blockchain::algo_ops::{AlgoOps};
use algonaut::core::{Address, ToMsgPack};
use algonaut::transaction::{
    builder::CallApplication,
    TransferAsset, TxnBuilder,
};
use std::str::FromStr;

#[path = "../../setup_localnet.rs"]
pub mod setup_localnet;
#[macro_use]
#[path = "../../test_util.rs"]
pub mod test_util;

#[cfg_attr(not(target_os = "ios"), test)]
#[ignore] // needs localnet
pub fn test_register_collision_same_block() {
    test_util::init_test_logging();
    skip_if_no_localnet!();
    let cfg = test_util::localnet_config();
    
    // 1. Setup accounts
    setup_localnet::ensure_localnet_accounts_funded(&cfg, &[test_util::ADDRESS_SPEND, test_util::ADDRESS_RECEIVE])
        .expect("Failed to fund accounts");

    let ops_a = test_util::ops_from_mnemonic(test_util::ADDRESS_SPEND, test_util::PASSPHRASE_SPEND, cfg.clone());
    let ops_b = test_util::ops_from_mnemonic(test_util::ADDRESS_RECEIVE, test_util::PASSPHRASE_RECEIVE, cfg.clone());

    // 2. Deploy app and asset
    let (app_id, asset_id) = test_util::deploy_bingle_app_and_asset(&ops_a, "BINGLE", 1000000);
    
    // 3. Opt-in B to app and asset
    let ab_b = AlgoBingle::new(ops_b.clone(), app_id, asset_id);
    ab_b.opt_in_sender_to_asset(asset_id).expect("B opt-in asset");
    ops_b.opt_in_app(app_id).expect("B opt-in app");
    
    // Give some Bingle$ to B so it can pay the registration fee
    ops_a.send_asset(asset_id, 10, test_util::ADDRESS_RECEIVE).expect("send Bingle$ to B");

    // Also opt-in A to app (deployer is already creator, but needs to opt-in to write local state)
    ops_a.opt_in_app(app_id).expect("A opt-in app");

    // 4. Build group of 4 transactions
    let client = ops_a.algod_client().expect("algod client");
    
    // Use tokio runtime to get params
    let params = {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        rt.block_on(client.suggested_transaction_params()).expect("params")
    };
    
    let addr_a = Address::from_str(test_util::ADDRESS_SPEND).unwrap();
    let addr_b = Address::from_str(test_util::ADDRESS_RECEIVE).unwrap();
    let app_addr = Address::from_str(&ops_a.contract_address(app_id).unwrap()).unwrap();

    // A: AssetTransfer
    let ax_a = TransferAsset::new(addr_a, asset_id, 1, app_addr).build();
    let tx_ax_a = TxnBuilder::with(&params, ax_a).note(AlgoOps::unique_note()).build().unwrap();

    // A: AppCall register("foo")
    let mut args_a: Vec<Vec<u8>> = Vec::new();
    args_a.push(AlgoOps::arc4_selector("register(string)void").to_vec());
    let handle = "foo";
    let handle_bytes = handle.as_bytes();
    let h_len = handle_bytes.len();
    let mut arg_a = Vec::with_capacity(2 + h_len);
    arg_a.extend_from_slice(&(h_len as u16).to_be_bytes());
    arg_a.extend_from_slice(handle_bytes);
    args_a.push(arg_a);
    let call_a = CallApplication::new(addr_a, app_id).app_arguments(args_a).foreign_assets(vec![asset_id]).build();
    let tx_app_a = TxnBuilder::with(&params, call_a).note(AlgoOps::unique_note()).build().unwrap();

    // B: AssetTransfer
    let ax_b = TransferAsset::new(addr_b, asset_id, 1, app_addr).build();
    let tx_ax_b = TxnBuilder::with(&params, ax_b).note(AlgoOps::unique_note()).build().unwrap();

    // B: AppCall register("foo")
    let mut args_b: Vec<Vec<u8>> = Vec::new();
    args_b.push(AlgoOps::arc4_selector("register(string)void").to_vec());
    let mut arg_b = Vec::with_capacity(2 + h_len);
    arg_b.extend_from_slice(&(h_len as u16).to_be_bytes());
    arg_b.extend_from_slice(handle_bytes);
    args_b.push(arg_b);
    let call_b = CallApplication::new(addr_b, app_id).app_arguments(args_b).foreign_assets(vec![asset_id]).build();
    let tx_app_b = TxnBuilder::with(&params, call_b).note(AlgoOps::unique_note()).build().unwrap();

    let mut txs = vec![tx_ax_a, tx_app_a, tx_ax_b, tx_app_b];
    AlgoBingle::assign_group_id(&mut txs).expect("assign group id");

    // 5. Sign
    let sk_a = ops_a.private_key_bytes().unwrap();
    let acc_a = algonaut::transaction::account::Account::from_seed(sk_a.as_slice().try_into().unwrap());
    
    let sk_b = ops_b.private_key_bytes().unwrap();
    let acc_b = algonaut::transaction::account::Account::from_seed(sk_b.as_slice().try_into().unwrap());

    let s1 = acc_a.sign_transaction(txs[0].clone()).unwrap().to_msg_pack().unwrap();
    let s2 = acc_a.sign_transaction(txs[1].clone()).unwrap().to_msg_pack().unwrap();
    let s3 = acc_b.sign_transaction(txs[2].clone()).unwrap().to_msg_pack().unwrap();
    let s4 = acc_b.sign_transaction(txs[3].clone()).unwrap().to_msg_pack().unwrap();

    // 6. Broadcast
    let ab_a = AlgoBingle::new(ops_a.clone(), app_id, asset_id);
    ab_a.broadcast_group(&client, vec![s1, s2, s3, s4]).expect("broadcast group");

    // 7. Verify
    // Wait for indexer to catch up
    std::thread::sleep(std::time::Duration::from_secs(5));

    let winner = ab_a.handle_lookup("foo").expect("handle lookup").expect("winner found");
    assert_eq!(winner, test_util::ADDRESS_SPEND, "Account A should be the winner as it was first in the group");

    // Verify local state HandleTime
    let state_a = ops_a.local_state_for_account(app_id, test_util::ADDRESS_SPEND).unwrap().expect("A local state");
    let state_b = ops_b.local_state_for_account(app_id, test_util::ADDRESS_RECEIVE).unwrap().expect("B local state");

    let time_a = state_a.iter().find(|(k, _)| k == "HandleTime").map(|(_, v)| v.parse::<u64>().unwrap()).expect("HandleTime A");
    let time_b = state_b.iter().find(|(k, _)| k == "HandleTime").map(|(_, v)| v.parse::<u64>().unwrap()).expect("HandleTime B");

    assert!(time_b > time_a, "Account B should have a later HandleTime than Account A even in the same block");
    assert_eq!(time_b, time_a + 1, "Account B should have HandleTime exactly time_a + 1 due to our fix");
}
