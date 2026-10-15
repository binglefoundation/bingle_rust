use std::time::Duration;

use crate::util::periodic_task::{FirstPass, PeriodicPass, PeriodicTask};

/// How often a relay re-checks the on-chain `allow_relay` permission of the relays in its set
/// (issue #120). A relay disabled on chain leaves the set within this interval, without a sign-off
/// or a cluster restart. Each pass makes one chain read per relay in the set, which is a handful.
pub const RELAY_PERMISSION_REFRESH_INTERVAL: Duration = Duration::from_secs(300);

/// One refresh pass. Returns `false` when the owner has gone away and the loop should end.
pub type RefreshPass = PeriodicPass;

/// Background loop on a relay that runs a refresh pass at start and then once per interval, to
/// evict relays whose on-chain `allow_relay` permission has been withdrawn from the relay set
/// (issue #120). The pass itself is supplied by the engine, so the loop is testable on its own.
pub struct RelayPermissionRefresher(PeriodicTask);

impl RelayPermissionRefresher {
    /// Create a refresher that runs `pass` every `interval` once started.
    pub fn new(interval: Duration, pass: RefreshPass) -> Self {
        Self(PeriodicTask::new(
            "RelayPermissionRefresher",
            interval,
            FirstPass::Immediately,
            pass,
        ))
    }

    /// Number of refresh passes run so far.
    pub fn pass_count(&self) -> u64 {
        self.0.pass_count()
    }

    /// Start the loop: one pass now, then one per interval until [`stop`](Self::stop).
    /// A second call is a no-op.
    pub fn start(&mut self) {
        self.0.start();
    }

    /// Stop the loop and wait for it to finish. An in-flight pass runs to completion first.
    pub fn stop(&mut self) {
        self.0.stop();
    }
}
