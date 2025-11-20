use rust_comms::util::cli_utils::parse_start_options_from_args;

#[test]
fn parse_run_args_with_positional_handle() {
    let args = vec!["alice".to_string(), "--relay".to_string()];
    let opts = parse_start_options_from_args(args).expect("should parse");
    assert_eq!(opts.handle, "alice");
    assert!(opts.am_relay);
    // Validate Option fields explicitly per guidelines
    assert!(opts.algo_passphrase.is_none());
    assert!(opts.static_ip.is_none());
}
