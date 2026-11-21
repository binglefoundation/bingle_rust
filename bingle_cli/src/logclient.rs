//! The `logclient` subcommand (epic #63, issue #64): run as a registered account and hand every
//! inbound text message to a [`MessageSink`], for a mailer to forward.
//!
//! `logclient` is `chat` without the REPL. It takes the same connection flags and `--state_file`
//! bridge, receives through the same paths (the engine's `on_message` callback into
//! [`receive_message`], and the store-and-forward Mailbox poll), and so inherits their filtering:
//! protocol messages, blocked senders and second copies of a message are dropped before the sink
//! sees them. It never sends, so the chat flags that only affect sending or the prompt are refused.
//!
//! The engine wiring and process exit live in `main.rs`; this module holds the pure parts so they can
//! be unit tested from the test tree.

use std::io::Write;

use bingle_local::api::bingle_local_api::Message;
use serde_json::{Value, json};

use crate::chat::{ChatArgs, StoreForwardMode, parse_chat_args};
use crate::chat_receive::receive_message;
use crate::chat_state::ChatState;

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
}

/// Parse the arguments following `logclient` into a [`LogClientArgs`].
///
/// `--state_file` is required: it holds the account and the history that drops duplicate copies.
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

    let mut args = args;
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
    Ok(LogClientArgs { chat })
}

/// Where `logclient` hands each received message.
pub trait MessageSink: Send {
    /// Deliver one received message. An error is logged by the caller; the message stays in the
    /// state-file history either way.
    fn deliver(&mut self, message: &Message) -> Result<(), String>;
}

/// A [`MessageSink`] that writes each message as one JSON object per line, flushing after each:
/// `{"id": …, "sender": …, "sent_time": …, "text": …}`, where `id` is the stable message id (issue
/// #209), `sender` the sender's handle (or id, when the handle is unknown) and `sent_time` the
/// [`Message::order_time`] in epoch milliseconds.
///
/// JSON keeps a message with line breaks on one line. The default sink until the Maildir sink
/// (issue #65) lands, and the one used when no Maildir is configured.
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
    fn deliver(&mut self, message: &Message) -> Result<(), String> {
        let line = json!({
            "id": message.id,
            "sender": message.sender_handle,
            "sent_time": message.order_time(),
            "text": message.text,
        });
        writeln!(self.out, "{line}").map_err(|e| e.to_string())?;
        self.out.flush().map_err(|e| e.to_string())
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

    /// Handle a message from the engine's `on_message` callback: record it through the `chat`
    /// receive path and, when that accepts it as a new text message, deliver it. Returns whether the
    /// sink accepted a message.
    pub fn receive(&mut self, sender_id: &str, sender_handle: &str, message: &Value) -> bool {
        match receive_message(&mut self.state, sender_id, sender_handle, message) {
            Some(stored) => self.deliver(&stored),
            None => false,
        }
    }

    /// Deliver messages read from the Mailbox, which the poll has already recorded. Returns how many
    /// the sink accepted.
    pub fn deliver_polled(&mut self, read: &[Message]) -> usize {
        read.iter().filter(|m| self.deliver(m)).count()
    }

    fn deliver(&mut self, message: &Message) -> bool {
        match self.sink.deliver(message) {
            Ok(()) => {
                self.delivered += 1;
                true
            }
            Err(e) => {
                tracing::warn!(
                    "logclient: could not deliver message {} from {}: {e}",
                    message.id,
                    message.sender_handle
                );
                false
            }
        }
    }
}
