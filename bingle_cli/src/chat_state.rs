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

use bingle_core::api::bingle_api::{BingleError, StartOptions};
use bingle_core::blockchain::algo_bingle::AlgoBingle;
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
        let cfg = LocalApiConfig::with_notify(
            algo_config,
            opts.app_id.unwrap_or(0),
            opts.asset_id.unwrap_or(0),
            None,
            None,
        );
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

    /// Add a contact to both the persistent store and the in-memory recipient map. The caller is
    /// responsible for calling [`save_state`](ChatState::save_state) to persist it.
    pub fn add_contact(&mut self, handle: &str, id: &str) -> Result<(), String> {
        self.local
            .add_contact(handle.to_string(), id.to_string(), ContactSource::Manual)
            .map_err(|e| e.to_string())?;
        self.contacts.insert(handle.to_string(), id.to_string());
        Ok(())
    }

    /// Append a message to the persistent history. The caller persists via
    /// [`save_state`](ChatState::save_state).
    pub fn record_message(
        &mut self,
        sender_handle: &str,
        recipient_handles: Vec<String>,
        timestamp: i64,
        text: &str,
        cipher_suite: Option<String>,
    ) -> Result<(), String> {
        self.local
            .add_message(
                sender_handle.to_string(),
                recipient_handles,
                timestamp,
                text.to_string(),
                cipher_suite,
            )
            .map_err(|e| e.to_string())
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
    /// `keypair_status()` reports `ACTIVE` without inspecting the balance, so `chat` checks it here:
    /// the operating minimum is the live registration cost from
    /// [`AlgoBingle::required_funding`](bingle_core::blockchain::algo_bingle::AlgoBingle::required_funding),
    /// falling back to [`REQUIRED_ALGO`] when the chain read fails or no app is configured.
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
            bingle.required_funding().unwrap_or(REQUIRED_ALGO)
        } else {
            REQUIRED_ALGO
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
}

/// Load a BingleLocal state file into `local`, mapping any load error to a clear, user-facing
/// string. Missing and malformed files both surface here as errors (no panic).
fn load_state(local: &mut BingleApiLocalImpl, path: &str) -> Result<(), String> {
    local
        .load(path)
        .map_err(|e| format!("failed to load chat state from {}: {}", path, e))
}
