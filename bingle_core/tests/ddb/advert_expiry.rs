// Tests for expiring client adverts by age in the in-memory DDB backend (issue #280).

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bingle_core::ddb::{
    AdvertRecord, DEFAULT_ADVERT_TTL, DdbBackend, DdbClock, InMemoryDdbBackend, InetSocketAddress,
    effective_advert_ttl,
};

const TTL: Duration = Duration::from_secs(600);

/// A clock that only moves when the test advances it.
#[derive(Clone)]
struct TestClock(Arc<Mutex<Instant>>);

impl TestClock {
    fn new() -> Self {
        Self(Arc::new(Mutex::new(Instant::now())))
    }
    fn advance(&self, by: Duration) {
        *self.0.lock().unwrap() += by;
    }
    fn clock(&self) -> DdbClock {
        let now = self.0.clone();
        Arc::new(move || *now.lock().unwrap())
    }
}

fn record(id: &str, am_relay: bool) -> AdvertRecord {
    AdvertRecord::new_unsigned(
        id.to_string(),
        Some(InetSocketAddress {
            host: "10.0.0.1".to_string(),
            port: 12121,
        }),
        Some(am_relay),
        None,
        None,
        "1970-01-01T00:00:00Z".to_string(),
    )
}

fn backend(clock: &TestClock) -> InMemoryDdbBackend {
    InMemoryDdbBackend::with_advert_ttl(Some(TTL)).with_clock(clock.clock())
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn ttl_defaults_to_ten_minutes_and_zero_disables() {
    assert_eq!(DEFAULT_ADVERT_TTL, Duration::from_secs(600));
    assert_eq!(effective_advert_ttl(None), Some(DEFAULT_ADVERT_TTL));
    assert_eq!(effective_advert_ttl(Some(Duration::ZERO)), None);
    assert_eq!(
        effective_advert_ttl(Some(Duration::from_secs(5))),
        Some(Duration::from_secs(5))
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn client_advert_is_found_until_the_ttl_then_not_found() {
    let clock = TestClock::new();
    let mut db = backend(&clock);
    db.upsert(record("CLIENT", false));

    clock.advance(TTL - Duration::from_secs(1));
    assert!(
        db.lookup("CLIENT").is_some(),
        "still live just before the ttl"
    );
    assert_eq!(db.len(), 1);

    clock.advance(Duration::from_secs(1));
    assert!(
        db.lookup("CLIENT").is_none(),
        "absent once the ttl has passed"
    );
    assert_eq!(db.len(), 0, "an expired advert is not counted");
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn rewriting_an_advert_restarts_its_ttl() {
    let clock = TestClock::new();
    let mut db = backend(&clock);
    db.upsert(record("CLIENT", false));
    clock.advance(TTL - Duration::from_secs(10));
    db.upsert(record("CLIENT", false));
    clock.advance(TTL - Duration::from_secs(10));
    assert!(
        db.lookup("CLIENT").is_some(),
        "a refreshed advert lives a full ttl from the refresh"
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn age_is_measured_from_receipt_not_the_record_date() {
    // Clients send a fixed 1970 date, so judging by `date` would expire every advert at once.
    let clock = TestClock::new();
    let mut db = backend(&clock);
    db.upsert(record("CLIENT", false));
    assert!(db.lookup("CLIENT").is_some());
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn relay_adverts_and_own_advert_never_expire() {
    let clock = TestClock::new();
    let mut db = backend(&clock);
    db.set_own_id("SELF");
    db.upsert(record("RELAY", true));
    db.upsert(record("SELF", false));
    db.upsert(record("CLIENT", false));

    clock.advance(TTL * 10);
    assert!(db.lookup("RELAY").is_some());
    assert!(db.lookup("SELF").is_some());
    assert!(db.lookup("CLIENT").is_none());
    assert_eq!(db.make_epoch_info().0, vec!["RELAY".to_string()]);
    assert_eq!(db.sweep_expired(), vec!["CLIENT".to_string()]);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn sweep_removes_expired_records_from_memory() {
    let clock = TestClock::new();
    let mut db = backend(&clock);
    db.upsert(record("OLD", false));
    clock.advance(TTL / 2);
    db.upsert(record("NEW", false));

    assert!(db.sweep_expired().is_empty(), "nothing has expired yet");
    clock.advance(TTL / 2);
    assert_eq!(db.sweep_expired(), vec!["OLD".to_string()]);
    assert!(
        db.sweep_expired().is_empty(),
        "the record is gone, not hidden"
    );
    assert!(db.lookup("NEW").is_some());
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn expired_adverts_are_not_sent_to_a_joining_relay() {
    let clock = TestClock::new();
    let mut db = backend(&clock);
    db.upsert(record("OLD", false));
    clock.advance(TTL);
    db.upsert(record("LIVE", false));

    let sent = Mutex::new(Vec::<serde_json::Value>::new());
    let nsk =
        bingle_core::api::bingle_api::NetworkEndpoint::new_direct("127.0.0.1:1".parse().unwrap());
    db.handle_init(&nsk, "PEER", None, &|_, _, json| {
        sent.lock().unwrap().push(json);
        true
    });
    let sent = sent.into_inner().unwrap();
    assert_eq!(sent[0]["dbCount"], 1, "only the live advert is counted");
    let dumped: Vec<&str> = sent[1..]
        .iter()
        .filter_map(|j| j["record"]["id"].as_str())
        .collect();
    assert_eq!(dumped, vec!["LIVE"]);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn without_a_ttl_nothing_expires() {
    let clock = TestClock::new();
    let mut db = InMemoryDdbBackend::new().with_clock(clock.clock());
    db.upsert(record("CLIENT", false));
    clock.advance(Duration::from_secs(365 * 24 * 3600));
    assert!(db.lookup("CLIENT").is_some());
    assert!(db.sweep_expired().is_empty());
}
