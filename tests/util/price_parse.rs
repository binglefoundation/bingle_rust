use rust_comms::util::config_utils::parse_algos_decimal_to_microalgos;

#[test]
#[cfg(not(target_os = "ios"))]
pub fn parses_integer_algos_to_microalgos() {
    assert_eq!(parse_algos_decimal_to_microalgos("0").expect("ok"), 0);
    assert_eq!(
        parse_algos_decimal_to_microalgos("1").expect("ok"),
        1_000_000
    );
    assert_eq!(
        parse_algos_decimal_to_microalgos("42").expect("ok"),
        42_000_000
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn parses_fractional_up_to_6dp() {
    assert_eq!(
        parse_algos_decimal_to_microalgos("0.000001").expect("ok"),
        1
    );
    assert_eq!(
        parse_algos_decimal_to_microalgos("1.000001").expect("ok"),
        1_000_001
    );
    assert_eq!(
        parse_algos_decimal_to_microalgos("2.5").expect("ok"),
        2_500_000
    );
    assert_eq!(
        parse_algos_decimal_to_microalgos("10.250000").expect("ok"),
        10_250_000
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn rejects_more_than_6_fractional_digits() {
    let err = parse_algos_decimal_to_microalgos("0.0000001").unwrap_err();
    assert!(err.contains("more than 6 fractional digits"));
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn rejects_negative_or_invalid() {
    assert!(parse_algos_decimal_to_microalgos("").is_err());
    assert!(parse_algos_decimal_to_microalgos("-1").is_err());
    assert!(parse_algos_decimal_to_microalgos("abc").is_err());
    assert!(parse_algos_decimal_to_microalgos("1.a").is_err());
    assert!(parse_algos_decimal_to_microalgos("1.").is_ok());
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn overflow_is_rejected() {
    // Choose a value that would overflow u64 microalgos when multiplied
    // u64::MAX / 1_000_000 is 18_446_744_07365 (truncated), so add 1 to force overflow
    let big = format!("{}", (u128::from(u64::MAX) / 1_000_000u128) + 1);
    assert!(parse_algos_decimal_to_microalgos(&big).is_err());
}
