use std::net::SocketAddr;
use std::sync::Mutex;

use crate::api::bingle_api::NetworkEndpoint;
use crate::relay::relay_finder::{RelayFinderTrait, RelayInfo};

/// In-memory relay cache used by relay-finder refactors and tests.
/// Stores a mutable list of relay entries and exposes simple maintenance operations.
pub struct RelayInfoCache {
    relays: Mutex<Vec<RelayInfo>>,
}

impl RelayInfoCache {
    pub fn new(relays: Vec<RelayInfo>) -> Self {
        Self {
            relays: Mutex::new(relays),
        }
    }

    pub fn add_relay(&self, relay: RelayInfo) -> bool {
        if let Ok(mut relays) = self.relays.lock() {
            if relays.iter().any(|entry| entry.id == relay.id) {
                return false;
            }
            relays.push(relay);
            relays.sort_by(|left, right| left.id.cmp(&right.id));
            return true;
        }
        false
    }

    pub fn update_relay(&self, relay: RelayInfo) -> bool {
        if let Ok(mut relays) = self.relays.lock() {
            if let Some(index) = relays.iter().position(|entry| entry.id == relay.id) {
                relays[index] = relay.with_updated();
                relays.sort_by(|left, right| left.id.cmp(&right.id));
                return true;
            }
        }
        false
    }

    pub fn delete_relay(&self, relay_id: &str) -> bool {
        if let Ok(mut relays) = self.relays.lock() {
            let original_len = relays.len();
            relays.retain(|entry| entry.id != relay_id);
            return relays.len() != original_len;
        }
        false
    }

    pub fn is_empty(&self) -> bool {
        self.relays.lock().map(|guard| guard.is_empty()).unwrap_or(true)
    }

    pub fn replace_relays(&self, relays: Vec<RelayInfo>) {
        if let Ok(mut guard) = self.relays.lock() {
            *guard = relays;
            guard.sort_by(|left, right| left.id.cmp(&right.id));
        }
    }

    fn snapshot_relays(&self) -> Vec<RelayInfo> {
        self.relays.lock().map(|guard| guard.clone()).unwrap_or_default()
    }
}

impl RelayFinderTrait for RelayInfoCache {
    fn list_all_relays(&self, my_id: &str, include_self: bool) -> Vec<RelayInfo> {
        let my_id_norm = my_id.trim_end_matches(crate::protocol::ISSUER_SUFFIX);
        let mut relays = self.snapshot_relays();
        if !include_self {
            relays.retain(|relay| relay.id != my_id_norm);
        }
        relays.sort_by(|left, right| left.id.cmp(&right.id));
        relays
    }

    fn find_relay(&self, my_id: &str) -> Result<RelayInfo, String> {
        self.find_relay_excluding(my_id, &[])
    }

    fn find_relay_excluding(
        &self,
        my_id: &str,
        exclude: &[SocketAddr],
    ) -> Result<RelayInfo, String> {
        let my_id_norm = my_id.trim_end_matches(crate::protocol::ISSUER_SUFFIX);
        let mut relays = self.snapshot_relays();
        relays.retain(|relay| relay.id != my_id_norm && !exclude.contains(&relay.address));
        relays.sort_by(|left, right| left.id.cmp(&right.id));
        if let Some(relay) = relays.into_iter().next() {
            return Ok(relay);
        }
        Err("no relays discovered (after excluding self and specified endpoints)".to_string())
    }

    fn list_root_relays(&self, my_id: &str, include_self: bool) -> Vec<RelayInfo> {
        let my_id_norm = my_id.trim_end_matches(crate::protocol::ISSUER_SUFFIX);
        let mut relays = self.snapshot_relays();
        relays.retain(|relay| relay.is_root);
        if !include_self {
            relays.retain(|relay| relay.id != my_id_norm);
        }
        relays.sort_by(|left, right| left.id.cmp(&right.id));
        relays
    }

    fn clear_state_cache(&self) {
        if let Ok(mut relays) = self.relays.lock() {
            for relay in &mut *relays {
                relay.state = None;
            }
        }
    }

    fn lookup_root_id(&self, id: &str) -> Option<NetworkEndpoint> {
        let id_norm = id.trim_end_matches(crate::protocol::ISSUER_SUFFIX);
        self.snapshot_relays()
            .into_iter()
            .find(|relay| relay.id == id_norm && relay.is_root)
            .map(|relay| NetworkEndpoint::new_direct(relay.address))
    }

    fn remove_relay(&self, relay_id: &str) {
        self.delete_relay(relay_id);
    }
}