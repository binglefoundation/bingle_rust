//! The sealed state file (issue #459), with the Keychain store over the in-memory Keychain fake:
//! what is on disk, migration from plain JSON, the locked device, the data key in memory, opening a
//! restored file with the recovery phrase, and deletion.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use bingle_core::api::bingle_api::BingleError;
use bingle_local::api::keychain::KeychainKeyStore;
use bingle_local::api::keychain::items::Accessibility;
use bingle_local::api::keychain::memory::MemoryItems;
use bingle_local::api::{
    BingleApiLocalImpl, BingleLocalApi, ContactSource, Keypair, LocalApiConfig, StateFileStatus,
};
use bingle_test::temp_file_helpers::project_tmp_dir_path;

const SCOPE: &str = "bingle_state_testnet";
const STATE_ITEM: &str = "state/bingle_state_testnet";

/// One device: its Keychain and the directory holding its state file.
struct Device {
    items: Arc<MemoryItems>,
    dir: PathBuf,
}

impl Device {
    fn new(name: &str) -> Self {
        Self {
            items: Arc::new(MemoryItems::new()),
            dir: project_tmp_dir_path(name),
        }
    }

    fn path(&self) -> String {
        self.dir
            .join(format!("{SCOPE}.json"))
            .to_str()
            .expect("utf-8 path")
            .to_string()
    }

    /// The local API on this device, sealing when `encrypt_state`.
    fn api(&self, encrypt_state: bool) -> BingleApiLocalImpl {
        let store = Arc::new(KeychainKeyStore::new(self.items.clone(), SCOPE, &self.dir));
        BingleApiLocalImpl::with_key_store(
            LocalApiConfig::default().with_encrypt_state(encrypt_state),
            store,
        )
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// A sealing account with a contact and a message, saved.
fn sealed_account(device: &Device) -> (BingleApiLocalImpl, Keypair) {
    let mut api = device.api(true);
    let kp = api.generate_keypair().expect("generate");
    api.add_contact("friend".into(), "FRIEND_ID".into(), ContactSource::Manual)
        .expect("add contact");
    api.add_message(
        "secret_sender".into(),
        vec!["friend".into()],
        1,
        "the message text".into(),
        None,
    )
    .expect("add message");
    api.save(&device.path()).expect("save");
    (api, kp)
}

fn file_bytes(path: &str) -> Vec<u8> {
    std::fs::read(path).expect("read state file")
}

fn contains(haystack: &[u8], needle: &str) -> bool {
    haystack
        .windows(needle.len())
        .any(|w| w == needle.as_bytes())
}

fn copy_file(from: &str, to_dir: &Path) -> String {
    std::fs::create_dir_all(to_dir).expect("mkdir");
    let to = to_dir.join(format!("{SCOPE}.json"));
    std::fs::copy(from, &to).expect("copy state file");
    to.to_str().expect("utf-8 path").to_string()
}

#[test]
#[cfg(not(target_os = "ios"))]
fn the_sealed_file_holds_no_readable_handle_contact_or_message() {
    let device = Device::new("sealed-raw-bytes");
    let (_api, kp) = sealed_account(&device);
    let bytes = file_bytes(&device.path());
    for text in [
        "friend",
        "FRIEND_ID",
        "secret_sender",
        "the message text",
        kp.id.as_str(),
    ] {
        assert!(!contains(&bytes, text), "'{text}' is readable in the file");
    }
    assert!(!contains(&bytes, &kp.passphrase));
}

#[test]
#[cfg(not(target_os = "ios"))]
fn the_device_secret_is_this_device_only_and_never_synced() {
    let device = Device::new("sealed-secret-item");
    sealed_account(&device);
    assert_eq!(
        device.items.accessibility(STATE_ITEM, false),
        Some(Accessibility::WhenUnlockedThisDeviceOnly)
    );
    assert_eq!(device.items.accessibility(STATE_ITEM, true), None);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn a_sealed_file_loads_with_the_device_key() {
    let device = Device::new("sealed-reload");
    let (_api, kp) = sealed_account(&device);

    let mut reloaded = device.api(true);
    reloaded.load(&device.path()).expect("load");

    assert_eq!(reloaded.state_file_status(), StateFileStatus::Sealed);
    assert_eq!(reloaded.get_keypair().unwrap(), Some(kp));
    assert_eq!(reloaded.get_contacts().unwrap().len(), 1);
    assert_eq!(reloaded.get_messages().unwrap()[0].text, "the message text");
}

#[test]
#[cfg(not(target_os = "ios"))]
fn a_plain_file_is_sealed_on_load_without_losing_messages() {
    let device = Device::new("sealed-migration");
    let mut old = device.api(false);
    old.generate_keypair().expect("generate");
    old.add_message("me".into(), vec!["friend".into()], 1, "kept".into(), None)
        .expect("add message");
    old.save(&device.path()).expect("save plain");
    assert!(
        contains(&file_bytes(&device.path()), "kept"),
        "plain before"
    );

    let mut api = device.api(true);
    api.load(&device.path()).expect("load");

    assert!(
        !contains(&file_bytes(&device.path()), "kept"),
        "sealed after"
    );
    assert_eq!(api.state_file_status(), StateFileStatus::Sealed);
    let mut reloaded = device.api(true);
    reloaded.load(&device.path()).expect("reload");
    assert_eq!(reloaded.get_messages().unwrap()[0].text, "kept");
}

#[test]
#[cfg(not(target_os = "ios"))]
fn a_migration_that_cant_seal_leaves_the_plain_file_and_seals_later() {
    let device = Device::new("sealed-migration-interrupted");
    let mut old = device.api(false);
    old.add_message("me".into(), vec!["friend".into()], 1, "kept".into(), None)
        .expect("add message");
    old.save(&device.path()).expect("save plain");

    // The Keychain refuses the new device secret: the file stays plain and whole.
    device.items.set_fail_writes(true);
    let mut api = device.api(true);
    api.load(&device.path()).expect("load still works");
    assert_eq!(api.state_file_status(), StateFileStatus::Plain);
    assert!(contains(&file_bytes(&device.path()), "kept"));
    assert!(api.save(&device.path()).is_err(), "never written plain");
    assert!(contains(&file_bytes(&device.path()), "kept"));

    // Next time it works.
    device.items.set_fail_writes(false);
    api.save(&device.path()).expect("save");
    assert!(!contains(&file_bytes(&device.path()), "kept"));
    assert_eq!(api.get_messages().unwrap()[0].text, "kept");
}

#[test]
#[cfg(not(target_os = "ios"))]
fn a_locked_device_neither_loads_nor_saves_over_the_file() {
    let device = Device::new("sealed-locked-launch");
    sealed_account(&device);
    let before = file_bytes(&device.path());

    // A prewarmed launch while locked.
    device.items.set_locked(true);
    let mut api = device.api(true);
    let err = api.load(&device.path()).expect_err("locked");
    assert!(matches!(err, BingleError::StateLocked(_)), "{err:?}");
    assert_eq!(api.state_file_status(), StateFileStatus::Locked);
    let err = api.save(&device.path()).expect_err("not saved over");
    assert!(matches!(err, BingleError::StateLocked(_)), "{err:?}");
    assert_eq!(file_bytes(&device.path()), before, "file untouched");

    // After the unlock it loads.
    device.items.set_locked(false);
    api.load(&device.path()).expect("load after unlock");
    assert_eq!(api.get_messages().unwrap()[0].text, "the message text");
}

#[test]
#[cfg(not(target_os = "ios"))]
fn the_held_data_key_saves_after_the_lock_until_it_is_forgotten() {
    let device = Device::new("sealed-lock-in-session");
    let (mut api, _kp) = sealed_account(&device);

    // Locked, key still held: a message arriving now is saved.
    device.items.set_locked(true);
    api.add_message(
        "me".into(),
        vec!["friend".into()],
        2,
        "after lock".into(),
        None,
    )
    .expect("add message");
    api.save(&device.path()).expect("save with the held key");

    // Flushed, then wiped: no save until the unlock, and the file isn't changed.
    api.forget_state_key();
    let before = file_bytes(&device.path());
    api.add_message(
        "me".into(),
        vec!["friend".into()],
        3,
        "while wiped".into(),
        None,
    )
    .expect("add message");
    let err = api.save(&device.path()).expect_err("no key while locked");
    assert!(matches!(err, BingleError::StateLocked(_)), "{err:?}");
    assert_eq!(file_bytes(&device.path()), before);

    // Unlocked: the next save reads the device key again and writes everything.
    device.items.set_locked(false);
    api.save(&device.path()).expect("save after unlock");
    let mut reloaded = device.api(true);
    reloaded.load(&device.path()).expect("reload");
    let texts: Vec<String> = reloaded
        .get_messages()
        .unwrap()
        .into_iter()
        .map(|m| m.text)
        .collect();
    assert!(texts.contains(&"after lock".to_string()));
    assert!(texts.contains(&"while wiped".to_string()));
}

#[test]
#[cfg(not(target_os = "ios"))]
fn a_restored_file_opens_with_the_recovery_phrase_on_a_new_device() {
    let old_device = Device::new("sealed-restore-old");
    let (_api, kp) = sealed_account(&old_device);
    let new_device = Device::new("sealed-restore-new");
    let path = copy_file(&old_device.path(), &new_device.dir);
    let restored = file_bytes(&path);

    let mut api = new_device.api(true);
    let err = api.load(&path).expect_err("no key on this device");
    assert!(
        matches!(err, BingleError::RecoveryPhraseRequired(_)),
        "{err:?}"
    );
    assert_eq!(
        api.state_file_status(),
        StateFileStatus::RecoveryPhraseRequired
    );
    assert!(api.save(&path).is_err(), "not saved over");

    // A valid phrase for another account opens nothing and changes nothing.
    let other = new_device.api(false).generate_keypair().expect("other");
    let err = api
        .open_state_with_recovery_phrase(&path, other.passphrase)
        .expect_err("wrong account");
    assert!(
        matches!(err, BingleError::RecoveryPhraseRequired(_)),
        "{err:?}"
    );
    assert_eq!(file_bytes(&path), restored, "file intact");
    assert!(
        api.open_state_with_recovery_phrase(&path, "not a phrase".into())
            .is_err()
    );
    assert_eq!(file_bytes(&path), restored, "file intact");

    // The right phrase opens it and makes the account current.
    let opened = api
        .open_state_with_recovery_phrase(&path, kp.passphrase.clone())
        .expect("open with the phrase");
    assert_eq!(opened.id, kp.id);
    assert_eq!(api.get_keypair().unwrap(), Some(kp.clone()));
    assert_eq!(api.get_messages().unwrap()[0].text, "the message text");
    assert_eq!(api.state_file_status(), StateFileStatus::Sealed);

    // From now on it opens on this device without the phrase.
    let mut reloaded = new_device.api(true);
    reloaded.load(&path).expect("load on the new device");
    assert_eq!(reloaded.get_keypair().unwrap(), Some(kp));
}

#[test]
#[cfg(not(target_os = "ios"))]
fn the_recovery_slot_follows_the_current_account() {
    let device = Device::new("sealed-account-change");
    let (mut api, first) = sealed_account(&device);
    let second = api.generate_keypair().expect("generate another");
    api.save(&device.path()).expect("save");

    let new_device = Device::new("sealed-account-change-new");
    let path = copy_file(&device.path(), &new_device.dir);
    let mut restorer = new_device.api(true);
    let _ = restorer.load(&path);
    assert!(
        restorer
            .open_state_with_recovery_phrase(&path, first.passphrase)
            .is_err(),
        "the old account's phrase no longer opens it"
    );
    restorer
        .open_state_with_recovery_phrase(&path, second.passphrase)
        .expect("the current account's phrase does");
}

#[test]
#[cfg(not(target_os = "ios"))]
fn a_file_with_no_keypair_is_sealed_with_the_device_slot_only() {
    let device = Device::new("sealed-no-keypair");
    let api = device.api(true);
    api.add_contact_shared("friend".into(), "FRIEND_ID".into(), ContactSource::Manual)
        .expect("add contact");
    api.save(&device.path()).expect("save");
    assert!(!contains(&file_bytes(&device.path()), "FRIEND_ID"));

    let mut reloaded = device.api(true);
    reloaded.load(&device.path()).expect("load");
    assert_eq!(reloaded.get_contacts().unwrap().len(), 1);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn removing_the_key_deletes_the_device_secret() {
    let device = Device::new("sealed-remove");
    let (mut api, _kp) = sealed_account(&device);
    assert!(device.items.accessibility(STATE_ITEM, false).is_some());

    api.remove_key_from_device().expect("remove");

    assert_eq!(device.items.accessibility(STATE_ITEM, false), None);
    assert_eq!(api.state_file_status(), StateFileStatus::NotLoaded);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn sealing_needs_a_store_that_holds_the_device_secret() {
    let dir = project_tmp_dir_path("sealed-file-store");
    let path = dir.join("state.json").to_str().unwrap().to_string();
    let api = BingleApiLocalImpl::new(LocalApiConfig::default().with_encrypt_state(true));
    assert!(api.save(&path).is_err(), "no plain fallback");
    assert!(!Path::new(&path).exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
#[cfg(not(target_os = "ios"))]
fn a_large_history_seals_and_opens_about_as_fast_as_plain_json() {
    // Save and load 10,000 messages, plain and sealed.
    fn round_trip(device: &Device, encrypt_state: bool) -> std::time::Duration {
        let mut api = device.api(encrypt_state);
        api.generate_keypair().expect("generate");
        for i in 0..10_000 {
            api.add_message(
                "me".into(),
                vec!["friend".into()],
                i,
                format!("message number {i} with a little text in it"),
                None,
            )
            .expect("add message");
        }
        let start = Instant::now();
        api.save(&device.path()).expect("save");
        let mut reloaded = device.api(encrypt_state);
        reloaded.load(&device.path()).expect("load");
        let elapsed = start.elapsed();
        assert_eq!(reloaded.get_messages().unwrap().len(), 10_000);
        elapsed
    }
    let plain = round_trip(&Device::new("sealed-large-plain"), false);
    let sealed = round_trip(&Device::new("sealed-large-sealed"), true);
    println!("10,000 messages: plain {plain:?}, sealed {sealed:?}");
    // AES-GCM over a few megabytes costs tens of milliseconds next to the JSON work (the cipher
    // crates are optimised in the dev profile, see the workspace Cargo.toml); allow for noise.
    assert!(
        sealed <= plain * 3 + std::time::Duration::from_millis(500),
        "sealed {sealed:?} against plain {plain:?}"
    );
}
