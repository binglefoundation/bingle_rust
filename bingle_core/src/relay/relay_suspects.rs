use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Wait before the first re-check of a relay reported failed (issue #516). At least the engine's
/// `SEND_FAIL_BACKOFF`, so the check is a real send rather than the fast "Sending is failing"
/// refusal left by the send that failed.
pub const RELAY_SUSPECT_FIRST_RECHECK: Duration = Duration::from_secs(30);

/// Longest wait between re-checks of a suspect relay. Each failed re-check doubles the wait up to
/// this, so a relay that is down for hours costs one probe per interval.
pub const RELAY_SUSPECT_MAX_RECHECK: Duration = Duration::from_secs(600);

/// How often a relay looks for suspect relays due a re-check.
pub const RELAY_SUSPECT_CHECK_INTERVAL: Duration = Duration::from_secs(10);

/// Clock used to time re-checks; injectable so tests need not sleep.
pub type SuspectClock = Arc<dyn Fn() -> Instant + Send + Sync>;

#[derive(Debug, Clone, Copy)]
struct Suspect {
    wait: Duration,
    next_check: Instant,
}

/// Peer relays that were reported failed and have not been heard from since (issue #516).
///
/// A suspect relay stays in the relay set, so ripples to it resume as soon as it is reachable. It
/// is re-checked with a doubling backoff, and stops being suspect when a re-check finds it
/// Available or when any authenticated message arrives from it. Held by the engine.
pub struct RelaySuspects {
    first_recheck: Duration,
    max_recheck: Duration,
    clock: SuspectClock,
    suspects: Mutex<HashMap<String, Suspect>>,
}

impl RelaySuspects {
    /// A set using the default backoff and the real clock.
    pub fn new() -> Self {
        Self::with_clock(
            RELAY_SUSPECT_FIRST_RECHECK,
            RELAY_SUSPECT_MAX_RECHECK,
            Arc::new(Instant::now),
        )
    }

    /// A set with the given backoff and clock.
    pub fn with_clock(first_recheck: Duration, max_recheck: Duration, clock: SuspectClock) -> Self {
        Self {
            first_recheck,
            max_recheck,
            clock,
            suspects: Mutex::new(HashMap::new()),
        }
    }

    /// Mark `relay_id` suspect, with its first re-check one `first_recheck` from now. Returns
    /// `false`, leaving its backoff as it was, when it is already suspect.
    pub fn mark(&self, relay_id: &str) -> bool {
        let now = (self.clock)();
        let Ok(mut g) = self.suspects.lock() else {
            return false;
        };
        if g.contains_key(relay_id) {
            return false;
        }
        g.insert(
            relay_id.to_string(),
            Suspect {
                wait: self.first_recheck,
                next_check: now + self.first_recheck,
            },
        );
        true
    }

    /// Stop treating `relay_id` as suspect. Returns `true` when it was suspect.
    pub fn clear(&self, relay_id: &str) -> bool {
        self.suspects
            .lock()
            .map(|mut g| g.remove(relay_id).is_some())
            .unwrap_or(false)
    }

    /// Whether `relay_id` is suspect.
    pub fn is_suspect(&self, relay_id: &str) -> bool {
        self.suspects
            .lock()
            .map(|g| g.contains_key(relay_id))
            .unwrap_or(false)
    }

    /// Whether any relay is suspect.
    pub fn is_empty(&self) -> bool {
        self.suspects.lock().map(|g| g.is_empty()).unwrap_or(true)
    }

    /// The suspect relays whose re-check is due, sorted by id.
    pub fn due(&self) -> Vec<String> {
        let now = (self.clock)();
        let Ok(g) = self.suspects.lock() else {
            return Vec::new();
        };
        let mut ids: Vec<String> = g
            .iter()
            .filter(|(_, s)| s.next_check <= now)
            .map(|(id, _)| id.clone())
            .collect();
        ids.sort();
        ids
    }

    /// Record a failed re-check of `relay_id`: double its wait, up to `max_recheck`, and schedule
    /// the next re-check. Returns the new wait, or `None` when it is not suspect.
    pub fn recheck_failed(&self, relay_id: &str) -> Option<Duration> {
        let now = (self.clock)();
        let Ok(mut g) = self.suspects.lock() else {
            return None;
        };
        let suspect = g.get_mut(relay_id)?;
        suspect.wait = (suspect.wait * 2).min(self.max_recheck);
        suspect.next_check = now + suspect.wait;
        Some(suspect.wait)
    }
}

impl Default for RelaySuspects {
    fn default() -> Self {
        Self::new()
    }
}

/// Ask the relay `relay_id` at `addr` for its status with one `DdbGetRelaysStatus`, returning the
/// state it reports. Used to check a relay reported failed (issue #516).
pub fn probe_relay_state(
    api: &dyn crate::api::bingle_api::BingleApiBoth,
    relay_id: &str,
    addr: std::net::SocketAddr,
) -> Result<crate::engine::RelayState, String> {
    use crate::messages::types::{DdbGetRelaysStatus, DdbMessage, Message};
    let request = Message::Ddb(DdbMessage::GetRelaysStatus(DdbGetRelaysStatus {
        app: "ddb".to_string(),
        epoch_id: 0,
        tag: None,
        text: None,
        data: None,
    }));
    let nsk = crate::api::bingle_api::NetworkEndpoint::new_direct(addr);
    let request_json = crate::messages::marshal::to_json_value(&request);
    let response_json = api
        .send_message_to_network_with_response(&nsk, &relay_id.to_string(), request_json, None)
        .map_err(|e| format!("failed to send DdbGetRelaysStatus: {e}"))?;
    match crate::messages::marshal::from_json_value(response_json) {
        Ok(Message::Ddb(DdbMessage::RelaysStatusResponse(status))) => Ok(status.responder_state),
        _ => Err("unexpected response to DdbGetRelaysStatus".to_string()),
    }
}
