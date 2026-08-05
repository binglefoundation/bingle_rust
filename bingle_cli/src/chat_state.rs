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

use bingle_core::api::bingle_api::StartOptions;
use bingle_local::api::bingle_local_api::{BingleLocalApi, ContactSource};
use bingle_local::api::bingle_local_api_impl::{BingleApiLocalImpl, LocalApiConfig};

use crate::chat::ChatArgs;

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
    /// path that does not exist yet is treated as an empty first-run store (the account is created
    /// and registered by a later subtask). Returns an error string if no handle can be resolved
    /// (neither on the command line nor in the file), since the engine needs one to start.
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

        // The engine needs a handle to start; if we could resolve neither a CLI nor a stored one,
        // fail with a clear message rather than starting with an empty identity.
        if opts.handle.is_empty() {
            return Err(
                "no handle available: pass --handle <handle>, or use a --state_file with a registered account"
                    .to_string(),
            );
        }

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
}

/// Load a BingleLocal state file into `local`, mapping any load error to a clear, user-facing
/// string. Missing and malformed files both surface here as errors (no panic).
fn load_state(local: &mut BingleApiLocalImpl, path: &str) -> Result<(), String> {
    local
        .load(path)
        .map_err(|e| format!("failed to load chat state from {}: {}", path, e))
}
