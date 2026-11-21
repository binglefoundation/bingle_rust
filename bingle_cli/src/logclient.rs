//! The `logclient` subcommand (epic #63, issue #64): run as a registered account and hand every
//! inbound text message to a [`MessageSink`], for a mailer to forward.
//!
//! `logclient` is `chat` without the REPL. It takes the same connection flags and `--state_file`
//! bridge, receives through the same paths (the engine's `on_message` callback into
//! [`accept_message`], and the store-and-forward Mailbox poll), and so inherits their filtering:
//! protocol messages, blocked senders and second copies of a message are dropped before the sink
//! sees them. It never sends, so the chat flags that only affect sending or the prompt are refused.
//!
//! With `--maildir <dir> --from <address>` each message is written into a Maildir as an email
//! (issue #65, see [`crate::maildir`]); otherwise it is written to stdout as a JSON line.
//!
//! With `--spam-limit <n>` at most `n` messages a UTC day from each sender are handed on (issue #66,
//! see [`crate::spam_limit`]); the rest stay in the state-file history only.
//!
//! **Hand on, then save.** A message is handed to the sink *before* the state file is saved, so a
//! crash can never leave a message in the saved history that the sink did not get: the sink side is
//! at-least-once. (The real-time sender and the Mailbox both treat a message as delivered once this
//! client has it in memory, so a crash before the sink writes it still loses it; that window is the
//! time between receipt and the sink's write.)
//!
//! The engine wiring and process exit live in `main.rs`; this module holds the pure parts so they can
//! be unit tested from the test tree.

use std::io::Write;
use std::path::Path;

use bingle_local::api::bingle_local_api::Message;
use serde_json::{Value, json};

use crate::chat::{ChatArgs, StoreForwardMode, parse_chat_args};
use crate::chat_receive::accept_message;
use crate::chat_state::ChatState;
use crate::maildir::{Maildir, format_email, validate_from_address};
use crate::spam_limit::{Admission, SPAM_LIMIT_RECORD, SpamLimiter};

/// Chat flags `logclient` refuses: they only affect sending or the interactive prompt, so accepting
/// them would suggest they do something.
const REFUSED_CHAT_FLAGS: [&str; 6] = [
    "--to",
    "--to-id",
    "--no-retries",
    "--no-retry",
    "--notify",
    "--plain",
];

/// Parsed arguments for `bingle_cli logclient`.
#[derive(Debug)]
pub struct LogClientArgs {
    /// The connection, state-file and receive options, in the form the shared `chat` startup takes.
    /// `to`/`to_id` are always `None`, and `state_file` is always `Some`.
    pub chat: ChatArgs,
    /// `--maildir <dir>`: write each message into this Maildir as an email. Set exactly when
    /// [`from`](LogClientArgs::from) is.
    pub maildir: Option<String>,
    /// `--from <address>`: the SES-verified address the emails are from.
    pub from: Option<String>,
    /// `--spam-limit <n>`: hand on at most `n` messages per sender per UTC day; `None` for no limit.
    pub spam_limit: Option<u32>,
}

/// Parse the arguments following `logclient` into a [`LogClientArgs`].
///
/// `--state_file` is required: it holds the account and the history that drops duplicate copies.
/// `--maildir` and `--from` go together. `--spam-limit` takes a whole number of at least 1.
/// The store-and-forward receive gate is on by default (`--store-forward receive`), since a support
/// inbox must pick up messages held while it was offline; pass `--store-forward none` to turn it off.
/// Returns a human-readable error on any parse or validation failure; callers map it to a usage
/// error (exit 2).
pub fn parse_logclient_args(args: Vec<String>) -> Result<LogClientArgs, String> {
    if let Some(flag) = args
        .iter()
        .find(|a| REFUSED_CHAT_FLAGS.contains(&a.as_str()))
    {
        return Err(format!(
            "{flag} is not supported by logclient, which only receives"
        ));
    }

    let mut maildir: Option<String> = None;
    let mut from: Option<String> = None;
    let mut spam_limit: Option<u32> = None;
    let mut rest: Vec<String> = Vec::with_capacity(args.len());
    let mut it = args.into_iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--maildir" => maildir = Some(it.next().ok_or("--maildir requires a <dir> value")?),
            "--from" => from = Some(it.next().ok_or("--from requires an <address> value")?),
            "--spam-limit" => {
                let value = it.next().ok_or("--spam-limit requires an <n> value")?;
                match value.parse::<u32>() {
                    Ok(n) if n >= 1 => spam_limit = Some(n),
                    _ => {
                        return Err(format!(
                            "--spam-limit must be a whole number of at least 1, got '{value}'"
                        ));
                    }
                }
            }
            _ => rest.push(arg),
        }
    }
    match (&maildir, &from) {
        (Some(_), None) => return Err("--maildir needs --from <address>".to_string()),
        (None, Some(_)) => return Err("--from is only used with --maildir <dir>".to_string()),
        (Some(_), Some(address)) => validate_from_address(address)?,
        (None, None) => {}
    }

    let mut args = rest;
    if !args.iter().any(|a| a == "--store-forward") {
        args.push("--store-forward".to_string());
        args.push("receive".to_string());
    }

    let chat = parse_chat_args(args)?;
    if chat.state_file.is_none() {
        return Err("--state_file <file> is required".to_string());
    }
    if chat.store_forward == StoreForwardMode::Send || chat.store_forward == StoreForwardMode::Both
    {
        return Err(
            "--store-forward must be receive or none for logclient, which never sends".to_string(),
        );
    }
    Ok(LogClientArgs {
        chat,
        maildir,
        from,
        spam_limit,
    })
}

/// Where `logclient` hands each received message.
pub trait MessageSink: Send {
    /// Deliver one received message, from the sender with Bingle id `sender_id` when it is known.
    /// An error is logged by the caller; the message stays in the state-file history either way.
    fn deliver(&mut self, message: &Message, sender_id: Option<&str>) -> Result<(), String>;
}

/// A [`MessageSink`] that writes each message as one JSON object per line, flushing after each:
/// `{"id": …, "sender": …, "sender_id": …, "sent_time": …, "text": …}`, where `id` is the stable
/// message id (issue #209), `sender` the sender's handle (or id, when the handle is unknown),
/// `sender_id` its Bingle id (`null` when unknown) and `sent_time` the [`Message::order_time`] in
/// epoch milliseconds.
///
/// JSON keeps a message with line breaks on one line. Used when no Maildir is configured.
pub struct JsonLinesSink<W: Write + Send> {
    out: W,
}

impl<W: Write + Send> JsonLinesSink<W> {
    /// A sink writing to `out`.
    pub fn new(out: W) -> Self {
        Self { out }
    }

    /// The writer, for tests to inspect what was written.
    pub fn into_inner(self) -> W {
        self.out
    }
}

impl<W: Write + Send> MessageSink for JsonLinesSink<W> {
    fn deliver(&mut self, message: &Message, sender_id: Option<&str>) -> Result<(), String> {
        let line = json!({
            "id": message.id,
            "sender": message.sender_handle,
            "sender_id": sender_id,
            "sent_time": message.order_time(),
            "text": message.text,
        });
        writeln!(self.out, "{line}").map_err(|e| e.to_string())?;
        self.out.flush().map_err(|e| e.to_string())
    }
}

/// A [`MessageSink`] that writes each message into a Maildir as an email from a fixed address.
pub struct MaildirSink {
    maildir: Maildir,
    from: String,
}

impl MaildirSink {
    /// A sink writing into the Maildir at `dir` (created if missing), with emails from `from`.
    pub fn open(dir: &Path, from: &str) -> Result<MaildirSink, String> {
        validate_from_address(from)?;
        Ok(MaildirSink {
            maildir: Maildir::open(dir)?,
            from: from.to_string(),
        })
    }
}

impl MessageSink for MaildirSink {
    fn deliver(&mut self, message: &Message, sender_id: Option<&str>) -> Result<(), String> {
        let email = format_email(message, sender_id, &self.from);
        self.maildir.deliver(email.as_bytes()).map(|_| ())
    }
}

/// The receive side of a `logclient` session: the account state, the sink messages go to and the
/// optional spam limit.
///
/// Shared (behind a mutex) between the engine's `on_message` callback and the Mailbox poller.
pub struct LogClient {
    state: ChatState,
    sink: Box<dyn MessageSink>,
    delivered: u64,
    limiter: Option<SpamLimiter>,
    suppressed: u64,
    /// Whether a failure to store the limiter's counts has been logged, so it is logged once.
    record_error_logged: bool,
}

impl LogClient {
    /// A client over a ready account's `state`, delivering to `sink`.
    pub fn new(state: ChatState, sink: Box<dyn MessageSink>) -> Self {
        Self {
            state,
            sink,
            delivered: 0,
            limiter: None,
            suppressed: 0,
            record_error_logged: false,
        }
    }

    /// Apply `limiter` to every message, starting from the counts kept in the state file by an
    /// earlier run. Unreadable counts are logged and start from zero.
    pub fn with_spam_limit(mut self, mut limiter: SpamLimiter) -> Self {
        match self.state.app_record(SPAM_LIMIT_RECORD) {
            Ok(Some(record)) => {
                if let Err(e) = limiter.restore(&record) {
                    tracing::warn!("logclient: ignoring stored spam-limit counts ({e})");
                }
            }
            Ok(None) => {}
            Err(e) => tracing::warn!("logclient: could not read stored spam-limit counts: {e}"),
        }
        self.limiter = Some(limiter);
        self
    }

    /// The account state.
    pub fn state(&self) -> &ChatState {
        &self.state
    }

    /// How many messages the sink has accepted this session.
    pub fn delivered(&self) -> u64 {
        self.delivered
    }

    /// How many messages the spam limit kept from the sink this session.
    pub fn suppressed(&self) -> u64 {
        self.suppressed
    }

    /// The spam limiter, when `--spam-limit` is set.
    pub fn limiter(&self) -> Option<&SpamLimiter> {
        self.limiter.as_ref()
    }

    /// Handle a message from the engine's `on_message` callback: accept it through the `chat`
    /// receive path and, when that takes it as a new text message, deliver it, then save the state
    /// file. Returns whether the sink accepted a message.
    pub fn receive(&mut self, sender_id: &str, sender_handle: &str, message: &Value) -> bool {
        let Some(stored) = accept_message(&mut self.state, sender_id, sender_handle, message)
        else {
            return false;
        };
        let delivered = self.deliver(&stored, Some(sender_id));
        self.save();
        delivered
    }

    /// Deliver messages read from the Mailbox, which the poll has recorded in memory but not saved,
    /// then save the state file. Returns how many the sink accepted.
    pub fn deliver_polled(&mut self, read: &[Message]) -> usize {
        let mut delivered = 0;
        for message in read {
            // A Mailbox message names its sender by handle; the id is known only for a contact.
            let sender_id = self
                .state
                .resolve_recipient(&message.sender_handle)
                .map(str::to_string);
            if self.deliver(message, sender_id.as_deref()) {
                delivered += 1;
            }
        }
        if !read.is_empty() {
            self.save();
        }
        delivered
    }

    /// Save the state file, logging a failure.
    pub fn save(&self) {
        if let Err(e) = self.state.save_state() {
            tracing::warn!("logclient: could not save state: {e}");
        }
    }

    fn deliver(&mut self, message: &Message, sender_id: Option<&str>) -> bool {
        if !self.admit(message, sender_id) {
            return false;
        }
        match self.sink.deliver(message, sender_id) {
            Ok(()) => {
                self.delivered += 1;
                true
            }
            Err(e) => {
                // The message is kept in the history but will not be forwarded.
                tracing::error!(
                    "logclient: could not deliver message {} from {}: {e}",
                    message.id,
                    message.sender_handle
                );
                false
            }
        }
    }

    /// Apply the spam limit to `message`, keyed by its sender's id (or handle, when the id isn't
    /// known), and keep the counts in the state for the next save. Returns whether to hand it on.
    fn admit(&mut self, message: &Message, sender_id: Option<&str>) -> bool {
        let Some(limiter) = self.limiter.as_mut() else {
            return true;
        };
        let admission = limiter.admit(sender_id.unwrap_or(&message.sender_handle));
        let record = limiter.record();
        if let Some(record) = record
            && let Err(e) = self.state.set_app_record(SPAM_LIMIT_RECORD, record)
            && !self.record_error_logged
        {
            // The counts still apply in memory; only a restart would reset them.
            tracing::warn!("logclient: could not store spam-limit counts: {e}");
            self.record_error_logged = true;
        }
        match admission {
            Admission::Forward => true,
            Admission::Suppress { log } => {
                if let Some(line) = log {
                    tracing::warn!("logclient: {line}");
                }
                self.suppressed += 1;
                false
            }
        }
    }
}
