use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use bingle_core::relay::relay_origin::{RELAY_ORIGIN_MAX_ENTRIES, RelayLookup, RelayOriginCache};

/// A lookup that answers `answer` for every id and counts its calls.
fn counting(answer: Option<bool>) -> (RelayLookup, Arc<AtomicU32>) {
    let calls = Arc::new(AtomicU32::new(0));
    let c = calls.clone();
    let lookup: RelayLookup = Arc::new(move |_id: &str| {
        c.fetch_add(1, Ordering::SeqCst);
        answer
    });
    (lookup, calls)
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn confirmed_relay_is_cached() {
    let (lookup, calls) = counting(Some(true));
    let cache = RelayOriginCache::new(lookup);

    assert!(cache.is_relay("RELAY"));
    assert!(cache.is_relay("RELAY"));
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "second answer comes from the cache"
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn non_relay_answer_is_cached() {
    let (lookup, calls) = counting(Some(false));
    let cache = RelayOriginCache::new(lookup);

    assert!(!cache.is_relay("CLIENT"));
    assert!(!cache.is_relay("CLIENT"));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn unknown_answer_fails_closed_and_is_not_cached() {
    let (lookup, calls) = counting(None);
    let cache = RelayOriginCache::new(lookup);

    assert!(!cache.is_relay("RELAY"));
    assert!(!cache.is_relay("RELAY"));
    assert_eq!(
        calls.load(Ordering::SeqCst),
        2,
        "an unknown answer is asked again"
    );
    assert!(cache.is_empty());
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn expired_answers_are_looked_up_again() {
    let (lookup, calls) = counting(Some(true));
    let cache =
        RelayOriginCache::with_ttls(lookup, Duration::from_millis(20), Duration::from_millis(20));

    assert!(cache.is_relay("RELAY"));
    std::thread::sleep(Duration::from_millis(40));
    assert!(cache.is_relay("RELAY"));
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn denied_answer_expires_before_allowed_answer() {
    let calls = Arc::new(AtomicU32::new(0));
    let c = calls.clone();
    let lookup: RelayLookup = Arc::new(move |id: &str| {
        c.fetch_add(1, Ordering::SeqCst);
        Some(id == "RELAY")
    });
    let cache =
        RelayOriginCache::with_ttls(lookup, Duration::from_secs(60), Duration::from_millis(20));

    assert!(cache.is_relay("RELAY"));
    assert!(!cache.is_relay("CLIENT"));
    std::thread::sleep(Duration::from_millis(40));
    assert!(cache.is_relay("RELAY"));
    assert!(!cache.is_relay("CLIENT"));
    assert_eq!(
        calls.load(Ordering::SeqCst),
        3,
        "only the denied answer was looked up again"
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn cache_size_is_bounded() {
    let (lookup, _calls) = counting(Some(false));
    let cache = RelayOriginCache::new(lookup);

    for i in 0..(RELAY_ORIGIN_MAX_ENTRIES + 10) {
        cache.is_relay(&format!("ID{i}"));
    }
    assert!(
        cache.len() <= RELAY_ORIGIN_MAX_ENTRIES,
        "len {}",
        cache.len()
    );
}
