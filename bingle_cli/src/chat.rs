//! Argument parsing for the `chat` subcommand.
//!
//! Connection-shaped flags (`--handle`, `--passphrase`, `--node-file`, `--stun-servers`,
//! `--app-id`, `--asset-id`, ...) are delegated to [`parse_start_options_from_args`] so they behave
//! exactly as they do for `run`; the chat-specific flags (`--to`, `--to-id`, `--state_file`) are
//! handled here. Logging flags (`--debug`, `--info`, `--log-*`) are consumed globally before dispatch
//! by `init_logger_from_args`, so they never reach here; they are tolerated defensively.

use bingle_core::api::bingle_api::StartOptions;
use bingle_core::util::cli_utils::parse_start_options_from_args;

/// Placeholder positional handle injected so `parse_start_options_from_args` accepts args that omit
/// `--handle` when a `--state_file` is present. It is blanked immediately after parsing; the chat
/// state-file bridge fills the real handle from the stored keypair. Never surfaced to the user.
const HANDLE_FROM_STATE_FILE: &str = "__bingle_chat_handle_from_state_file__";

/// Parsed arguments for `bingle_cli chat`.
#[derive(Debug)]
pub struct ChatArgs {
    /// Connection/start options shared with `run` (handle, passphrase, node-file, stun servers,
    /// app/asset ids). Parsed via [`parse_start_options_from_args`] so behaviour matches `run`.
    pub opts: StartOptions,
    /// Recipient handle (`--to`). Mutually exclusive with [`ChatArgs::to_id`].
    pub to: Option<String>,
    /// Recipient id/address (`--to-id`). Mutually exclusive with [`ChatArgs::to`].
    pub to_id: Option<String>,
    /// Optional local state file (`--state_file`, also accepted as `--state-file`).
    pub state_file: Option<String>,
    /// `--no-retries`: send each message once and report failure immediately instead of keeping it
    /// pending and retrying while the recipient is offline. Retries are on by default.
    pub no_retries: bool,
}

/// Parse the arguments following `chat` into a [`ChatArgs`].
///
/// Chat-specific flags are pulled out first (so [`parse_start_options_from_args`] does not reject
/// them as unknown), then the remaining args are parsed as start options. Returns a human-readable
/// error string on any parse or validation failure; callers map this to a usage error (exit 2).
pub fn parse_chat_args(args: Vec<String>) -> Result<ChatArgs, String> {
    let mut to: Option<String> = None;
    let mut to_id: Option<String> = None;
    let mut state_file: Option<String> = None;
    let mut no_retries = false;
    // Everything not consumed here is forwarded to the shared start-options parser.
    let mut rest: Vec<String> = Vec::with_capacity(args.len());

    let mut it = args.into_iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--to" => {
                to = Some(it.next().ok_or("--to requires a <handle> value")?);
            }
            "--to-id" => {
                to_id = Some(it.next().ok_or("--to-id requires an <id> value")?);
            }
            "--state_file" | "--state-file" => {
                state_file = Some(it.next().ok_or("--state_file requires a <file> value")?);
            }
            // Send each message once instead of keeping it pending and retrying (accept the singular
            // `--no-retry` too).
            "--no-retries" | "--no-retry" => {
                no_retries = true;
            }
            // Logging flags are normally consumed before dispatch by `init_logger_from_args`. Tolerate
            // them here too (as no-ops) so they never reach `parse_start_options_from_args`, which
            // would reject `--info`/`--warn` as unknown.
            "--debug" | "--info" | "--warn" => {}
            _ => rest.push(arg),
        }
    }

    // --to and --to-id both name the recipient; accepting both would be ambiguous.
    if to.is_some() && to_id.is_some() {
        return Err("--to and --to-id are mutually exclusive".to_string());
    }

    // `parse_start_options_from_args` requires a handle. When a state file is supplied the handle
    // (and passphrase) can come from the stored keypair instead, so a missing handle is not an error
    // here — it is deferred to the state-file bridge. We detect that specific case from the parser's
    // error and re-parse with a placeholder positional handle, then blank it so the bridge fills it
    // in from the file (leaving it non-empty would be mistaken for a CLI-provided handle).
    let opts = match parse_start_options_from_args(rest.clone()) {
        Ok(o) => o,
        Err(e) if state_file.is_some() && e.starts_with("Missing handle") => {
            let mut with_placeholder = rest;
            with_placeholder.push(HANDLE_FROM_STATE_FILE.to_string());
            let mut o = parse_start_options_from_args(with_placeholder)?;
            o.handle = String::new();
            o
        }
        Err(e) => return Err(e),
    };

    Ok(ChatArgs {
        opts,
        to,
        to_id,
        state_file,
        no_retries,
    })
}
