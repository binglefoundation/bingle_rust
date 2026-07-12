// tests/blockchain/algo_bingle/required_funding.rs
// Unit-style tests for the registration cost model, without a live Algod node (issue #15, A3b).

use bingle_core::algo_bingle::AlgoBingle;

fn approx(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn test_registration_funding_zero_price_empty_schema() {
    // base 0.1 + app opt-in base 0.1 + asset opt-in 0.1 + fees (12 * 0.001) + margin 0.01
    // = 0.322 ALGO.
    let required = AlgoBingle::registration_funding_algos(0, 0, 0);
    assert!(approx(required, 0.322), "expected 0.322 got {}", required);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn test_registration_funding_includes_price_and_schema() {
    // price 0.2 ALGO (200_000 microalgos), schema 2 uints + 1 byte-slice.
    // app opt-in = 100_000 + 2*28_500 + 1*50_000 = 207_000
    // min balance = 100_000 (base) + 207_000 (app) + 100_000 (asset) = 407_000
    // fees = 12 * 1_000 = 12_000, safety margin = 10_000
    // total = 407_000 + 200_000 + 12_000 + 10_000 = 629_000 microalgos = 0.629 ALGO
    let required = AlgoBingle::registration_funding_algos(200_000, 2, 1);
    assert!(approx(required, 0.629), "expected 0.629 got {}", required);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn test_registration_funding_covers_min_balance_plus_spend() {
    // Regression for issue #15: with the deployed app's local schema (3 uints + 3 byte-slices)
    // the account minimum balance is 0.5355 ALGO (base 0.1 + app opt-in 0.3355 + asset 0.1).
    // Even at a near-zero price the target must exceed that minimum by a spend headroom, so the
    // multi-transaction register flow never drops below min mid-way (the reported failure was
    // "balance 534500 below min 535500").
    let min_balance_algos = 0.5355;
    let required = AlgoBingle::registration_funding_algos(1, 3, 3);
    assert!(
        required >= min_balance_algos + 0.02,
        "required {} should exceed min balance {} with spend headroom",
        required,
        min_balance_algos
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn test_registration_funding_grows_with_price() {
    let cheap = AlgoBingle::registration_funding_algos(100_000, 1, 1);
    let dear = AlgoBingle::registration_funding_algos(900_000, 1, 1);
    assert!(dear > cheap);
    // The difference is exactly the price delta (0.8 ALGO).
    assert!(approx(dear - cheap, 0.8), "expected 0.8 got {}", dear - cheap);
}
