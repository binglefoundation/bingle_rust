//! Argument parsing for the `chat` subcommand.
//!
//! This is the scaffold for the "Command line chat app" epic: it parses and validates the chat
//! arguments but does not yet start an engine or open any network connection — later subtasks fill
//! that in. Connection-shaped flags (`--handle`, `--passphrase`, `--node-file`, `--stun-servers`,
//! `--app-id`, `--asset-id`, ...) are delegated to [`parse_start_options_from_args`] so they behave
//! exactly as they do for `run`; the chat-specific flags (`--to`, `--to-id`, `--state_file`,
//! `--debug`) are handled here.

use bingle_core::api::bingle_api::StartOptions;
use bingle_core::util::cli_utils::parse_start_options_from_args;
use tracing_subscriber::filter::LevelFilter;

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
    /// Effective log level: `Warn` by default, raised to `Debug` when `--debug` is given.
    pub log_level: LevelFilter,
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
    let mut debug = false;
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
            "--debug" => {
                // Chat-local alias for verbose logging. Consumed here rather than forwarded: it is a
                // no-op for start-options parsing, and we want the effective level in ChatArgs.
                debug = true;
            }
            _ => rest.push(arg),
        }
    }

    // --to and --to-id both name the recipient; accepting both would be ambiguous.
    if to.is_some() && to_id.is_some() {
        return Err("--to and --to-id are mutually exclusive".to_string());
    }

    let opts = parse_start_options_from_args(rest)?;

    let log_level = if debug {
        LevelFilter::DEBUG
    } else {
        LevelFilter::WARN
    };

    Ok(ChatArgs {
        opts,
        to,
        to_id,
        state_file,
        log_level,
    })
}
