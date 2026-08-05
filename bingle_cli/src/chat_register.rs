//! First-run registration decision for the `chat` command.
//!
//! A user can chat only from a **registered** account. There are two supported paths (either
//! works): register beforehand with `bingle_cli register`, or register on the first `chat` run by
//! supplying a funded `--passphrase` and a `--handle`. This module holds the pure decision — given
//! the account's resolved status and the credentials on the command line, what should `chat` do:
//! proceed, register, block-and-fund, reject, or ask for credentials?
//!
//! The decision is kept free of any blockchain access so it can be unit-tested exhaustively; the
//! `cmd_chat` runtime gathers the facts (status, balance) and executes the chosen action.

/// The account's resolved startup status.
///
/// Mirrors the relevant `bingle_local::keypair_status()` outcomes, plus the extra balance facts the
/// `ACTIVE` case needs (which `keypair_status()` does not inspect — see the issue). All ALGO amounts
/// are in whole ALGOs.
#[derive(Debug, Clone, PartialEq)]
pub enum AccountStatus {
    /// No usable keypair yet: no state file, or a state file with no keypair.
    NoKeypair,
    /// A keypair exists but the balance is below the funding target. `shortfall_algos` is the
    /// top-up needed to reach it (not the full target).
    Unfunded { id: String, shortfall_algos: f64 },
    /// Funded but no handle registered yet.
    Funded { id: String },
    /// Registered (holds Bingle$ and a handle). `balance_algos` is the current balance and
    /// `operating_min_algos` the minimum needed to run a session (`chat` reads these itself,
    /// because `keypair_status()` reports `ACTIVE` without inspecting the balance).
    Active {
        id: String,
        handle: String,
        balance_algos: f64,
        operating_min_algos: f64,
    },
}

/// Why `chat` cannot proceed and what the user must supply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialGap {
    /// No registered account and no credentials to make one.
    NoAccount,
    /// The account is funded but unregistered; a `--handle` is needed to register it.
    FundedNeedsHandle,
}

/// What `chat` should do at startup.
#[derive(Debug, Clone, PartialEq)]
pub enum StartupDecision {
    /// The account is registered and adequately funded — go to the chat loop as `handle`.
    Proceed { handle: String },
    /// Register this account under `handle` (importing from the passphrase first if needed).
    Register { handle: String },
    /// Blocked: the account needs funding. `needed_algos` is the top-up to add to `id`.
    Fund { id: String, needed_algos: f64 },
    /// Blocked: registration is needed but the required credentials are missing.
    NeedCredentials { gap: CredentialGap },
    /// Blocked: an explicit `--handle` conflicts with the account's already-registered handle.
    HandleMismatch { existing: String, supplied: String },
}

/// Decide what `chat` should do, given the resolved account `status`, the explicit `--handle` from
/// the command line (if any), and whether a `--passphrase` was supplied.
///
/// Precedence for an already-registered (`ACTIVE`) account: an explicit `--handle` that differs
/// from the registered handle is rejected before anything else (no re-registration); a matching or
/// omitted handle is accepted, after which the balance is checked against the operating minimum.
pub fn decide_startup(
    status: &AccountStatus,
    cli_handle: Option<&str>,
    have_passphrase: bool,
) -> StartupDecision {
    match status {
        AccountStatus::NoKeypair => match (have_passphrase, cli_handle) {
            // Both credentials present: import from the passphrase and register the handle.
            (true, Some(handle)) => StartupDecision::Register {
                handle: handle.to_string(),
            },
            _ => StartupDecision::NeedCredentials {
                gap: CredentialGap::NoAccount,
            },
        },
        AccountStatus::Unfunded {
            id,
            shortfall_algos,
        } => StartupDecision::Fund {
            id: id.clone(),
            needed_algos: *shortfall_algos,
        },
        AccountStatus::Funded { id: _ } => match cli_handle {
            Some(handle) => StartupDecision::Register {
                handle: handle.to_string(),
            },
            None => StartupDecision::NeedCredentials {
                gap: CredentialGap::FundedNeedsHandle,
            },
        },
        AccountStatus::Active {
            id,
            handle,
            balance_algos,
            operating_min_algos,
        } => {
            // Reject a mismatching explicit handle before considering anything else.
            if let Some(supplied) = cli_handle
                && supplied != handle
            {
                return StartupDecision::HandleMismatch {
                    existing: handle.clone(),
                    supplied: supplied.to_string(),
                };
            }
            if balance_algos < operating_min_algos {
                return StartupDecision::Fund {
                    id: id.clone(),
                    needed_algos: operating_min_algos - balance_algos,
                };
            }
            StartupDecision::Proceed {
                handle: handle.clone(),
            }
        }
    }
}
