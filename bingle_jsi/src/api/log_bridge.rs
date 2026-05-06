use std::sync::{Arc, Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing_subscriber::Layer;
use tracing::Subscriber;
use tracing_subscriber::registry::LookupSpan;
use tracing::field::Visit;

use crate::api::callback::LogCallback;
use rust_comms::util::logging::HandleExtension;

/// Global storage for the user-provided log callback.
static GLOBAL_LOG_CALLBACK: OnceLock<Arc<Mutex<Option<Box<dyn LogCallback>>>>> = OnceLock::new();

fn global_callback() -> &'static Arc<Mutex<Option<Box<dyn LogCallback>>>> {
    GLOBAL_LOG_CALLBACK.get_or_init(|| Arc::new(Mutex::new(None)))
}

/// Set (or replace) the global log callback.
pub fn set_global_log_callback(callback: Box<dyn LogCallback>) {
    if let Ok(mut guard) = global_callback().lock() {
        *guard = Some(callback);
    }
}

struct MessageVisitor {
    message: String,
}

impl Visit for MessageVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = format!("{:?}", value);
        }
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.message = value.to_string();
        }
    }
}

/// Custom layer that forwards to the registered LogCallback.
struct CallbackLayer;

impl<S> Layer<S> for CallbackLayer 
where 
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_event(&self, event: &tracing::Event<'_>, ctx: tracing_subscriber::layer::Context<'_, S>) {
        if let Ok(guard) = global_callback().lock() {
            if let Some(ref cb) = *guard {
                let timestamp = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0);
                
                let level = event.metadata().level().to_string();
                
                let mut prefix = String::new();
                if let Some(scope) = ctx.event_scope(event) {
                    for span in scope {
                        if span.name() == "BingleApi" {
                            if let Some(ext) = span.extensions().get::<HandleExtension>() {
                                prefix.push_str(&format!("[{}]", ext.0));
                                continue;
                            }
                        }
                        prefix.push_str(&format!("[{}]", span.name()));
                        // Note: we could also extract fields here if we had a field-storage layer
                    }
                }

                let mut visitor = MessageVisitor { message: String::new() };
                event.record(&mut visitor);
                
                cb.on_log(timestamp, level, format!("{}{}", prefix, visitor.message));
            }
        }
    }
}

/// Install the callback layer as a global subscriber.
pub fn install_log_bridge(level: tracing_subscriber::filter::LevelFilter) -> bool {
    use tracing_subscriber::prelude::*;
    let _ = global_callback();
    let layer = CallbackLayer;
    let subscriber = tracing_subscriber::registry()
        .with(layer)
        .with(level);
    
    tracing::subscriber::set_global_default(subscriber).is_ok()
}
