//! Tests for on-chain account status resolution (issue #388).

use std::cell::RefCell;

use anyhow::{Result, anyhow};
use bingle_core::blockchain::account_status::{AccountStatusReads, resolve_account_status};

const APP_ID: u64 = 11;
const ASSET_ID: u64 = 22;
const FALLBACK: f64 = 1.5;

/// Canned chain reads that record which reads were made.
struct MockReads {
    opted_in: std::result::Result<bool, String>,
    handle: Option<String>,
    balance: Option<f64>,
    required: std::result::Result<f64, String>,
    calls: RefCell<Vec<String>>,
}

impl MockReads {
    fn new(opted_in: bool, handle: Option<&str>, balance: Option<f64>) -> Self {
        MockReads {
            opted_in: Ok(opted_in),
            handle: handle.map(str::to_string),
            balance,
            required: Ok(1.0),
            calls: RefCell::new(Vec::new()),
        }
    }

    fn calls(&self) -> Vec<String> {
        self.calls.borrow().clone()
    }
}

impl AccountStatusReads for MockReads {
    fn is_opted_in_to_asset(&self, address: &str) -> Result<bool> {
        self.calls
            .borrow_mut()
            .push(format!("opted_in:{}", address));
        self.opted_in.clone().map_err(|e| anyhow!(e))
    }

    fn handle(&self, address: &str) -> Result<Option<String>> {
        self.calls.borrow_mut().push(format!("handle:{}", address));
        Ok(self.handle.clone())
    }

    fn balance_algos(&self) -> Result<Option<f64>> {
        self.calls.borrow_mut().push("balance".to_string());
        Ok(self.balance)
    }

    fn required_funding_algos(&self) -> Result<f64> {
        self.calls.borrow_mut().push("required".to_string());
        self.required.clone().map_err(|e| anyhow!(e))
    }
}

#[test]
fn registered_account_skips_balance_reads() {
    let reads = MockReads::new(true, Some("alice"), Some(5.0));
    let facts = resolve_account_status(&reads, "ADDR".to_string(), APP_ID, ASSET_ID, FALLBACK)
        .expect("facts should resolve");
    assert!(facts.is_registered());
    assert_eq!(facts.address, "ADDR");
    assert_eq!(facts.handle.as_deref(), Some("alice"));
    assert_eq!(reads.calls(), vec!["opted_in:ADDR", "handle:ADDR"]);
}

#[test]
fn unregistered_account_reads_balance_and_target() {
    let reads = MockReads::new(false, None, Some(0.25));
    let facts = resolve_account_status(&reads, "ADDR".to_string(), APP_ID, ASSET_ID, FALLBACK)
        .expect("facts should resolve");
    assert!(!facts.is_registered());
    assert!(facts.handle.is_none());
    assert_eq!(facts.balance_algos, 0.25);
    assert_eq!(facts.required_funding_algos, 1.0);
    // Not opted in: the handle is never read.
    assert_eq!(reads.calls(), vec!["opted_in:ADDR", "balance", "required"]);
}

#[test]
fn unknown_account_counts_as_zero_balance() {
    let reads = MockReads::new(false, None, None);
    let facts = resolve_account_status(&reads, "ADDR".to_string(), APP_ID, ASSET_ID, FALLBACK)
        .expect("facts should resolve");
    assert_eq!(facts.balance_algos, 0.0);
}

#[test]
fn asset_without_handle_is_not_registered() {
    let reads = MockReads::new(true, None, Some(2.0));
    let facts = resolve_account_status(&reads, "ADDR".to_string(), APP_ID, ASSET_ID, FALLBACK)
        .expect("facts should resolve");
    assert!(facts.has_asset);
    assert!(!facts.is_registered());
    assert_eq!(facts.balance_algos, 2.0);
}

#[test]
fn unconfigured_app_skips_handle_read() {
    let reads = MockReads::new(true, Some("alice"), Some(2.0));
    let facts = resolve_account_status(&reads, "ADDR".to_string(), 0, ASSET_ID, FALLBACK)
        .expect("facts should resolve");
    assert!(!facts.is_registered());
    assert!(!reads.calls().iter().any(|c| c.starts_with("handle")));
}

#[test]
fn unconfigured_asset_skips_opt_in_read() {
    let reads = MockReads::new(true, Some("alice"), Some(2.0));
    let facts = resolve_account_status(&reads, "ADDR".to_string(), APP_ID, 0, FALLBACK)
        .expect("facts should resolve");
    assert!(!facts.has_asset);
    assert_eq!(reads.calls(), vec!["balance", "required"]);
}

#[test]
fn failed_required_funding_falls_back_to_caller_target() {
    let mut reads = MockReads::new(false, None, Some(0.0));
    reads.required = Err("price read failed".to_string());
    let facts = resolve_account_status(&reads, "ADDR".to_string(), APP_ID, ASSET_ID, FALLBACK)
        .expect("facts should resolve");
    assert_eq!(facts.required_funding_algos, FALLBACK);
}

#[test]
fn failed_opt_in_read_is_a_failure_with_context() {
    let mut reads = MockReads::new(false, None, Some(0.0));
    reads.opted_in = Err("node down".to_string());
    let failure = resolve_account_status(&reads, "ADDR".to_string(), APP_ID, ASSET_ID, FALLBACK)
        .expect_err("a failed read should fail");
    assert!(failure.context.contains("asset opt-in"));
    assert!(failure.error.to_string().contains("node down"));
}
