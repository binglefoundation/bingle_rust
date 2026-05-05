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
