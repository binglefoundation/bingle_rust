//! Unit tests for the on-chain registration seam (issue #15, A4).
//!
//! These exercise `run_registration` against a recording mock — no localnet required —
//! pinning the current call order and the short-circuit-on-failure behaviour. Later steps
//! (A1 fail-fast pre-check, A2 idempotent opt-ins) extend these tests on the same seam.

use std::cell::RefCell;

use bingle_core::api::bingle_api::BingleError;
use bingle_local::api::{RegistrationOps, run_registration};

/// Which step, if any, the mock should fail at.
#[derive(Clone, Copy, PartialEq)]
enum FailAt {
    None,
    OptInApp,
    OptInAsset,
    Price,
    Buy,
    Register,
}

struct RecordingOps {
    calls: RefCell<Vec<String>>,
    handle_owner: Option<String>,
    self_addr: String,
    price: u64,
    fail_at: FailAt,
    opted_in_app: bool,
    opted_in_asset: bool,
}

impl RecordingOps {
    fn new() -> Self {
        RecordingOps {
            calls: RefCell::new(Vec::new()),
            handle_owner: None,
            self_addr: "SELF_ADDR".to_string(),
            price: 1000,
            fail_at: FailAt::None,
            opted_in_app: false,
            opted_in_asset: false,
        }
    }

    fn failing_at(fail_at: FailAt) -> Self {
        let mut ops = RecordingOps::new();
        ops.fail_at = fail_at;
        ops
    }

    fn with_handle_owner(owner: Option<&str>) -> Self {
        let mut ops = RecordingOps::new();
        ops.handle_owner = owner.map(str::to_string);
        ops
    }

    fn already_opted_in(app: bool, asset: bool) -> Self {
        let mut ops = RecordingOps::new();
        ops.opted_in_app = app;
        ops.opted_in_asset = asset;
        ops
    }

    fn record(&self, call: impl Into<String>) {
        self.calls.borrow_mut().push(call.into());
    }

    fn err(step: &str) -> BingleError {
        BingleError::Other(format!("{} failed", step))
    }

    fn calls(&self) -> Vec<String> {
        self.calls.borrow().clone()
    }
}

impl RegistrationOps for RecordingOps {
    fn self_address(&self) -> Result<String, BingleError> {
        self.record("self_address");
        Ok(self.self_addr.clone())
    }

    fn handle_lookup(&self, _handle: &str) -> Result<Option<String>, BingleError> {
        self.record("handle_lookup");
        Ok(self.handle_owner.clone())
    }

    fn is_opted_in_app(&self) -> Result<bool, BingleError> {
        self.record("is_opted_in_app");
        Ok(self.opted_in_app)
    }

    fn is_opted_in_asset(&self) -> Result<bool, BingleError> {
        self.record("is_opted_in_asset");
        Ok(self.opted_in_asset)
    }

    fn opt_in_app(&self) -> Result<(), BingleError> {
        self.record("opt_in_app");
        if self.fail_at == FailAt::OptInApp {
            return Err(Self::err("opt_in_app"));
        }
        Ok(())
    }

    fn opt_in_to_asset(&self) -> Result<(), BingleError> {
        self.record("opt_in_to_asset");
        if self.fail_at == FailAt::OptInAsset {
            return Err(Self::err("opt_in_to_asset"));
        }
        Ok(())
    }

    fn get_bingle_price(&self) -> Result<u64, BingleError> {
        self.record("get_bingle_price");
        if self.fail_at == FailAt::Price {
            return Err(Self::err("get_bingle_price"));
        }
        Ok(self.price)
    }

    fn buy_bingle(&self, price_microalgos: u64) -> Result<(), BingleError> {
        self.record(format!("buy_bingle({})", price_microalgos));
        if self.fail_at == FailAt::Buy {
            return Err(Self::err("buy_bingle"));
        }
        Ok(())
    }

    fn register(&self, handle: &str) -> Result<(), BingleError> {
        self.record(format!("register({})", handle));
        if self.fail_at == FailAt::Register {
            return Err(Self::err("register"));
        }
        Ok(())
    }
}

#[test]
fn run_registration_drives_the_chain_in_order() {
    // Handle is free (handle_owner = None) and the account is not opted in, so the pre-check
    // passes and the full chain runs including both opt-ins.
    let ops = RecordingOps::new();
    run_registration(&ops, "alice").expect("registration should succeed");
    assert_eq!(
        ops.calls(),
        vec![
            "self_address".to_string(),
            "handle_lookup".to_string(),
            "is_opted_in_app".to_string(),
            "opt_in_app".to_string(),
            "is_opted_in_asset".to_string(),
            "opt_in_to_asset".to_string(),
            "get_bingle_price".to_string(),
            "buy_bingle(1000)".to_string(),
            "register(alice)".to_string(),
        ]
    );
}

#[test]
fn run_registration_stops_at_first_opt_in_failure() {
    let ops = RecordingOps::failing_at(FailAt::OptInApp);
    let err = run_registration(&ops, "alice").expect_err("should fail");
    assert!(err.to_string().contains("opt_in_app"));
    // Nothing after opt_in_app should have been attempted.
    assert_eq!(
        ops.calls(),
        vec![
            "self_address".to_string(),
            "handle_lookup".to_string(),
            "is_opted_in_app".to_string(),
            "opt_in_app".to_string(),
        ]
    );
}

#[test]
fn run_registration_stops_before_register_when_buy_fails() {
    let ops = RecordingOps::failing_at(FailAt::Buy);
    let err = run_registration(&ops, "alice").expect_err("should fail");
    assert!(err.to_string().contains("buy_bingle"));
    // register must never run once buy fails.
    assert_eq!(
        ops.calls(),
        vec![
            "self_address".to_string(),
            "handle_lookup".to_string(),
            "is_opted_in_app".to_string(),
            "opt_in_app".to_string(),
            "is_opted_in_asset".to_string(),
            "opt_in_to_asset".to_string(),
            "get_bingle_price".to_string(),
            "buy_bingle(1000)".to_string(),
        ]
    );
    assert!(!ops.calls().iter().any(|c| c.starts_with("register")));
}

#[test]
fn run_registration_skips_opt_ins_when_already_opted_in() {
    // Retry scenario (issue #15 A2): the account already holds the app and asset, so neither
    // opt-in runs again — only the read-only checks — and no min-balance is re-spent.
    let ops = RecordingOps::already_opted_in(true, true);
    run_registration(&ops, "alice").expect("registration should succeed");
    assert_eq!(
        ops.calls(),
        vec![
            "self_address".to_string(),
            "handle_lookup".to_string(),
            "is_opted_in_app".to_string(),
            "is_opted_in_asset".to_string(),
            "get_bingle_price".to_string(),
            "buy_bingle(1000)".to_string(),
            "register(alice)".to_string(),
        ]
    );
    assert!(
        !ops.calls()
            .iter()
            .any(|c| c == "opt_in_app" || c == "opt_in_to_asset")
    );
}

#[test]
fn run_registration_opts_in_asset_only_when_app_already_done() {
    // Mixed retry: app opt-in already succeeded on a prior attempt, asset did not. Only the
    // asset opt-in should be re-attempted.
    let ops = RecordingOps::already_opted_in(true, false);
    run_registration(&ops, "alice").expect("registration should succeed");
    assert_eq!(
        ops.calls(),
        vec![
            "self_address".to_string(),
            "handle_lookup".to_string(),
            "is_opted_in_app".to_string(),
            "is_opted_in_asset".to_string(),
            "opt_in_to_asset".to_string(),
            "get_bingle_price".to_string(),
            "buy_bingle(1000)".to_string(),
            "register(alice)".to_string(),
        ]
    );
    assert!(!ops.calls().iter().any(|c| c == "opt_in_app"));
}

#[test]
fn run_registration_fails_fast_when_handle_taken_by_other() {
    // The handle is owned by a different account: the pre-check must reject before any
    // opt-in / buy / register is attempted, so the balance is never touched (issue #15 A1).
    let ops = RecordingOps::with_handle_owner(Some("OTHER_ACCOUNT"));
    let err = run_registration(&ops, "alice").expect_err("should fail fast");

    match err {
        BingleError::HandleTaken(owner) => assert_eq!(owner, "OTHER_ACCOUNT"),
        other => panic!("expected HandleTaken, got {:?}", other),
    }

    // Only the read-only pre-check ran — no spending calls at all.
    assert_eq!(
        ops.calls(),
        vec!["self_address".to_string(), "handle_lookup".to_string()]
    );
    let spent = ["opt_in_app", "opt_in_to_asset", "buy_bingle", "register"];
    assert!(
        !ops.calls()
            .iter()
            .any(|c| spent.iter().any(|s| c.starts_with(s))),
        "no on-chain spend should occur when the handle is taken: {:?}",
        ops.calls()
    );
}

#[test]
fn run_registration_proceeds_when_handle_owned_by_self() {
    // Re-registering our own handle is not a collision: the pre-check passes and the chain
    // runs (self_addr matches the recorded owner).
    let ops = RecordingOps::with_handle_owner(Some("SELF_ADDR"));
    run_registration(&ops, "alice").expect("re-registering own handle should proceed");
    assert_eq!(
        ops.calls(),
        vec![
            "self_address".to_string(),
            "handle_lookup".to_string(),
            "is_opted_in_app".to_string(),
            "opt_in_app".to_string(),
            "is_opted_in_asset".to_string(),
            "opt_in_to_asset".to_string(),
            "get_bingle_price".to_string(),
            "buy_bingle(1000)".to_string(),
            "register(alice)".to_string(),
        ]
    );
}

#[test]
fn run_registration_surfaces_register_failure() {
    let ops = RecordingOps::failing_at(FailAt::Register);
    let err = run_registration(&ops, "bob").expect_err("should fail");
    assert!(err.to_string().contains("register"));
    // The whole chain ran; register was reached and failed.
    assert_eq!(
        ops.calls().last().map(String::as_str),
        Some("register(bob)")
    );
}
