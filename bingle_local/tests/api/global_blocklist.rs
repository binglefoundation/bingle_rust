// The global blocklist cache (issue #95): a block is kept for good, "not blocked" is trusted for the
// refresh period, and a failed lookup is neither cached nor treated as a block.
use std::cell::Cell;
use std::time::Duration;

use bingle_local::api::GlobalBlocklist;

const SENDER: &str = "SENDERADDRESS";

/// A lookup that counts its calls and answers `answer`.
fn counting(
    calls: &Cell<u32>,
    answer: Result<Option<bool>, String>,
) -> impl FnOnce(&str) -> Result<Option<bool>, String> + '_ {
    move |_| {
        calls.set(calls.get() + 1);
        answer
    }
}

#[test]
fn a_block_is_kept_without_asking_again() {
    let blocklist = GlobalBlocklist::new(Duration::ZERO);
    let calls = Cell::new(0);
    assert!(blocklist.is_blocked(SENDER, counting(&calls, Ok(Some(true)))));
    // Even with a zero refresh period, and even if the chain now says otherwise (the sender opted
    // out, clearing its local state), the block stands and the chain isn't asked.
    assert!(blocklist.is_blocked(SENDER, counting(&calls, Ok(None))));
    assert_eq!(calls.get(), 1);
}

#[test]
fn not_blocked_is_trusted_for_the_refresh_period() {
    let blocklist = GlobalBlocklist::new(Duration::from_secs(600));
    let calls = Cell::new(0);
    assert!(!blocklist.is_blocked(SENDER, counting(&calls, Ok(Some(false)))));
    assert!(!blocklist.is_blocked(SENDER, counting(&calls, Ok(Some(true)))));
    assert_eq!(calls.get(), 1, "the second check used the cache");
}

#[test]
fn a_block_set_after_a_clear_answer_takes_effect_at_the_refresh() {
    let blocklist = GlobalBlocklist::new(Duration::ZERO);
    let calls = Cell::new(0);
    assert!(!blocklist.is_blocked(SENDER, counting(&calls, Ok(Some(false)))));
    assert!(blocklist.is_blocked(SENDER, counting(&calls, Ok(Some(true)))));
    assert_eq!(calls.get(), 2);
    assert_eq!(blocklist.cached(SENDER), Some(true));
}

#[test]
fn an_account_that_is_not_opted_in_is_not_blocked() {
    let blocklist = GlobalBlocklist::new(Duration::from_secs(600));
    let calls = Cell::new(0);
    assert!(!blocklist.is_blocked(SENDER, counting(&calls, Ok(None))));
    assert_eq!(blocklist.cached(SENDER), Some(false));
}

#[test]
fn a_failed_lookup_is_not_a_block_and_is_not_cached() {
    let blocklist = GlobalBlocklist::new(Duration::from_secs(600));
    let calls = Cell::new(0);
    assert!(!blocklist.is_blocked(SENDER, counting(&calls, Err("node unreachable".into()))));
    assert_eq!(blocklist.cached(SENDER), None);
    assert!(blocklist.is_blocked(SENDER, counting(&calls, Ok(Some(true)))));
    assert_eq!(calls.get(), 2);
}

#[test]
fn a_later_clear_answer_does_not_lift_a_block() {
    let blocklist = GlobalBlocklist::new(Duration::from_secs(600));
    blocklist.record(SENDER, true);
    blocklist.record(SENDER, false);
    assert_eq!(blocklist.cached(SENDER), Some(true));
}

#[test]
fn senders_are_cached_separately() {
    let blocklist = GlobalBlocklist::new(Duration::from_secs(600));
    blocklist.record(SENDER, true);
    assert_eq!(blocklist.cached("OTHERADDRESS"), None);
}

// Issue #531: the blocklist can be switched off and on while running, keeping its cache.

#[test]
fn a_new_blocklist_is_switched_on() {
    let blocklist = GlobalBlocklist::new(Duration::from_secs(600));
    assert!(blocklist.is_enabled());
    assert!(blocklist.when_enabled().is_some());
}

#[test]
fn a_blocklist_built_switched_off_is_treated_as_none() {
    let blocklist = GlobalBlocklist::with_enabled(Duration::from_secs(600), false);
    assert!(!blocklist.is_enabled());
    assert!(blocklist.when_enabled().is_none());
}

#[test]
fn switching_off_and_on_keeps_a_cached_block() {
    let blocklist = GlobalBlocklist::new(Duration::from_secs(600));
    blocklist.record(SENDER, true);

    blocklist.set_enabled(false);
    assert!(blocklist.when_enabled().is_none());
    assert_eq!(blocklist.cached(SENDER), Some(true), "kept while off");

    blocklist.set_enabled(true);
    let calls = Cell::new(0);
    let active = blocklist.when_enabled().expect("switched on again");
    assert!(active.is_blocked(SENDER, counting(&calls, Ok(None))));
    assert_eq!(
        calls.get(),
        0,
        "the cached block is used without a chain read"
    );
}
