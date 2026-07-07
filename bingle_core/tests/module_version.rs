use bingle_core::module_version;

#[test]
fn test_base_module_version() {
    let info = module_version::get_version();
    assert!(!info.version.is_empty());
    // Check that version contains 4 parts (major.minor.patch.build)
    let parts: Vec<&str> = info.version.split('.').collect();
    assert!(
        parts.len() >= 4,
        "Version {} should have at least 4 parts",
        info.version
    );
    assert!(!info.build_number.is_empty());
}
