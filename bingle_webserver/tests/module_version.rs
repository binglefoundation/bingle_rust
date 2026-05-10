use bingle_webserver::module_version;

#[test]
fn test_web_module_version() {
    let info = module_version::get_version();
    assert!(!info.version.is_empty());
    assert!(!info.build_number.is_empty());
}
