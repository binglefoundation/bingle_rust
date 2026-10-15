use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::api::bingle_api::{BingleApiBothType, NetworkEndpoint};
use crate::ddb::AdvertRecord;
use crate::engine::RelayState;
use crate::messages::marshal::{from_json_value, to_json_value};
use crate::messages::types::{
    DdbGetRelaysStatus, DdbMessage, DdbRelaysStatusResponse, Message, RelayReportFailed,
    ReportFailMessage,
};
use crate::relay::relay_finder::{RelayFinderTrait, RelayInfo};
use crate::relay::relay_info_cache::RelayInfoCache;
use crate::relay::relay_probe_failures::RelayProbeFailures;

const LONG_TTL_SECS: u64 = 30_000;
const MEDIUM_TTL_SECS: u64 = 300;
const SHORT_TTL_SECS: u64 = 30;

/// A relay-status check is a local health probe that answers in well under a
/// second when the relay is reachable. Bound it tightly (instead of the ~90s
/// default response timeout) so that when the transport is down the probe fails
/// fast: this keeps the synchronous pending-message drain loop from being wedged
/// for the full default timeout while offline, so it can retry each tick and
/// deliver once connectivity returns (bingle_rust #31/#43).
const RELAY_STATUS_RESPONSE_TIMEOUT: Duration = Duration::from_secs(5);

/// Longest a relay selection waits for its probes (issue #122), however many candidates are
/// dead. It is above the 10s DTLS connect deadline, so a pass whose first-ranked candidate is dead
/// gives up only after that connect has finished. A connect left running could collide with the
/// peer's own connect if it came up meanwhile, failing both handshakes and putting the peer into
/// the engine's send backoff.
pub const RELAY_SELECTION_BUDGET: Duration = Duration::from_secs(12);

/// How long selection waits on a candidate before also probing the next-ranked one (issue #122).
/// A reachable relay answers well inside this, so a live preferred relay is the only one
/// contacted, while each dead one ahead of a live one costs this much rather than a connect
/// timeout. Probing one candidate at a time matters: a client's triangle test needs a corner
/// relay it has not sent to, so selection must not contact more relays than it needs.
pub const RELAY_PREFERENCE_GRACE: Duration = Duration::from_millis(1500);

/// Result of one candidate's status probe, sent back from its probe thread.
type ProbeResult = (usize, Result<DdbRelaysStatusResponse, String>);

pub struct RelayUpdater {
    my_id: String,
    api: Option<BingleApiBothType>,
    relay_info_cache: Arc<RelayInfoCache>,
    discover_roots: Arc<dyn Fn() -> Vec<RelayInfo> + Send + Sync>,
    probe_failures: Arc<RelayProbeFailures>,
    selection_budget: Duration,
    preference_grace: Duration,
}

impl RelayUpdater {
    pub fn new(
        my_id: String,
        discover_roots: Arc<dyn Fn() -> Vec<RelayInfo> + Send + Sync>,
    ) -> Self {
        Self {
            my_id,
            api: None,
            relay_info_cache: Arc::new(RelayInfoCache::new(Vec::new())),
            discover_roots,
            probe_failures: Arc::new(RelayProbeFailures::new()),
            selection_budget: RELAY_SELECTION_BUDGET,
            preference_grace: RELAY_PREFERENCE_GRACE,
        }
    }

    pub fn new_with_api(
        my_id: String,
        api: BingleApiBothType,
        discover_roots: Arc<dyn Fn() -> Vec<RelayInfo> + Send + Sync>,
    ) -> Self {
        Self {
            my_id,
            api: Some(api),
            relay_info_cache: Arc::new(RelayInfoCache::new(Vec::new())),
            discover_roots,
            probe_failures: Arc::new(RelayProbeFailures::new()),
            selection_budget: RELAY_SELECTION_BUDGET,
            preference_grace: RELAY_PREFERENCE_GRACE,
        }
    }

    /// Share a negative cache of unreachable relays with other updaters (issue #122).
    pub fn with_probe_failures(mut self, probe_failures: Arc<RelayProbeFailures>) -> Self {
        self.probe_failures = probe_failures;
        self
    }

    /// Override the selection budget and preference grace (tests).
    pub fn with_selection_timing(mut self, budget: Duration, preference_grace: Duration) -> Self {
        self.selection_budget = budget;
        self.preference_grace = preference_grace;
        self
    }

    pub fn relay_info_cache(&self) -> Arc<RelayInfoCache> {
        self.relay_info_cache.clone()
    }

    pub fn init_from_blockchain(&self) {
        let my_id_norm = self.my_id.trim_end_matches(crate::protocol::ISSUER_SUFFIX);
        let mut relays = (self.discover_roots)();
        relays.sort_by(|left, right| left.id().cmp(right.id()));

        // `discover_roots` returns an empty list when the indexer is unreachable
        // (see indexer_discover_closure — it swallows the error to avoid killing
        // the drain thread). Do NOT wipe the existing relay cache in that case:
        // keeping known relays lets a queued message still deliver over a
        // recovered UDP transport without first waiting for the indexer to come
        // back, decoupling reconnect delivery from indexer availability
        // (bingle_rust #31/#43). A genuine "no relays on chain" is treated the
        // same — an acceptable trade vs. re-wedging delivery on every indexer blip.
        if relays.is_empty() {
            tracing::warn!(
                "[RelayUpdater] discovery returned no relays (indexer unreachable?); keeping existing cache"
            );
            return;
        }

        let updated_relays: Vec<RelayInfo> = relays
            .into_iter()
            .map(|mut relay| {
                let is_own = relay.id() == my_id_norm;
                relay.state = Some(if is_own {
                    RelayState::Own
                } else {
                    RelayState::Unknown
                });
                relay.ttl = Some(if is_own {
                    LONG_TTL_SECS
                } else {
                    SHORT_TTL_SECS
                });
                relay.is_root = true;
                relay
            })
            .collect();

        self.relay_info_cache.replace_relays(updated_relays);
    }

    /// Update stale entries. If no entries are past their expiry (last_updated + ttl), returns
    /// immediately. If any root relay entries are expired, refreshes all roots from the blockchain
    /// via `init_from_blockchain`. Otherwise (only non-root entries expired), queries the
    /// selected relay via `relay_select_and_query` to update non-root state.
    pub fn update_when_expired(&self) {
        let my_id_norm = self.my_id.trim_end_matches(crate::protocol::ISSUER_SUFFIX);
        let now = Instant::now();

        let is_expired = |relay: &RelayInfo| {
            let ttl = Duration::from_secs(relay.ttl.unwrap_or(SHORT_TTL_SECS));
            relay.last_updated + ttl < now
        };

        let all_relays = self.relay_info_cache.list_all_relays(my_id_norm, true);

        let root_expired = all_relays
            .iter()
            .filter(|relay| relay.is_root)
            .any(&is_expired);
        let non_root_expired = all_relays
            .iter()
            .filter(|relay| !relay.is_root)
            .any(is_expired);

        if !root_expired && !non_root_expired {
            return;
        }

        if root_expired {
            self.init_from_blockchain();
        } else {
            self.relay_select_and_query(&[]);
        }
    }

    /// Select a relay to use (issue #122): probe the candidates in rank order and return the
    /// first that answers Available.
    ///
    /// Candidates are ranked by this node's id (preferred, then alternate, then the rest), so
    /// load spreads across relays. The next-ranked candidate is probed as soon as the latest one
    /// fails, or once it has not answered within `preference_grace`, so a dead relay costs at
    /// most the grace rather than a connect timeout. A candidate that recently could not be
    /// reached is skipped for a cooldown, unless every candidate is cooling down. Selection
    /// gives up after `selection_budget`.
    pub fn relay_select_and_query(&self, exclude_ids: &[String]) -> Option<RelayInfo> {
        let my_id_norm = self.my_id.trim_end_matches(crate::protocol::ISSUER_SUFFIX);
        let mut candidates = self.relay_info_cache.list_all_relays(my_id_norm, true);
        candidates.retain(|relay| {
            relay.state != Some(RelayState::Own)
                && relay.id() != my_id_norm
                && !exclude_ids.contains(&relay.id().to_string())
        });
        candidates.sort_by(|left, right| left.id().cmp(right.id()));
        // A relay listed twice must not be probed twice.
        candidates.dedup_by(|left, right| left.id() == right.id());

        if candidates.is_empty() {
            return None;
        }

        let ranked: Vec<RelayInfo> = self
            .selection_order(&candidates, my_id_norm)
            .into_iter()
            .filter_map(|index| candidates.get(index).cloned())
            .collect();
        let mut probed: Vec<RelayInfo> = ranked
            .iter()
            .filter(|relay| !self.probe_failures.is_cooling_down(relay.id()))
            .cloned()
            .collect();
        if probed.is_empty() {
            // Every candidate failed recently. Probe them anyway rather than strand the node.
            tracing::info!(
                "[RelayUpdater::relay_select_and_query] all {} candidate(s) recently unreachable; probing anyway",
                ranked.len()
            );
            probed = ranked;
        }

        let (sender, receiver) = std::sync::mpsc::channel::<ProbeResult>();
        let deadline = Instant::now() + self.selection_budget;
        let mut started = 0usize;
        let mut resolved = 0usize;
        let mut next_start_at = Instant::now();
        let mut failed_relay_ids: Vec<String> = Vec::new();

        loop {
            let now = Instant::now();
            // Start the next candidate when every started one has failed, or the latest has
            // not answered within the grace.
            if started < probed.len() && (resolved == started || now >= next_start_at) {
                self.spawn_probe(started, &probed[started], &sender);
                started += 1;
                next_start_at = now + self.preference_grace;
            }
            if resolved == started {
                // Every candidate has been probed and none answered Available.
                return None;
            }
            if now >= deadline {
                tracing::info!(
                    "[RelayUpdater::relay_select_and_query] no relay answered Available within {:?}",
                    self.selection_budget
                );
                return None;
            }

            let mut wake = deadline;
            if started < probed.len() {
                wake = wake.min(next_start_at);
            }
            let (rank, result) =
                match receiver.recv_timeout(wake.saturating_duration_since(Instant::now())) {
                    Ok(received) => received,
                    Err(_) => continue,
                };
            resolved += 1;
            let candidate = &probed[rank];
            match result {
                Ok(status) if status.responder_state == RelayState::Available => {
                    self.update_cached_relay_state(candidate.id(), status.responder_state);
                    if let Some(relay) = self.accept_available(candidate, &status, my_id_norm) {
                        self.report_failed_relays(&relay, &failed_relay_ids);
                        return Some(relay);
                    }
                    failed_relay_ids.push(candidate.id().to_string());
                }
                Ok(status) => {
                    self.update_cached_relay_state(candidate.id(), status.responder_state);
                    failed_relay_ids.push(candidate.id().to_string());
                }
                Err(err) => {
                    tracing::info!(
                        "[RelayUpdater::relay_select_and_query] relay {} unreachable: {}",
                        candidate.id(),
                        err
                    );
                    self.update_cached_relay_state(candidate.id(), RelayState::Off);
                    failed_relay_ids.push(candidate.id().to_string());
                }
            }
        }
    }

    /// Probe one relay's status on its own thread, reporting `(rank, result)` on `sender`. A probe
    /// that fails to reach its relay records it in the negative cache, even if selection has
    /// already finished by then.
    fn spawn_probe(
        &self,
        rank: usize,
        relay: &RelayInfo,
        sender: &std::sync::mpsc::Sender<ProbeResult>,
    ) {
        let probe_sender = sender.clone();
        let api = self.api.clone();
        let probe_relay = relay.clone();
        let failures = self.probe_failures.clone();
        let spawned = std::thread::Builder::new()
            .name("relay-probe".to_string())
            .spawn(move || {
                let result = Self::query_relay_status(api.as_ref(), &probe_relay);
                match &result {
                    Ok(_) => failures.record_success(probe_relay.id()),
                    Err(_) => failures.record_failure(probe_relay.id()),
                }
                let _ = probe_sender.send((rank, result));
            });
        if let Err(err) = spawned {
            tracing::error!(
                "[RelayUpdater::spawn_probe] could not spawn probe for {}: {}",
                relay.id(),
                err
            );
            let _ = sender.send((rank, Err(format!("probe not started: {err}"))));
        }
    }

    /// Take an Available candidate: refresh the relay cache from its status and return the relay,
    /// or `None` (marking it Off) when its status cannot be used.
    fn accept_available(
        &self,
        candidate: &RelayInfo,
        status_response: &DdbRelaysStatusResponse,
        my_id_norm: &str,
    ) -> Option<RelayInfo> {
        if !self.update_cache_from_status(status_response, my_id_norm) {
            self.update_cached_relay_state(candidate.id(), RelayState::Off);
            return None;
        }

        let selected = self
            .relay_info_cache
            .list_all_relays(my_id_norm, true)
            .into_iter()
            .find(|relay| relay.id() == candidate.id());
        if let Some(relay) = selected {
            return Some(relay);
        }

        let mut relay = candidate.clone();
        relay.is_root = true;
        relay.state = Some(RelayState::Available);
        relay.ttl = Some(MEDIUM_TTL_SECS);
        Some(relay)
    }

    fn report_failed_relays(&self, selected_relay: &RelayInfo, failed_relay_ids: &[String]) {
        if failed_relay_ids.is_empty() {
            return;
        }
        let Some(api) = &self.api else {
            return;
        };
        let timestamp = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        let nsk = NetworkEndpoint::new_direct(selected_relay.address());
        for failed_id in failed_relay_ids {
            let report =
                Message::ReportFail(ReportFailMessage::RelayReportFailed(RelayReportFailed {
                    app: "reportFail".to_string(),
                    tag: None,
                    response_tag: None,
                    failed_relay_id: failed_id.clone(),
                    fail_type: "send_rejected".to_string(),
                    timestamp: timestamp.clone(),
                }));
            if let Some(api_ref) = api.upgrade() {
                let _ = api_ref.send_message_to_network(
                    &nsk,
                    &selected_relay.id().to_string(),
                    to_json_value(&report),
                    None,
                );
            } else {
                tracing::error!("[report_failed_relays] could not upgrade api to send message");
            }
        }
    }

    /// Rank every candidate: the preferred relay for this node's id, then the alternate, then
    /// the rest in order after them, so load spreads across relays.
    fn selection_order(&self, relays: &[RelayInfo], my_id: &str) -> Vec<usize> {
        let n = relays.len();
        if n == 0 {
            return Vec::new();
        }
        let (preferred, _alternate) = self.select_indices_internal(relays, my_id);
        (0..n).map(|offset| (preferred + offset) % n).collect()
    }

    fn select_indices_internal(&self, relays: &[RelayInfo], my_id: &str) -> (usize, usize) {
        let n = relays.len();
        if n == 1 {
            return (0usize, 0usize);
        }

        let bucket = self.id_bucket_u32(my_id);
        let span = u64::from(u32::MAX) + 1;
        let partition_size = span / (n as u64);
        let index = ((bucket as u64) / partition_size) as usize % n;
        let alternate = (index + 1) % n;
        (index, alternate)
    }

    fn id_bucket_u32(&self, id: &str) -> u32 {
        if let Ok(bytes) = algo_ops::address_to_byte_key(id) {
            return u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        }

        use sha2::{Digest, Sha512_256};
        let mut hasher = Sha512_256::new();
        hasher.update(id.as_bytes());
        let digest: [u8; 32] = hasher.finalize().into();
        u32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]])
    }

    fn query_relay_status(
        api: Option<&BingleApiBothType>,
        relay: &RelayInfo,
    ) -> Result<DdbRelaysStatusResponse, String> {
        let Some(api) = api else {
            return Err("api unavailable".to_string());
        };

        let request = Message::Ddb(DdbMessage::GetRelaysStatus(DdbGetRelaysStatus {
            app: "ddb".to_string(),
            epoch_id: 0,
            tag: None,
            text: None,
            data: None,
        }));

        let nsk = NetworkEndpoint::new_direct(relay.address());
        let api_ref = api.upgrade().ok_or_else(|| {
            let err = "api dropped".to_string();
            tracing::error!("[RelayUpdater::query_relay_status] {}", err);
            err
        })?;
        let response = api_ref
            .send_message_to_network_with_response_timeout(
                &nsk,
                &relay.id().to_string(),
                to_json_value(&request),
                None,
                RELAY_STATUS_RESPONSE_TIMEOUT,
            )
            .map_err(|err| err.to_string())?;

        let parsed = from_json_value(response).map_err(|err| format!("{err:?}"))?;
        match parsed {
            Message::Ddb(DdbMessage::RelaysStatusResponse(status_response)) => Ok(status_response),
            _ => Err("unexpected response type".to_string()),
        }
    }

    fn update_cache_from_status(&self, status: &DdbRelaysStatusResponse, my_id_norm: &str) -> bool {
        let Some(endpoints) = &status.relay_endpoints else {
            return false;
        };

        if status.relay_ids.len() != status.relay_states.len()
            || status.relay_ids.len() != endpoints.len()
        {
            return false;
        }

        let mut updated = Vec::with_capacity(status.relay_ids.len());
        for index in 0..status.relay_ids.len() {
            let id = status.relay_ids[index].clone();
            let Ok(_address) = std::net::SocketAddr::try_from(endpoints[index].clone()) else {
                return false;
            };

            let state = if id == my_id_norm {
                RelayState::Own
            } else {
                status.relay_states[index]
            };

            let record = AdvertRecord::new_unsigned(
                id,
                Some(endpoints[index].clone()),
                Some(true),
                None,
                None,
                chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            );
            updated.push(RelayInfo::root_with(
                record,
                Some(state),
                Some(Self::ttl_for_state(state)),
            ));
        }

        self.relay_info_cache.replace_relays(updated);
        true
    }

    fn update_cached_relay_state(&self, relay_id: &str, state: RelayState) {
        let my_id_norm = self.my_id.trim_end_matches(crate::protocol::ISSUER_SUFFIX);
        let mut relays = self.relay_info_cache.list_all_relays(my_id_norm, true);
        if let Some(relay) = relays.iter_mut().find(|relay| relay.id() == relay_id) {
            relay.state = Some(state);
            relay.ttl = Some(Self::ttl_for_state(state));
            self.relay_info_cache.replace_relays(relays);
        }
    }

    fn ttl_for_state(state: RelayState) -> u64 {
        match state {
            RelayState::Own => LONG_TTL_SECS,
            RelayState::Available => MEDIUM_TTL_SECS,
            _ => SHORT_TTL_SECS,
        }
    }
}
