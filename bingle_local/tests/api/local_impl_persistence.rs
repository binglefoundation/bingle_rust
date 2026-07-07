use bingle_local::api::{BingleApiLocalImpl, BingleLocalApi, ContactSource, LocalApiConfig};
use bingle_test::temp_file_helpers::{project_tmp_dir_path, project_tmp_file_path};
use std::fs;

#[test]
fn persistence_roundtrip_preserves_state() {
    let mut api = BingleApiLocalImpl::new(LocalApiConfig::default());
    // Generate a keypair so AlgoOps can be re-created after load
    let _kp = api.generate_keypair().expect("keypair");

    // Contacts: one visible, one blocked
    api.add_contact("alice".into(), "ID_ALICE".into(), ContactSource::Manual)
        .expect("add alice");
    api.add_contact("bob".into(), "ID_BOB".into(), ContactSource::Received)
        .expect("add bob");
    api.block_contact("ID_BOB".into()).expect("block bob");

    // Messages
    api.add_message("alice".into(), vec!["bob".into()], 1, "m1".into(), None)
        .expect("add m1");
    api.add_message("bob".into(), vec!["alice".into()], 2, "m2".into(), None)
        .expect("add m2");

    let file_path = project_tmp_file_path("bingle-local-persistence", ".json");
    let path = file_path
        .to_str()
        .expect("project tmp path should be valid utf-8")
        .to_string();
    api.save(&path).expect("save ok");

    // Load into a fresh instance
    let mut api2 = BingleApiLocalImpl::new(LocalApiConfig::default());
    api2.load(&path).expect("load ok");

    // Contacts: only alice should be visible
    let contacts = api2.get_contacts().expect("contacts");
    assert_eq!(contacts.len(), 1);
    assert_eq!(contacts[0].handle, "alice");
    assert!(api2.is_blocked("ID_BOB").unwrap());
    assert!(!api2.is_blocked("ID_ALICE").unwrap());

    // Messages: order and contents preserved
    let msgs = api2.get_messages().expect("messages");
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[0].text, "m1");
    assert_eq!(msgs[1].text, "m2");

    // AlgoOps can be obtained (keypair loaded)
    let ops = api2.get_algo_ops().expect("ops after load");
    assert!(ops.address.is_some());
}

#[test]
fn load_missing_file_errors() {
    let mut api = BingleApiLocalImpl::new(LocalApiConfig::default());
    let missing = project_tmp_file_path("bingle-local-missing", ".json");
    let missing = missing
        .to_str()
        .expect("project tmp path should be valid utf-8")
        .to_string();
    let res = api.load(&missing);
    assert!(res.is_err());
}

#[test]
fn load_empty_json_object_yields_empty_state() {
    let mut api = BingleApiLocalImpl::new(LocalApiConfig::default());
    let file_path = project_tmp_file_path("bingle-local-empty", ".json");
    fs::write(&file_path, "{}").expect("write empty json");
    let path = file_path
        .to_str()
        .expect("project tmp path should be valid utf-8")
        .to_string();

    api.load(&path).expect("load empty {} ok");

    // No contacts and no messages
    assert!(api.get_contacts().expect("contacts").is_empty());
    assert!(api.get_messages().expect("messages").is_empty());

    // No keypair loaded, so AlgoOps cannot be constructed
    assert!(api.get_algo_ops().is_err());
}

#[test]
fn save_creates_parent_directories() {
    let mut api = BingleApiLocalImpl::new(LocalApiConfig::default());
    let _kp = api.generate_keypair().expect("keypair");
    let base = project_tmp_dir_path("bingle-local-parent-dirs");
    let nested = base.join("nested/sub/dir/state.json");
    let path = nested
        .to_str()
        .expect("project tmp path should be valid utf-8")
        .to_string();
    api.save(&path).expect("save ok");
    let meta = fs::metadata(&nested).expect("file exists");
    assert!(meta.is_file());
}

#[test]
fn save_succeeds_when_parent_directory_already_exists() {
    let mut api = BingleApiLocalImpl::new(LocalApiConfig::default());
    let _kp = api.generate_keypair().expect("keypair");
    let base = project_tmp_dir_path("bingle-local-existing-parent");
    let file_path = base.join("state.json");
    let path = file_path
        .to_str()
        .expect("project tmp path should be valid utf-8")
        .to_string();

    // First save creates the file
    api.save(&path).expect("first save ok");
    assert!(fs::metadata(&file_path).expect("file exists").is_file());

    // Second save should succeed without EEXIST error since parent already exists
    api.save(&path)
        .expect("second save ok (parent dir already exists)");
    assert!(
        fs::metadata(&file_path)
            .expect("file still exists")
            .is_file()
    );
}
