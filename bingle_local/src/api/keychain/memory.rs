//! An in-memory Keychain for host tests (feature `test-support`).

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use super::items::{Accessibility, ItemError, SecureItems};

/// The fake's items: (account, synced) to (accessibility, data).
type ItemMap = HashMap<(String, bool), (Accessibility, Vec<u8>)>;

/// Behaves like the Keychain for the operations the key store uses: items are keyed by account
/// and sync setting, an add of an existing item fails, and a delete of a missing one doesn't.
#[derive(Default)]
pub struct MemoryItems {
    items: Mutex<ItemMap>,
    fail_writes: AtomicBool,
}

impl MemoryItems {
    /// An empty fake Keychain.
    pub fn new() -> Self {
        Self::default()
    }

    /// Makes every add, update and delete fail, to test that a failed write leaves things intact.
    pub fn set_fail_writes(&self, fail: bool) {
        self.fail_writes.store(fail, Ordering::SeqCst);
    }

    /// The accessibility an item was added with, or `None` if there is no such item.
    pub fn accessibility(&self, account: &str, synced: bool) -> Option<Accessibility> {
        self.items
            .lock()
            .expect("memory items lock")
            .get(&(account.to_string(), synced))
            .map(|(accessibility, _)| *accessibility)
    }

    fn check_writes(&self) -> Result<(), ItemError> {
        if self.fail_writes.load(Ordering::SeqCst) {
            Err(ItemError::Status(-1))
        } else {
            Ok(())
        }
    }
}

impl SecureItems for MemoryItems {
    fn read(&self, account: &str, synced: bool) -> Result<Option<Vec<u8>>, ItemError> {
        Ok(self
            .items
            .lock()
            .expect("memory items lock")
            .get(&(account.to_string(), synced))
            .map(|(_, data)| data.clone()))
    }

    fn add(
        &self,
        account: &str,
        synced: bool,
        accessibility: Accessibility,
        data: &[u8],
    ) -> Result<(), ItemError> {
        self.check_writes()?;
        if synced && accessibility != Accessibility::WhenUnlocked {
            return Err(ItemError::Other(
                "a synced item can't be ThisDeviceOnly".to_string(),
            ));
        }
        let mut items = self.items.lock().expect("memory items lock");
        let key = (account.to_string(), synced);
        if items.contains_key(&key) {
            return Err(ItemError::Duplicate);
        }
        items.insert(key, (accessibility, data.to_vec()));
        Ok(())
    }

    fn update(&self, account: &str, synced: bool, data: &[u8]) -> Result<(), ItemError> {
        self.check_writes()?;
        let mut items = self.items.lock().expect("memory items lock");
        match items.get_mut(&(account.to_string(), synced)) {
            Some((_, stored)) => {
                *stored = data.to_vec();
                Ok(())
            }
            None => Err(ItemError::Other("no such item to update".to_string())),
        }
    }

    fn delete(&self, account: &str, synced: bool) -> Result<(), ItemError> {
        self.check_writes()?;
        self.items
            .lock()
            .expect("memory items lock")
            .remove(&(account.to_string(), synced));
        Ok(())
    }
}
