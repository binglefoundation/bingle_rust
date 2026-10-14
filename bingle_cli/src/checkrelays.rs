//! Outcome classification for `bingle_cli checkrelays` (issue #313).
//!
//! `checkrelays` used to print "No relays discovered" whenever the engine's relay finder had
//! nothing to list, which covered three different situations: no relay exists for the app, the
//! relays refused this account, and the relays were found but did not answer. These pure helpers
//! tell them apart so the command can name the real cause. The network work stays in `main.rs`.

use std::time::Duration;

use bingle_core::api::bingle_api::REFUSAL_UNAUTHENTICATED;

/// How long `checkrelays` waits for relay discovery to settle (the engine registering with a
/// relay, or a relay refusing us) before it reports. Selecting a relay probes candidates one at a
/// time, so this allows for a few slow candidates.
pub const DISCOVERY_WAIT: Duration = Duration::from_secs(30);

/// A relay named in a report: its id (Algorand address) and network address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelayRef {
    /// The relay's id (Algorand address).
    pub id: String,
    /// The relay's network address, as `host:port`.
    pub address: String,
}

/// A relay's refusal of this account, as recorded by the engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelayRefusal {
    /// The refusing relay's id.
    pub relay_id: String,
    /// The reason the relay gave.
    pub reason: String,
}

/// What `checkrelays` should do after discovery has settled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelayCheckPlan {
    /// The engine found relays: probe each of them.
    Probe(Vec<RelayRef>),
    /// No account on the app advertises a relay endpoint.
    NoneDiscovered,
    /// Relays refused this account.
    Refused(Vec<RelayRefusal>),
    /// Relays were discovered on chain but none answered.
    NoneAnswered(Vec<RelayRef>),
}

/// Decide the outcome from what the engine lists (`listed`), what chain discovery returns
/// (`discovered`), and the refusals the engine recorded. Relays the engine lists win; otherwise a
/// refusal explains the silence; otherwise discovery tells "no relays" from "no answer".
pub fn plan_relay_check(
    listed: Vec<RelayRef>,
    discovered: Vec<RelayRef>,
    refusals: Vec<RelayRefusal>,
) -> RelayCheckPlan {
    if !listed.is_empty() {
        RelayCheckPlan::Probe(listed)
    } else if !refusals.is_empty() {
        RelayCheckPlan::Refused(refusals)
    } else if discovered.is_empty() {
        RelayCheckPlan::NoneDiscovered
    } else {
        RelayCheckPlan::NoneAnswered(discovered)
    }
}

/// The lines to report for a plan other than [`RelayCheckPlan::Probe`] (which reports per-relay
/// probe results instead, so it yields no lines here).
///
/// `own_handle_registered` is whether this account has a handle on the app, checked on chain when
/// a relay refuses us: `Some(false)` names registration as the cause, `Some(true)` says the
/// account is registered so the relay must have failed to read the chain, and `None` (the check
/// itself failed) gives both possibilities.
pub fn describe_plan(
    plan: &RelayCheckPlan,
    app_id: u64,
    my_address: &str,
    own_handle_registered: Option<bool>,
    waited: Duration,
) -> Vec<String> {
    match plan {
        RelayCheckPlan::Probe(_) => Vec::new(),
        RelayCheckPlan::NoneDiscovered => vec![format!(
            "No relays discovered for app {app_id}: no account advertises a relay endpoint on chain"
        )],
        RelayCheckPlan::Refused(refusals) => {
            let cause = match own_handle_registered {
                Some(false) => format!(
                    "this account ({my_address}) has no handle on app {app_id}: register it \
                     (bingle_cli register) or, if it is registered on an earlier app, migrate it \
                     (bingle_cli migrate)"
                ),
                Some(true) => format!(
                    "this account ({my_address}) has a handle on app {app_id}, so the relay could \
                     not read it from the chain; check the relay's Algorand node"
                ),
                None => format!(
                    "either this account ({my_address}) has no handle on app {app_id} (register or \
                     migrate it), or the relay could not read the chain"
                ),
            };
            let mut lines = vec![format!("Relays refused this account: {cause}")];
            for r in refusals {
                if r.reason == REFUSAL_UNAUTHENTICATED {
                    lines.push(format!(
                        "  relay {} could not authenticate this account",
                        r.relay_id
                    ));
                } else {
                    lines.push(format!("  relay {} refused: {}", r.relay_id, r.reason));
                }
            }
            lines
        }
        RelayCheckPlan::NoneAnswered(discovered) => {
            let mut lines = vec![format!(
                "Found {} relay(s) for app {app_id} but none answered within {}s:",
                discovered.len(),
                waited.as_secs()
            )];
            for r in discovered {
                lines.push(format!("  relay {} @ {}", r.id, r.address));
            }
            lines
        }
    }
}
