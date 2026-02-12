use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use serde_json::Value as JsonValue;

use crate::api::bingle_api::{BingleApi, NetworkEndpoint};
use crate::messages::types::*;
use crate::messages::{to_json_value, Message};
use crate::relay::relay_finder::{RelayFinder, RelayInfo};

/// Public DDB client interface used by higher layers.
/// Provides register_ip and lookup helpers that send DDB messages over the network
/// and validate typed responses.
pub trait DdbClient: Send + Sync {
    /// Register/update our endpoint IP:port in the DDB via a relay.
    fn register_ip(&self, endpoint: SocketAddr) -> Result<(), String>;
    /// Alias with camelCase to match external nomenclature.
    #[allow(non_snake_case)]
    fn registerIP(&self, endpoint: SocketAddr) -> Result<(), String> { self.register_ip(endpoint) }

    /// Register/update our chosen relay association in the DDB via a relay.
    /// relay_id must be a valid Algorand base32 address (36-byte decoded).
    fn register_relay(&self, relay_id: String, relay_sig: Option<String>) -> Result<(), String>;

    /// Lookup an id in the DDB and build a NetworkSourceKey from its AdvertRecord.
    fn lookup(&self, id: &str) -> Result<NetworkEndpoint, String>;

    /// Start loading the DDB from a peer relay by sending DdbInitResolve.
    /// Returns the number of records reported by DdbInitResponse (dbCount) on success.
    fn start_load_from_peer(&self, peer_id: &str) -> Result<usize, String>;

    // /// Obtain list of all relays via DdbGetEpoch, returning pairs of (relay_id, SocketAddr).
    // /// Implementations should attempt multiple known relays and fallback to discovery when unavailable.
    // fn get_relays(&self) -> Result<Vec<(String, SocketAddr)>, String>;
}

pub struct DdbClientImpl {
    api: Weak<Mutex<dyn BingleApi>>,
    discover: Arc<dyn Fn() -> Vec<RelayInfo> + Send + Sync>,
}

impl DdbClientImpl {
    /// Create a DdbClientImpl using indexer-based discovery (requires app_id configured on API or via env BINGLE_APP_ID).
    #[cfg(not(target_os = "ios"))]
    pub fn new(api: Weak<Mutex<dyn BingleApi>>) -> Self {
        let app_id_opt = api.upgrade().expect("BingleApi dropped").lock().unwrap()
            .get_app_id()
            .or_else(|| std::env::var("BINGLE_APP_ID").ok().and_then(|s| s.parse::<u64>().ok()));
        let app_id = app_id_opt.expect("DdbClientImpl::new: app_id is required (API options.app_id or BINGLE_APP_ID)");
        let cfg = api.upgrade().expect("BingleApi dropped").lock().unwrap().get_algo_provider_config();
        let discover = crate::relay::discovery::indexer_discover_closure(app_id, cfg);
        Self { api, discover }
    }

    /// Constructor that accepts a custom discovery closure (used by tests to avoid external dependencies).
    pub fn with_discovery(api: Weak<Mutex<dyn BingleApi>>, discover: Arc<dyn Fn() -> Vec<RelayInfo> + Send + Sync>) -> Self {
        Self { api, discover }
    }

    fn find_relay(&self) -> Result<RelayInfo, String> {
        log::info!("[DdbClientImpl::find_relay] create RelayFinder");
        let finder = RelayFinder::new(self.api.clone(), Duration::from_secs(60), self.discover.clone());
        log::info!("[DdbClientImpl::find_relay] get_my_id");
        let my_id = self.api.upgrade().expect("BingleApi dropped").lock().unwrap().get_my_id().ok_or_else(|| "get_my_id returned None".to_string())?;
        log::info!("[DdbClientImpl::find_relay] RelayFinder::find_relay");
        finder.find_relay(&my_id)
    }

    fn relay_user_id(relay_id: &str) -> Result<String, String> {
        // Validate Algorand base32 address by decoding to 36 bytes
        match data_encoding::BASE32_NOPAD.decode(relay_id.as_bytes()) {
            Ok(bytes) if bytes.len() == 36 => Ok(relay_id.to_string()),
            Ok(bytes) => Err(format!("invalid relay id: base32 decoded length {} != 36", bytes.len())),
            Err(e) => Err(format!("invalid relay id: {}", e)),
        }
    }

    /// Send a single getEpoch request to the specified relay and parse the relay list.
    /// This avoids probing multiple roots and is used by RelayFinder::list_all_relays.
    pub fn get_relays_from(&self, relay: &RelayInfo) -> Result<Vec<(String, SocketAddr)>, String> {
        log::info!("[DddClientImpl::get_relays_from] relay: {:?}", relay);
        let nsk = NetworkEndpoint::new_direct(relay.address);
        let relay_user = match Self::relay_user_id(&relay.id) {
            Ok(u) => u,
            Err(e) => {
                log::error!("[DdbClientImpl::get_relays_from] relay_user_id failed: {}", e);
                return Err(e);
            }
        };
        let req = Message::Ddb(DdbMessage::GetEpoch(DdbGetEpoch { app: "ddb".into(), epoch_id: -1, tag: None, response_tag: None, text: None, data: None }));
        let json: JsonValue = to_json_value(&req);
        let resp = match self.api.upgrade().expect("BingleApi dropped").lock().unwrap().send_message_to_network_with_response(&nsk, &relay_user, json, None) {
            Ok(r) => r,
            Err(e) => {
                log::error!("[DdbClientImpl::get_relays_from] send_message_to_network_with_response failed: {}", e);
                return Err(e);
            }
        };
        let app_ok = resp.get("app").and_then(|v| v.as_str()) == Some("ddb");
        let ty_ok = resp.get("type").and_then(|v| v.as_str()) == Some("getEpochResponse");
        if !app_ok || !ty_ok { return Err("unexpected response to getEpoch".to_string()); }
        let ids_arr = match resp.get("relayIds").and_then(|v| v.as_array()) {
            Some(a) => a,
            None => {
                let err = "missing relayIds".to_string();
                log::error!("[DdbClientImpl::get_relays_from] {}", err);
                return Err(err);
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
                            log::error!("[DdbClientImpl::get_relays_from] {}", err);
                            return Err(err);
                        }
                    };
                    let port = match ep.get("port").and_then(|v| v.as_u64()) {
                        Some(p) => p as u16,
                        None => {
                            let err = "endpoint missing port".to_string();
                            log::error!("[DdbClientImpl::get_relays_from] {}", err);
                            return Err(err);
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
    fn register_ip(&self, _endpoint: SocketAddr) -> Result<(), String> {
        Err("DDB client not configured (missing app_id or unsupported platform)".to_string())
    }
    fn register_relay(&self, _relay_id: String, _relay_sig: Option<String>) -> Result<(), String> {
        Err("DDB client not configured (missing app_id or unsupported platform)".to_string())
    }
    fn lookup(&self, _id: &str) -> Result<NetworkEndpoint, String> {
        Err("DDB client not configured (missing app_id or unsupported platform)".to_string())
    }
    fn start_load_from_peer(&self, _peer_id: &str) -> Result<usize, String> {
        Err("DDB client not configured (missing app_id or unsupported platform)".to_string())
    }
    // fn get_relays(&self) -> Result<Vec<(String, SocketAddr)>, String> {
    //     Err("DDB client not configured (missing app_id or unsupported platform)".to_string())
    // }
}

impl DdbClient for DdbClientImpl {
    fn start_load_from_peer(&self, peer_id: &str) -> Result<usize, String> {
        log::info!("[DdbClientImpl::start_load_from_peer][enter] peer_id={}", peer_id);
        // Resolve the peer relay's direct endpoint using RelayFinder first, then fall back to DDB lookup
        let finder = RelayFinder::new(self.api.clone(), Duration::from_secs(60), self.discover.clone());
        let mut nsk_opt = finder.lookup_root_id(peer_id);
        if nsk_opt.is_none() {
            // Fallback to DDB lookup which may return a direct endpoint
            match self.lookup(peer_id) {
                Ok(nsk) => nsk_opt = Some(nsk),
                Err(e) => {
                    let err = format!("failed to resolve peer '{}' via roots or DDB: {}", peer_id, e);
                    log::warn!("[DdbClientImpl::start_load_from_peer] {}", err);
                    return Err(err);
                }
            }
        }
        let nsk = nsk_opt.expect("nsk set above");

        let addr = match nsk.inet_socket_address() {
            Some(a) => a,
            None => {
                let err = "peer NetworkEndpoint is not a direct address".to_string();
                log::error!("[DdbClientImpl::start_load_from_peer] {}", err);
                return Err(err);
            }
        };

        let relay_user = match Self::relay_user_id(peer_id) {
            Ok(u) => u,
            Err(e) => {
                log::error!("[DdbClientImpl::start_load_from_peer] {}", e);
                return Err(e);
            }
        };

        // Compose InitResolve
        let init = Message::Ddb(DdbMessage::InitResolve(DdbInitResolve { app: "ddb".into(), tag: None, response_tag: None, text: None, data: None }));
        let json: JsonValue = to_json_value(&init);

        // Send and await InitResponse
        let nsk_direct = NetworkEndpoint::new_direct(addr);
        let resp: serde_json::Value = match self.api.upgrade().expect("BingleApi dropped").lock().unwrap().send_message_to_network_with_response(&nsk_direct, &relay_user, json, None) {
            Ok(r) => r,
            Err(e) => {
                log::error!("[DdbClientImpl::start_load_from_peer] send_message_to_network_with_response failed: {}", e);
                return Err(e);
            }
        };

        let app_ok = resp.get("app").and_then(|v| v.as_str()) == Some("ddb");
        let ty_ok = resp.get("type").and_then(|v| v.as_str()) == Some("initResponse");
        if !app_ok || !ty_ok {
            return Err("unexpected response (expected DdbInitResponse)".to_string());
        }

        let cnt = match resp.get("dbCount").and_then(|v| v.as_i64()) {
            Some(c) => c,
            None => {
                let err = "missing dbCount".to_string();
                log::error!("[DdbClientImpl::start_load_from_peer] {}", err);
                return Err(err);
            }
        };

        let out = match usize::try_from(cnt) {
            Ok(u) => u,
            Err(_) => {
                let err = "dbCount out of range".to_string();
                log::error!("[DdbClientImpl::start_load_from_peer] {}", err);
                return Err(err);
            }
        };

        log::info!("[DdbClientImpl::start_load_from_peer][exit] Ok({})", out);
        Ok(out)
    }

    // fn get_relays(&self) -> Result<Vec<(String, SocketAddr)>, String> {
    //     // Try each discovered relay to obtain epoch info
    //     let discovered: Vec<RelayInfo> = (self.discover)();
    //     let mut last_err: Option<String> = None;
    //     for cand in &discovered {
    //         let nsk = NetworkEndpoint::new_direct(cand.address);
    //         let relay_user = match Self::relay_user_id(&cand.id) { Ok(u) => u, Err(e) => { last_err = Some(e); continue; } };
    //         let req = Message::Ddb(DdbMessage::GetEpoch(DdbGetEpoch { app: "ddb".into(), epoch_id: -1, tag: None, response_tag: None, text: None, data: None }));
    //         let json: JsonValue = to_json_value(&req);
    //         match self.api.send_message_to_network_with_response(&nsk, &relay_user, json, None) {
    //             Ok(resp) => {
    //                 let app_ok = resp.get("app").and_then(|v| v.as_str()) == Some("ddb");
    //                 let ty_ok = resp.get("type").and_then(|v| v.as_str()) == Some("getEpochResponse");
    //                 if !app_ok || !ty_ok { last_err = Some("unexpected response to getEpoch".to_string()); continue; }
    //                 let ids_arr = match resp.get("relayIds").and_then(|v| v.as_array()) { Some(a) => a, None => { last_err = Some("missing relayIds".into()); continue; } };
    //                 let ids: Vec<String> = ids_arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
    //                 let eps_opt = resp.get("relayEndpoints").and_then(|v| v.as_array());
    //                 // If endpoints provided and length matches, pair by index
    //                 if let Some(eps) = eps_opt {
    //                     if eps.len() == ids.len() {
    //                         let mut out: Vec<(String, SocketAddr)> = Vec::new();
    //                         for (i, id) in ids.iter().enumerate() {
    //                             let ep = &eps[i];
    //                             let host = match ep.get("host").and_then(|v| v.as_str()) { Some(h) => h, None => { last_err = Some("endpoint missing host".into()); continue; } };
    //                             let port = match ep.get("port").and_then(|v| v.as_u64()) { Some(p) => p as u16, None => { last_err = Some("endpoint missing port".into()); continue; } };
    //                             if let Ok(ip) = host.parse::<IpAddr>() { out.push((id.clone(), SocketAddr::new(ip, port))); }
    //                         }
    //                         if !out.is_empty() { return Ok(out); }
    //                     }
    //                 }
    //                 // Fallback: map using discovered addresses where ids match
    //                 if !discovered.is_empty() {
    //                     let map: std::collections::HashMap<String, SocketAddr> = discovered.iter().map(|r| (r.id.clone(), r.address)).collect();
    //                     let mut out: Vec<(String, SocketAddr)> = Vec::new();
    //                     for id in ids.into_iter() { if let Some(addr) = map.get(&id) { out.push((id, *addr)); } }
    //                     if !out.is_empty() { return Ok(out); }
    //                 }
    //             }
    //             Err(e) => { last_err = Some(e); continue; }
    //         }
    //     }
    //     // Final fallback: use discovery directly
    //     if !discovered.is_empty() {
    //         let mut out: Vec<(String, SocketAddr)> = discovered.into_iter().map(|r| (r.id, r.address)).collect();
    //         // Sort by id for deterministic order
    //         out.sort_by(|a, b| a.0.cmp(&b.0));
    //         return Ok(out);
    //     }
    //     Err(last_err.unwrap_or_else(|| "no relays discovered".to_string()))
    // }

    fn register_ip(&self, endpoint: SocketAddr) -> Result<(), String> {
        // 1) Find relay to talk to
        let relay = match self.find_relay() {
            Ok(r) => r,
            Err(e) => {
                log::error!("[DdbClientImpl::register_ip] find_relay failed: {}", e);
                return Err(e);
            }
        };
        let relay_user_b64 = match Self::relay_user_id(&relay.id) {
            Ok(u) => u,
            Err(e) => {
                log::error!("[DdbClientImpl::register_ip] relay_user_id failed: {}", e);
                return Err(e);
            }
        };
        let nsk = NetworkEndpoint::new_direct(relay.address);

        // 2) Build UpsertResolve using our id as startId and record.id
        let my_id = match self.api.upgrade().expect("BingleApi dropped").lock().unwrap().get_my_id() {
            Some(id) => id,
            None => {
                let err = "get_my_id returned None".to_string();
                log::error!("[DdbClientImpl::register_ip] {}", err);
                return Err(err);
            }
        };
        let record = AdvertRecord {
            id: my_id.clone(),
            endpoint: Some(InetSocketAddress { host: match endpoint.ip() { IpAddr::V4(v4) => v4.to_string(), IpAddr::V6(v6) => v6.to_string() }, port: endpoint.port() }),
            am_relay: Some(false),
            relay_id: None,
            relay_sig: None,
            date: "1970-01-01T00:00:00Z".to_string(),
            sig: None,
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
        log::info!("[DdbClientImpl::register_ip] sending DdbUpsertResolve: {:?}", json);
        let resp = match self.api.upgrade().expect("BingleApi dropped").lock().unwrap().send_message_to_network_with_response(&nsk, &relay_user_b64, json, None) {
            Ok(r) => r,
            Err(e) => {
                log::error!("[DdbClientImpl::register_ip] send_message_to_network_with_response failed: {}", e);
                return Err(e);
            }
        };
        log::info!("[DdbClientImpl::register_ip] received DdbUpdateResponse: {:?}", resp);

        let app_ok = resp.get("app").and_then(|v| v.as_str()) == Some("ddb");
        let ty_ok = resp.get("type").and_then(|v| v.as_str()) == Some("updateResponse");
        if app_ok && ty_ok { Ok(()) } else { Err("unexpected response (expected DdbUpdateResponse)".to_string()) }
    }

    fn register_relay(&self, relay_id: String, relay_sig: Option<String>) -> Result<(), String> {
        // 1) Find relay to talk to (we also accept the relay_id parameter for the record)
        let relay = match self.find_relay() {
            Ok(r) => r,
            Err(e) => {
                log::error!("[DdbClientImpl::register_relay] find_relay failed: {}", e);
                return Err(e);
            }
        };
        let relay_user_b64 = match Self::relay_user_id(&relay.id) {
            Ok(u) => u,
            Err(e) => {
                log::error!("[DdbClientImpl::register_relay] relay_user_id (root) failed: {}", e);
                return Err(e);
            }
        };
        // validate provided relay_id shape
        if let Err(e) = Self::relay_user_id(&relay_id) {
            log::error!("[DdbClientImpl::register_relay] relay_user_id (param) failed: {}", e);
            return Err(e);
        }
        let nsk = NetworkEndpoint::new_direct(relay.address);

        // 2) Build UpsertResolve using our id as startId and record.id
        let my_id = match self.api.upgrade().expect("BingleApi dropped").lock().unwrap().get_my_id() {
            Some(id) => id,
            None => {
                let err = "get_my_id returned None".to_string();
                log::error!("[DdbClientImpl::register_relay] {}", err);
                return Err(err);
            }
        };
        let record = AdvertRecord {
            id: my_id.clone(),
            endpoint: None,
            am_relay: Some(false),
            relay_id: Some(relay_id),
            relay_sig,
            date: "1970-01-01T00:00:00Z".to_string(),
            sig: None,
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
        log::info!("[DdbClientImpl::register_relay] sending DdbUpsertResolve: {:?}", json);
        let resp = match self.api.upgrade().expect("BingleApi dropped").lock().unwrap().send_message_to_network_with_response(&nsk, &relay_user_b64, json, None) {
            Ok(r) => r,
            Err(e) => {
                log::error!("[DdbClientImpl::register_relay] send_message_to_network_with_response failed: {}", e);
                return Err(e);
            }
        };
        log::info!("[DdbClientImpl::register_relay] received DdbUpdateResponse: {:?}", resp);

        let app_ok = resp.get("app").and_then(|v| v.as_str()).is_some_and(|s| s == "ddb") || resp.get("app").is_none();
        let ty_ok = resp.get("type").and_then(|v| v.as_str()) == Some("updateResponse");
        if app_ok && ty_ok { Ok(()) } else { Err("unexpected response (expected DdbUpdateResponse)".to_string()) }
    }

    fn lookup(&self, id: &str) -> Result<NetworkEndpoint, String> {
        // 1) Find relay to talk to
        let relay = match self.find_relay() {
            Ok(r) => r,
            Err(e) => {
                log::error!("[DdbClientImpl::lookup] find_relay failed: {}", e);
                return Err(e);
            }
        };
        let relay_user_b64 = match Self::relay_user_id(&relay.id) {
            Ok(u) => u,
            Err(e) => {
                log::error!("[DdbClientImpl::lookup] relay_user_id failed: {}", e);
                return Err(e);
            }
        };
        let nsk = NetworkEndpoint::new_direct(relay.address);

        // 2) Build QueryResolve
        let q = Message::Ddb(DdbMessage::QueryResolve(DdbQueryResolve {
            app: "ddb".to_string(),
            id: id.to_string(),
            tag: None,
            response_tag: None,
            text: None,
            data: None,
        }));
        let json: JsonValue = to_json_value(&q);

        // 3) Send and await response
        let resp = match self.api.upgrade().expect("BingleApi dropped").lock().unwrap().send_message_to_network_with_response(&nsk, &relay_user_b64, json, None) {
            Ok(r) => r,
            Err(e) => {
                log::error!("[DdbClientImpl::lookup] send_message_to_network_with_response failed: {}", e);
                return Err(e);
            }
        };

        let is_ddb = resp.get("app").and_then(|v| v.as_str()) == Some("ddb");
        let ty = resp.get("type").and_then(|v| v.as_str());
        if !is_ddb || ty != Some("queryResponse") {
            return Err("unexpected response (expected DdbQueryResponse)".to_string());
        }
        let found = resp.get("found").and_then(|v| v.as_bool()).unwrap_or(false);
        if !found {
            return Err("not found".to_string());
        }
        // Build NetworkEndpoint from advert (supporting both direct endpoint and relay_id)
        let advert = match resp.get("advert") {
            Some(a) => a,
            None => {
                let err = "missing advert".to_string();
                log::error!("[DdbClientImpl::lookup] {}", err);
                return Err(err);
            }
        };

        // Check if this is a relay-based record
        if let Some(relay_id_value) = advert.get("relayId") {
            let relay_id = match relay_id_value.as_str() {
                Some(s) => s.to_string(),
                None => {
                    let err = "relayId is not a string".to_string();
                    log::error!("[DdbClientImpl::lookup] {}", err);
                    return Err(err);
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
                    log::error!("[DdbClientImpl::lookup] {}", err);
                    return Err(err);
                }
            };
            let port = match endpoint.get("port").and_then(|v| v.as_u64()) {
                Some(p) => p as u16,
                None => {
                    let err = "missing endpoint.port".to_string();
                    log::error!("[DdbClientImpl::lookup] {}", err);
                    return Err(err);
                }
            };
            let ip: IpAddr = match host.parse() {
                Ok(ip) => ip,
                Err(_) => {
                    let err = format!("invalid host '{}': not an IP address", host);
                    log::error!("[DdbClientImpl::lookup] {}", err);
                    return Err(err);
                }
            };
            let addr = SocketAddr::new(ip, port);
            return Ok(NetworkEndpoint::new_direct(addr));
        }

        Err("AdvertRecord contains neither relayId nor endpoint".to_string())
    }
}
