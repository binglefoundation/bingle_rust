// Unit tests for the pure first-run registration decision (bingle_cli::chat_register).
use bingle_cli::chat_register::{AccountStatus, CredentialGap, StartupDecision, decide_startup};

fn active(handle: &str, balance: f64, min: f64) -> AccountStatus {
    AccountStatus::Active {
        id: "ACCT_ID".to_string(),
        handle: handle.to_string(),
        balance_algos: balance,
        operating_min_algos: min,
    }
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn no_keypair_with_passphrase_and_handle_registers() {
    let d = decide_startup(&AccountStatus::NoKeypair, Some("alice"), true);
    assert_eq!(
        d,
        StartupDecision::Register {
            handle: "alice".to_string()
        }
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn no_keypair_missing_handle_needs_credentials() {
    let d = decide_startup(&AccountStatus::NoKeypair, None, true);
    assert_eq!(
        d,
        StartupDecision::NeedCredentials {
            gap: CredentialGap::NoAccount
        }
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn no_keypair_missing_passphrase_needs_credentials() {
    let d = decide_startup(&AccountStatus::NoKeypair, Some("alice"), false);
    assert_eq!(
        d,
        StartupDecision::NeedCredentials {
            gap: CredentialGap::NoAccount
        }
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn unfunded_blocks_with_shortfall() {
    let status = AccountStatus::Unfunded {
        id: "ACCT_ID".to_string(),
        shortfall_algos: 0.75,
    };
    let d = decide_startup(&status, Some("alice"), true);
    assert_eq!(
        d,
        StartupDecision::Fund {
            id: "ACCT_ID".to_string(),
            needed_algos: 0.75,
        }
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn funded_with_handle_registers() {
    let status = AccountStatus::Funded {
        id: "ACCT_ID".to_string(),
    };
    let d = decide_startup(&status, Some("alice"), false);
    assert_eq!(
        d,
        StartupDecision::Register {
            handle: "alice".to_string()
        }
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn funded_without_handle_needs_handle() {
    let status = AccountStatus::Funded {
        id: "ACCT_ID".to_string(),
    };
    let d = decide_startup(&status, None, false);
    assert_eq!(
        d,
        StartupDecision::NeedCredentials {
            gap: CredentialGap::FundedNeedsHandle
        }
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn active_above_minimum_proceeds() {
    let d = decide_startup(&active("alice", 2.0, 1.5), None, false);
    assert_eq!(
        d,
        StartupDecision::Proceed {
            handle: "alice".to_string()
        }
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn active_matching_handle_proceeds() {
    let d = decide_startup(&active("alice", 2.0, 1.5), Some("alice"), false);
    assert_eq!(
        d,
        StartupDecision::Proceed {
            handle: "alice".to_string()
        }
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn active_mismatching_handle_is_rejected() {
    let d = decide_startup(&active("alice", 2.0, 1.5), Some("bob"), false);
    assert_eq!(
        d,
        StartupDecision::HandleMismatch {
            existing: "alice".to_string(),
            supplied: "bob".to_string(),
        }
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn active_below_minimum_blocks_for_funding() {
    // Balance 1.0, minimum 1.5 -> needs 0.5 top-up.
    let d = decide_startup(&active("alice", 1.0, 1.5), Some("alice"), false);
    assert_eq!(
        d,
        StartupDecision::Fund {
            id: "ACCT_ID".to_string(),
            needed_algos: 0.5,
        }
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn active_mismatch_takes_precedence_over_low_balance() {
    // Even below the minimum, a mismatching handle is rejected first (no re-registration).
    let d = decide_startup(&active("alice", 0.1, 1.5), Some("bob"), false);
    assert_eq!(
        d,
        StartupDecision::HandleMismatch {
            existing: "alice".to_string(),
            supplied: "bob".to_string(),
        }
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn second_run_active_needs_no_passphrase_or_handle() {
    // The core "second run needs no --passphrase" guarantee at the decision level: an ACTIVE,
    // adequately funded account proceeds with neither a passphrase nor a handle on the CLI.
    let d = decide_startup(&active("alice", 2.0, 1.5), None, false);
    assert_eq!(
        d,
        StartupDecision::Proceed {
            handle: "alice".to_string()
        }
    );
}
