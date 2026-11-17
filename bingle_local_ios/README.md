# bingle_local_ios

iOS Keychain storage for the Bingle recovery phrase (issue #385). `KeychainKeyStore` implements `bingle_local`'s `KeyStore`, so the 25-word phrase lives in the Keychain and the state file keeps only the keypair's address.

Not published: only `bingle_jsi` uses it, and that builds from this workspace.

## Behaviour

- **New keys start on this device only.** Every new key (generated, imported, or moved out of an existing state file) is a `WhenUnlockedThisDeviceOnly` item that never syncs, with the sync choice pending. It reaches iCloud Keychain only when the app calls `set_device_only(false)` after the user chooses sync, so it is never uploaded before they can say no.
- **Device-only mode.** `set_device_only(true)` moves a synced key back to this device only. That deletes the synced item, which removes it from the user's other devices, so it is refused until the user has confirmed the phrase is backed up (`mark_phrase_backed_up`). A key that has never synced needs no such check.
- **Safe switching.** The sync setting is part of a Keychain item's identity, so a switch writes the new item, reads it back, and only then deletes the old one. A failure leaves the old item in place.
- **Reinstall detection.** iOS deletes the app container on uninstall but keeps the Keychain. A missing install marker in the state directory increments the device-only `install` item's count, and if a key is still stored the store reports `reinstall_detected`, so the app can offer to continue with it.
- **A pending recovery key** (handle clash recovery, issue #344) is a separate item and follows the current key's sync setting.

## Keychain items

The service is the app's bundle id with `.bingle` appended. The scope is the state file's stem, so each network's state file has its own key.

| Account | Contents | Synced | Accessibility |
|---|---|---|---|
| `key/<scope>` | versioned JSON: address, phrase, `phrase_backed_up`, `sync_chosen`, `device_only` | only after the user chooses sync | `WhenUnlockedThisDeviceOnly`, or `WhenUnlocked` when synced |
| `recovery/<scope>` | the pending recovery key, same format | follows `key` | follows `key` |
| `install` | versioned JSON: install count, first and last install time | never | `AfterFirstUnlockThisDeviceOnly` |

## Structure

- `SecureItems`: add, read, update and delete one generic-password item, keyed by account and sync setting.
- `KeychainItems` (Apple platforms): the real backend, calling `SecItem*` through `security-framework-sys`. A synced item needs `kSecAttrAccessible`, which the safe `security-framework` password API can't set (and access-control objects can't be synchronised).
- `MemoryItems` (feature `test-support`): an in-memory fake, so the store's logic is unit-tested on the host.
- `KeychainKeyStore`: the logic above, on any `SecureItems`.

## Tests

```
cargo test -p bingle_local_ios --test unit
```

The real Keychain backend is exercised by the Swift XCTest in the `bingle_jsi` iOS harness, run on a local Mac.
