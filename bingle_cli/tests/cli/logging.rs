// Unit tests for the CLI log-level defaults and filter directives (bingle_cli::logging).
use bingle_cli::logging::{HTTP_STACK_TARGETS, default_log_level, filter_directives, log_filter};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::{Context, Layer, SubscriberExt};

#[test]
#[cfg(not(target_os = "ios"))]
pub fn chat_and_checkrelays_default_to_warn() {
    // Both print their own output (REPL prompt / per-relay summary) that would otherwise be buried
    // under INFO engine tracing, so they default to WARN.
    assert_eq!(default_log_level(Some("chat")), LevelFilter::WARN);
    assert_eq!(default_log_level(Some("checkrelays")), LevelFilter::WARN);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn other_subcommands_and_none_default_to_info() {
    for sc in ["run", "register", "migrate", "buybingle", "sellbingle"] {
        assert_eq!(
            default_log_level(Some(sc)),
            LevelFilter::INFO,
            "subcommand {sc} should keep the INFO default"
        );
    }
    assert_eq!(default_log_level(None), LevelFilter::INFO);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn http_stack_is_capped_at_warn_at_every_level() {
    for level in [LevelFilter::INFO, LevelFilter::DEBUG, LevelFilter::TRACE] {
        let directives = filter_directives(level, None);
        let lvl = level.to_string().to_ascii_lowercase();
        assert!(directives.starts_with(&format!("{lvl},")), "{directives}");
        for target in HTTP_STACK_TARGETS {
            assert!(
                directives.contains(&format!("{target}=warn")),
                "{target} should be capped at warn at {lvl}: {directives}"
            );
        }
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn http_stack_cap_follows_a_quieter_level() {
    let directives = filter_directives(LevelFilter::ERROR, None);
    assert!(directives.starts_with("error,"), "{directives}");
    for target in HTTP_STACK_TARGETS {
        assert!(
            directives.contains(&format!("{target}=error")),
            "{directives}"
        );
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn rust_log_overrides_the_default_directives() {
    assert_eq!(
        filter_directives(LevelFilter::WARN, Some("h2=debug,info")),
        "h2=debug,info"
    );
    // An empty or blank RUST_LOG is ignored.
    assert_eq!(
        filter_directives(LevelFilter::DEBUG, Some("  ")),
        filter_directives(LevelFilter::DEBUG, None)
    );
}

/// Counts the events that reach it, by target prefix, so a test can see what a filter lets through.
#[derive(Clone, Default)]
struct Counter {
    http: Arc<AtomicUsize>,
    engine: Arc<AtomicUsize>,
}

impl<S: tracing::Subscriber> Layer<S> for Counter {
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        if event.metadata().target().starts_with("hyper_util") {
            self.http.fetch_add(1, Ordering::SeqCst);
        } else {
            self.engine.fetch_add(1, Ordering::SeqCst);
        }
    }
}

fn counts_through(level: LevelFilter, rust_log: Option<&str>) -> (usize, usize) {
    let counter = Counter::default();
    let subscriber = tracing_subscriber::registry()
        .with(log_filter(level, rust_log))
        .with(counter.clone());
    tracing::subscriber::with_default(subscriber, || {
        tracing::debug!(target: "hyper_util::client::legacy::pool", "pooling idle connection");
        tracing::trace!(target: "hyper_util::client::legacy::pool", "trace detail");
        tracing::warn!(target: "hyper_util::client::legacy::pool", "http warning");
        tracing::debug!(target: "bingle_core::engine", "engine debug");
        tracing::trace!(target: "bingle_core::engine", "engine trace");
    });
    (
        counter.http.load(Ordering::SeqCst),
        counter.engine.load(Ordering::SeqCst),
    )
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn debug_filter_passes_engine_detail_but_only_http_warnings() {
    assert_eq!(counts_through(LevelFilter::DEBUG, None), (1, 1));
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn trace_filter_still_caps_http_detail() {
    assert_eq!(counts_through(LevelFilter::TRACE, None), (1, 2));
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn rust_log_brings_back_http_detail() {
    assert_eq!(counts_through(LevelFilter::WARN, Some("trace")), (3, 2));
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn an_unparseable_rust_log_falls_back_to_the_level() {
    assert_eq!(counts_through(LevelFilter::DEBUG, Some("=[bad")), (1, 1));
}
