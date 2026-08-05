//! Pure pieces of the `chat` interactive REPL: the current recipient, the prompt it renders, and
//! the classification of an input line into an action. The terminal/readline glue and the engine
//! calls live in `cmd_chat`; keeping this logic pure makes the dispatcher unit-testable without IO.

use crate::chat_send::SendTarget;

/// The recipient the next typed line is sent to. Seeded from `--to` / `--to-id` and changed by the
/// `/prefix` switch command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CurrentRecipient {
    /// No recipient chosen yet — the prompt asks the user to pick one.
    None,
    /// A recipient handle, with the id cached once resolved (via `/switch` or a send).
    Handle { handle: String, id: Option<String> },
    /// A recipient known only by id (`--to-id`), with no handle.
    Id(String),
}

impl CurrentRecipient {
    /// Seed the recipient from the CLI `--to` (handle) / `--to-id` (id). `--to` wins if both are
    /// somehow present (`parse_chat_args` already rejects that combination).
    pub fn from_args(to: Option<&str>, to_id: Option<&str>) -> Self {
        if let Some(handle) = to.filter(|h| !h.is_empty()) {
            CurrentRecipient::Handle {
                handle: handle.to_string(),
                id: None,
            }
        } else if let Some(id) = to_id.filter(|i| !i.is_empty()) {
            CurrentRecipient::Id(id.to_string())
        } else {
            CurrentRecipient::None
        }
    }

    /// The prompt line: `[handle] > ` / `[id] > ` when a recipient is set, `User? > ` when not.
    pub fn prompt(&self) -> String {
        match self {
            CurrentRecipient::None => "User? > ".to_string(),
            CurrentRecipient::Handle { handle, .. } => format!("[{handle}] > "),
            CurrentRecipient::Id(id) => format!("[{id}] > "),
        }
    }

    /// The send target for the current recipient, or `None` when no recipient is set.
    pub fn target(&self) -> Option<SendTarget> {
        match self {
            CurrentRecipient::None => None,
            CurrentRecipient::Handle { handle, .. } => Some(SendTarget::Handle(handle.clone())),
            CurrentRecipient::Id(id) => Some(SendTarget::Id(id.clone())),
        }
    }
}

/// The action a line of input maps to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatInput {
    /// A blank line — just reprompt.
    Empty,
    /// `!exit` — leave the REPL.
    Exit,
    /// `/prefix` — switch the recipient to the handle matching `prefix`.
    Switch { prefix: String },
    /// Plain text — send it to the current recipient.
    Send { text: String },
}

/// Classify a line of input. Whitespace is trimmed; a leading `/` is a switch command, `!exit`
/// leaves, an empty line reprompts, and anything else is message text.
pub fn parse_input(line: &str) -> ChatInput {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        ChatInput::Empty
    } else if trimmed == "!exit" {
        ChatInput::Exit
    } else if let Some(prefix) = trimmed.strip_prefix('/') {
        ChatInput::Switch {
            prefix: prefix.trim().to_string(),
        }
    } else {
        ChatInput::Send {
            text: trimmed.to_string(),
        }
    }
}
