use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use serde_json::Value as JsonValue;

use crate::api::bingle_api::{BingleError, NetworkEndpoint};
use crate::messages::types::*;
use crate::messages::{to_json_value, Message};
use crate::engine::BingleAccess;
use crate::relay::relay_finder::{RelayInfo, RelayFinderTrait};

/// Public DDB client interface used by higher layers.
/// Provides register_ip and lookup helpers that send DDB messages over the network
/// and validate typed responses.
pub trait DdbClient: Send + Sync {
    /// Register/update our endpoint IP:port in the DDB via a relay.
    fn register_ip(&self, endpoint: SocketAddr, am_relay: bool) -> Result<(), BingleError>;
    /// Alias with camelCase to match external nomenclature.
    #[allow(non_snake_case)]
    fn registerIP(&self, endpoint: SocketAddr, am_relay: bool) -> Result<(), BingleError> { self.register_ip(endpoint, am_relay) }

    /// Register/update our chosen relay association in the DDB via a relay.
    /// relay_id must be a valid Algorand base32 address (36-byte decoded).
    fn register_relay(&self, relay_id: String, relay_sig: Option<String>) -> Result<(), BingleError>;

    /// Lookup an id in the DDB and build a NetworkSourceKey from its AdvertRecord.
    fn lookup(&self, id: &str) -> Result<NetworkEndpoint, BingleError>;

    /// Start loading the DDB from a peer relay by sending DdbInitResolve.
    /// Returns the number of records reported by DdbInitResponse (dbCount) on success.
    fn start_load_from_peer(&self, peer_id: &str) -> Result<usize, BingleError>;
}

pub struct DdbClientImpl {
    api: crate::api::bingle_api::BingleApiBothType,
    discover: Arc<dyn Fn() -> Vec<RelayInfo> + Send + Sync>,
}

impl DdbClientImpl {
    /// Create a DdbClientImpl using indexer-based discovery (requires app_id configured on API or via env BINGLE_APP_ID).
    pub fn new(api: crate::api::bingle_api::BingleApiBothType, app_id: u64, cfg: Option<crate::blockchain::algo_ops::AlgoChainConfig>) -> Self {
        let discover = crate::relay::discovery::indexer_discover_closure(app_id, cfg);
        Self { api, discover }
    }

    /// Constructor that accepts a custom discovery closure (used by tests to avoid external dependencies).
    pub fn with_discovery(api: crate::api::bingle_api::BingleApiBothType, discover: Arc<dyn Fn() -> Vec<RelayInfo> + Send + Sync>) -> Self {
        Self { api, discover }
    }

    fn find_relay(&self) -> Result<RelayInfo, BingleError> {
        tracing::debug!("[DdbClientImpl::find_relay] create RelayFinder");
        let finder = crate::relay::relay_finder::RelayFinder::new(self.api.clone(), self.discover.clone());
        tracing::trace!("[DdbClientImpl::find_relay] get_my_id");
        let my_id = self.api.access(|a| a.get_my_id()).ok_or_else(|| BingleError::Other("get_my_id returned None".to_string()))?;
        tracing::trace!("[DdbClientImpl::find_relay] RelayFinder::find_relay");
        finder.find_relay(&my_id).map_err(BingleError::Other)
    }

    fn relay_user_id(relay_id: &str) -> Result<String, BingleError> {
        // Validate Algorand base32 address by decoding to 36 bytes
        match data_encoding::BASE32_NOPAD.decode(relay_id.as_bytes()) {
            Ok(bytes) if bytes.len() == 36 => Ok(relay_id.to_string()),
            Ok(bytes) => Err(BingleError::Other(format!("invalid relay id: base32 decoded length {} != 36", bytes.len()))),
            Err(e) => Err(BingleError::Other(format!("invalid relay id: {}", e))),
        }
    }

    /// Send a single getRelaysStatus request to the specified relay and parse the relay list.
    /// This avoids probing multiple roots and is used by RelayFinder::list_all_relays.
    pub fn get_relays_from(&self, relay: &RelayInfo) -> Result<Vec<(String, SocketAddr)>, BingleError> {
        tracing::debug!("[DddClientImpl::get_relays_from] relay: {:?}", relay);
        let nsk = NetworkEndpoint::new_direct(relay.address);
        let relay_user = match Self::relay_user_id(&relay.id) {
            Ok(u) => u,
            Err(e) => {
                tracing::error!("[DdbClientImpl::get_relays_from] relay_user_id failed: {}", e);
                return Err(e);
            }
        };
        let req = Message::Ddb(DdbMessage::GetRelaysStatus(DdbGetRelaysStatus { app: "ddb".into(), epoch_id: -1, tag: None, text: None, data: None }));
        let json: JsonValue = to_json_value(&req);
        let resp = match self.api.access(|a| a.send_message_to_network_with_response(&nsk, &relay_user, json, None)) {
            Ok(r) => r,
            Err(e) => {
                tracing::error!("[DdbClientImpl::get_relays_from] send_message_to_network_with_response failed: {}", e);
                return Err(e);
            }
        };
        let app_ok = resp.get("app").and_then(|v| v.as_str()) == Some("ddb");
        let ty_ok = resp.get("type").and_then(|v| v.as_str()) == Some("relaysStatusResponse");
        if !app_ok || !ty_ok { return Err(BingleError::Other("unexpected response to getRelaysStatus".to_string())); }
        let ids_arr = match resp.get("relayIds").and_then(|v| v.as_array()) {
            Some(a) => a,
            None => {
                let err = "missing relayIds".to_string();
                tracing::error!("[DdbClientImpl::get_relays_from] {}", err);
                return Err(BingleError::Other(err));
            }
        };
        let ids: Vec<String> = ids_arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
        let eps_opt = resp.get("relayEndpoints").and_then(|v| v.as_array());
        if let Some(eps) = eps_opt {
            if eps.len() == ids.len() {
                let mut out: Vec<(String, SocketAddr)> = Vec::new();
                for (i, id) in ids.iter().enumerate() {
                    let ep = &eps[i];
                    let host = match ep.get("host").and_then(|v| v.as_str()) {
                        Some(h) => h,
                        None => {
                            let err = "endpoint missing host".to_string();
                            tracing::error!("[DdbClientImpl::get_relays_from] {}", err);
                            return Err(BingleError::Other(err));
                        }
                    };
                    let port = match ep.get("port").and_then(|v| v.as_u64()) {
                        Some(p) => p as u16,
                        None => {
                            let err = "endpoint missing port".to_string();
                            tracing::error!("[DdbClientImpl::get_relays_from] {}", err);
                            return Err(BingleError::Other(err));
                        }
                    };
                    if let Ok(ip) = host.parse::<IpAddr>() { out.push((id.clone(), SocketAddr::new(ip, port))); }
                }
                return Ok(out);
            }
        }
        // If endpoints are absent or misaligned, return empty to allow caller fallback
        Ok(Vec::new())
    }
}

pub struct NullDdbClient;

impl NullDdbClient {
    pub fn new() -> Self { Self }
}

impl DdbClient for NullDdbClient {
    fn register_ip(&self, _endpoint: SocketAddr, _am_relay: bool) -> Result<(), BingleError> {
        Err(BingleError::Other("DDB client not configured (missing app_id or unsupported platform)".to_string()))
    }
    fn register_relay(&self, _relay_id: String, _relay_sig: Option<String>) -> Result<(), BingleError> {
        Err(BingleError::Other("DDB client not configured (missing app_id or unsupported platform)".to_string()))
    }
    fn lookup(&self, _id: &str) -> Result<NetworkEndpoint, BingleError> {
        Err(BingleError::Other("DDB client not configured (missing app_id or unsupported platform)".to_string()))
    }
    fn start_load_from_peer(&self, _peer_id: &str) -> Result<usize, BingleError> {
        Err(BingleError::Other("DDB client not configured (missing app_id or unsupported platform)".to_string()))
    }
}

impl DdbClient for DdbClientImpl {
    fn start_load_from_peer(&self, peer_id: &str) -> Result<usize, BingleError> {
        tracing::debug!("[DdbClientImpl::start_load_from_peer][enter] peer_id={}", peer_id);
        // Resolve the peer relay's direct endpoint using RelayFinder first, then fall back to DDB lookup
        let finder = crate::relay::relay_finder::RelayFinder::new(self.api.clone(), self.discover.clone());
        let mut nsk_opt = finder.lookup_root_id(peer_id);
        if nsk_opt.is_none() {
            // Fallback to DDB lookup which may return a direct endpoint
            match self.lookup(peer_id) {
                Ok(nsk) => nsk_opt = Some(nsk),
                Err(e) => {
                    let err = format!("failed to resolve peer '{}' via roots or DDB: {}", peer_id, e);
                    tracing::warn!("[DdbClientImpl::start_load_from_peer] {}", err);
                    return Err(e);
                }
            }
        }
        let nsk = nsk_opt.expect("nsk set above");

        let addr = match nsk.inet_socket_address() {
            Some(a) => a,
            None => {
                let err = "peer NetworkEndpoint is not a direct address".to_string();
                tracing::error!("[DdbClientImpl::start_load_from_peer] {}", err);
                return Err(BingleError::Other(err));
            }
        };

        let relay_user = match Self::relay_user_id(peer_id) {
            Ok(u) => u,
            Err(e) => {
                tracing::error!("[DdbClientImpl::start_load_from_peer] {}", e);
                return Err(e);
            }
        };

        // Compose InitResolve
        let init = Message::Ddb(DdbMessage::InitResolve(DdbInitResolve { app: "ddb".into(), tag: None, response_tag: None, text: None, data: None }));
        let json: JsonValue = to_json_value(&init);

        // Send and await InitResponse
        let nsk_direct = NetworkEndpoint::new_direct(addr);
        let resp: serde_json::Value = match self.api.access(|a| a.send_message_to_network_with_response(&nsk_direct, &relay_user, json, None)) {
            Ok(r) => r,
            Err(e) => {
                tracing::error!("[DdbClientImpl::start_load_from_peer] send_message_to_network_with_response failed: {}", e);
                return Err(e);
            }
        };

        let app_ok = resp.get("app").and_then(|v| v.as_str()) == Some("ddb");
        let ty_ok = resp.get("type").and_then(|v| v.as_str()) == Some("initResponse");
        if !app_ok || !ty_ok {
            return Err(BingleError::Other("unexpected response (expected DdbInitResponse)".to_string()));
        }

        let cnt = match resp.get("dbCount").and_then(|v| v.as_i64()) {
            Some(c) => c,
            None => {
                let err = "missing dbCount".to_string();
                tracing::error!("[DdbClientImpl::start_load_from_peer] {}", err);
                return Err(BingleError::Other(err));
            }
        };

        let out = match usize::try_from(cnt) {
            Ok(u) => u,
            Err(_) => {
                let err = "dbCount out of range".to_string();
                tracing::error!("[DdbClientImpl::start_load_from_peer] {}", err);
                return Err(BingleError::Other(err));
            }
        };

        tracing::debug!("[DdbClientImpl::start_load_from_peer][exit] Ok({})", out);
        Ok(out)
    }

    fn register_ip(&self, endpoint: SocketAddr, am_relay: bool) -> Result<(), BingleError> {
        // 1) Find relay to talk to
        let relay = match self.find_relay() {
            Ok(r) => r,
            Err(e) => {
                tracing::error!("[DdbClientImpl::register_ip] find_relay failed: {}", e);
                return Err(e);
            }
        };
        let relay_user_b64 = match Self::relay_user_id(&relay.id) {
            Ok(u) => u,
            Err(e) => {
                tracing::error!("[DdbClientImpl::register_ip] relay_user_id failed: {}", e);
                return Err(e);
            }
        };
        let nsk = NetworkEndpoint::new_direct(relay.address);

        // 2) Build UpsertResolve using our id as startId and record.id
        let my_id = match self.api.access(|a| a.get_my_id()) {
            Some(id) => id,
            None => {
                let err = "get_my_id returned None".to_string();
                tracing::error!("[DdbClientImpl::register_ip] {}", err);
                return Err(BingleError::Other(err));
            }
        };
        let date = "1970-01-01T00:00:00Z".to_string();
        let record = if let Some(sk) = self.api.access(|a| a.get_signing_key()) {
            AdvertRecord::new(
                my_id.clone(),
                Some(InetSocketAddress::from(endpoint)),
                Some(am_relay),
                None,
                None,
                date,
                &sk,
            )
        } else {
            AdvertRecord::new_unsigned(
                my_id.clone(),
                Some(InetSocketAddress::from(endpoint)),
                Some(am_relay),
                None,
                None,
                date,
            )
        };
        let up = Message::Ddb(DdbMessage::UpsertResolve(DdbUpsertResolve {
            app: "ddb".to_string(),
            start_id: my_id,
            epoch: 1,
            record,
            original_signature: "SIG".to_string(),
            rippled: false,
            tag: None,
            response_tag: None,
            text: None,
            data: None,
        }));
        let json: JsonValue = to_json_value(&up);

        // 3) Send and wait for response; validate UpdateResponse
        tracing::debug!("[DdbClientImpl::register_ip] sending DdbUpsertResolve: {:?}", json);
        let resp = match self.api.access(|a| a.send_message_to_network_with_response(&nsk, &relay_user_b64, json, None)) {
            Ok(r) => r,
            Err(e) => {
                tracing::error!("[DdbClientImpl::register_ip] send_message_to_network_with_response failed: {}", e);
                return Err(e);
            }
        };
        tracing::debug!("[DdbClientImpl::register_ip] received DdbUpdateResponse: {:?}", resp);

        let app_ok = resp.get("app").and_then(|v| v.as_str()) == Some("ddb");
        let ty_ok = resp.get("type").and_then(|v| v.as_str()) == Some("updateResponse");
        if app_ok && ty_ok { Ok(()) } else { Err(BingleError::Other("unexpected response (expected DdbUpdateResponse)".to_string())) }
    }

    fn register_relay(&self, relay_id: String, relay_sig: Option<String>) -> Result<(), BingleError> {
        // 1) Find relay to talk to (we also accept the relay_id parameter for the record)
        let relay = match self.find_relay() {
            Ok(r) => r,
            Err(e) => {
                tracing::error!("[DdbClientImpl::register_relay] find_relay failed: {}", e);
                return Err(e);
            }
        };
        let relay_user_b64 = match Self::relay_user_id(&relay.id) {
            Ok(u) => u,
            Err(e) => {
                tracing::error!("[DdbClientImpl::register_relay] relay_user_id (root) failed: {}", e);
                return Err(e);
            }
        };
        // validate provided relay_id shape
        if let Err(e) = Self::relay_user_id(&relay_id) {
            tracing::error!("[DdbClientImpl::register_relay] relay_user_id (param) failed: {}", e);
            return Err(e);
        }
        let nsk = NetworkEndpoint::new_direct(relay.address);

        // 2) Build UpsertResolve using our id as startId and record.id
        let my_id = match self.api.access(|a| a.get_my_id()) {
            Some(id) => id,
            None => {
                let err = "get_my_id returned None".to_string();
                tracing::error!("[DdbClientImpl::register_relay] {}", err);
                return Err(BingleError::Other(err));
            }
        };
        let date = "1970-01-01T00:00:00Z".to_string();
        let record = if let Some(sk) = self.api.access(|a| a.get_signing_key()) {
            AdvertRecord::new(
                my_id.clone(),
                None,
                Some(false),
                Some(relay_id),
                relay_sig,
                date,
                &sk,
            )
        } else {
            AdvertRecord::new_unsigned(
                my_id.clone(),
                None,
                Some(false),
                Some(relay_id),
                relay_sig,
                date,
            )
        };
        let up = Message::Ddb(DdbMessage::UpsertResolve(DdbUpsertResolve {
            app: "ddb".to_string(),
            start_id: my_id,
            epoch: 1,
            record,
            original_signature: "SIG".to_string(),
            rippled: false,
            tag: None,
            response_tag: None,
            text: None,
            data: None,
        }));
        let json: JsonValue = to_json_value(&up);

        // 3) Send and wait for response; validate UpdateResponse
        tracing::debug!("[DdbClientImpl::register_relay] sending DdbUpsertResolve: {:?}", json);
        let resp = match self.api.access(|a| a.send_message_to_network_with_response(&nsk, &relay_user_b64, json, None)) {
            Ok(r) => r,
            Err(e) => {
                tracing::error!("[DdbClientImpl::register_relay] send_message_to_network_with_response failed: {}", e);
                return Err(e);
            }
        };
        tracing::debug!("[DdbClientImpl::register_relay] received DdbUpdateResponse: {:?}", resp);

        let app_ok = resp.get("app").and_then(|v| v.as_str()).is_some_and(|s| s == "ddb") || resp.get("app").is_none();
        let ty_ok = resp.get("type").and_then(|v| v.as_str()) == Some("updateResponse");
        if app_ok && ty_ok { Ok(()) } else { Err(BingleError::Other("unexpected response (expected DdbUpdateResponse)".to_string())) }
    }

    fn lookup(&self, id: &str) -> Result<NetworkEndpoint, BingleError> {
        // 1) Find relay to talk to
        let relay = match self.find_relay() {
            Ok(r) => r,
            Err(e) => {
                tracing::error!("[DdbClientImpl::lookup] find_relay failed: {}", e);
                return Err(e);
            }
        };
        let relay_user_b64 = match Self::relay_user_id(&relay.id) {
            Ok(u) => u,
            Err(e) => {
                tracing::error!("[DdbClientImpl::lookup] relay_user_id failed: {}", e);
                return Err(e);
            }
        };
        let nsk = NetworkEndpoint::new_direct(relay.address);

        // 2) Build QueryResolve
        let q = Message::Ddb(DdbMessage::QueryResolve(DdbQueryResolve {
            app: "ddb".to_string(),
            id: id.to_string(),
            tag: None,
            text: None,
            data: None,
        }));
        let json: JsonValue = to_json_value(&q);

        // 3) Send and await response
        let resp = match self.api.access(|a| a.send_message_to_network_with_response(&nsk, &relay_user_b64, json, None)) {
            Ok(r) => r,
            Err(e) => {
                tracing::error!("[DdbClientImpl::lookup] send_message_to_network_with_response failed: {}", e);
                return Err(e);
            }
        };

        let is_ddb = resp.get("app").and_then(|v| v.as_str()) == Some("ddb");
        let ty = resp.get("type").and_then(|v| v.as_str());
        if !is_ddb || ty != Some("queryResponse") {
            return Err(BingleError::Other("unexpected response (expected DdbQueryResponse)".to_string()));
        }
        let found = resp.get("found").and_then(|v| v.as_bool()).unwrap_or(false);
        if !found {
            return Err(BingleError::Other("not found".to_string()));
        }
        // Build NetworkEndpoint from advert (supporting both direct endpoint and relay_id)
        let advert = match resp.get("advert") {
            Some(a) => a,
            None => {
                let err = "missing advert".to_string();
                tracing::error!("[DdbClientImpl::lookup] {}", err);
                return Err(BingleError::Other(err));
            }
        };

        // Deserialize advert record and validate signature
        let record: super::AdvertRecord = serde_json::from_value(advert.clone())
            .map_err(|e| BingleError::Other(format!("Failed to deserialize AdvertRecord: {}", e)))?;

        if !record.verify() {
            return Err(BingleError::Other(format!("AdvertRecord signature verification failed for {}", id)));
        }

        // Check if this is a relay-based record
        if let Some(relay_id_value) = advert.get("relayId") {
            let relay_id = match relay_id_value.as_str() {
                Some(s) => s.to_string(),
                None => {
                    let err = "relayId is not a string".to_string();
                    tracing::error!("[DdbClientImpl::lookup] {}", err);
                    return Err(BingleError::Other(err));
                }
            };

            // For relay endpoints, we don't have direct address/channel info in the DDB record
            // The relay_id will be used to establish connection via the relay system
            return Ok(NetworkEndpoint::new_relay(relay_id, None, None));
        }

        // Fall back to direct endpoint if no relay_id
        if let Some(endpoint) = advert.get("endpoint") {
            let host = match endpoint.get("host").and_then(|v| v.as_str()) {
                Some(h) => h,
                None => {
                    let err = "missing endpoint.host".to_string();
                    tracing::error!("[DdbClientImpl::lookup] {}", err);
                    return Err(BingleError::Other(err));
                }
            };
            let port = match endpoint.get("port").and_then(|v| v.as_u64()) {
                Some(p) => p as u16,
                None => {
                    let err = "missing endpoint.port".to_string();
                    tracing::error!("[DdbClientImpl::lookup] {}", err);
                    return Err(BingleError::Other(err));
                }
            };
            let ip: IpAddr = match host.parse() {
                Ok(ip) => ip,
                Err(_) => {
                    let err = format!("invalid host '{}': not an IP address", host);
                    tracing::error!("[DdbClientImpl::lookup] {}", err);
                    return Err(BingleError::Other(err));
                }
            };
            let addr = SocketAddr::new(ip, port);
            return Ok(NetworkEndpoint::new_direct(addr));
        }

        Err(BingleError::Other("AdvertRecord contains neither relayId nor endpoint".to_string()))
    }
}
