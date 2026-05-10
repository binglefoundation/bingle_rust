use tracing_subscriber::fmt::{format, FormatEvent, FormatFields};
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::layer::{Layer, Context};
use tracing::span::{Attributes, Id};
use tracing::{Subscriber, field::{Visit, Field}};
use std::fmt;

// Deprecated file logger shim. All logging should go through the `log` facade.
// These functions are kept as no-ops (or simple log forwarding) to avoid touching all call sites.

/// No-op: previously appended to a debug log file. Use the `log` crate instead.
pub fn removed_log_line<S: AsRef<str>>(_msg: S) {
    // intentionally empty
}

/// Forward to warn! only; no file writes.
pub fn tee_stderr<S: AsRef<str>>(msg: S) {
    tracing::warn!("{}", msg.as_ref());
}

#[macro_export]
macro_rules! info_theme {
    ($theme:expr, $($arg:tt)*) => {
        tracing::info!("[{}]{}", $theme, format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! warn_theme {
    ($theme:expr, $($arg:tt)*) => {
        tracing::warn!("[{}]{}", $theme, format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! error_theme {
    ($theme:expr, $($arg:tt)*) => {
        tracing::error!("[{}]{}", $theme, format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! debug_theme {
    ($theme:expr, $($arg:tt)*) => {
        tracing::debug!("[{}]{}", $theme, format_args!($($arg)*))
    };
}

use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum LogMode {
    Plain,
    ANSI,
    AWS,
    JS,
}

impl Default for LogMode {
    fn default() -> Self {
        LogMode::Plain
    }
}

pub struct HandleExtension(pub String);

#[derive(Default)]
pub struct HandleVisitor {
    pub handle: Option<String>,
}

impl Visit for HandleVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        if field.name() == "handle" {
            self.handle = Some(format!("{:?}", value));
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "handle" {
            self.handle = Some(value.to_string());
        }
    }
}

pub struct BingleFormatter {
    pub mode: LogMode,
}

impl Default for BingleFormatter {
    fn default() -> Self {
        Self { mode: LogMode::Plain }
    }
}

impl<S, N> FormatEvent<S, N> for BingleFormatter
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        ctx: &tracing_subscriber::fmt::FmtContext<'_, S, N>,
        mut writer: format::Writer<'_>,
        event: &tracing::Event<'_>,
    ) -> fmt::Result {
        let meta = event.metadata();

        let show_handle = match self.mode {
            LogMode::Plain | LogMode::ANSI => true,
            LogMode::AWS | LogMode::JS => false,
        };

        let use_ansi = match self.mode {
            LogMode::ANSI => true,
            LogMode::Plain | LogMode::AWS | LogMode::JS => false,
        };

        // 1. Try to find handle in span extensions
        let mut handle_str = String::new();
        if show_handle {
            if let Some(scope) = ctx.event_scope() {
                for span in scope {
                    if let Some(ext) = span.extensions().get::<HandleExtension>() {
                        handle_str = ext.0.clone();
                        break;
                    }
                }
            }
        }

        // 2. Format handle with color if present
        if !handle_str.is_empty() {
            if use_ansi {
                let color_code = match hash_str(&handle_str) % 6 {
                    0 => "31", // Red
                    1 => "32", // Green
                    2 => "33", // Yellow
                    3 => "34", // Blue
                    4 => "35", // Magenta
                    5 => "36", // Cyan
                    _ => "37",
                };
                write!(writer, "\x1b[1;{}m[{}]\x1b[0m ", color_code, handle_str)?;
            } else {
                write!(writer, "[{}] ", handle_str)?;
            }
        }

        // 3. Level (with default colors if enabled in writer)
        let level = *meta.level();
        if use_ansi && writer.has_ansi_escapes() {
            let level_str = match level {
                tracing::Level::TRACE => "\x1b[35mTRACE\x1b[0m",
                tracing::Level::DEBUG => "\x1b[34mDEBUG\x1b[0m",
                tracing::Level::INFO  => "\x1b[32m INFO\x1b[0m",
                tracing::Level::WARN  => "\x1b[33m WARN\x1b[0m",
                tracing::Level::ERROR => "\x1b[31mERROR\x1b[0m",
            };
            write!(writer, "{} ", level_str)?;
        } else {
            write!(writer, "{:>5} ", level)?;
        }

        // 4. Target/Module (optional, maybe keep it simple)
        // write!(writer, "{}: ", meta.target())?;

        // 5. Message
        ctx.format_fields(writer.by_ref(), event)?;

        writeln!(writer)
    }
}

fn hash_str(s: &str) -> usize {
    let mut h = 0usize;
    for b in s.as_bytes() {
        h = h.wrapping_add(*b as usize);
    }
    h
}

pub struct HandleLayer;

impl<S> Layer<S> for HandleLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        let span = ctx.span(id).expect("Span not found, this is a bug");
        if span.name() == "BingleApi" {
            let mut visitor = HandleVisitor::default();
            attrs.record(&mut visitor);
            if let Some(handle) = visitor.handle {
                span.extensions_mut().insert(HandleExtension(handle));
            }
        }
    }
}
