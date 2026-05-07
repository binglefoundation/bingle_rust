use bingle_local::api::{BingleApiLocalImpl, BingleLocalApi, ContactSource, LocalApiConfig};

fn mk_api() -> BingleApiLocalImpl { BingleApiLocalImpl::new(LocalApiConfig::default()) }

#[test]
fn contacts_initially_empty() {
    let api = mk_api();
    let list = api.get_contacts().expect("get_contacts");
    assert!(list.is_empty());
}

#[test]
fn add_and_list_contacts() {
    let mut api = mk_api();
    api.add_contact("alice".into(), "ID_ALICE".into(), ContactSource::Manual).expect("add alice");
    api.add_contact("bob".into(), "ID_BOB".into(), ContactSource::Received).expect("add bob");

    let mut list = api.get_contacts().expect("get_contacts");
    list.sort_by(|a, b| a.handle.cmp(&b.handle));
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].handle, "alice");
    assert_eq!(list[1].handle, "bob");
}

#[test]
fn duplicate_add_errors() {
    let mut api = mk_api();
    api.add_contact("alice".into(), "ID_ALICE".into(), ContactSource::Manual).unwrap();
    let err = api.add_contact("alice2".into(), "ID_ALICE".into(), ContactSource::Manual).unwrap_err();
    assert!(err.to_lowercase().contains("exists"));
}

#[test]
fn block_hides_contact_and_is_blocked_true() {
    let mut api = mk_api();
    api.add_contact("alice".into(), "ID_ALICE".into(), ContactSource::Manual).unwrap();
    api.add_contact("bob".into(), "ID_BOB".into(), ContactSource::Received).unwrap();

    api.block_contact("ID_BOB".into()).expect("block");

    assert!(!api.is_blocked("ID_ALICE").unwrap());
    assert!(api.is_blocked("ID_BOB").unwrap());

    let list = api.get_contacts().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].handle, "alice");
}

#[test]
fn remove_contact_works_and_then_missing_errors() {
    let mut api = mk_api();
    api.add_contact("alice".into(), "ID_ALICE".into(), ContactSource::Manual).unwrap();
    api.remove_contact("ID_ALICE".into()).expect("remove");

    // Removing again should error
    assert!(api.remove_contact("ID_ALICE".into()).is_err());

    // After removal, not blocked and not present in list
    assert!(!api.is_blocked("ID_ALICE").unwrap());
    assert!(api.get_contacts().unwrap().is_empty());
}
