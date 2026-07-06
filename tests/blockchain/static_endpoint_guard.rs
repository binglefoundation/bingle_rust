// Tests for AlgoBingle::should_clear_static_endpoint, the shutdown guard that
// prevents an old task from clobbering the record a replacement task has just
// registered under the same account (AWS redeploy race).

use rust_comms::blockchain::algo_bingle::AlgoBingle;

#[test]
#[cfg(not(target_os = "ios"))]
pub fn clears_when_on_chain_record_matches_ours() {
    assert!(AlgoBingle::should_clear_static_endpoint(
        Some("id,1.2.3.4:5000,relay,date,sig"),
        Some("id,1.2.3.4:5000,relay,date,sig"),
    ));
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn skips_when_on_chain_record_differs_from_ours() {
    // a replacement task registered a newer record; do not clear it
    assert!(!AlgoBingle::should_clear_static_endpoint(
        Some("id,1.2.3.4:5000,relay,2026-07-02T00:00:00Z,sig"),
        Some("id,1.2.3.4:5000,relay,2026-07-03T00:00:00Z,sig"),
    ));
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn skips_when_this_process_never_registered() {
    assert!(!AlgoBingle::should_clear_static_endpoint(
        None,
        Some("id,1.2.3.4:5000,relay,date,sig"),
    ));
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn skips_when_no_record_is_on_chain() {
    assert!(!AlgoBingle::should_clear_static_endpoint(
        Some("id,1.2.3.4:5000,relay,date,sig"),
        None,
    ));
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn skips_when_neither_side_has_a_record() {
    assert!(!AlgoBingle::should_clear_static_endpoint(None, None));
}
