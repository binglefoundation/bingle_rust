//! Store-and-forward end-to-end against a real Sidewinder node on LocalNet (epic #200, story #216).
//!
//! This target is gated behind the `sidewinder-e2e` cargo feature and is **not** part of any normal
//! suite or CI. It stands up a one-node Sidewinder Mailbox network on algokit LocalNet via
//! `scripts/e2e-bingle-localnet.sh` (which drives the installed `sw-node` binary), then drives the
//! store-and-forward path against it. Per the LocalNet-suite policy it **fails, not skips**, when the
//! prerequisites are missing.
//!
//! Prerequisites (see the script header):
//!   - `sw-node` on PATH (`cargo install --path sw-node` in the sidewinder repo),
//!   - algokit LocalNet up (`algokit localnet start`), and `curl`.
//!
//! Run it explicitly:
//!   `cargo test -p bingle_cli --features sidewinder-e2e --test store_and_forward_e2e -- --nocapture`
//!
//! Stage A (this file) proves the harness + node: a message posted to a recipient's Mailbox is popped
//! back through the [`Mailbox`] client. Stage B (the full two-`bingle_local` offline-send → reconnect
//! flow with handle registration and the epic invariants) builds on this same harness.

use algo_ops::AlgoOps;
use bingle_local::api::sidewinder::{Mailbox, MailboxConfig};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Access details printed by `e2e-bingle-localnet.sh up`.
struct Endpoints {
    api_url: String,
    token: String,
    sender_mnemonic: String,
    receiver_mnemonic: String,
    receiver_address: String,
}

/// A running one-node Sidewinder Mailbox network, torn down (`script down`) on drop.
struct Cluster {
    script: PathBuf,
    work: PathBuf,
    endpoints: Endpoints,
}

impl Cluster {
    /// Stand up the network via the script, failing (not skipping) with the script's diagnostics when
    /// a prerequisite is missing.
    fn up() -> Cluster {
        let script =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../scripts/e2e-bingle-localnet.sh");
        assert!(
            script.exists(),
            "harness script not found at {} — it is vendored from the sidewinder repo",
            script.display()
        );
        let work = Path::new(env!("CARGO_TARGET_TMPDIR")).join("sw-e2e");

        let output = Command::new("sh")
            .arg(&script)
            .arg("up")
            .env("WORK", &work)
            .output()
            .expect("run e2e-bingle-localnet.sh up");
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success(),
            "e2e-bingle-localnet.sh up failed (is sw-node installed and algokit localnet running?).\n\
             --- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
        );
        let endpoints = parse_endpoints(&stdout)
            .unwrap_or_else(|| panic!("could not parse endpoints from script output:\n{stdout}"));
        Cluster {
            script,
            work,
            endpoints,
        }
    }
}

impl Drop for Cluster {
    fn drop(&mut self) {
        let _ = Command::new("sh")
            .arg(&self.script)
            .arg("down")
            .env("WORK", &self.work)
            .output();
    }
}

/// Parse the `key : value` block the script prints (endpoint, token, and the sender/receiver address
/// + mnemonic under their `[sender]` / `[receiver]` sections).
fn parse_endpoints(stdout: &str) -> Option<Endpoints> {
    let mut api_url = None;
    let mut token = None;
    let mut sender_mnemonic = None;
    let mut receiver_mnemonic = None;
    let mut receiver_address = None;
    let mut section = "";

    for line in stdout.lines() {
        let trimmed = line.trim();
        match trimmed {
            "[sender]" => section = "sender",
            "[receiver]" => section = "receiver",
            _ => {}
        }
        if let Some(value) = after_key(trimmed, "API endpoint") {
            api_url = Some(value);
        } else if let Some(value) = after_key(trimmed, "Bearer token") {
            token = Some(value);
        } else if let Some(value) = after_key(trimmed, "mnemonic") {
            match section {
                "sender" => sender_mnemonic = Some(value),
                "receiver" => receiver_mnemonic = Some(value),
                _ => {}
            }
        } else if let Some(value) = after_key(trimmed, "address") {
            if section == "receiver" {
                receiver_address = Some(value);
            }
        }
    }

    Some(Endpoints {
        api_url: api_url?,
        token: token?,
        sender_mnemonic: sender_mnemonic?,
        receiver_mnemonic: receiver_mnemonic?,
        receiver_address: receiver_address?,
    })
}

/// If `line` is `<key> ... : <value>`, return the trimmed value. Tolerates the variable spacing the
/// script uses before the colon (`address  :`, `mnemonic :`).
fn after_key(line: &str, key: &str) -> Option<String> {
    let rest = line.strip_prefix(key)?;
    let value = rest.trim_start().strip_prefix(':')?.trim();
    (!value.is_empty()).then(|| value.to_string())
}

/// Whether `popped` carries `message`, tolerating an ARC-4 `byte[]` length prefix around the payload
/// (the wire may length-prefix the declared `byte[]` return; the FIFO primitive stores it raw).
fn payload_matches(popped: &[u8], message: &[u8]) -> bool {
    if popped == message {
        return true;
    }
    if popped.len() == message.len() + 2 {
        let len = u16::from_be_bytes([popped[0], popped[1]]) as usize;
        return len == message.len() && &popped[2..] == message;
    }
    false
}

/// Stage A: a message posted to the receiver's Mailbox is popped back — the harness stands up a real
/// node and the [`Mailbox`] client round-trips a message through it.
#[test]
fn mailbox_round_trips_against_a_real_localnet_node() {
    let cluster = Cluster::up();
    let e = &cluster.endpoints;

    // Sender posts to the receiver's Mailbox (keyed by the receiver's address).
    let sender = Mailbox::new(
        AlgoOps::new_for_algorand(Some(e.sender_mnemonic.clone()), None, None),
        MailboxConfig::new(e.api_url.clone(), e.token.clone()),
    )
    .expect("sender mailbox");
    let message = b"store-and-forward end-to-end #216 (stage A)".to_vec();
    sender
        .post(&e.receiver_address, &message)
        .expect("post to the receiver's mailbox");

    // Receiver pops its own Mailbox and reads the message back.
    let receiver = Mailbox::new(
        AlgoOps::new_for_algorand(Some(e.receiver_mnemonic.clone()), None, None),
        MailboxConfig::new(e.api_url.clone(), e.token.clone()),
    )
    .expect("receiver mailbox");
    let popped = receiver
        .pop()
        .expect("pop succeeds")
        .expect("a message is queued for the receiver");
    assert!(
        payload_matches(&popped, &message),
        "the receiver reads the posted message back: got {popped:?}"
    );

    // Read-and-drop: a second pop finds nothing.
    let drained = receiver.pop().expect("second pop succeeds");
    assert!(
        drained.is_none(),
        "the message is dropped from the sidechain and not re-read"
    );
}
