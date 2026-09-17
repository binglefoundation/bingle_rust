//! Bridge between a BingleLocal `--state_file` and the `chat` command's engine configuration.
//!
//! `bingle_cli` drives `bingle_core::BingleApiImpl` from a [`StartOptions`], while `bingle_local`
//! (`BingleApiLocalImpl`) owns the persisted keypair, contacts and message history. This module
//! loads that state file, surfaces the stored keypair/handle into `StartOptions` (so the engine can
//! start without `--passphrase`/`--handle` once the account is registered), seeds an in-memory
//! contact map for `--to <handle>` resolution, and writes state back on change via
//! [`ChatState::save_state`].
//!
//! Later subtasks of the chat epic (#56) drive the transport and interactive I/O; this subtask is
//! the storage bridge only.

use std::collections::HashMap;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use bingle_core::api::bingle_api::{BingleError, SendFailureKind, StartOptions};
use bingle_core::blockchain::algo_bingle::AlgoBingle;
use bingle_local::api::MailboxConfig;
use bingle_local::api::bingle_local_api::{BingleLocalApi, ContactSource, Message, REQUIRED_ALGO};
use bingle_local::api::bingle_local_api_impl::{BingleApiLocalImpl, LocalApiConfig};

use crate::chat::ChatArgs;
use crate::chat_register::AccountStatus;

/// Outcome of an on-chain registration attempt, distinguishing the "handle already taken by another
/// account" case (which `run_registration` reports before spending anything) so `cmd_chat` can give
/// it a dedicated message.
#[derive(Debug)]
pub enum RegisterError {
    /// The chosen handle is already registered to another account (payload: the owning address).
    HandleTaken(String),
    /// Any other registration/persistence failure.
    Other(String),
}

/// The chat session's persisted state, bridged from a BingleLocal state file.
///
/// Holds the concrete [`BingleApiLocalImpl`] so mutations (new contacts, message history) can be
/// written back to the same file via [`save_state`](ChatState::save_state).
pub struct ChatState {
    /// Owned local store: keypair, contacts and messages loaded from (and saved back to) the file.
    local: BingleApiLocalImpl,
    /// Path to persist to on [`save_state`](ChatState::save_state); `None` when no `--state_file`
    /// was given (state is in-memory only for this run).
    state_file: Option<String>,
    /// Engine start options derived from the file (keypair passphrase and registered handle) with
    /// CLI-provided `--handle`/`--passphrase` taking precedence.
    pub opts: StartOptions,
    /// `handle -> id` map seeded from the file's contacts, so a known `--to <handle>` resolves
    /// without a chain round-trip.
    pub contacts: HashMap<String, String>,
}

impl ChatState {
    /// Build the chat state from parsed [`ChatArgs`].
    ///
    /// When `--state_file` names an existing file it is loaded and its keypair/handle/contacts are
    /// surfaced into [`opts`](ChatState::opts) and [`contacts`](ChatState::contacts). CLI-provided
    /// `--passphrase`/`--handle` win over the stored values. A malformed file is a hard error; a
    /// path that does not exist yet is treated as an empty first-run store. When no handle can be
    /// resolved (neither on the command line nor in the file) [`opts.handle`](ChatState::opts) is
    /// left empty — that is a not-yet-registered account, which the `cmd_chat` first-run flow
    /// resolves via [`resolve_account_status`](ChatState::resolve_account_status); it is not an error
    /// here.
    ///
    /// Never logs the passphrase.
    pub fn from_chat_args(args: &ChatArgs) -> Result<ChatState, String> {
        let mut opts = args.opts.clone();

        // Configure the local store with whatever chain ids the CLI/node-file resolved; 0 means
        // "unset" for BingleLocal, matching how the webserver builds its config.
        let algo_config = opts.algo_provider_config.clone().unwrap_or_default();

        // Store-and-forward (epic #200, issues #241/#244): configure the Sidewinder Mailbox.
        // `SIDEWINDER_NODE_URL` + `SIDEWINDER_TOKEN` select the bearer (plaintext) override; otherwise
        // the Bingle DApp app id drives on-chain discovery + identity-pinned mutual TLS (story #244).
        // Neither available leaves store-and-forward unconfigured.
        //
        // `--store-forward` (issue #241) selects which gates to enable; `--notify <url>` enables the
        // give-up nudge to the bingle_notify gateway.
        let (send_gate, receive_gate) = args.store_forward.gates();
        let (notify_on_giveup, notify_gateway_url) = match args.notify_url.as_ref() {
            Some(url) => (Some(true), Some(url.clone())),
            None => (None, None),
        };

        let cfg = LocalApiConfig::with_notify(
            algo_config,
            opts.app_id.unwrap_or(0),
            opts.asset_id.unwrap_or(0),
            notify_on_giveup,
            notify_gateway_url,
        )
        .with_sidewinder(MailboxConfig::select(
            std::env::var("SIDEWINDER_NODE_URL").ok(),
            std::env::var("SIDEWINDER_TOKEN").ok(),
            opts.app_id,
        ))
        .with_store_and_forward(Some(send_gate), Some(receive_gate));
        // Fail loudly if store-and-forward is gated on with no reachable Mailbox (issues #241/#244):
        // a Mailbox is configured via either the app id (discovery + mTLS) or SIDEWINDER_NODE_URL +
        // SIDEWINDER_TOKEN (bearer). Supersedes #241's env-var-only `validate_store_forward`.
        cfg.validate_store_and_forward()?;
        let mut local = BingleApiLocalImpl::new(cfg);

        let state_file = args.state_file.clone();
        let mut contacts: HashMap<String, String> = HashMap::new();

        if let Some(path) = state_file.as_deref() {
            if Path::new(path).exists() {
                load_state(&mut local, path)?;

                // Surface the stored keypair. The CLI passphrase wins if one was supplied.
                match local.get_keypair().map_err(|e| e.to_string())? {
                    Some(keypair) => {
                        if opts.algo_passphrase.is_none() {
                            opts.algo_passphrase = Some(keypair.passphrase);
                        }
                        // Fill the handle from the account's registered handle only when the CLI did
                        // not provide one (parse_chat_args leaves it empty in that case).
                        if opts.handle.is_empty()
                            && let Some(handle) = local.own_handle()
                        {
                            opts.handle = handle;
                        }
                    }
                    None => {
                        tracing::info!(
                            "chat: state file {} has no keypair yet; account setup happens on first run",
                            path
                        );
                    }
                }

                // Seed the recipient map from stored contacts.
                for contact in local.get_contacts().map_err(|e| e.to_string())? {
                    contacts.insert(contact.handle, contact.id);
                }
            } else {
                tracing::info!(
                    "chat: state file {} not found; starting with empty local state",
                    path
                );
            }
        }

        // An empty handle here is not an error: it means the account is not yet registered on this
        // machine. The first-run registration flow in `cmd_chat` (issue #59) decides what to do —
        // register from a supplied passphrase/handle, or ask for credentials — based on the resolved
        // account status. Callers that need a definitely-registered handle go through that flow.

        Ok(ChatState {
            local,
            state_file,
            opts,
            contacts,
        })
    }

    /// Persist the current local state back to the `--state_file`. A no-op (returns `Ok`) when no
    /// state file was configured.
    pub fn save_state(&self) -> Result<(), String> {
        match self.state_file.as_deref() {
            Some(path) => self
                .local
                .save(path)
                .map_err(|e| format!("failed to save chat state to {}: {}", path, e)),
            None => Ok(()),
        }
    }

    /// Append a message to the persistent history and return the stored [`Message`] record. The
    /// caller persists to disk via [`save_state`](ChatState::save_state).
    pub fn record_message(
        &mut self,
        sender_handle: &str,
        recipient_handles: Vec<String>,
        timestamp: i64,
        text: &str,
        cipher_suite: Option<String>,
    ) -> Result<Message, String> {
        self.local
            .add_message(
                sender_handle.to_string(),
                recipient_handles,
                timestamp,
                text.to_string(),
                cipher_suite,
            )
            .map_err(|e| e.to_string())?;
        // Return the record as stored (add_message fills in progress/failure_reason), so callers
        // display exactly what was persisted rather than reconstructing it.
        self.local
            .get_messages()
            .map_err(|e| e.to_string())?
            .pop()
            .ok_or_else(|| "message missing after add_message".to_string())
    }

    /// Resolve a recipient handle to its id using the contact map seeded from the state file.
    pub fn resolve_recipient(&self, handle: &str) -> Option<&str> {
        self.contacts.get(handle).map(String::as_str)
    }

    /// Whether any known contact has this id. Used by the receive path to add a sender as a contact
    /// only when it is genuinely new (so an existing Manual contact is not downgraded to Received).
    pub fn knows_id(&self, id: &str) -> bool {
        self.contacts.values().any(|known| known == id)
    }

    /// Add a contact discovered by receiving a message from them ([`ContactSource::Received`]), to
    /// both the persistent store and the in-memory recipient map. The caller persists via
    /// [`save_state`](ChatState::save_state).
    pub fn add_received_contact(&mut self, handle: &str, id: &str) -> Result<(), String> {
        self.local
            .add_contact(handle.to_string(), id.to_string(), ContactSource::Received)
            .map_err(|e| e.to_string())?;
        self.contacts.insert(handle.to_string(), id.to_string());
        Ok(())
    }

    /// The stored message history, newest-appended last.
    pub fn messages(&self) -> Result<Vec<Message>, String> {
        self.local.get_messages().map_err(|e| e.to_string())
    }

    /// Persist an outbound message as **pending** (`progress = 0.0`) and return its timestamp, which
    /// keys later [`mark_delivered`](ChatState::mark_delivered) /
    /// [`mark_send_failed`](ChatState::mark_send_failed) updates. Persisting before the send attempt
    /// means a failed send survives in the state file for retry. Saves the state file.
    pub fn queue_outbound(&mut self, recipient_handle: &str, text: &str) -> Result<i64, String> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .map_err(|e| e.to_string())?;
        let sender = self.opts.handle.clone();
        // add_message records it delivered (progress 1.0); immediately mark it pending so the retry
        // path owns its lifecycle.
        self.local
            .add_message(
                sender,
                vec![recipient_handle.to_string()],
                timestamp,
                text.to_string(),
                None,
            )
            .map_err(|e| e.to_string())?;
        self.local
            .update_message_status(timestamp, 0.0, None, None)
            .map_err(|e| e.to_string())?;
        self.save_state()?;
        Ok(timestamp)
    }

    /// Mark a previously queued outbound message (by `timestamp`) delivered, and save.
    pub fn mark_delivered(&mut self, timestamp: i64) -> Result<(), String> {
        self.local
            .update_message_status(timestamp, 1.0, None, None)
            .map_err(|e| e.to_string())?;
        self.save_state()
    }

    /// Record a failed send attempt on a queued message. `permanent` distinguishes a give-up
    /// (`progress = 1.0` with the reason — terminal) from a transient failure (`progress = 0.0` —
    /// stays pending for retry). Saves the state file.
    pub fn mark_send_failed(
        &mut self,
        timestamp: i64,
        reason: &str,
        failure_kind: Option<SendFailureKind>,
        permanent: bool,
    ) -> Result<(), String> {
        let progress = if permanent { 1.0 } else { 0.0 };
        self.local
            .update_message_status(timestamp, progress, Some(reason.to_string()), failure_kind)
            .map_err(|e| e.to_string())?;
        self.save_state()
    }

    /// Outbound messages still awaiting delivery (`progress < 1.0`).
    pub fn pending_outbound(&self) -> Result<Vec<Message>, String> {
        self.local.get_pending_messages().map_err(|e| e.to_string())
    }

    /// Whether the store-and-forward SEND gate is on for this session (issue #272). When it is, a
    /// failed direct send is routed to the recipient's Sidewinder Mailbox (bingle_local's
    /// post-on-give-up, #214) rather than being kept only for direct retry.
    pub fn store_and_forward_send_enabled(&self) -> bool {
        self.local.store_and_forward_send()
    }

    /// Whether the message at `timestamp` has been handed off — delivered directly or posted to the
    /// recipient's Sidewinder Mailbox — i.e. it is complete (`progress == 1.0`) and carries no
    /// failure. The send path uses this after recording a failed direct send to tell a
    /// store-and-forward handoff (the forward succeeded) apart from a still-failing send (issue #272).
    pub fn is_handed_off(&self, timestamp: i64) -> bool {
        self.local
            .get_messages()
            .ok()
            .into_iter()
            .flatten()
            .find(|m| m.timestamp == timestamp)
            .map(|m| m.progress == Some(1.0) && m.failure_reason.is_none())
            .unwrap_or(false)
    }

    /// Whether the store-and-forward RECEIVE gate is on for this session (issue #274). When it is, the
    /// chat session polls this account's Sidewinder Mailbox (bingle_local read-on-reconnect, #215) on
    /// connect and on a backstop cycle, so messages held while offline are picked up.
    pub fn store_and_forward_receive_enabled(&self) -> bool {
        self.local.store_and_forward_receive()
    }

    /// Drain this account's Sidewinder Mailbox once, decrypting and storing each held message on the
    /// local history, and return the batch read this poll (sorted by sent time). A no-op returning an
    /// empty vector when the receive gate is off or no Sidewinder node is configured. Best-effort: a
    /// node/keypair problem is logged by bingle_local and surfaces here as an empty batch, never an
    /// error that would tear down the session. The caller persists via [`save_state`](Self::save_state).
    pub fn poll_mailbox(&self) -> Result<Vec<Message>, String> {
        self.local.poll_mailbox().map_err(|e| e.to_string())
    }

    /// Whether the local store currently holds a keypair.
    pub fn has_keypair(&self) -> bool {
        matches!(self.local.get_keypair(), Ok(Some(_)))
    }

    /// Import an account from its 25-word Algorand mnemonic, replacing any current keypair. Used by
    /// the first-run flow when the state file has no keypair. Never logs the passphrase.
    pub fn import_keypair(&mut self, passphrase: &str) -> Result<(), String> {
        self.local
            .import_keypair(passphrase.to_string())
            .map(|_keypair| ())
            .map_err(|e| e.to_string())
    }

    /// Resolve the account's startup status for the registration decision, reading balance/funding
    /// from chain for the `ACTIVE` case (which `keypair_status()` does not inspect).
    ///
    /// Maps the `bingle_local` status strings to [`AccountStatus`]. A `None` (no keypair) maps to
    /// [`AccountStatus::NoKeypair`]; `UPGRADE_REQUIRED` and any unrecognized/blockchain-unreachable
    /// status become an error string the caller surfaces and exits on.
    pub fn resolve_account_status(&self) -> Result<AccountStatus, String> {
        let status = self.local.keypair_status().map_err(|e| e.to_string())?;
        match status.status.as_str() {
            "None" => Ok(AccountStatus::NoKeypair),
            "UNFUNDED" => Ok(AccountStatus::Unfunded {
                id: status.id.unwrap_or_default(),
                // required_algo carries the shortfall/top-up; fall back to the flat target.
                shortfall_algos: status.required_algo.unwrap_or(REQUIRED_ALGO),
            }),
            "FUNDED" => Ok(AccountStatus::Funded {
                id: status.id.unwrap_or_default(),
            }),
            "ACTIVE" => {
                let handle = status
                    .handle
                    .or_else(|| self.local.own_handle())
                    .unwrap_or_default();
                let (balance_algos, operating_min_algos) = self.operating_funding()?;
                Ok(AccountStatus::Active {
                    id: status.id.unwrap_or_default(),
                    handle,
                    balance_algos,
                    operating_min_algos,
                })
            }
            "UPGRADE_REQUIRED" => Err(
                "this client is out of date for the configured app; please upgrade to continue"
                    .to_string(),
            ),
            other => Err(format!(
                "cannot determine account status ('{other}'); is the Algorand node reachable?"
            )),
        }
    }

    /// The current balance and the operating minimum (both in ALGOs) for the account.
    ///
    /// `keypair_status()` reports `ACTIVE` without inspecting the balance, so `chat` checks it here.
    /// The operating minimum is the account's **post-registration minimum balance** (MBR) — what a
    /// registered account must retain — not the one-time registration cost. Using
    /// [`required_funding`](bingle_core::blockchain::algo_bingle::AlgoBingle::required_funding) here
    /// (as an earlier version did) re-charged the Bingle$ price + fees already spent at registration,
    /// so a freshly registered account was wrongly flagged short by exactly what it just paid. If the
    /// MBR cannot be read (chain hiccup, or no app configured) we do not block: a registered account
    /// is guaranteed on-chain to hold at least its MBR, so `0.0` proceeds.
    fn operating_funding(&self) -> Result<(f64, f64), String> {
        let ops = self.local.get_algo_ops().map_err(|e| e.to_string())?;
        let balance_algos = ops
            .account_balance()
            .map_err(|e| e.to_string())?
            .unwrap_or(0.0);
        let app_id = self.opts.app_id.unwrap_or(0);
        let asset_id = self.opts.asset_id.unwrap_or(0);
        let operating_min_algos = if app_id != 0 {
            let bingle = AlgoBingle::new(ops, app_id, asset_id);
            bingle.post_registration_mbr().unwrap_or_else(|e| {
                tracing::warn!(
                    "chat: could not read account minimum balance ({e}); not blocking a registered account"
                );
                0.0
            })
        } else {
            0.0
        };
        Ok((balance_algos, operating_min_algos))
    }

    /// Register `handle` on-chain for the current keypair, then persist the (now ACTIVE) account to
    /// the state file. Assumes a keypair is present (import first if not). The handle-uniqueness
    /// pre-check in `run_registration` fails fast with [`RegisterError::HandleTaken`] before spending
    /// anything if the handle belongs to another account.
    pub fn register(&mut self, handle: &str) -> Result<(), RegisterError> {
        self.local
            .register_keypair(handle.to_string())
            .map(|_ok| ())
            .map_err(|e| match e {
                BingleError::HandleTaken(owner) => RegisterError::HandleTaken(owner),
                other => RegisterError::Other(other.to_string()),
            })?;
        // Persist the registered keypair + handle so later runs need no --passphrase/--handle.
        self.save_state().map_err(RegisterError::Other)
    }

    /// Test seam: build a `ChatState` directly from an already-configured local store and options,
    /// bypassing the `--state_file` bridge (and its `validate_store_and_forward`). Lets a test drive
    /// the send/forward path with an arbitrary Mailbox config and no state file (issue #272).
    #[doc(hidden)]
    pub fn from_parts_for_tests(local: BingleApiLocalImpl, opts: StartOptions) -> ChatState {
        ChatState {
            local,
            state_file: None,
            opts,
            contacts: HashMap::new(),
        }
    }

    /// Test seam: record a `(timestamp, handle)` as already posted to a Mailbox, so a test can drive
    /// the fully-forwarded handoff without a live Sidewinder node (delegates to the bingle_local
    /// seam of the same name). Issue #272.
    #[doc(hidden)]
    pub fn mark_forwarded_for_tests(&self, timestamp: i64, handle: &str) {
        self.local.mark_forwarded_for_tests(timestamp, handle);
    }
}

/// Load a BingleLocal state file into `local`, mapping any load error to a clear, user-facing
/// string. Missing and malformed files both surface here as errors (no panic).
fn load_state(local: &mut BingleApiLocalImpl, path: &str) -> Result<(), String> {
    local
        .load(path)
        .map_err(|e| format!("failed to load chat state from {}: {}", path, e))
}
