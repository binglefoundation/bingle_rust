use std::sync::Arc;

use crate::engine::RelayState;
use crate::relay::relay_finder::RelayInfo;
use crate::relay::relay_info_cache::RelayInfoCache;

const LONG_TTL_SECS: u64 = 30_000;
const SHORT_TTL_SECS: u64 = 30;

pub struct RelayUpdater {
    my_id: String,
    relay_info_cache: Arc<RelayInfoCache>,
    discover_roots: Arc<dyn Fn() -> Vec<RelayInfo> + Send + Sync>,
}

impl RelayUpdater {
    pub fn new(my_id: String, discover_roots: Arc<dyn Fn() -> Vec<RelayInfo> + Send + Sync>) -> Self {
        Self {
            my_id,
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
        relays.sort_by(|left, right| left.id.cmp(&right.id));

        let updated_relays: Vec<RelayInfo> = relays
            .into_iter()
            .map(|relay| {
                let is_own = relay.id == my_id_norm;
                RelayInfo::root_with(
                    relay.id,
                    relay.address,
                    Some(if is_own { RelayState::Own } else { RelayState::Unknown }),
                    Some(if is_own { LONG_TTL_SECS } else { SHORT_TTL_SECS }),
                )
            })
            .collect();

        self.relay_info_cache.replace_relays(updated_relays);
    }
}