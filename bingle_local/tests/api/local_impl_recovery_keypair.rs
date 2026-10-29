//! The offline keypair and the pending handle clash recovery keypair (issue #344).

use bingle_local::api::{BingleApiLocalImpl, BingleLocalApi, LocalApiConfig};
use bingle_test::temp_file_helpers::project_tmp_file_path;
use std::fs;

fn state_path(name: &str) -> String {
    project_tmp_file_path(name, ".json")
        .to_str()
        .expect("project tmp path should be valid utf-8")
        .to_string()
}

#[test]
#[cfg(not(target_os = "ios"))]
fn generated_keypair_restores_identically_from_its_recovery_phrase() {
    // Generating needs no network: the default config has no node.
    let mut api = BingleApiLocalImpl::new(LocalApiConfig::default());
    let generated = api.generate_keypair().expect("generate offline");
    assert_eq!(generated.passphrase.split_whitespace().count(), 25);

    let path = state_path("bingle-local-offline-keypair");
    api.save(&path).expect("save");
    let mut reloaded = BingleApiLocalImpl::new(LocalApiConfig::default());
    reloaded.load(&path).expect("load");
    assert_eq!(
        reloaded.get_keypair().expect("get"),
        Some(generated.clone())
    );

    let mut restored = BingleApiLocalImpl::new(LocalApiConfig::default());
    let imported = restored
        .import_keypair(generated.passphrase.clone())
        .expect("restore");
    assert_eq!(imported, generated);
    let _ = fs::remove_file(path);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn recovery_needs_a_current_keypair() {
    let mut api = BingleApiLocalImpl::new(LocalApiConfig::default());
    assert!(api.begin_recovery_keypair().is_err());
    assert_eq!(api.get_recovery_keypair().expect("get"), None);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn recovery_keypair_sits_beside_the_current_one_and_resumes() {
    let mut api = BingleApiLocalImpl::new(LocalApiConfig::default());
    let current = api.generate_keypair().expect("generate");
    let pending = api.begin_recovery_keypair().expect("begin");
    assert_ne!(pending.id, current.id);
    assert_eq!(api.get_keypair().expect("get"), Some(current.clone()));
    assert_eq!(
        api.get_recovery_keypair().expect("get"),
        Some(pending.clone())
    );
    // Beginning again resumes the same recovery rather than replacing its key.
    assert_eq!(api.begin_recovery_keypair().expect("resume"), pending);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn recovery_keypair_survives_a_restart() {
    let mut api = BingleApiLocalImpl::new(LocalApiConfig::default());
    let current = api.generate_keypair().expect("generate");
    let pending = api.begin_recovery_keypair().expect("begin");
    let path = state_path("bingle-local-recovery-keypair");
    api.save(&path).expect("save");

    let mut reloaded = BingleApiLocalImpl::new(LocalApiConfig::default());
    reloaded.load(&path).expect("load");
    assert_eq!(reloaded.get_keypair().expect("get"), Some(current));
    assert_eq!(
        reloaded.get_recovery_keypair().expect("get"),
        Some(pending.clone())
    );
    assert_eq!(reloaded.begin_recovery_keypair().expect("resume"), pending);
    let _ = fs::remove_file(path);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn state_without_a_recovery_omits_it() {
    let mut api = BingleApiLocalImpl::new(LocalApiConfig::default());
    api.generate_keypair().expect("generate");
    let path = state_path("bingle-local-no-recovery");
    api.save(&path).expect("save");
    let saved = fs::read_to_string(&path).expect("read");
    assert!(!saved.contains("recovery_keypair"));
    let _ = fs::remove_file(path);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn completing_a_recovery_makes_its_keypair_current() {
    let mut api = BingleApiLocalImpl::new(LocalApiConfig::default());
    api.generate_keypair().expect("generate");
    let pending = api.begin_recovery_keypair().expect("begin");
    assert_eq!(api.complete_recovery_keypair().expect("complete"), pending);
    assert_eq!(api.get_keypair().expect("get"), Some(pending));
    assert_eq!(api.get_recovery_keypair().expect("get"), None);
    assert!(api.complete_recovery_keypair().is_err());
}
