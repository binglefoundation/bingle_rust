use bingle_local::api::{BingleApiLocalImpl, BingleLocalApi, ContactSource, LocalApiConfig};

fn mk_api() -> BingleApiLocalImpl {
    BingleApiLocalImpl::new(LocalApiConfig::default())
}

#[test]
fn contacts_initially_empty() {
    let api = mk_api();
    let list = api.get_contacts().expect("get_contacts");
    assert!(list.is_empty());
}

#[test]
fn add_and_list_contacts() {
    let mut api = mk_api();
    api.add_contact("alice".into(), "ID_ALICE".into(), ContactSource::Manual)
        .expect("add alice");
    api.add_contact("bob".into(), "ID_BOB".into(), ContactSource::Received)
        .expect("add bob");

    let mut list = api.get_contacts().expect("get_contacts");
    list.sort_by(|a, b| a.handle.cmp(&b.handle));
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].handle, "alice");
    assert_eq!(list[1].handle, "bob");
}

#[test]
fn duplicate_add_errors() {
    let mut api = mk_api();
    api.add_contact("alice".into(), "ID_ALICE".into(), ContactSource::Manual)
        .unwrap();
    let err = api
        .add_contact("alice2".into(), "ID_ALICE".into(), ContactSource::Manual)
        .unwrap_err();
    assert!(err.to_string().to_lowercase().contains("exists"));
}

#[test]
fn block_hides_contact_and_is_blocked_true() {
    let mut api = mk_api();
    api.add_contact("alice".into(), "ID_ALICE".into(), ContactSource::Manual)
        .unwrap();
    api.add_contact("bob".into(), "ID_BOB".into(), ContactSource::Received)
        .unwrap();

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
    api.add_contact("alice".into(), "ID_ALICE".into(), ContactSource::Manual)
        .unwrap();
    api.remove_contact("ID_ALICE".into()).expect("remove");

    // Removing again should error
    assert!(api.remove_contact("ID_ALICE".into()).is_err());

    // After removal, not blocked and not present in list
    assert!(!api.is_blocked("ID_ALICE").unwrap());
    assert!(api.get_contacts().unwrap().is_empty());
}

#[test]
fn unblock_restores_a_blocked_contact() {
    let mut api = mk_api();
    api.add_contact("bob".into(), "ID_BOB".into(), ContactSource::Received)
        .unwrap();
    api.block_contact("ID_BOB".into()).unwrap();
    assert!(api.is_blocked("ID_BOB").unwrap());

    api.unblock_contact("ID_BOB".into()).expect("unblock");

    assert!(!api.is_blocked("ID_BOB").unwrap());
    let list = api.get_contacts().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].handle, "bob");
    assert!(api.get_blocked_contacts().unwrap().is_empty());
}

#[test]
fn unblock_unknown_or_empty_id_errors() {
    let mut api = mk_api();
    let err = api.unblock_contact("ID_NOBODY".into()).unwrap_err();
    assert!(err.to_string().contains("contact not found"));
    assert!(api.unblock_contact("  ".into()).is_err());
}

#[test]
fn get_blocked_contacts_lists_only_blocked() {
    let mut api = mk_api();
    api.add_contact("alice".into(), "ID_ALICE".into(), ContactSource::Manual)
        .unwrap();
    api.add_contact("bob".into(), "ID_BOB".into(), ContactSource::Received)
        .unwrap();
    api.block_contact("ID_BOB".into()).unwrap();

    let blocked = api.get_blocked_contacts().expect("get_blocked_contacts");
    assert_eq!(blocked.len(), 1);
    assert_eq!(blocked[0].handle, "bob");
    assert_eq!(blocked[0].id, "ID_BOB");
}

#[test]
fn mailbox_sender_blocked_only_for_a_blocked_contact() {
    let mut api = mk_api();
    api.add_contact("alice".into(), "ID_ALICE".into(), ContactSource::Manual)
        .unwrap();
    api.add_contact("bob".into(), "ID_BOB".into(), ContactSource::Received)
        .unwrap();
    api.block_contact("ID_BOB".into()).unwrap();

    assert!(api.is_mailbox_sender_blocked(Some("ID_BOB")));
    assert!(!api.is_mailbox_sender_blocked(Some("ID_ALICE")));
    assert!(!api.is_mailbox_sender_blocked(Some("ID_STRANGER")));
    // An undecodable sender address is never treated as blocked.
    assert!(!api.is_mailbox_sender_blocked(None));
}
