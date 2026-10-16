// Unit tests for the start-up registration check (bingle_cli::account_check, issue #314).
use bingle_cli::account_check::{
    AccountReadiness, classify_account, migration_needed_message, registration_needed_message,
};

const ACCOUNT: &str = "YYZLJMSTL7UCD7VZBVEIDLVO4REE7XKK6BH7O4JOASCOVPL6GNSTLD2VRQ";

#[test]
#[cfg(not(target_os = "ios"))]
pub fn holding_a_handle_is_registered_whatever_the_ancestors_hold() {
    assert_eq!(classify_account(true, None), AccountReadiness::Registered);
    assert_eq!(
        classify_account(true, Some(7)),
        AccountReadiness::Registered
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn no_handle_with_ancestor_state_needs_migration_from_that_ancestor() {
    assert_eq!(
        classify_account(false, Some(771828003)),
        AccountReadiness::NeedsMigration {
            ancestor: 771828003
        }
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn no_state_anywhere_is_unregistered() {
    assert_eq!(
        classify_account(false, None),
        AccountReadiness::Unregistered
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn migration_message_names_account_apps_and_both_remedies() {
    let msg = migration_needed_message(ACCOUNT, 900, 800);
    assert!(msg.contains(ACCOUNT), "names the account: {msg}");
    assert!(msg.contains("app 900"), "names the configured app: {msg}");
    assert!(
        msg.contains("ancestor app 800"),
        "names the ancestor: {msg}"
    );
    assert!(msg.contains("bingle_cli migrate"), "names migrate: {msg}");
    assert!(
        msg.contains("--auto-migrate"),
        "names --auto-migrate: {msg}"
    );
}

#[test]
#[cfg(not(target_os = "ios"))]
pub fn registration_message_names_account_app_and_register() {
    let msg = registration_needed_message(ACCOUNT, 900);
    assert!(msg.contains(ACCOUNT), "names the account: {msg}");
    assert!(msg.contains("app 900"), "names the configured app: {msg}");
    assert!(msg.contains("bingle_cli register"), "names register: {msg}");
    assert!(
        !msg.contains("migrate"),
        "does not suggest migrating: {msg}"
    );
}
