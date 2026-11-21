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
}

/// Parse the arguments following `logclient` into a [`LogClientArgs`].
///
/// `--state_file` is required: it holds the account and the history that drops duplicate copies.
/// `--maildir` and `--from` go together.
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
    let mut rest: Vec<String> = Vec::with_capacity(args.len());
    let mut it = args.into_iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--maildir" => maildir = Some(it.next().ok_or("--maildir requires a <dir> value")?),
            "--from" => from = Some(it.next().ok_or("--from requires an <address> value")?),
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

/// The receive side of a `logclient` session: the account state and the sink messages go to.
///
/// Shared (behind a mutex) between the engine's `on_message` callback and the Mailbox poller.
pub struct LogClient {
    state: ChatState,
    sink: Box<dyn MessageSink>,
    delivered: u64,
}

impl LogClient {
    /// A client over a ready account's `state`, delivering to `sink`.
    pub fn new(state: ChatState, sink: Box<dyn MessageSink>) -> Self {
        Self {
            state,
            sink,
            delivered: 0,
        }
    }

    /// The account state.
    pub fn state(&self) -> &ChatState {
        &self.state
    }

    /// How many messages the sink has accepted this session.
    pub fn delivered(&self) -> u64 {
        self.delivered
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
}
