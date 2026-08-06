//! End-to-end test for `bingle_cli chat` against a real algokit localnet.
//!
//! Provisions the same environment `bingle_core`'s `send_message_to_id_integration` uses — funded
//! accounts, a deployed Bingle app + asset, registered handles, two root relays and STUN servers —
//! via the shared `bingle_test::localnet` harness, then drives the **compiled `bingle_cli chat`
//! binary** as the sender and asserts an in-process peer receives the message.
//!
//! Requires a running `algokit localnet` (algod `localhost:4001`, indexer `localhost:8980`) and the
//! `algokit`/`goal` CLIs on `PATH`. Skips cleanly (passes) when localnet is not reachable.

use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bingle_core::api::bingle_api::{BingleApi, OnMessageHandler};
use bingle_core::api::bingle_api_impl::BingleApiImpl;
use bingle_core::engine::BingleAccessUnsafeForTests;
use bingle_local::api::bingle_local_api::BingleLocalApi;
use bingle_local::api::bingle_local_api_impl::{BingleApiLocalImpl, LocalApiConfig};
use bingle_test::localnet::{provision, setup_localnet, test_util};

// A funded localnet account used as the in-process receiver (not one of the relay accounts).
const RECEIVER_ADDRESS: &str = "P577OS2FPV7COU3Y43PCTS2IIZ5HAXHBZRHINAATVA5ECCEYKFSEVIYTHE";
const RECEIVER_PASSPHRASE: &str = "lift all minute first hair appear panel unfold pony property also dinosaur start robot board erupt tent pink essence stem protect ugly orphan absent dust";

/// Install an OnMessage handler on an in-process client that records the first message and its text.
fn capture_messages(
    api: &Arc<BingleApiImpl>,
    received: &Arc<AtomicBool>,
    text: &Arc<Mutex<Option<String>>>,
) {
    let received = received.clone();
    let text_store = text.clone();
    let handler: Arc<OnMessageHandler> = Arc::new(move |sender, sender_handle, message| {
        tracing::info!("[e2e receiver] got message from {sender} ({sender_handle}): {message}");
        if let Some(t) = message.get("text").and_then(|v| v.as_str())
            && let Ok(mut g) = text_store.lock()
        {
            *g = Some(t.to_string());
        }
        received.store(true, Ordering::SeqCst);
    });
    api.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.set_on_message(Some(handler)));
}

/// Write a `--state_file` for the sender so `chat` runs in "already registered" mode: it holds the
/// funded account's keypair and its (on-chain) handle, so no `--passphrase`/`--handle` is needed and
/// registration is skipped.
fn write_sender_state(path: &std::path::Path, passphrase: &str, handle: &str) {
    let mut local = BingleApiLocalImpl::new(LocalApiConfig::default());
    local
        .import_keypair(passphrase.to_string())
        .expect("import sender keypair");
    // Mark the account ACTIVE with its registered handle so the bridge resolves it offline.
    local.seed_own_handle_for_tests(handle.to_string());
    local
        .save(path.to_string_lossy().as_ref())
        .expect("save sender state");
}

#[ntest::timeout(300_000)]
#[serial_test::serial(localnet_chat)]
#[test]
#[cfg(not(target_os = "ios"))]
fn localnet_chat_send_from_cli_delivers_to_peer() {
    if !provision::localnet_available() {
        eprintln!("skipping localnet chat e2e: algokit localnet not reachable at localhost:4001");
        return;
    }
    test_util::init_test_logging();

    let cfg = test_util::localnet_config();
    // Fund every account we use (relays + sender + receiver).
    setup_localnet::ensure_localnet_accounts_funded(
        &cfg,
        &[
            test_util::ADDRESS_SPEND,
            test_util::ADDRESS_RECEIVE,
            test_util::ADDRESS_10MIL,
            RECEIVER_ADDRESS,
        ],
    )
    .expect("fund localnet accounts");

    // Deploy app + asset, register two root relays.
    let creator = test_util::ops_from_mnemonic(
        test_util::ADDRESS_SPEND,
        test_util::PASSPHRASE_SPEND,
        cfg.clone(),
    );
    let (app_id, asset_id) = test_util::deploy_bingle_app_and_asset(&creator, "BINGLE$", 1_000_000);

    let r1_port = test_util::find_unused_loopback_port();
    let r2_port = test_util::find_unused_loopback_port();
    let relay1_addr = std::net::SocketAddr::new(std::net::Ipv4Addr::LOCALHOST.into(), r1_port);
    let relay2_addr = std::net::SocketAddr::new(std::net::Ipv4Addr::LOCALHOST.into(), r2_port);
    provision::register_relays(app_id, asset_id, relay1_addr, relay2_addr);

    let relay1 = test_util::start_root_relay(
        "relay1",
        relay1_addr,
        test_util::PASSPHRASE_SPEND,
        app_id,
        cfg.clone(),
    );
    let relay2 = test_util::start_root_relay(
        "relay2",
        relay2_addr,
        test_util::PASSPHRASE_RECEIVE,
        app_id,
        cfg.clone(),
    );

    let (mut s1, mut s2, stun_list) = provision::setup_stun_servers(false);

    // Register the sender ("sender") and receiver ("receiver") handles on-chain so both resolve.
    test_util::register_client_on_blockchain(
        test_util::ADDRESS_10MIL,
        test_util::PASSPHRASE_10MIL,
        "sender",
        app_id,
        asset_id,
        &creator,
        cfg.clone(),
    );
    test_util::register_client_on_blockchain(
        RECEIVER_ADDRESS,
        RECEIVER_PASSPHRASE,
        "receiver",
        app_id,
        asset_id,
        &creator,
        cfg.clone(),
    );

    // Start the in-process receiver and capture incoming messages.
    let receiver = provision::start_client(
        "receiver",
        RECEIVER_PASSPHRASE,
        stun_list.clone(),
        app_id,
        cfg.clone(),
    );
    let received = Arc::new(AtomicBool::new(false));
    let got_text: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    capture_messages(&receiver, &received, &got_text);
    assert!(
        test_util::wait_for_registered(&receiver, Duration::from_secs(180)),
        "receiver did not reach Registered state"
    );

    // Prepare the sender's state file + a localnet node file for the chat binary.
    let dir = tempfile::tempdir().expect("tempdir");
    let sender_state = dir.path().join("sender.state.json");
    write_sender_state(&sender_state, test_util::PASSPHRASE_10MIL, "sender");
    let node_file = dir.path().join("localnet.node.json");
    provision::write_localnet_node_file(&node_file, app_id, asset_id);

    let stun_arg = format!("{},{}", stun_list[0], stun_list[1]);

    // Drive the compiled `bingle_cli chat` binary as the sender. Keep stdin open (don't send !exit)
    // so the REPL stays alive and the send + background retry can complete before we tear down.
    let mut child = Command::new(env!("CARGO_BIN_EXE_bingle_cli"))
        .args([
            "chat",
            "--info",
            "--state_file",
            &sender_state.to_string_lossy(),
            "--node-file",
            &node_file.to_string_lossy(),
            "--stun-servers",
            &stun_arg,
            "--to",
            "receiver",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn bingle_cli chat");

    let mut child_stdin = child.stdin.take().expect("child stdin");
    // Give the CLI a moment to start its engine and reach listening before sending.
    std::thread::sleep(Duration::from_secs(20));
    writeln!(child_stdin, "Hello from CLI").expect("write message to chat stdin");
    child_stdin.flush().ok();

    // Wait for the receiver to get it (the pending-retry model keeps trying while the relay path
    // settles).
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(120) {
        if received.load(Ordering::SeqCst) {
            break;
        }
        std::thread::sleep(Duration::from_millis(200));
    }

    // Tear down the chat process (drop stdin first so the REPL sees EOF).
    drop(child_stdin);
    let _ = child.kill();
    let _ = child.wait();

    let got = received.load(Ordering::SeqCst);
    let text = got_text.lock().expect("lock text").clone();

    // Tear down infra before asserting so a failure doesn't leak relays/STUN.
    relay1.access_unsafe_for_tests(|r: &mut BingleApiImpl| r.stop());
    relay2.access_unsafe_for_tests(|r: &mut BingleApiImpl| r.stop());
    receiver.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.stop());
    s1.stop();
    s2.stop();

    assert!(
        got,
        "receiver did not get the message from the chat CLI within the timeout"
    );
    assert_eq!(
        text.as_deref(),
        Some("Hello from CLI"),
        "receiver got an unexpected message payload"
    );
}
