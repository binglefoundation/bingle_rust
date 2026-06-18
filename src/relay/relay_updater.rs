use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::api::bingle_api::{BingleApiBothType, NetworkEndpoint};
use crate::engine::RelayState;
use crate::messages::marshal::{from_json_value, to_json_value};
use crate::messages::types::{DdbGetRelaysStatus, DdbMessage, DdbRelaysStatusResponse, Message, RelayReportFailed, ReportFailMessage};
use crate::relay::relay_finder::{RelayFinderTrait, RelayInfo};
use crate::relay::relay_info_cache::RelayInfoCache;
use crate::ddb::AdvertRecord;

const LONG_TTL_SECS: u64 = 30_000;
const MEDIUM_TTL_SECS: u64 = 300;
const SHORT_TTL_SECS: u64 = 30;

pub struct RelayUpdater {
    my_id: String,
    api: Option<BingleApiBothType>,
    relay_info_cache: Arc<RelayInfoCache>,
    discover_roots: Arc<dyn Fn() -> Vec<RelayInfo> + Send + Sync>,
}

impl RelayUpdater {
    pub fn new(my_id: String, discover_roots: Arc<dyn Fn() -> Vec<RelayInfo> + Send + Sync>) -> Self {
        Self {
            my_id,
            api: None,
            relay_info_cache: Arc::new(RelayInfoCache::new(Vec::new())),
            discover_roots,
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
        }
    }

    pub fn relay_info_cache(&self) -> Arc<RelayInfoCache> {
        self.relay_info_cache.clone()
    }

    pub fn init_from_blockchain(&self) {
        let my_id_norm = self.my_id.trim_end_matches(crate::protocol::ISSUER_SUFFIX);
        let mut relays = (self.discover_roots)();
        relays.sort_by(|left, right| left.id().cmp(right.id()));

        let updated_relays: Vec<RelayInfo> = relays
            .into_iter()
            .map(|mut relay| {
                let is_own = relay.id() == my_id_norm;
                relay.state = Some(if is_own { RelayState::Own } else { RelayState::Unknown });
                relay.ttl = Some(if is_own { LONG_TTL_SECS } else { SHORT_TTL_SECS });
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

        let root_expired = all_relays.iter().filter(|relay| relay.is_root).any(|relay| is_expired(relay));
        let non_root_expired = all_relays.iter().filter(|relay| !relay.is_root).any(|relay| is_expired(relay));

        if !root_expired && !non_root_expired {
            return;
        }

        if root_expired {
            self.init_from_blockchain();
        } else {
            self.relay_select_and_query(&[]);
        }
    }

    pub fn relay_select_and_query(&self, exclude_ids: &[String]) -> Option<RelayInfo> {
        let my_id_norm = self.my_id.trim_end_matches(crate::protocol::ISSUER_SUFFIX);
        let mut candidates = self.relay_info_cache.list_all_relays(my_id_norm, true);
        candidates.retain(|relay| {
            relay.state != Some(RelayState::Own)
                && relay.id() != my_id_norm
                && !exclude_ids.contains(&relay.id().to_string())
        });
        candidates.sort_by(|left, right| left.id().cmp(right.id()));

        if candidates.is_empty() {
            return None;
        }

        let mut failed_relay_ids: Vec<String> = Vec::new();
        let selection_order = self.selection_order(&candidates, my_id_norm);
        for index in selection_order {
            let Some(candidate) = candidates.get(index).cloned() else {
                continue;
            };

            match self.query_relay_status(&candidate) {
                Ok(status_response) => {
                    self.update_cached_relay_state(candidate.id(), status_response.responder_state);
                    if status_response.responder_state != RelayState::Available {
                        failed_relay_ids.push(candidate.id().to_string());
                        continue;
                    }

                    if !self.update_cache_from_status(&status_response, my_id_norm) {
                        self.update_cached_relay_state(candidate.id(), RelayState::Off);
                        failed_relay_ids.push(candidate.id().to_string());
                        continue;
                    }

                    let selected = self
                        .relay_info_cache
                        .list_all_relays(my_id_norm, true)
                        .into_iter()
                        .find(|relay| relay.id() == candidate.id());

                    if let Some(relay) = selected {
                        self.report_failed_relays(&relay, &failed_relay_ids);
                        return Some(relay);
                    }

                    let mut relay = candidate.clone();
                    relay.is_root = true;
                    relay.state = Some(RelayState::Available);
                    relay.ttl = Some(MEDIUM_TTL_SECS);
                    self.report_failed_relays(&relay, &failed_relay_ids);
                    return Some(relay);
                }
                Err(_) => {
                    self.update_cached_relay_state(candidate.id(), RelayState::Off);
                    failed_relay_ids.push(candidate.id().to_string());
                }
            }
        }

        None
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
            let report = Message::ReportFail(ReportFailMessage::RelayReportFailed(RelayReportFailed {
                app: "reportFail".to_string(),
                tag: None,
                response_tag: None,
                failed_relay_id: failed_id.clone(),
                fail_type: "send_rejected".to_string(),
                timestamp: timestamp.clone(),
            }));
            if let Some(api_ref) = api.upgrade() {
                api_ref.send_message_to_network(&nsk, &selected_relay.id().to_string(), to_json_value(&report), None)
            }
            else {
                tracing::error!("[report_failed_relays] could not upgrade api to send message");
            }
        }
    }

    fn selection_order(&self, relays: &[RelayInfo], my_id: &str) -> Vec<usize> {
        if relays.is_empty() {
            return Vec::new();
        }

        if relays.len() == 1 {
            return vec![0];
        }

        let (preferred, alternate) = self.select_indices_internal(relays, my_id);
        if preferred == alternate {
            vec![preferred]
        } else {
            vec![preferred, alternate]
        }
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
        if let Ok(bytes) = crate::blockchain::algo_ops::address_to_byte_key(id) {
            return u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        }

        use sha2::{Digest, Sha512_256};
        let mut hasher = Sha512_256::new();
        hasher.update(id.as_bytes());
        let digest: [u8; 32] = hasher.finalize().into();
        u32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]])
    }

    fn query_relay_status(&self, relay: &RelayInfo) -> Result<DdbRelaysStatusResponse, String> {
        let Some(api) = &self.api else {
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
        let api_ref = api.upgrade().ok_or_else(|| "api dropped".to_string())?;
        let response = api_ref
            .send_message_to_network_with_response(&nsk, &relay.id().to_string(), to_json_value(&request), None)
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

        if status.relay_ids.len() != status.relay_states.len() || status.relay_ids.len() != endpoints.len() {
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