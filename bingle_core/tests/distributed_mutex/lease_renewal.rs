// Issue #37: a requester's retries must not keep renewing the lease a node granted it.
//
// The node under test is driven directly through its message handlers, with a manual clock and
// sends that are recorded rather than delivered, so each interleaving is exact.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bingle_core::distributed_mutex::{ModifiedLamportDistributedMutex, MutexClock};
use bingle_core::messages::types::{MutexRelease, MutexRequest, MutexResponse};

const LEASE: Duration = Duration::from_secs(1);

struct Harness {
    node: ModifiedLamportDistributedMutex,
    now: Arc<Mutex<Instant>>,
    grants: Arc<Mutex<Vec<String>>>,
}

impl Harness {
    /// Node C of a three-node group A, B, C, which never requests the lock itself.
    fn bystander() -> Self {
        let now = Arc::new(Mutex::new(Instant::now()));
        let reader = now.clone();
        let clock: MutexClock = Arc::new(move || *reader.lock().unwrap());
        let grants = Arc::new(Mutex::new(Vec::new()));
        let grants_log = grants.clone();
        let ids = vec!["A".to_string(), "B".to_string(), "C".to_string()];
        let node = ModifiedLamportDistributedMutex::new(
            "C".to_string(),
            ids,
            |_: &str, _: &MutexRequest| true,
            move |dest: &str, resp: &MutexResponse| {
                if resp.response_tag.as_deref() != Some("membership_only") {
                    grants_log.lock().unwrap().push(dest.to_string());
                }
                true
            },
            |_: &str, _: &MutexRelease| true,
        )
        .with_lease(LEASE, clock);
        Self { node, now, grants }
    }

    fn advance(&self, by: Duration) {
        *self.now.lock().unwrap() += by;
    }

    fn request(&self, from: &str, ts: i64) {
        let req = MutexRequest {
            app: "mutex".into(),
            lamport_timestamp: ts,
            tag: None,
            known_ids: Some(["A", "B", "C"].iter().map(|s| s.to_string()).collect()),
        };
        self.node.handle_request(from, &req);
    }

    fn release(&self, from: &str) {
        let rel = MutexRelease {
            app: "mutex".into(),
            tag: None,
            known_ids: Some(["A", "B", "C"].iter().map(|s| s.to_string()).collect()),
        };
        self.node.handle_release(from, &rel);
    }

    /// Grants sent since the last call.
    fn take_grants(&self) -> Vec<String> {
        std::mem::take(&mut *self.grants.lock().unwrap())
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
fn retried_request_does_not_extend_the_lease() {
    let c = Harness::bystander();
    c.request("B", 1);
    assert_eq!(c.take_grants(), vec!["B"]);

    // A asks while B's lease is live, so C defers it.
    c.request("A", 1);
    assert!(c.take_grants().is_empty());

    // B, still waiting on A, retries; C re-acknowledges it.
    c.advance(Duration::from_millis(600));
    c.request("B", 1);
    assert_eq!(c.take_grants(), vec!["B"]);

    // B's original lease has run out. Had the retry renewed it, A would be deferred again.
    c.advance(Duration::from_millis(600));
    c.request("A", 1);
    assert_eq!(c.take_grants(), vec!["A"]);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn retries_cannot_hold_the_grant_forever() {
    let c = Harness::bystander();
    c.request("B", 1);
    c.request("A", 1);
    c.take_grants();

    // B retries every 400ms and A every 500ms; A must win once B's single lease lapses.
    let mut a_granted_at = None;
    for step in 1..=10u64 {
        c.advance(Duration::from_millis(100));
        let ms = step * 100;
        if ms % 400 == 0 {
            c.request("B", 1);
        }
        if ms % 500 == 0 {
            c.request("A", 1);
        }
        if c.take_grants().iter().any(|g| g == "A") {
            a_granted_at = Some(ms);
            break;
        }
    }
    assert_eq!(a_granted_at, Some(1000));
}

#[test]
#[cfg(not(target_os = "ios"))]
fn new_request_from_the_holder_gets_a_fresh_lease() {
    let c = Harness::bystander();
    c.request("B", 1);
    c.take_grants();

    // B's next acquisition (a later timestamp) is a new grant with its own lease.
    c.advance(Duration::from_millis(900));
    c.request("B", 5);
    assert_eq!(c.take_grants(), vec!["B"]);

    c.advance(Duration::from_millis(500));
    c.request("A", 6);
    assert!(
        c.take_grants().is_empty(),
        "B's fresh lease should still defer A"
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
fn granting_a_retry_clears_its_deferral() {
    let c = Harness::bystander();
    c.request("B", 1);
    c.request("A", 1);
    c.take_grants();

    // B's lease lapses and A's retry is granted directly.
    c.advance(LEASE + Duration::from_millis(1));
    c.request("A", 1);
    assert_eq!(c.take_grants(), vec!["A"]);

    // A's release must not pop A's old deferral and grant A again.
    c.release("A");
    assert!(c.take_grants().is_empty());
}
