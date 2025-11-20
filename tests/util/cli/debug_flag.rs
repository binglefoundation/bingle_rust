use rust_comms::util::cli_utils::parse_start_options_from_args;

#[test]
fn parse_accepts_debug_flag_without_error() {
    let args = vec![
        "alice".to_string(),
        "--debug".to_string(),
    ];
    let opts = parse_start_options_from_args(args).expect("should parse with --debug");
    assert_eq!(opts.handle, "alice");
}
