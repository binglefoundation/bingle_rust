use bingle_core::api::bingle_api::IpFamily;
use bingle_core::util::cli_utils::parse_start_options_from_args;

fn parse(extra: &[&str]) -> Result<bingle_core::api::bingle_api::StartOptions, String> {
    let mut args = vec!["alice".to_string()];
    args.extend(extra.iter().map(|s| s.to_string()));
    parse_start_options_from_args(args)
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn ip_family_defaults_to_auto() {
    assert_eq!(parse(&[]).unwrap().ip_family, IpFamily::Auto);
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn ip_family_flag_sets_the_family() {
    assert_eq!(
        parse(&["--ip-family", "v4"]).unwrap().ip_family,
        IpFamily::V4
    );
    assert_eq!(
        parse(&["--ip-family", "v6"]).unwrap().ip_family,
        IpFamily::V6
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn ip_family_flag_rejects_bad_values() {
    let err = parse(&["--ip-family", "both"]).unwrap_err();
    assert!(err.contains("--ip-family"), "{err}");
    assert!(parse(&["--ip-family"]).is_err(), "a value is required");
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn nat64_prefix_flag_is_validated() {
    assert_eq!(parse(&[]).unwrap().nat64_prefix, None);
    assert_eq!(
        parse(&["--nat64-prefix", "64:ff9b::/96"])
            .unwrap()
            .nat64_prefix
            .as_deref(),
        Some("64:ff9b::/96")
    );
    assert!(parse(&["--nat64-prefix", "64:ff9b::/80"]).is_err());
}
