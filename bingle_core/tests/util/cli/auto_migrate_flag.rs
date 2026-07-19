use bingle_core::util::cli_utils::{args_request_auto_migrate, parse_start_options_from_args};

#[test]
#[cfg(not(target_os = "ios"))]
pub fn parse_accepts_auto_migrate_flag_without_error() {
    // The flag is tolerated by the parser (it does not perturb StartOptions, like --debug).
    let args = vec!["alice".to_string(), "--auto-migrate".to_string()];
    let opts = parse_start_options_from_args(args).expect("should parse with --auto-migrate");
    assert_eq!(opts.handle, "alice");
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn args_request_auto_migrate_detects_the_flag() {
    let with = vec![
        "alice".to_string(),
        "--auto-migrate".to_string(),
        "--relay".to_string(),
    ];
    assert!(args_request_auto_migrate(&with), "flag present -> true");

    let without = vec!["alice".to_string(), "--relay".to_string()];
    assert!(!args_request_auto_migrate(&without), "flag absent -> false");
}
