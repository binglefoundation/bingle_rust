use rust_comms::algo_ops::{AlgoChainConfig, AlgoOps, AppArg, address_to_byte_key};
use rust_comms::api::bingle_api::{BingleApi, BingleApiInternal, StartOptions};
use rust_comms::api::bingle_api_impl::BingleApiImpl;
use rust_comms::blockchain::algo_bingle::{
    ACCOUNT_APP_ADMIN, ACCOUNT_APP_WITHDRAWER, ACCOUNT_ASSET_CREATOR, ACCOUNT_ASSET_FREEZE,
    ACCOUNT_ASSET_MANAGER, ACCOUNT_ASSET_RESERVE, AlgoBingle,
};
use rust_comms::engine::{BingleAccessUnsafeForTests, EngineState};
use rust_comms::util::logging::{BingleFormatter, HandleLayer, LogMode};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::net::{SocketAddr, TcpStream};
use std::sync::{Arc, Once};
use std::time::{Duration, Instant};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::prelude::*;

// Localnet token from Algorand docs / Algokit localnet
#[allow(dead_code)]
pub const LOCALNET_TOKEN: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

// Provided accounts and mnemonics (mnemonics are used here via algonaut to derive the seed)
#[allow(dead_code)]
pub const PASSPHRASE_10MIL: &str = "provide protect forest couch shaft buyer tenant language almost response chief roast spider scorpion injury they good ecology super east domain thunder shrimp absent output";
#[allow(dead_code)]
pub const ADDRESS_10MIL: &str = "P577PSTDICQ6PQFBR5YMDMJ2YVK7LT5V4GOPNVDLCEDJIL7XGRWC5BRFWA";
#[allow(dead_code)]
pub const ADDRESS_SPEND: &str = "4TKGNGRAUHMQI4EOQ34L2AIDX2VGS4OZNZIOE6BLEQFZUDRSB6RJRBPVRE";
#[allow(dead_code)]
pub const PASSPHRASE_SPEND: &str = "theme term glow reflect essence artefact tired bicycle february demand vacuum tent faculty arch elevator rent already anchor rough cry sketch nurse mom able inquiry";

#[allow(dead_code)]
pub const ADDRESS_RECEIVE: &str = "OO3BIFZDJPGMNXZ74NOVH5KZ5WBL3KCPLPELAF32P7HDCQGQIBID7PJC7A";
#[allow(dead_code)]
pub const PASSPHRASE_RECEIVE: &str = "earth idle country misery matrix wolf tired cabin craft roof quantum comfort answer praise second scout title napkin crop trial industry glue kid absorb midnight";

// Granular dapp lifecycle accounts (shared with blockchain_users.rs in integration_blockchain)
#[allow(dead_code)]
pub const ADDRESS_APP_CREATOR: &str = "L4IOKR5LM7Q7UIYB5Y735HV3H4JPKWKHTONM5Z6WHLE6RQWHRGRUVPRGKE";
#[allow(dead_code)]
pub const PASSPHRASE_APP_CREATOR: &str = "prize local popular life bronze require amused beef opinion shock gaze utility state hunt raccoon inform junior express zebra find crash blame tide about palace";
#[allow(dead_code)]
pub const ADDRESS_APP_ADMIN: &str = "TA2XNGWKWXXSWNHVVK23PW6A5JVYGC3WL2IFAILU4MOMCRJHHD46PCAIL4";
#[allow(dead_code)]
pub const PASSPHRASE_APP_ADMIN: &str = "sunset fuel problem limit share same dilemma cool member real satoshi capable brush during body wool kiss parade smooth fan rude assume clever absorb across";
#[allow(dead_code)]
pub const ADDRESS_APP_WITHDRAWER: &str =
    "5FMPY3U5XCCDUOROVX34JYCRXHOZTPDSXDEZ576PXRHOTD4OSWNXXDEA74";
#[allow(dead_code)]
pub const PASSPHRASE_APP_WITHDRAWER: &str = "post all tuition hero axis erupt profit same dizzy stage like fly inquiry betray electric glue just space gentle jacket annual hello betray abstract way";
#[allow(dead_code)]
pub const ADDRESS_ASSET_CREATOR: &str =
    "TETZ5CZVNJRMKBY63RFJGJKH6JNLTXX6TS5EHYAZTBY7TX76VWW6UXMAG4";
#[allow(dead_code)]
pub const PASSPHRASE_ASSET_CREATOR: &str = "eyebrow bleak multiply material flush host panel column rubber maximum clean episode plate trim excess dignity barrel beyond minute rebuild cliff divert planet absent spray";
#[allow(dead_code)]
pub const ADDRESS_ASSET_RESERVE: &str =
    "ZKPYCKDPCF75XTMJPCTJY5OG32BQDIPJUFFBRGAFATCYUUWPSYCDLXCQKA";
#[allow(dead_code)]
pub const PASSPHRASE_ASSET_RESERVE: &str = "weasel open guide until scale stove pull keep truly push tongue anxiety throw acoustic hamster total rare door cost response promote grain adapt ability muffin";
#[allow(dead_code)]
pub const ADDRESS_ASSET_MANAGER: &str =
    "PPVIJ3JCZ34DUE3Q3CKTY2ZSKTJV5A32C35A62G7DX462WRPZBE45DOA5Q";
#[allow(dead_code)]
pub const PASSPHRASE_ASSET_MANAGER: &str = "narrow tuition slot toddler slim copper pool permit subject elegant favorite cigar legal nurse muscle jewel rifle broom canoe eagle hint uncover unfair about similar";
#[allow(dead_code)]
pub const ADDRESS_ASSET_FREEZE: &str = "JSR33VO7TGVWZAHULWH4QNBI4APJFEPUBA3563C5FBO3Q2PNCMS4UVASGM";
#[allow(dead_code)]
pub const PASSPHRASE_ASSET_FREEZE: &str = "loan warfare heart chat giraffe skirt radio interest tiger sentence episode cross concert dream under fuel avoid good border congress hope stadium permit about sunset";

#[allow(dead_code)]
pub fn localnet_config() -> AlgoChainConfig {
    AlgoChainConfig {
        client_api_url: "http://localhost".to_string(),
        client_api_port: 4001,
        indexer_api_url: "http://localhost".to_string(),
        indexer_api_port: 8980,
        token: Some(LOCALNET_TOKEN.to_string()),
        token_key: Some("X-Algo-API-Token".to_string()),
        app_id: None,
        asset_id: None,
    }
}

#[allow(dead_code)]
pub fn assert_localnet_available() {
    let cfg = localnet_config();
    let addr = format!(
        "{}:{}",
        cfg.client_api_url
            .trim_start_matches("http://")
            .trim_start_matches("https://"),
        cfg.client_api_port
    );
    TcpStream::connect(&addr).unwrap_or_else(|e| {
        panic!(
            "Localnet is not available at {} - ensure algokit localnet is running: {}",
            addr, e
        )
    });
}

#[allow(dead_code)]
pub fn ops_from_mnemonic(addr: &str, mnem: &str, cfg: AlgoChainConfig) -> AlgoOps {
    // Pass the mnemonic directly as the passphrase (ASCII string)
    let pass = mnem.to_string();
    AlgoOps::new(Some(pass), Some(addr.to_string()), Some(cfg))
}

#[allow(dead_code)]
pub fn make_standard_accounts(cfg: &AlgoChainConfig) -> HashMap<String, AlgoOps> {
    let mut accounts = HashMap::new();
    accounts.insert(
        ACCOUNT_APP_ADMIN.to_string(),
        ops_from_mnemonic(ADDRESS_APP_ADMIN, PASSPHRASE_APP_ADMIN, cfg.clone()),
    );
    accounts.insert(
        ACCOUNT_APP_WITHDRAWER.to_string(),
        ops_from_mnemonic(
            ADDRESS_APP_WITHDRAWER,
            PASSPHRASE_APP_WITHDRAWER,
            cfg.clone(),
        ),
    );
    accounts.insert(
        ACCOUNT_ASSET_CREATOR.to_string(),
        ops_from_mnemonic(ADDRESS_ASSET_CREATOR, PASSPHRASE_ASSET_CREATOR, cfg.clone()),
    );
    accounts.insert(
        ACCOUNT_ASSET_RESERVE.to_string(),
        ops_from_mnemonic(ADDRESS_ASSET_RESERVE, PASSPHRASE_ASSET_RESERVE, cfg.clone()),
    );
    accounts.insert(
        ACCOUNT_ASSET_MANAGER.to_string(),
        ops_from_mnemonic(ADDRESS_ASSET_MANAGER, PASSPHRASE_ASSET_MANAGER, cfg.clone()),
    );
    accounts.insert(
        ACCOUNT_ASSET_FREEZE.to_string(),
        ops_from_mnemonic(ADDRESS_ASSET_FREEZE, PASSPHRASE_ASSET_FREEZE, cfg.clone()),
    );
    accounts
}

// Shared helper for tests: allocate a free UDP port on loopback.
#[allow(dead_code)]
pub fn find_unused_loopback_port() -> u16 {
    use std::net::{IpAddr, Ipv4Addr, UdpSocket};
    let sock = UdpSocket::bind((IpAddr::V4(Ipv4Addr::LOCALHOST), 0)).expect("bind temp socket");
    let port = sock.local_addr().expect("local addr").port();
    drop(sock);
    port
}

#[allow(dead_code)]
pub fn maybe_unwrap_data_single(packet: &[u8]) -> &[u8] {
    if packet.len() >= 4 {
        let version = packet[0] >> 4;
        let packet_type = packet[0] & 0x0F;
        if version == 1 && packet_type == 1 {
            return &packet[4..];
        }
    }

    packet
}

// Helper: print current working directory for debugging path issues in tests
#[allow(dead_code)]
pub fn print_cwd_for_debug() {
    match std::env::current_dir() {
        Ok(cwd) => eprintln!("Current working directory: {}", cwd.display()),
        Err(e) => eprintln!("Failed to get current working directory: {}", e),
    }
}

#[allow(dead_code)]
pub fn init_test_logging() {
    init_test_logging_with_filter("debug");
}

#[allow(dead_code)]
pub fn init_test_logging_with_filter(filter_str: &str) {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let mut final_filter = filter_str.to_string();
        if !rust_comms::util::logging::is_algo_debug_enabled() {
            // Suppress noisy external Algorand connection logs
            final_filter.push_str(",hyper=info,reqwest=info,rustls=info,h2=info");
        }

        let filter =
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(final_filter));

        let log_mode = if let Ok(val) = env::var("BINGLE_LOG_MODE") {
            match val.to_ascii_lowercase().as_str() {
                "plain" => LogMode::Plain,
                "ansi" => LogMode::ANSI,
                "aws" => LogMode::AWS,
                "js" => LogMode::JS,
                _ => LogMode::Plain,
            }
        } else {
            LogMode::Plain
        };

        let fmt_layer = tracing_subscriber::fmt::layer()
            .with_test_writer()
            .event_format(BingleFormatter { mode: log_mode });

        let subscriber = tracing_subscriber::registry()
            .with(filter)
            .with(HandleLayer)
            .with(fmt_layer);

        let _ = tracing::subscriber::set_global_default(subscriber);

        // Panic hook that logs at error! and then defers to default behavior
        let default_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |pi| {
            tracing::error!("PANIC: {}", pi);
            default_hook(pi);
        }));
    });
}

/// Deploy the BingleDapp smart contract to localnet from pre-compiled TEAL artifacts.
///
/// Caller: the `ops` account acts as the application **creator** for all transactions.
///
/// Blockchain steps:
///
/// 1. **Compile TEAL** — POSTs `BingleDapp.approval.teal` and `BingleDapp.clear.teal` to the
///    algod `/v2/teal/compile` endpoint, returning AVM bytecode for each program.
///
/// 2. **ApplicationCreate** — submits a `CreateApplication` transaction signed by the creator.
///    Declares the following state schema:
///    - Global: 2 ints (`BinglePrice`, `LastHandleTime`)
///    - Local:  3 ints (`handle_time`, `allow_static`, `allow_relay`),
///              3 byteslices (`Handle`, `static_endpoint`, `static_endpoint_x`)
///    Returns the assigned `app_id`.
///
/// 3. **`set_bingle_price(uint64)void`** — ApplicationCall (NoOp) by the **creator**.
///    Dapp: asserts `Txn.sender == Global.creator_address`, writes `price` to global state
///    key `"BinglePrice"`. Called here with `price = 1` microAlgo so registration and buy
///    flows work out of the box in tests without a separate price setup step.
///
/// Returns the `app_id` of the deployed contract.
#[allow(dead_code)]
pub fn deploy_bingle_app(ops: &AlgoOps) -> u64 {
    let approval_path =
        "dapp/projects/dapp/smart_contracts/artifacts/bingle_dapp/BingleDapp.approval.teal";
    let clear_path =
        "dapp/projects/dapp/smart_contracts/artifacts/bingle_dapp/BingleDapp.clear.teal";
    let arc56_path =
        "dapp/projects/dapp/smart_contracts/artifacts/bingle_dapp/BingleDapp.arc56.json";

    let approval_src =
        fs::read_to_string(approval_path).expect("read approval teal from artifacts");
    let clear_src = fs::read_to_string(clear_path).expect("read clear teal from artifacts");
    let arc56_json = fs::read_to_string(arc56_path).expect("read arc56 app spec from artifacts");

    let approval_bytes = ops
        .compile_teal(&approval_src)
        .expect("compile approval teal");
    let clear_bytes = ops.compile_teal(&clear_src).expect("compile clear teal");

    // Set creator as initial admin and withdrawer via create(address,address)void.
    let creator_addr = ops.address_str().expect("creator address");
    let creator_pk = address_to_byte_key(&creator_addr).expect("creator pk");
    let app_id = ops
        .deploy_app(
            &approval_bytes,
            &clear_bytes,
            None,
            Some("create(address,address)void"),
            &[
                AppArg::Bytes(creator_pk.to_vec()),
                AppArg::Bytes(creator_pk.to_vec()),
            ],
            "opt_in_to_bingle(uint64)void",
            &arc56_json,
        )
        .expect("deploy app call")
        .expect("failed to get app_id after deployment");

    // Default: set Bingle price to 1 microAlgo; works because creator == initial admin.
    let _ = ops
        .call_app(
            app_id,
            None,
            Some("set_bingle_price(uint64)void"),
            &[AppArg::Uint(1)],
        )
        .expect("set_bingle_price(1) call");

    app_id
}

/// Deploy the BingleDapp smart contract and create the corresponding Bingle$ ASA using the
/// canonical set of granular role accounts (APP_CREATOR, APP_ADMIN, APP_WITHDRAWER,
/// ASSET_CREATOR, ASSET_RESERVE). The `ops` parameter provides the chain configuration only;
/// the account performing each role is determined by the standard account constants.
///
/// Returns `(app_id, asset_id)`.
#[allow(dead_code)]
pub fn deploy_bingle_app_and_asset(
    ops: &AlgoOps,
    asset_name: &str,
    total_units: u64,
) -> (u64, u64) {
    let cfg = ops.config.clone();
    crate::setup_localnet::ensure_localnet_accounts_funded(
        &cfg,
        &[
            ADDRESS_APP_CREATOR,
            ADDRESS_APP_ADMIN,
            ADDRESS_APP_WITHDRAWER,
            ADDRESS_ASSET_CREATOR,
            ADDRESS_ASSET_RESERVE,
            ADDRESS_ASSET_MANAGER,
            ADDRESS_ASSET_FREEZE,
        ],
    )
    .expect("ensure standard accounts funded");
    let creator_ops = ops_from_mnemonic(ADDRESS_APP_CREATOR, PASSPHRASE_APP_CREATOR, cfg.clone());
    let accounts = make_standard_accounts(&cfg);
    let teal_dir = std::path::Path::new("dapp/projects/dapp/smart_contracts/artifacts/bingle_dapp");
    let ab = AlgoBingle::new(creator_ops, 0, 0);
    ab.deploy_app_and_asset(
        teal_dir,
        true,
        true,
        None,
        None,
        asset_name,
        total_units,
        10,
        &accounts,
    )
    .expect("deploy_bingle_app_and_asset: deploy failed")
}

#[allow(dead_code)]
pub fn register_client_on_blockchain(
    address: &str,
    passphrase: &str,
    handle: &str,
    app_id: u64,
    asset_id: u64,
    _creator: &AlgoOps,
    cfg: AlgoChainConfig,
) {
    let ops = ops_from_mnemonic(address, passphrase, cfg);
    let ab = AlgoBingle::new(ops.clone(), app_id, asset_id);
    // Buy 1 unit from the app to cover the registration fee; this also opts the client into the ASA
    ab.buy_bingle(app_id, asset_id, 1)
        .unwrap_or_else(|e| panic!("buy Bingle$ for {}: {}", handle, e));
    ab.register(app_id, asset_id, handle, 1)
        .unwrap_or_else(|e| panic!("register handle for {}: {}", handle, e));

    // Wait until local state for the client reflects the Handle key to avoid race conditions
    let start = Instant::now();
    let timeout = Duration::from_secs(30);
    let mut ok = false;
    while start.elapsed() < timeout {
        if let Ok(Some(entries)) = ops.local_state_for_account(app_id, address) {
            if entries.iter().any(|(k, v)| k == "Handle" && v == handle) {
                ok = true;
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    assert!(
        ok,
        "{} Handle not visible in local state within timeout",
        handle
    );
}

#[allow(dead_code)]
pub fn wait_for_registered(api: &Arc<BingleApiImpl>, timeout: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if let Some(st) =
            api.access_unsafe_for_tests(|a: &mut BingleApiImpl| a.engine_state_for_tests())
        {
            if st == EngineState::Registered {
                return true;
            }
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    false
}

#[allow(dead_code)]
pub fn wait_for_relay_available(api: &Arc<BingleApiImpl>, timeout: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        let st = api.get_relay_state();
        if st == "available" {
            return true;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    false
}

#[allow(dead_code)]
pub fn get_compact_advert_record(
    ops: &AlgoOps,
    addr: std::net::SocketAddr,
    am_relay: bool,
) -> String {
    use chrono::Utc;
    use ed25519_dalek::SigningKey;
    use rust_comms::ddb::{AdvertRecord, InetSocketAddress};

    let sk_bytes = ops.private_key_bytes().expect("private key bytes");
    let sk_arr: [u8; 32] = sk_bytes.try_into().expect("32 bytes sk");
    let signing_key = SigningKey::from_bytes(&sk_arr);

    let record = AdvertRecord::new(
        ops.address.as_ref().expect("ops has address").clone(),
        Some(InetSocketAddress::from(addr)),
        Some(am_relay),
        None,
        None,
        Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        &signing_key,
    );
    record.serialize_csv()
}

#[allow(dead_code)]
pub fn get_signed_advert_record(
    id: &str,
    passphrase: &str,
    addr: std::net::SocketAddr,
    am_relay: bool,
) -> rust_comms::ddb::AdvertRecord {
    use chrono::Utc;
    use ed25519_dalek::SigningKey;
    use rust_comms::ddb::{AdvertRecord, InetSocketAddress};

    // Use a simple seed derivation if passphrase is not 32 bytes
    let mut seed = [0u8; 32];
    let bytes = passphrase.as_bytes();
    let len = bytes.len().min(32);
    seed[..len].copy_from_slice(&bytes[..len]);
    let signing_key = SigningKey::from_bytes(&seed);

    AdvertRecord::new(
        id.to_string(),
        Some(InetSocketAddress::from(addr)),
        Some(am_relay),
        None,
        None,
        Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        &signing_key,
    )
}

#[allow(dead_code)]
pub fn signed_root_relay(
    id: &str,
    addr: std::net::SocketAddr,
) -> rust_comms::relay::relay_finder::RelayInfo {
    use rust_comms::relay::relay_finder::RelayInfo;
    RelayInfo::root(get_signed_advert_record(id, "test_passphrase", addr, true))
}

#[allow(dead_code)]
pub fn signed_non_root_relay(
    id: &str,
    addr: std::net::SocketAddr,
) -> rust_comms::relay::relay_finder::RelayInfo {
    use rust_comms::relay::relay_finder::RelayInfo;
    RelayInfo::non_root(get_signed_advert_record(id, "test_passphrase", addr, false))
}

#[allow(dead_code)]
pub fn signed_root_relay_with(
    id: &str,
    addr: std::net::SocketAddr,
    state: Option<rust_comms::engine::RelayState>,
    ttl: Option<u64>,
) -> rust_comms::relay::relay_finder::RelayInfo {
    let mut r = signed_root_relay(id, addr);
    r.state = state;
    r.ttl = ttl;
    r
}

#[allow(dead_code)]
pub fn signed_non_root_relay_with(
    id: &str,
    addr: std::net::SocketAddr,
    state: Option<rust_comms::engine::RelayState>,
    ttl: Option<u64>,
) -> rust_comms::relay::relay_finder::RelayInfo {
    let mut r = signed_non_root_relay(id, addr);
    r.state = state;
    r.ttl = ttl;
    r
}

// Helper: start a relay node at a fixed address
pub fn start_root_relay(
    name: &str,
    addr: SocketAddr,
    passphrase: &str,
    app_id: u64,
    cfg: rust_comms::blockchain::algo_ops::AlgoChainConfig,
) -> Arc<BingleApiImpl> {
    tracing::info!(
        "[Test] start_root_relay name={} addr={} app_id={}",
        name,
        addr,
        app_id
    );
    let opts = StartOptions {
        handle: name.into(),
        algo_passphrase: Some(passphrase.parse().unwrap()),
        static_ip: Some(addr),
        am_relay: true,
        stun_servers: None,
        algo_provider_config: Some(cfg),
        algo_network: None,
        app_id: Some(app_id),
        asset_id: None,
        log_level: None,
        handle_cache_expiry: None,
        dangerous_debug: false,
        log_mode: rust_comms::util::logging::LogMode::Plain,
        wait_response_timeout: None,
    };
    let api = BingleApiImpl::new(&opts);
    api.access_unsafe_for_tests(|a: &mut BingleApiImpl| a.start(&opts))
        .expect("relay start");
    tracing::info!("[Test] root relay {} started, wait for registered", name);

    wait_for_registered(&api, Duration::from_secs(30));
    tracing::info!("[Test] root relay {} registered", name);

    if !wait_for_relay_available(&api, Duration::from_secs(360)) {
        panic!("root relay {} did not become Available within 360s", name);
    }
    tracing::info!("[Test] root relay {} Available", name);

    api
}
