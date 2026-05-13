use rust_comms::util::version::{get_version_info, VersionsMap};

#[test]
fn test_get_version_info() {
    crate::util::test_util::init_test_logging();
    let info = get_version_info();
    
    // Check that version contains 4 parts (major.minor.patch.build)
    let parts: Vec<&str> = info.version.split('.').collect();
    assert!(parts.len() >= 4, "Version {} should have at least 4 parts", info.version);
    
    // Check that it's not empty
    assert!(!info.version.is_empty());
    assert!(!info.build_timestamp.is_empty());
    assert!(!info.build_number.is_empty());
    
    // Check consistency
    assert!(info.version.ends_with(&info.build_number));
    
    // Check that build_number is a number
    assert!(info.build_number.parse::<u32>().is_ok());
    
    // Git SHA might be None in some environments, but if we're in a git repo it should be there
    // For the test, we just check it doesn't crash
    println!("Git SHA: {:?}", info.git_sha);
}

#[test]
fn test_versions_map() {
    let mut map = VersionsMap::new();
    let info = get_version_info();
    map.insert("Base".to_string(), info.clone());
    
    assert_eq!(map.len(), 1);
    assert_eq!(map.get("Base").unwrap().version, info.version);
}
