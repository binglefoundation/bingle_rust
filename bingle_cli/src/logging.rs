//! Logging helpers for the CLI. Only the pure default-level and filter decisions live here (in the
//! lib target) so they can be unit-tested from the test tree; the actual subscriber wiring stays in
//! `main.rs`.

use std::io::Write;
use std::sync::{Arc, Mutex};

use tracing_subscriber::EnvFilter;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::fmt::MakeWriter;

use crate::chat_console::ConsoleOut;

/// The default tracing level for a subcommand when the user passes no explicit `--log-*` flag.
///
/// `chat` (an interactive REPL) and `checkrelays` (a diagnostic whose result is a printed per-relay
/// summary) default to [`LevelFilter::WARN`] so their real output is not buried under the INFO-level
/// engine tracing; every other subcommand defaults to [`LevelFilter::INFO`]. Override with
/// `--info`/`--debug` (or `--log-level error` for just the result).
pub fn default_log_level(subcommand: Option<&str>) -> LevelFilter {
    match subcommand {
        Some("chat") | Some("checkrelays") => LevelFilter::WARN,
        _ => LevelFilter::INFO,
    }
}

/// HTTP client stack targets (the hyper/h2 client under reqwest, used for the indexer and Sidewinder
/// calls). Their per-request detail (connect, frames, connection pooling) is capped at WARN at every
/// level, `--log-level trace` included: the HTTP path is stable and its detail drowned the engine
/// output (issue #402). Only `RUST_LOG` brings it back.
pub const HTTP_STACK_TARGETS: [&str; 4] = ["h2", "hyper", "hyper_util", "reqwest"];

/// The tracing filter directives for the chosen `level`.
///
/// A non-empty `rust_log` (the `RUST_LOG` environment variable) is used as-is, overriding both the
/// level and the HTTP stack cap. Otherwise the directives are `level`, with each of
/// [`HTTP_STACK_TARGETS`] capped at WARN (or at `level`, when that is quieter).
pub fn filter_directives(level: LevelFilter, rust_log: Option<&str>) -> String {
    if let Some(directives) = rust_log.map(str::trim).filter(|d| !d.is_empty()) {
        return directives.to_string();
    }
    let http_level = std::cmp::min(level, LevelFilter::WARN);
    let mut directives = vec![level.to_string().to_ascii_lowercase()];
    directives.extend(
        HTTP_STACK_TARGETS
            .iter()
            .map(|target| format!("{target}={}", http_level.to_string().to_ascii_lowercase())),
    );
    directives.join(",")
}

/// The tracing filter for the chosen `level`, from [`filter_directives`]. Falls back to the plain
/// `level` if `rust_log` does not parse, so a bad `RUST_LOG` never silences the CLI.
pub fn log_filter(level: LevelFilter, rust_log: Option<&str>) -> EnvFilter {
    EnvFilter::try_new(filter_directives(level, rust_log))
        .unwrap_or_else(|_| EnvFilter::new(filter_directives(level, None)))
}

/// Where the CLI's log lines go: stdout, or, once a chat session attaches its console, above the
/// chat prompt (issue #81). Log lines written to stdout while the line editor is reading would break
/// into the prompt and the half-typed line; through the console they print above it instead.
///
/// Cloned into the subscriber's fmt layer at startup (it is the layer's
/// [`MakeWriter`](tracing_subscriber::fmt::MakeWriter)); the chat session attaches its
/// [`ConsoleOut`] to the same sink.
#[derive(Clone, Default)]
pub struct ConsoleLogSink {
    console: Arc<Mutex<Option<Arc<dyn ConsoleOut>>>>,
}

impl ConsoleLogSink {
    /// Send log lines above the prompt of `console` from now on.
    pub fn attach(&self, console: Arc<dyn ConsoleOut>) {
        if let Ok(mut current) = self.console.lock() {
            *current = Some(console);
        }
    }

    fn console(&self) -> Option<Arc<dyn ConsoleOut>> {
        self.console.lock().ok().and_then(|c| c.clone())
    }
}

impl<'a> MakeWriter<'a> for ConsoleLogSink {
    type Writer = LogLineWriter;

    fn make_writer(&'a self) -> Self::Writer {
        LogLineWriter {
            console: self.console(),
            buf: Vec::new(),
        }
    }
}

/// Collects one formatted log event and writes it out when dropped: above the chat prompt when a
/// console is attached, to stdout otherwise.
pub struct LogLineWriter {
    console: Option<Arc<dyn ConsoleOut>>,
    buf: Vec<u8>,
}

impl Write for LogLineWriter {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        self.buf.extend_from_slice(data);
        Ok(data.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Drop for LogLineWriter {
    fn drop(&mut self) {
        if self.buf.is_empty() {
            return;
        }
        match &self.console {
            Some(console) => {
                let text = String::from_utf8_lossy(&self.buf);
                console.print_line(text.trim_end_matches(['\r', '\n']));
            }
            None => {
                let mut stdout = std::io::stdout().lock();
                let _ = stdout.write_all(&self.buf);
                let _ = stdout.flush();
            }
        }
    }
}
