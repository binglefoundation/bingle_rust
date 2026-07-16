use bingle_local::api::REQUIRED_ALGO;
use bingle_local::api::bingle_local_api::BingleLocalApi;
use bingle_local::api::bingle_local_api_impl::{
    BingleApiLocalImpl, LocalApiConfig, keypair_status_from_facts,
};

/// Small helper for comparing the f64 required_algo top-up.
fn approx(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

#[test]
fn test_keypair_status_none_when_no_keypair() {
    let api = BingleApiLocalImpl::new(LocalApiConfig::default());
    let status = api.keypair_status().expect("keypair_status should succeed");
    assert_eq!(status.status, "None");
    assert!(status.id.is_none());
    assert!(status.handle.is_none());
    assert!(status.required_algo.is_none());
}

#[test]
fn test_keypair_status_has_id_after_generate() {
    let mut api = BingleApiLocalImpl::new(LocalApiConfig::default());
    let kp = api
        .generate_keypair()
        .expect("generate_keypair should succeed");

    // With default config (no real blockchain), get_algo_ops will succeed but
    // balance/asset checks will fail. keypair_status should still return a status
    // with the algorand id set. The exact status depends on blockchain availability;
    // since we have no real node, we expect an error or UNFUNDED.
    let result = api.keypair_status();
    match result {
        Ok(status) => {
            // Should have the id set
            let id = status.id.expect("id should be present when keypair exists");
            assert_eq!(id, kp.id);
            // Status should not be "None" since keypair exists
            assert_ne!(status.status, "None");
        }
        Err(_) => {
            // Blockchain query failure is acceptable in unit tests without a real node
        }
    }
}

#[test]
fn test_status_active_when_asset_and_handle() {
    let status = keypair_status_from_facts(
        "ID_ACTIVE".to_string(),
        true,
        Some("alice".to_string()),
        // balance and target are irrelevant when ACTIVE
        0.0,
        REQUIRED_ALGO,
    );
    assert_eq!(status.status, "ACTIVE");
    assert_eq!(status.id.as_deref(), Some("ID_ACTIVE"));
    assert_eq!(status.handle.as_deref(), Some("alice"));
    assert!(status.required_algo.is_none());
}

#[test]
fn test_status_falls_through_to_balance_when_asset_but_no_handle() {
    // Holds the Bingle$ asset but has no Handle entry: must not be ACTIVE.
    // With a funded balance we expect FUNDED, not ACTIVE.
    let funded =
        keypair_status_from_facts("ID_NO_HANDLE".to_string(), true, None, 2.0, REQUIRED_ALGO);
    assert_eq!(funded.status, "FUNDED");
    assert_eq!(funded.id.as_deref(), Some("ID_NO_HANDLE"));
    assert!(funded.handle.is_none());
    assert!(funded.required_algo.is_none());

    // With an insufficient balance we expect UNFUNDED.
    let unfunded =
        keypair_status_from_facts("ID_NO_HANDLE".to_string(), true, None, 0.0, REQUIRED_ALGO);
    assert_eq!(unfunded.status, "UNFUNDED");
    assert_eq!(unfunded.id.as_deref(), Some("ID_NO_HANDLE"));
    assert!(unfunded.handle.is_none());
    assert!(unfunded.required_algo.is_some());
}

#[test]
fn test_status_unfunded_when_no_asset_and_low_balance() {
    let status = keypair_status_from_facts("ID_POOR".to_string(), false, None, 0.5, REQUIRED_ALGO);
    assert_eq!(status.status, "UNFUNDED");
    assert!(status.handle.is_none());
    assert!(status.required_algo.is_some());
}

#[test]
fn test_required_algo_is_full_target_when_balance_zero() {
    // A brand-new account should be asked for the whole adequate-funding target.
    let status = keypair_status_from_facts("ID_EMPTY".to_string(), false, None, 0.0, REQUIRED_ALGO);
    assert_eq!(status.status, "UNFUNDED");
    let required = status
        .required_algo
        .expect("required_algo should be set when unfunded");
    assert!(
        approx(required, REQUIRED_ALGO),
        "expected {} got {}",
        REQUIRED_ALGO,
        required
    );
}

#[test]
fn test_required_algo_is_shortfall_when_semi_funded() {
    // Semi-funded (issue #15): a partial balance means we only ask for the delta, not the
    // full target again. Balance 0.04 -> required = target - 0.04.
    let balance = 0.04;
    let status =
        keypair_status_from_facts("ID_SEMI".to_string(), false, None, balance, REQUIRED_ALGO);
    assert_eq!(status.status, "UNFUNDED");
    let required = status
        .required_algo
        .expect("required_algo should be set when unfunded");
    assert!(
        approx(required, REQUIRED_ALGO - balance),
        "expected {} got {}",
        REQUIRED_ALGO - balance,
        required
    );
    // And it must be strictly less than the target — the whole point of the top-up.
    assert!(required < REQUIRED_ALGO);
}

#[test]
fn test_required_algo_none_when_funded() {
    // At or above the target the account is FUNDED and nothing more is required.
    let status = keypair_status_from_facts(
        "ID_FUNDED".to_string(),
        false,
        None,
        REQUIRED_ALGO,
        REQUIRED_ALGO,
    );
    assert_eq!(status.status, "FUNDED");
    assert!(status.required_algo.is_none());
}

#[test]
fn test_status_uses_provided_target_not_flat_constant() {
    // A3b: the target is supplied by the caller (derived from live cost), not baked in.
    // A tiny target should mark a small balance FUNDED even though it is well below REQUIRED_ALGO.
    let target = 0.5;
    let funded = keypair_status_from_facts("ID_CHEAP".to_string(), false, None, 0.6, target);
    assert_eq!(funded.status, "FUNDED");
    assert!(funded.required_algo.is_none());

    // And the shortfall is measured against that target, not REQUIRED_ALGO.
    let unfunded = keypair_status_from_facts("ID_CHEAP".to_string(), false, None, 0.2, target);
    assert_eq!(unfunded.status, "UNFUNDED");
    let required = unfunded.required_algo.expect("required when unfunded");
    assert!(
        approx(required, target - 0.2),
        "expected {} got {}",
        target - 0.2,
        required
    );
}

// --- ACTIVE memoization: no blockchain read once registered (issue #18/#31) ---

fn temp_state_path(name: &str) -> String {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "bingle_test_{}_{}_{}.json",
        name,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    p.to_string_lossy().to_string()
}

/// A persisted state with a keypair + own_handle represents an ACTIVE account. keypair_status must
/// serve ACTIVE from the memo with no blockchain read — the default config has no node, so a live
/// read would fail; returning ACTIVE proves it short-circuited.
#[test]
fn test_keypair_status_active_from_memoized_handle_without_blockchain() {
    let path = temp_state_path("memo_active");
    std::fs::write(
        &path,
        r#"{ "keypair": { "id": "MEMOID", "passphrase": "seed" }, "contacts": [], "messages": [], "own_handle": "alice" }"#,
    )
    .expect("write state");

    let mut api = BingleApiLocalImpl::new(LocalApiConfig::default());
    api.load(&path).expect("load should succeed");

    let status = api
        .keypair_status()
        .expect("keypair_status should succeed from the memo");
    assert_eq!(status.status, "ACTIVE");
    assert_eq!(status.id.as_deref(), Some("MEMOID"));
    assert_eq!(status.handle.as_deref(), Some("alice"));
    assert!(!status.stale);

    let _ = std::fs::remove_file(&path);
}

/// Generating a fresh keypair clears the memo so the new (unregistered) account no longer
/// short-circuits to ACTIVE.
#[test]
fn test_generate_keypair_clears_memoized_active() {
    let path = temp_state_path("memo_clear");
    std::fs::write(
        &path,
        r#"{ "keypair": { "id": "OLDID", "passphrase": "seed" }, "contacts": [], "messages": [], "own_handle": "alice" }"#,
    )
    .expect("write state");

    let mut api = BingleApiLocalImpl::new(LocalApiConfig::default());
    api.load(&path).expect("load");
    assert_eq!(api.keypair_status().unwrap().status, "ACTIVE");

    api.generate_keypair().expect("generate");
    // No memo now: with default config and no node, status must not be ACTIVE (either a non-ACTIVE
    // status or a blockchain error — both acceptable, but never a false ACTIVE).
    match api.keypair_status() {
        Ok(s) => assert_ne!(s.status, "ACTIVE", "memo should be cleared on regenerate"),
        Err(_) => {}
    }

    let _ = std::fs::remove_file(&path);
}

/// The memoized handle round-trips through save/load into a fresh instance.
#[test]
fn test_memoized_handle_survives_save_load() {
    let seed = temp_state_path("memo_persist_seed");
    std::fs::write(
        &seed,
        r#"{ "keypair": { "id": "PID", "passphrase": "seed" }, "contacts": [], "messages": [], "own_handle": "bob" }"#,
    )
    .expect("write seed");

    let mut api = BingleApiLocalImpl::new(LocalApiConfig::default());
    api.load(&seed).expect("load");

    let out = temp_state_path("memo_persist_out");
    api.save(&out).expect("save");

    let mut api2 = BingleApiLocalImpl::new(LocalApiConfig::default());
    api2.load(&out).expect("reload");
    let status = api2.keypair_status().expect("status");
    assert_eq!(status.status, "ACTIVE");
    assert_eq!(status.handle.as_deref(), Some("bob"));

    let _ = std::fs::remove_file(&seed);
    let _ = std::fs::remove_file(&out);
}
