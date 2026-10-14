// Unit tests for `checkrelays` outcome classification (bingle_cli::checkrelays), issue #313: tell
// "no relays exist", "relays refused this account" and "relays found but silent" apart instead of
// reporting all three as "No relays discovered".
use std::time::Duration;

use bingle_cli::checkrelays::{
    RelayCheckPlan, RelayRef, RelayRefusal, describe_plan, plan_relay_check,
};
use bingle_core::api::bingle_api::REFUSAL_UNAUTHENTICATED;

fn relay(id: &str, address: &str) -> RelayRef {
    RelayRef {
        id: id.to_string(),
        address: address.to_string(),
    }
}

fn refusal(relay_id: &str) -> RelayRefusal {
    RelayRefusal {
        relay_id: relay_id.to_string(),
        reason: REFUSAL_UNAUTHENTICATED.to_string(),
    }
}

const WAIT: Duration = Duration::from_secs(30);

#[test]
#[cfg(not(target_os = "ios"))]
fn listed_relays_are_probed_even_when_discovery_or_refusals_disagree() {
    let plan = plan_relay_check(
        vec![relay("R1", "1.2.3.4:12121")],
        vec![relay("R2", "5.6.7.8:12121")],
        vec![refusal("R2")],
    );
    assert_eq!(
        plan,
        RelayCheckPlan::Probe(vec![relay("R1", "1.2.3.4:12121")])
    );
    assert!(describe_plan(&plan, 7, "ME", None, WAIT).is_empty());
}

#[test]
#[cfg(not(target_os = "ios"))]
fn nothing_on_chain_is_reported_as_no_relays_discovered() {
    let plan = plan_relay_check(Vec::new(), Vec::new(), Vec::new());
    assert_eq!(plan, RelayCheckPlan::NoneDiscovered);
    let lines = describe_plan(&plan, 7, "ME", None, WAIT);
    assert_eq!(lines.len(), 1);
    assert!(
        lines[0].starts_with("No relays discovered for app 7"),
        "{lines:?}"
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
fn a_refusal_wins_over_discovery_and_names_registration_when_we_have_no_handle() {
    let plan = plan_relay_check(
        Vec::new(),
        vec![relay("R1", "1.2.3.4:12121")],
        vec![refusal("R1")],
    );
    assert_eq!(plan, RelayCheckPlan::Refused(vec![refusal("R1")]));
    let lines = describe_plan(&plan, 7, "ME", Some(false), WAIT);
    assert!(
        lines[0].contains("ME") && lines[0].contains("no handle on app 7"),
        "{lines:?}"
    );
    assert!(lines[0].contains("bingle_cli register") && lines[0].contains("bingle_cli migrate"));
    assert_eq!(lines[1], "  relay R1 could not authenticate this account");
    assert!(lines.iter().all(|l| !l.contains("No relays discovered")));
}

#[test]
#[cfg(not(target_os = "ios"))]
fn a_refusal_of_a_registered_account_points_at_the_relays_chain_reads() {
    let plan = plan_relay_check(Vec::new(), Vec::new(), vec![refusal("R1")]);
    let lines = describe_plan(&plan, 7, "ME", Some(true), WAIT);
    assert!(lines[0].contains("has a handle on app 7"), "{lines:?}");
    assert!(
        lines[0].contains("could not read it from the chain"),
        "{lines:?}"
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
fn a_refusal_with_an_unchecked_registration_gives_both_causes() {
    let plan = plan_relay_check(Vec::new(), Vec::new(), vec![refusal("R1")]);
    let lines = describe_plan(&plan, 7, "ME", None, WAIT);
    assert!(
        lines[0].contains("either this account (ME) has no handle"),
        "{lines:?}"
    );
    assert!(lines[0].contains("could not read the chain"), "{lines:?}");
}

#[test]
#[cfg(not(target_os = "ios"))]
fn an_unknown_refusal_reason_is_reported_verbatim() {
    let plan = plan_relay_check(
        Vec::new(),
        Vec::new(),
        vec![RelayRefusal {
            relay_id: "R1".to_string(),
            reason: "busy".to_string(),
        }],
    );
    let lines = describe_plan(&plan, 7, "ME", None, WAIT);
    assert_eq!(lines[1], "  relay R1 refused: busy");
}

#[test]
#[cfg(not(target_os = "ios"))]
fn discovered_but_silent_relays_are_named() {
    let discovered = vec![relay("R1", "1.2.3.4:12121"), relay("R2", "5.6.7.8:12121")];
    let plan = plan_relay_check(Vec::new(), discovered.clone(), Vec::new());
    assert_eq!(plan, RelayCheckPlan::NoneAnswered(discovered));
    let lines = describe_plan(&plan, 7, "ME", None, WAIT);
    assert_eq!(
        lines,
        vec![
            "Found 2 relay(s) for app 7 but none answered within 30s:".to_string(),
            "  relay R1 @ 1.2.3.4:12121".to_string(),
            "  relay R2 @ 5.6.7.8:12121".to_string(),
        ]
    );
}
