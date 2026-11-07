//! A one-node Sidewinder Mailbox on LocalNet, in `on-chain-identity` mode (#246).
//!
//! Drives `scripts/e2e_sidewinder_localnet.sh` and does the on-chain enrolment the script cannot (it
//! runs only `sw-node`): the node account is opted in to the Bingle app and given `allow_sw_node` by
//! the app admin, so the node can publish its endpoint at boot. Clients are enrolled by registering a
//! handle, which grants `allow_sw_client`. A client then reaches the node by app id only: it finds the
//! endpoint on-chain and connects over mutual Transport Layer Security (mTLS).
//!
//! The sequence is [`SidewinderScript::accounts`] → deploy the app and register the client handles →
//! [`enrol_node`] → [`SidewinderScript::up`] → [`connect_mailbox`].

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, Instant};

use algo_ops::{AlgoChainConfig, AlgoOps};
use bingle_core::api::bingle_api::BingleError;
use bingle_core::blockchain::algo_bingle::AlgoBingle;
use bingle_local::api::sidewinder::{Mailbox, MailboxConfig};

use super::test_util;

/// The repo's one-node LocalNet Sidewinder script.
pub fn script_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../scripts/e2e_sidewinder_localnet.sh")
}

/// One account the script generated: its address and 25-word mnemonic.
#[derive(Debug, Clone)]
pub struct SidewinderAccount {
    /// Algorand address.
    pub address: String,
    /// 25-word mnemonic.
    pub mnemonic: String,
}

/// The accounts the script generates and funds: the node's own account and two clients.
#[derive(Debug, Clone)]
pub struct SidewinderAccounts {
    /// The node account: its anchoring signer and its mutual-TLS identity.
    pub node: SidewinderAccount,
    /// A client account to post from.
    pub sender: SidewinderAccount,
    /// A client account to post to.
    pub receiver: SidewinderAccount,
}

/// Runs the script with a work directory and extra environment.
pub struct SidewinderScript {
    work: PathBuf,
    env: Vec<(String, String)>,
}

impl SidewinderScript {
    /// A script run against `work` (the node's config, data, account keys and log).
    pub fn new(work: impl Into<PathBuf>) -> Self {
        Self {
            work: work.into(),
            env: Vec::new(),
        }
    }

    /// Pass `key=value` to every run (e.g. `API_PORT`, `ADVERTISE_HOST`).
    pub fn env(mut self, key: &str, value: impl Into<String>) -> Self {
        self.env.push((key.to_string(), value.into()));
        self
    }

    /// The work directory.
    pub fn work(&self) -> &Path {
        &self.work
    }

    fn run(&self, action: &str, app_id: Option<u64>) -> Result<Output, String> {
        let mut command = Command::new("sh");
        command
            .arg(script_path())
            .arg(action)
            .env("WORK", &self.work);
        for (key, value) in &self.env {
            command.env(key, value);
        }
        if let Some(app_id) = app_id {
            command.env("BINGLE_APP_ID", app_id.to_string());
        }
        let output = command
            .output()
            .map_err(|e| format!("could not run {}: {e}", script_path().display()))?;
        if output.status.success() {
            Ok(output)
        } else {
            Err(format!(
                "e2e_sidewinder_localnet.sh {action} failed (is the pinned sw-node installed and \
                 algokit localnet running?)\n--- stdout ---\n{}\n--- stderr ---\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ))
        }
    }

    /// Generate (or reuse) and fund the node, sender and receiver accounts.
    pub fn accounts(&self) -> Result<SidewinderAccounts, String> {
        let output = self.run("accounts", None)?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        parse_accounts(&stdout).ok_or_else(|| {
            format!("could not parse the accounts from the script output:\n{stdout}")
        })
    }

    /// Start the node against `app_id`. The node account must already be enrolled on it
    /// ([`enrol_node`]), since the node publishes its endpoint once, at boot.
    pub fn up(&self, app_id: u64) -> Result<(), String> {
        self.run("up", Some(app_id)).map(|_| ())
    }

    /// Stop the node and remove the work directory. Best-effort.
    pub fn down(&self) {
        let _ = self.run("down", None);
    }
}

/// Parse the `[node]` / `[sender]` / `[receiver]` blocks the script prints, each with an
/// `address :` and a `mnemonic :` line.
pub fn parse_accounts(stdout: &str) -> Option<SidewinderAccounts> {
    let mut found: [(Option<String>, Option<String>); 3] = Default::default();
    let mut section = None;
    for line in stdout.lines() {
        let trimmed = line.trim();
        match trimmed {
            "[node]" => section = Some(0),
            "[sender]" => section = Some(1),
            "[receiver]" => section = Some(2),
            _ => {}
        }
        let Some(index) = section else { continue };
        if let Some(value) = after_key(trimmed, "address") {
            found[index].0 = Some(value);
        } else if let Some(value) = after_key(trimmed, "mnemonic") {
            found[index].1 = Some(value);
        }
    }
    let [node, sender, receiver] = found;
    let account = |(address, mnemonic): (Option<String>, Option<String>)| {
        Some(SidewinderAccount {
            address: address?,
            mnemonic: mnemonic?,
        })
    };
    Some(SidewinderAccounts {
        node: account(node)?,
        sender: account(sender)?,
        receiver: account(receiver)?,
    })
}

/// If `line` is `<key> ... : <value>`, return the trimmed value. Tolerates the variable spacing the
/// script uses before the colon.
fn after_key(line: &str, key: &str) -> Option<String> {
    let rest = line.strip_prefix(key)?;
    let value = rest.trim_start().strip_prefix(':')?.trim();
    (!value.is_empty()).then(|| value.to_string())
}

/// Enrol the node account on `app_id` (a Bingle app deployed by
/// [`deploy_bingle_app_and_asset`](test_util::deploy_bingle_app_and_asset), whose admin is the
/// standard `APP_ADMIN` account): opt it in, then grant it `allow_sw_node` so it may publish its
/// endpoint and clients accept it as a cluster node.
pub fn enrol_node(
    cfg: &AlgoChainConfig,
    app_id: u64,
    node: &SidewinderAccount,
) -> Result<(), String> {
    let node_ops = test_util::ops_from_mnemonic(&node.address, &node.mnemonic, cfg.clone());
    node_ops
        .opt_in_app(app_id)
        .map_err(|e| format!("node opt-in to app {app_id}: {e}"))?;
    let admin_ops = test_util::ops_from_mnemonic(
        test_util::ADDRESS_APP_ADMIN,
        test_util::PASSPHRASE_APP_ADMIN,
        cfg.clone(),
    );
    AlgoBingle::new(admin_ops, app_id, 0)
        .set_allow_sw_node(app_id, &node.address, true)
        .map_err(|e| format!("set_allow_sw_node for the node on app {app_id}: {e}"))?;
    Ok(())
}

/// Connect a [`Mailbox`] for `algo` to the node of `app_id`, retrying until discovery sees the
/// node's published endpoint (the indexer lags the chain) and the mutual-TLS connect succeeds, or
/// `timeout` passes. Returns the last error on timeout.
pub fn connect_mailbox(
    algo: AlgoOps,
    app_id: u64,
    timeout: Duration,
) -> Result<Mailbox, BingleError> {
    let deadline = Instant::now() + timeout;
    loop {
        match Mailbox::new(algo.clone(), MailboxConfig::discovered(app_id)) {
            Ok(mailbox) => return Ok(mailbox),
            Err(e) if Instant::now() < deadline => {
                tracing::debug!("[sidewinder] mailbox not reachable yet: {e}");
                std::thread::sleep(Duration::from_millis(500));
            }
            Err(e) => return Err(e),
        }
    }
}
