use bingle_jsi::module_version;

#[test]
fn test_jsi_module_version() {
    let info = module_version::get_version();
    assert!(!info.version.is_empty());
    assert!(!info.build_number.is_empty());
    assert_ne!(info.build_number, "0", "Build number should be incremented from 0");
}
