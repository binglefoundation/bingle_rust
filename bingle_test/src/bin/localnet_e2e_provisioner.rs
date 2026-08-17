//! Localnet backend provisioner for the bingle_jsi Detox e2e (issue #123 / #111 follow-up).
//!
//! The iOS/Android e2e suites need a live network to send against. On testnet that is the
//! always-on `echo-testnet-1` peer plus the two staging relays; on localnet there is nothing
//! running, so this binary stands up the equivalent in one process and keeps it alive for the
//! duration of a test run:
//!
//!   * a deployed Bingle app + asset on the running `algokit localnet`;
//!   * two root relays on loopback (reachable from the iOS simulator, which shares the host
//!     network stack, as `127.0.0.1:<port>`);
//!   * two local STUN servers on loopback so the simulator client discovers a direct endpoint;
//!   * an in-process echo peer (`echo-localnet-1`) that replies `Echo: <text>` — the counterpart
//!     of testnet's `echo-testnet-1`;
//!   * a funded, already-registered **sender** account for the app to import;
//!   * a registered-but-offline **fixture** account (handle on chain, never started, so it has no
//!     AdvertRecord) for the `RecipientNotAdvertised` failure-cause test.
//!
//! It writes the app/asset ids into a `bingle_cli`-compatible node file, the STUN list into a STUN
//! file, and all the derived `BINGLE_E2E_*` values into an env file that `run_e2e_ios.sh` sources.
//! The env file doubling as the readiness signal: it is removed on startup and only written once
//! everything is up, so the orchestrating script can wait on it. The process then parks until it
//! is killed (the shell sends SIGTERM after the suite finishes); the in-process relays, echo peer
//! and STUN servers are threads/sockets in this process and are released when it exits.
//!
//! Run indirectly via `BINGLE_E2E_BACKEND=localnet bash bingle_jsi/example/scripts/run_e2e_ios.sh`.
//! Requires a running `algokit localnet` and the `algokit`/`goal` CLIs on `PATH`. Build with the
//! `localnet` feature (the crate's `[[bin]]` sets it as a required feature).

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use bingle_core::api::bingle_api::{BingleApi, OnMessageHandler};
use bingle_core::api::bingle_api_impl::BingleApiImpl;
use bingle_core::engine::BingleAccessUnsafeForTests;
use bingle_test::localnet::{provision, relay_test_util, setup_localnet, test_util};

// Sender account the app imports (funded + pre-registered so start() sees it as registered, exactly
// like the testnet `BINGLE_E2E_HANDLE`/`BINGLE_E2E_PASSPHRASE` sender).
const SENDER_HANDLE: &str = "e2e-sender";
const SENDER_ADDRESS: &str = test_util::ADDRESS_10MIL;
const SENDER_PASSPHRASE: &str = test_util::PASSPHRASE_10MIL;

// In-process echo peer (counterpart of testnet `echo-testnet-1`).
const ECHO_HANDLE: &str = "echo-localnet-1";
const ECHO_ADDRESS: &str = "P577OS2FPV7COU3Y43PCTS2IIZ5HAXHBZRHINAATVA5ECCEYKFSEVIYTHE";
const ECHO_PASSPHRASE: &str = "lift all minute first hair appear panel unfold pony property also dinosaur start robot board erupt tent pink essence stem protect ugly orphan absent dust";

// Registered-but-offline fixture: its handle resolves on chain but it never starts, so it has no
// AdvertRecord — the `RecipientNotAdvertised` failure-cause case.
const OFFLINE_HANDLE: &str = "e2e-offline";
const OFFLINE_ADDRESS: &str = "QASXBML72DKIJEJ5GLMEBBX33KCKW3TSJW7ETFOTLEREQCDMW5BXCLXSQU";
const OFFLINE_PASSPHRASE: &str = "group avocado audit dentist baby index pipe attack enough stairs fame position column media copper athlete resource noodle forward wage middle into fitness ability dragon";

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

/// Install an OnMessage handler that echoes `Echo: <text>` back to the sender, mirroring
/// `bingle_cli run --echo` and the CLI localnet e2e's in-process peer.
fn install_echo_handler(api: &Arc<BingleApiImpl>) {
    let echo_api = api.clone();
    let handler: Arc<OnMessageHandler> = Arc::new(move |sender, sender_handle, message| {
        let text = message
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        tracing::info!("[echo peer] from {sender} ({sender_handle}): {text:?}; echoing back");
        let mut reply = serde_json::json!({ "text": format!("Echo: {}", text) });
        // Reflect the request's correlation tag as `responseTag` so a `send_*_with_response` waiter
        // on the sender completes (the engine matches responses by responseTag). Plain sends carry no
        // tag, so this is a no-op for them — the echo still arrives as an ordinary message (#139).
        if let Some(tag) = message.get("tag").and_then(|v| v.as_str()) {
            reply["responseTag"] = serde_json::Value::String(tag.to_string());
        }
        if let Err(e) = echo_api.send_message_to_id(&sender, reply, None) {
            tracing::warn!("[echo peer] echo send back to {sender} failed: {e:?}");
        }
    });
    api.access_unsafe_for_tests(|c: &mut BingleApiImpl| c.set_on_message(Some(handler)));
}

fn main() {
    test_util::init_test_logging_with_filter("info");

    let node_file = env_or("BINGLE_E2E_NODE_FILE", "/tmp/bingle_e2e_node.json");
    let stun_file = env_or("BINGLE_E2E_STUN_FILE", "/tmp/bingle_e2e_stun.txt");
    let env_file = env_or("BINGLE_E2E_ENV_FILE", "/tmp/bingle_e2e_localnet.env");

    // The env file doubles as the readiness signal: remove any stale copy so the orchestrating
    // script only proceeds once we (re)write it after everything is up.
    let _ = std::fs::remove_file(&env_file);

    if !provision::localnet_available() {
        eprintln!(
            "Error: algokit localnet is not reachable at localhost:4001.\n\
             Start it with `algokit localnet start` before running the localnet e2e."
        );
        std::process::exit(1);
    }

    let cfg = test_util::localnet_config();

    tracing::info!("[provision] funding accounts");
    setup_localnet::ensure_localnet_accounts_funded(
        &cfg,
        &[
            test_util::ADDRESS_SPEND,
            test_util::ADDRESS_RECEIVE,
            SENDER_ADDRESS,
            ECHO_ADDRESS,
            OFFLINE_ADDRESS,
        ],
    )
    .expect("fund localnet accounts");

    tracing::info!("[provision] deploying Bingle app + asset");
    let creator = test_util::ops_from_mnemonic(
        test_util::ADDRESS_SPEND,
        test_util::PASSPHRASE_SPEND,
        cfg.clone(),
    );
    let (app_id, asset_id) = test_util::deploy_bingle_app_and_asset(&creator, "BINGLE$", 1_000_000);
    tracing::info!("[provision] app_id={app_id} asset_id={asset_id}");

    // Emulator mode (#137): when BINGLE_E2E_EMULATOR_HOST is set (e.g. "10.0.2.2"), advertise the
    // relays + STUN at that host address — reachable from an Android emulator via its qemu gateway
    // (→ host loopback) and from the in-process host echo peer via a matching loopback alias — point
    // the app's node-file algod/indexer there, and force relay use (the app is behind the emulator
    // NAT, so it has no directly-reachable endpoint). Unset = loopback mode (iOS sim / host).
    let emulator_host: Option<IpAddr> = std::env::var("BINGLE_E2E_EMULATOR_HOST")
        .ok()
        .and_then(|s| s.trim().parse().ok());
    let advert_ip = emulator_host.unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));
    let broken_nat = emulator_host.is_some();
    if let Some(h) = emulator_host {
        tracing::info!("[provision] emulator mode: advertise relays/STUN at {h}, broken_nat=true");
    }

    // Two root relays. The engine always binds UDP to 0.0.0.0:<port> and uses the address here only
    // as the advertised endpoint, so advert_ip controls what peers dial.
    let r1_port = test_util::find_unused_loopback_port();
    let r2_port = test_util::find_unused_loopback_port();
    let relay1_addr = SocketAddr::new(advert_ip, r1_port);
    let relay2_addr = SocketAddr::new(advert_ip, r2_port);
    tracing::info!("[provision] registering relays at {relay1_addr} / {relay2_addr}");
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

    // STUN servers. Loopback mode uses working NAT so the client discovers a direct loopback
    // endpoint (matching the CLI localnet e2e); emulator mode uses broken NAT so the app (behind the
    // emulator NAT) falls back to a relay, and advertises STUN at the emulator-reachable host.
    let (s1, s2, stun_list) = provision::setup_stun_servers_advertised(broken_nat, advert_ip);

    // Pre-register the sender so the app's start() sees it already registered.
    tracing::info!("[provision] registering sender handle '{SENDER_HANDLE}'");
    test_util::register_client_on_blockchain(
        SENDER_ADDRESS,
        SENDER_PASSPHRASE,
        SENDER_HANDLE,
        app_id,
        asset_id,
        &creator,
        cfg.clone(),
    );

    // Register the offline fixture on chain but never start it: handle resolves, no AdvertRecord.
    tracing::info!("[provision] registering offline fixture handle '{OFFLINE_HANDLE}'");
    test_util::register_client_on_blockchain(
        OFFLINE_ADDRESS,
        OFFLINE_PASSPHRASE,
        OFFLINE_HANDLE,
        app_id,
        asset_id,
        &creator,
        cfg.clone(),
    );

    // Register + start the echo peer.
    tracing::info!("[provision] registering + starting echo peer '{ECHO_HANDLE}'");
    test_util::register_client_on_blockchain(
        ECHO_ADDRESS,
        ECHO_PASSPHRASE,
        ECHO_HANDLE,
        app_id,
        asset_id,
        &creator,
        cfg.clone(),
    );
    let echo = provision::start_client(
        ECHO_HANDLE,
        ECHO_PASSPHRASE,
        stun_list.clone(),
        app_id,
        cfg.clone(),
    );
    install_echo_handler(&echo);
    if !test_util::wait_for_registered(&echo, Duration::from_secs(180)) {
        eprintln!("Error: echo peer did not reach Registered state within 180s");
        std::process::exit(1);
    }

    // Wait for the sender + offline handles to be resolvable via the indexer, so the app's first
    // lookup does not race registration.
    if !relay_test_util::wait_for_handles_visible(
        cfg.clone(),
        app_id,
        &[SENDER_HANDLE, OFFLINE_HANDLE, ECHO_HANDLE],
        Duration::from_secs(60),
    ) {
        eprintln!("Error: e2e handles did not become visible via indexer within 60s");
        std::process::exit(1);
    }

    // Stage the node file + STUN file the app init()s with. In emulator mode, algod/indexer are
    // reached at the emulator's host alias (its own localhost is the emulator, not the host).
    let node_host = match emulator_host {
        Some(h) => format!("http://{h}"),
        None => "http://localhost".to_string(),
    };
    provision::write_localnet_node_file_host(
        std::path::Path::new(&node_file),
        app_id,
        asset_id,
        &node_host,
    );
    let stun_text = format!("{}\n{}\n", stun_list[0], stun_list[1]);
    std::fs::write(&stun_file, stun_text).expect("write stun file");

    // Write the env file last — it is the readiness signal the script waits on.
    let env_body = format!(
        "# Written by localnet_e2e_provisioner (issue #123). Source this to get BINGLE_E2E_* creds.\n\
         export BINGLE_E2E_APP_ID={app_id}\n\
         export BINGLE_E2E_ASSET_ID={asset_id}\n\
         export BINGLE_E2E_HANDLE={SENDER_HANDLE}\n\
         export BINGLE_E2E_PASSPHRASE='{SENDER_PASSPHRASE}'\n\
         export BINGLE_E2E_ECHO_TO={ECHO_HANDLE}\n\
         export BINGLE_E2E_OFFLINE_HANDLE={OFFLINE_HANDLE}\n"
    );
    std::fs::write(&env_file, env_body).expect("write env file");

    tracing::info!(
        "[provision] ready: app_id={app_id} asset_id={asset_id} sender='{SENDER_HANDLE}' \
         echo='{ECHO_HANDLE}' offline='{OFFLINE_HANDLE}'"
    );
    println!("PROVISIONER READY (env file: {env_file}). Ctrl-C / SIGTERM to stop.");

    // Park until killed. The relays/echo peer/STUN servers are threads/sockets in this process and
    // are released when it exits, so an abrupt SIGTERM from the orchestrating script is a clean
    // enough teardown for an ephemeral localnet. These bindings keep the handles alive meanwhile.
    let _keep_alive = (relay1, relay2, echo, s1, s2);
    loop {
        std::thread::sleep(Duration::from_secs(3600));
    }
}
