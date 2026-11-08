//! Input and output for the interactive `chat` REPL (issue #336).
//!
//! In a terminal, lines are read through a [rustyline] line editor: arrow keys move within the
//! line, Home/End and Ctrl-A/Ctrl-E jump, up/down step through the lines entered this session and
//! Ctrl-R searches them. Unbound keys and escape sequences are ignored rather than echoed into the
//! message. History is in memory only ([`MemHistory`]): it starts empty in each session and is never
//! written to a file, because it holds message text.
//!
//! Output from other threads (received messages, send results, connection changes) goes through
//! [`ConsoleOut`]. In the editor it uses rustyline's external printer, which prints above the prompt
//! and redraws the half-typed line with its cursor intact.
//!
//! When stdin or stdout is not a terminal (piped input, as in the integration tests), lines are
//! read with plain `read_line` and output is printed above a reprinted prompt, as before.

use std::io::{IsTerminal, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use bingle_local::api::bingle_local_api_impl::BingleApiLocalImpl;
use bingle_local::api::pending_sender::{LocalOutboundStore, OutboundStore, PendingSender};
use rustyline::error::ReadlineError;
use rustyline::history::MemHistory;
use rustyline::{Config, Editor, ExternalPrinter};

use crate::chat_send::{EXIT_FLUSH_DEADLINE, shutdown_lines};

/// The chat line editor: no helper, in-memory history only.
pub type ChatEditor = Editor<(), MemHistory>;

/// Build the chat line editor (issue #336). History is in memory ([`MemHistory`]), so nothing is
/// ever written to disk; lines are added explicitly by [`ChatConsole::read_line`], and consecutive
/// duplicates are dropped.
pub fn new_editor() -> rustyline::Result<ChatEditor> {
    let config = Config::builder()
        .auto_add_history(false)
        .history_ignore_dups(true)?
        .build();
    Editor::with_history(config, MemHistory::new())
}

/// Add a line entered at the prompt to the session history, unless it is blank.
pub fn add_to_history(editor: &mut ChatEditor, line: &str) {
    if !line.trim().is_empty() {
        let _ = editor.add_history_entry(line);
    }
}

/// Prints a line above the prompt, from any thread.
pub trait ConsoleOut: Send + Sync {
    /// Print `text` (one or more lines) above the prompt, keeping the prompt and any half-typed
    /// line below it.
    fn print_line(&self, text: &str);
}

/// Output for plain (non-terminal) input: print the line, then reprint the prompt so the cursor
/// sits after it again.
pub struct PlainOut {
    prompt: Arc<Mutex<String>>,
}

impl ConsoleOut for PlainOut {
    fn print_line(&self, text: &str) {
        println!("\n{text}");
        let prompt = self.prompt.lock().map(|g| g.clone()).unwrap_or_default();
        print!("{prompt}");
        let _ = std::io::stdout().flush();
    }
}

/// Output for the line editor: rustyline's external printer, which redraws the prompt and the
/// half-typed line after printing.
struct EditorOut {
    printer: Mutex<Box<dyn ExternalPrinter + Send>>,
}

impl ConsoleOut for EditorOut {
    fn print_line(&self, text: &str) {
        let printed = self
            .printer
            .lock()
            .map(|mut p| p.print(format!("{text}\n")).is_ok())
            .unwrap_or(false);
        if !printed {
            println!("{text}");
        }
    }
}

/// What reading a line produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadLine {
    /// A line was entered (without its line ending).
    Line(String),
    /// Ctrl-C at the prompt (the editor reads it as a key, not a signal).
    Interrupted,
    /// Ctrl-D on an empty line, end of input, or an input error.
    Eof,
}

/// The chat REPL's line source and output.
pub struct ChatConsole {
    editor: Option<ChatEditor>,
    prompt: Arc<Mutex<String>>,
    out: Arc<dyn ConsoleOut>,
}

impl ChatConsole {
    /// Use the line editor when stdin and stdout are terminals, and plain line reads otherwise.
    pub fn new() -> Self {
        let interactive = std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
        if interactive {
            match Self::with_editor() {
                Ok(console) => return console,
                Err(e) => tracing::warn!("chat: line editing unavailable ({e}); using plain input"),
            }
        }
        Self::plain()
    }

    /// Plain line reads, with output printed above a reprinted prompt.
    pub fn plain() -> Self {
        let prompt = Arc::new(Mutex::new(String::new()));
        let out = Arc::new(PlainOut {
            prompt: Arc::clone(&prompt),
        });
        Self {
            editor: None,
            prompt,
            out,
        }
    }

    fn with_editor() -> rustyline::Result<Self> {
        let mut editor = new_editor()?;
        let printer = editor.create_external_printer()?;
        Ok(Self {
            editor: Some(editor),
            prompt: Arc::new(Mutex::new(String::new())),
            out: Arc::new(EditorOut {
                printer: Mutex::new(Box::new(printer)),
            }),
        })
    }

    /// Whether lines are read through the line editor.
    pub fn is_editing(&self) -> bool {
        self.editor.is_some()
    }

    /// The output handle, for threads printing above the prompt.
    pub fn out(&self) -> Arc<dyn ConsoleOut> {
        Arc::clone(&self.out)
    }

    /// Show `prompt` and read one line.
    pub fn read_line(&mut self, prompt: &str) -> ReadLine {
        if let Ok(mut current) = self.prompt.lock() {
            *current = prompt.to_string();
        }
        match &mut self.editor {
            Some(editor) => match editor.readline(prompt) {
                Ok(line) => {
                    add_to_history(editor, &line);
                    ReadLine::Line(line)
                }
                Err(ReadlineError::Interrupted) => ReadLine::Interrupted,
                Err(ReadlineError::Eof) => ReadLine::Eof,
                Err(e) => {
                    tracing::warn!("chat: input error: {e}");
                    ReadLine::Eof
                }
            },
            None => {
                print!("{prompt}");
                let _ = std::io::stdout().flush();
                let mut line = String::new();
                match std::io::stdin().read_line(&mut line) {
                    Ok(0) => {
                        println!();
                        ReadLine::Eof
                    }
                    Ok(_) => ReadLine::Line(line.trim_end_matches(['\r', '\n']).to_string()),
                    Err(e) => {
                        tracing::warn!("chat: input error: {e}");
                        ReadLine::Eof
                    }
                }
            }
        }
    }
}

impl Default for ChatConsole {
    fn default() -> Self {
        Self::new()
    }
}

/// The chat session's clean shutdown, shared by every way of leaving (issue #336): `!exit`, Ctrl-D,
/// Ctrl-C at the prompt, and the Ctrl-C/SIGTERM signal handler.
///
/// It flushes what is still queued to the recipients' Mailboxes (issue #282), stops the engine and
/// saves the state. Only the first caller runs it; [`claim`](Self::claim) tells a later one that
/// shutdown is already under way, so a second Ctrl-C can quit at once.
pub struct SessionShutdown {
    started: AtomicBool,
    sender: Arc<PendingSender>,
    store: LocalOutboundStore,
    stop_engine: Box<dyn Fn() + Send + Sync>,
    local: Arc<BingleApiLocalImpl>,
    state_file: Option<String>,
}

impl SessionShutdown {
    /// A shutdown over the session's pending-message `sender` and its `store`, which calls
    /// `stop_engine` and then saves `local` to `state_file` (when one is configured).
    pub fn new(
        sender: Arc<PendingSender>,
        store: LocalOutboundStore,
        stop_engine: Box<dyn Fn() + Send + Sync>,
        local: Arc<BingleApiLocalImpl>,
        state_file: Option<String>,
    ) -> Self {
        Self {
            started: AtomicBool::new(false),
            sender,
            store,
            stop_engine,
            local,
            state_file,
        }
    }

    /// Claim the shutdown: `true` for the first caller, which must then [`run`](Self::run) it;
    /// `false` when it is already under way.
    pub fn claim(&self) -> bool {
        !self.started.swap(true, Ordering::SeqCst)
    }

    /// Flush queued messages, printing what happened to each through `print`, then stop the engine
    /// and save the state.
    pub fn run(&self, print: &dyn Fn(&str)) {
        let queued = self.store.pending_messages().map(|p| p.len()).unwrap_or(0);
        if queued > 0 {
            print(&format!(
                "finishing {queued} queued message(s)… (Ctrl-C again to quit now)"
            ));
        }
        let report = self.sender.shutdown(EXIT_FLUSH_DEADLINE);
        for line in shutdown_lines(&report) {
            print(&line);
        }
        (self.stop_engine)();
        crate::chat_poll::save_shared(&self.local, self.state_file.as_deref());
    }
}
