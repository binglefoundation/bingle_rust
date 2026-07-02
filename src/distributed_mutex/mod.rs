//! Distributed mutex abstraction.
//! This module defines a trait for executing a closure inside an exclusive
//! critical section, intended to be backed by a distributed lock implementation.

use std::collections::{BTreeSet, HashSet};
use std::cmp::Ordering;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};
use tracing::debug;

/// Trait representing a distributed mutex.
///
/// Implementations should guarantee that the provided closure runs exclusively
/// with respect to other acquire calls on the same logical mutex.
///
/// Note: This trait intentionally provides no default implementation.
pub trait DistributedMutex {
    /// Acquire the mutex and run the provided closure as a critical section.
    /// The critical section must run exclusively with respect to other callers
    /// on the same mutex. The return value of the closure is returned.
    fn acquire<F, R>(&self, f: F) -> R
    where
        F: FnOnce() -> R;
}

/// A simple, in-process implementation of `DistributedMutex` backed by
/// `std::sync::Mutex`. This is not distributed but is useful for testing
/// and as a placeholder until a networked implementation is provided.
#[derive(Debug)]
pub struct LocalDistributedMutex {
    inner: Mutex<()>,
}

impl LocalDistributedMutex {
    pub fn new() -> Self {
        Self { inner: Mutex::new(()) }
    }

    fn lock(&self) -> MutexGuard<'_, ()> {
        // Validate the lock result; panic with message on poisoning.
        self.inner.lock().expect("mutex poisoned")
    }
}

impl Default for LocalDistributedMutex {
    fn default() -> Self {
        Self::new()
    }
}

impl DistributedMutex for LocalDistributedMutex {
    fn acquire<F, R>(&self, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        let _guard = self.lock();
        f()
    }
}

// Modified Lamport with Lease-Based Safety implementation
#[derive(Clone)]
pub struct ModifiedLamportDistributedMutex {
    self_id: String,
    lease_duration: Duration,
    send_request: Arc<dyn Fn(&str, &crate::messages::types::MutexRequest) + Send + Sync>,
    send_reply: Arc<dyn Fn(&str, &crate::messages::types::MutexResponse) + Send + Sync>,
    send_release: Arc<dyn Fn(&str, &crate::messages::types::MutexRelease) + Send + Sync>,
    inner: Arc<Mutex<InnerState>>,
    cv: Arc<Condvar>,
}

impl std::fmt::Debug for ModifiedLamportDistributedMutex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let st = self.inner.lock().expect("lock");
        f.debug_struct("ModifiedLamportDistributedMutex")
            .field("self_id", &self.self_id)
            .field("dynamic_node_ids", &st.dynamic_node_ids)
            .finish()
    }
}

#[derive(Debug)]
struct InnerState {
    lamport: i64,
    current_request_ts: Option<i64>,
    acks: HashSet<String>,
    // Requests we have deferred (ordered by (timestamp, node_id))
    deferred: BTreeSet<(i64, String)>,
    // Last holder we granted (or observed) with lease deadline
    last_holder: Option<(String, i64, Instant)>,
    in_cs: bool,
    dynamic_node_ids: HashSet<String>,
}

impl InnerState {
    fn required_acks(&self) -> usize {
        self.dynamic_node_ids.len() / 2 + if self.dynamic_node_ids.len() > 2 { 1 } else { 0 }
    }

    fn update_membership(&mut self, self_id: &str, from_id: &str, known_ids: &Option<HashSet<String>>) -> bool {
        let mut changed = false;
        if !self.dynamic_node_ids.contains(from_id) {
            tracing::info!("[mutex:{}] adding new node to dynamic_node_ids: {}", self_id, from_id);
            self.dynamic_node_ids.insert(from_id.to_string());
            changed = true;
        }
        if let Some(known) = known_ids {
            for id in known {
                if !self.dynamic_node_ids.contains(id) {
                    tracing::info!("[mutex:{}] adding new node from known_ids: {}", self_id, id);
                    self.dynamic_node_ids.insert(id.clone());
                    changed = true;
                }
            }
        }
        changed
    }
}

impl ModifiedLamportDistributedMutex {
    pub fn new<Req, Rep, Rel>(
        self_id: String,
        node_ids: Vec<String>,
        send_request: Req,
        send_reply: Rep,
        send_release: Rel,
    ) -> Self
    where
        Req: Fn(&str, &crate::messages::types::MutexRequest) + Send + Sync + 'static,
        Rep: Fn(&str, &crate::messages::types::MutexResponse) + Send + Sync + 'static,
        Rel: Fn(&str, &crate::messages::types::MutexRelease) + Send + Sync + 'static,
    {
        tracing::info!("Creating ModifiedLamportDistributedMutex on {:?} with {:?}", self_id, node_ids);
        let inner = InnerState {
            lamport: 0,
            current_request_ts: None,
            acks: HashSet::new(),
            deferred: BTreeSet::new(),
            last_holder: None,
            in_cs: false,
            dynamic_node_ids: node_ids.into_iter().collect(),
        };
        Self {
            self_id,
            lease_duration: Duration::from_millis(1000),
            send_request: Arc::new(send_request),
            send_reply: Arc::new(send_reply),
            send_release: Arc::new(send_release),
            inner: Arc::new(Mutex::new(inner)),
            cv: Arc::new(Condvar::new()),
        }
    }

    fn broadcast_request(&self, ts: i64) {
        let (ids, msg) = {
            let st = self.inner.lock().expect("lock");
            let ids: Vec<String> = st.dynamic_node_ids.iter().cloned().collect();
            let msg = crate::messages::types::MutexRequest {
                app: "mutex".into(),
                lamport_timestamp: ts,
                tag: None,
                known_ids: Some(ids.clone().into_iter().collect()),
            };
            (ids, msg)
        };
        for id in ids {
            if id == self.self_id {
                continue;
            }
            (self.send_request)(&id, &msg);
        }
    }

    fn broadcast_release(&self) {
        let (ids, msg) = {
            let st = self.inner.lock().expect("lock");
            let ids: Vec<String> = st.dynamic_node_ids.iter().cloned().collect();
            let msg = crate::messages::types::MutexRelease {
                app: "mutex".into(),
                tag: None,
                known_ids: Some(ids.clone().into_iter().collect()),
            };
            (ids, msg)
        };
        tracing::info!("[mutex:{}] broadcast release: {:?}", self.self_id, ids);
        for id in ids {
            if id == self.self_id { continue; }
            (self.send_release)(&id, &msg);
        }
    }

    fn broadcast_membership(&self) {
        let (ids, msg) = {
            let st = self.inner.lock().expect("lock");
            let ids: Vec<String> = st.dynamic_node_ids.iter().cloned().collect();
            let msg = crate::messages::types::MutexResponse {
                app: "mutex".into(),
                known_ids: Some(ids.clone().into_iter().collect()),
                response_tag: None,
            };
            (ids, msg)
        };
        tracing::debug!("[mutex:{}] broadcasting membership: {:?}", self.self_id, ids);
        for id in ids {
            if id == self.self_id { continue; }
            (self.send_reply)(&id, &msg);
        }
    }

    // External message handlers
    pub fn handle_request(&self, from_id: &str, req: &crate::messages::types::MutexRequest) {
        tracing::debug!("[mutex:{}] handle REQUEST from {}: ts={}", self.self_id, from_id, req.lamport_timestamp);
        // Update Lamport clock
        let mut st = self.inner.lock().expect("lock");
        st.lamport = st.lamport.max(req.lamport_timestamp) + 1;

        let membership_changed = st.update_membership(&self.self_id, from_id, &req.known_ids);
        if membership_changed && st.current_request_ts.is_some() {
            self.cv.notify_all();
        }

        let should_broadcast_membership = membership_changed;
        let mut should_send_reply = false;

        // If we are currently inside the critical section, defer all incoming requests
        if st.in_cs {
            st.deferred.insert((req.lamport_timestamp, from_id.to_string()));
        } else {
            // Check current holder lease
            let now = Instant::now();
            let mut deferred_by_lease = false;
            if let Some((holder, _ts, deadline)) = &st.last_holder
                && *holder != from_id && *deadline > now {
                    // Defer granting while current lease valid
                    st.deferred.insert((req.lamport_timestamp, from_id.to_string()));
                    deferred_by_lease = true;
                }

            if !deferred_by_lease {
                // Decide based on our own request status
                let grant_now = match st.current_request_ts {
                    None => true,
                    Some(my_ts) => {
                        // If we are requesting, only grant if their (ts, id) < (my_ts, self_id)
                        match (req.lamport_timestamp.cmp(&my_ts), from_id.cmp(&self.self_id)) {
                            (Ordering::Less, _) => true,
                            (Ordering::Equal, ord_id) => ord_id == Ordering::Less,
                            (Ordering::Greater, _) => false,
                        }
                    }
                };

                if grant_now {
                    // Record last holder lease for requester
                    st.last_holder = Some((from_id.to_string(), req.lamport_timestamp, Instant::now() + self.lease_duration));
                    should_send_reply = true;
                } else {
                    st.deferred.insert((req.lamport_timestamp, from_id.to_string()));
                }
            }
        }

        let known_ids = Some(st.dynamic_node_ids.iter().cloned().collect());
        drop(st);

        if should_broadcast_membership {
            self.broadcast_membership();
        }
        if should_send_reply {
            let resp = crate::messages::types::MutexResponse { app: "mutex".into(), response_tag: None, known_ids };
            tracing::debug!("[mutex:{}] [handle_request] grant_now - send REPLY: {:?}", self.self_id, resp);
            (self.send_reply)(from_id, &resp);
        }
    }

    pub fn handle_reply(&self, from_id: &str, resp: &crate::messages::types::MutexResponse) {
        tracing::debug!("[mutex:{}] handle REPLY from {}, known ids={:?}", self.self_id, from_id, resp.known_ids);
        let mut st = self.inner.lock().expect("lock");

        let membership_changed = st.update_membership(&self.self_id, from_id, &resp.known_ids);
        if membership_changed && st.current_request_ts.is_some() {
            self.cv.notify_all();
        }

        if st.current_request_ts.is_some() && resp.response_tag.as_deref() != Some("membership_only") {
            st.acks.insert(from_id.to_string());
            if st.acks.len() >= st.required_acks() {
                self.cv.notify_all();
            }
        }
        drop(st);
        if membership_changed {
            self.broadcast_membership();
        }
    }

    pub fn handle_release(&self, from_id: &str, rel: &crate::messages::types::MutexRelease) {
        tracing::debug!("[mutex:{}] handle RELEASE from {}", self.self_id, from_id);
        let mut st = self.inner.lock().expect("lock");
        let membership_changed = st.update_membership(&self.self_id, from_id, &rel.known_ids);
        if membership_changed && st.current_request_ts.is_some() {
            self.cv.notify_all();
        }

        if let Some((holder, _ts, _)) = &st.last_holder
            && holder == from_id { st.last_holder = None; }

        let mut next_grant = None;
        // On release, grant only the next (earliest) deferred request to preserve majority safety
        if let Some(first) = st.deferred.iter().next().cloned() {
            st.deferred.remove(&first);
            let next_id = first.1.clone();
            let next_ts = first.0;
            st.last_holder = Some((next_id.clone(), next_ts, Instant::now() + self.lease_duration));
            next_grant = Some(next_id);
        }

        let known_ids = Some(st.dynamic_node_ids.iter().cloned().collect());
        drop(st);

        if membership_changed {
            self.broadcast_membership();
        }

        if let Some(id) = next_grant {
            let resp = crate::messages::types::MutexResponse { app: "mutex".into(), response_tag: None, known_ids };
            tracing::debug!("[mutex:{}] [handle_release] grant deferred - send REPLY: {:?}", self.self_id, resp);
            (self.send_reply)(&id, &resp);
        }
    }
}

impl DistributedMutex for ModifiedLamportDistributedMutex {
    fn acquire<F, R>(&self, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        // Start a request cycle
        // 1) set lamport, record request, send broadcast
        let ts = {
            let mut st = self.inner.lock().expect("inner lock 1");
            st.lamport += 1;
            let ts = st.lamport;
            st.current_request_ts = Some(ts);
            st.acks.clear();
            st.acks.insert(self.self_id.clone()); // count self towards majority
            debug!("[mutex:{}] REQUEST start: ts={}, required_acks={}", self.self_id, ts, st.required_acks());
            ts
        };

        debug!("[mutex:{}] Broadcasting REQUEST ts={} to peers {:?}", self.self_id, ts, { let st = self.inner.lock().expect("inner lock 2"); st.dynamic_node_ids.clone() });
        self.broadcast_request(ts);

        // 2) Wait for majority with exponential backoff retries
        let mut backoff = Duration::from_millis(50);
        loop {
            let mut st = self.inner.lock().expect("inner lock 3");
            let req_acks = st.required_acks();
            if st.acks.len() >= req_acks {
                // Only enter if we have not granted our vote to someone else whose lease is still valid
                let now = Instant::now();
                let can_enter = match &st.last_holder {
                    None => true,
                    Some((holder, _hts, deadline)) => {
                        if holder == &self.self_id { true } else { *deadline <= now }
                    }
                };
                debug!(
                    "[mutex:{}] Have majority acks={} (need {}), can_enter={} (holder={:?})",
                    self.self_id,
                    st.acks.len(),
                    req_acks,
                    can_enter,
                    st.last_holder.as_ref().map(|(h, ts, dl)| (h.clone(), *ts, dl.saturating_duration_since(Instant::now())))
                );
                if can_enter {
                    // Enter critical section with a lease
                    st.in_cs = true;
                    st.last_holder = Some((self.self_id.clone(), ts, Instant::now() + self.lease_duration));
                    debug!("[mutex:{}] ENTER critical section ts={} (lease {:?})", self.self_id, ts, self.lease_duration);
                    drop(st);
                    break;
                }
            }
            let timeout = backoff;
            debug!("[mutex:{}] Waiting for acks: have={}, need={}, timeout={:?}", self.self_id, st.acks.len(), req_acks, timeout);
            let res = self.cv.wait_timeout(st, timeout).expect("cv");
            let mut st2 = res.0;
            let req_acks2 = st2.required_acks();
            if st2.acks.len() >= req_acks2 {
                let now2 = Instant::now();
                let can_enter2 = match &st2.last_holder {
                    None => true,
                    Some((holder, _hts, deadline)) => {
                        if holder == &self.self_id { true } else { *deadline <= now2 }
                    }
                };
                debug!(
                    "[mutex:{}] Woke up: acks={}, can_enter={} (holder={:?})",
                    self.self_id,
                    st2.acks.len(),
                    can_enter2,
                    st2.last_holder.as_ref().map(|(h, ts, dl)| (h.clone(), *ts, dl.saturating_duration_since(Instant::now())))
                );
                if can_enter2 {
                    st2.in_cs = true;
                    st2.last_holder = Some((self.self_id.clone(), ts, Instant::now() + self.lease_duration));
                    debug!("[mutex:{}] ENTER critical section (post-wake) ts={} (lease {:?})", self.self_id, ts, self.lease_duration);
                    drop(st2);
                    break;
                }
            }
            // Timeout fired; retry broadcast for nodes that may have missed
            debug!("[mutex:{}] Timeout/backoff {:?} fired; re-broadcasting REQUEST ts={} tp {:?}", self.self_id, backoff, ts, st2.dynamic_node_ids);
            drop(st2);
            self.broadcast_request(ts);
            // Exponential backoff up to 1s
            backoff = Duration::from_millis((backoff.as_millis().min(1000) as u64).saturating_mul(2).min(1000));
            debug!("[mutex:{}] Backoff updated to {:?}", self.self_id, backoff);
        }

        // Run critical section
        debug!("[mutex:{}] RUN critical section ts={}", self.self_id, ts);
        let result = f();

        // 3) Release: grant only the next deferred requester (by ts, id) then broadcast RELEASE
        let (maybe_grant, known_ids): (Option<(i64, String)>, Option<HashSet<String>>) = {
            let mut st = self.inner.lock().expect("lock");
            // Choose the earliest deferred request to grant
            let first = st.deferred.iter().next().cloned();
            if let Some((ts_grant, id_grant)) = &first {
                // Remove from deferred and set a lease for the next holder
                st.deferred.remove(&(*ts_grant, id_grant.clone()));
                st.last_holder = Some((id_grant.clone(), *ts_grant, Instant::now() + self.lease_duration));
                debug!("[mutex:{}] RELEASE: granting next {:?} with ts={} and broadcasting reply", self.self_id, id_grant, ts_grant);
            } else {
                st.last_holder = None;
                debug!("[mutex:{}] RELEASE: no deferred requests; clearing last_holder", self.self_id);
            }
            st.in_cs = false;
            st.current_request_ts = None;
            st.acks.clear();
            let known = Some(st.dynamic_node_ids.iter().cloned().collect());
            (first, known)
        };
        // Send a single reply outside the lock (if any)
        if let Some((_ts, id)) = maybe_grant {
            let resp = crate::messages::types::MutexResponse { app: "mutex".into(), response_tag: None, known_ids };
            tracing::debug!("[mutex:{}] [acquire] maybe_grant - send REPLY: {:?}", self.self_id, resp);
            (self.send_reply)(&id, &resp);
        }
        debug!("[mutex:{}] Broadcasting RELEASE to peers", self.self_id);
        self.broadcast_release();
        debug!("[mutex:{}] COMPLETE ts={}", self.self_id, ts);
        result
    }
}
