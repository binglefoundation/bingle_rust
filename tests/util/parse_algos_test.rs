// Tests for util::cli_utils parse_algos_decimal_to_microalgos

use rust_comms::util::config_utils::parse_algos_decimal_to_microalgos;

#[test]
fn parses_integers_and_decimals() {
    assert_eq!(parse_algos_decimal_to_microalgos("0").unwrap(), 0);
    assert_eq!(parse_algos_decimal_to_microalgos("1").unwrap(), 1_000_000);
    assert_eq!(parse_algos_decimal_to_microalgos("0.5").unwrap(), 500_000);
    assert_eq!(parse_algos_decimal_to_microalgos("1.234567").unwrap(), 1_234_567);
}

#[test]
fn rejects_invalid_inputs() {
    assert!(parse_algos_decimal_to_microalgos("").is_err());
    assert!(parse_algos_decimal_to_microalgos("-1").is_err());
    assert!(parse_algos_decimal_to_microalgos("1.2345678").is_err());
    assert!(parse_algos_decimal_to_microalgos("abc").is_err());
}
