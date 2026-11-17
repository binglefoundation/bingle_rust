//! The real Keychain backend, through the `SecItem` functions.
//!
//! The safe `security-framework` password API can't set `kSecAttrAccessible`, and a synced item
//! needs it: access-control objects (its alternative) can't be synchronised. So the queries are
//! built here and passed to `SecItemAdd`, `SecItemCopyMatching`, `SecItemUpdate` and
//! `SecItemDelete` directly. Every query uses the data protection keychain, which is the only one
//! on iOS and gives the same behaviour on macOS.

use core_foundation::base::{CFType, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::data::CFData;
use core_foundation::dictionary::CFMutableDictionary;
use core_foundation::string::{CFString, CFStringRef};
use core_foundation_sys::base::CFTypeRef;
use core_foundation_sys::bundle::{CFBundleGetIdentifier, CFBundleGetMainBundle};
use security_framework_sys::access_control::{
    kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly, kSecAttrAccessibleWhenUnlocked,
    kSecAttrAccessibleWhenUnlockedThisDeviceOnly,
};
use security_framework_sys::base::{errSecDuplicateItem, errSecItemNotFound, errSecSuccess};
use security_framework_sys::item::{
    kSecAttrAccount, kSecAttrService, kSecAttrSynchronizable, kSecClass, kSecClassGenericPassword,
    kSecReturnData, kSecValueData,
};
use security_framework_sys::keychain_item::{
    SecItemAdd, SecItemCopyMatching, SecItemDelete, SecItemUpdate,
};

use crate::items::{Accessibility, ItemError, SecureItems};

#[link(name = "Security", kind = "framework")]
unsafe extern "C" {
    // The attribute key itself isn't exported by security-framework-sys; its values are.
    static kSecAttrAccessible: CFStringRef;
    // Exported by security-framework-sys only behind its macOS 10.15 feature; present on iOS and
    // on every macOS this builds for.
    static kSecUseDataProtectionKeychain: CFStringRef;
}

/// The service used when the main bundle has no identifier (a command-line test binary).
const FALLBACK_SERVICE: &str = "bingle";

/// Generic-password items under one service in the data protection keychain.
pub struct KeychainItems {
    service: String,
}

impl KeychainItems {
    /// Items under `service`.
    pub fn new(service: impl Into<String>) -> Self {
        Self {
            service: service.into(),
        }
    }

    /// Items under the app's bundle id with `.bingle` appended, or `bingle` if the process has no
    /// bundle id.
    pub fn for_main_bundle() -> Self {
        // SAFETY: CFBundleGetMainBundle and CFBundleGetIdentifier follow the Get rule: neither
        // result is owned, and both are checked for null before use.
        let identifier = unsafe {
            let bundle = CFBundleGetMainBundle();
            if bundle.is_null() {
                None
            } else {
                let id = CFBundleGetIdentifier(bundle);
                if id.is_null() {
                    None
                } else {
                    Some(CFString::wrap_under_get_rule(id).to_string())
                }
            }
        };
        match identifier {
            Some(id) => Self::new(format!("{id}.bingle")),
            None => Self::new(FALLBACK_SERVICE),
        }
    }

    /// The query identifying one item: class, service, account, sync setting, and the data
    /// protection keychain.
    fn query(&self, account: &str, synced: bool) -> CFMutableDictionary<CFString, CFType> {
        let mut dict = CFMutableDictionary::new();
        // SAFETY: the kSec* constants are static CFStrings owned by the Security framework, so the
        // Get rule applies.
        unsafe {
            dict.add(
                &CFString::wrap_under_get_rule(kSecClass),
                &CFString::wrap_under_get_rule(kSecClassGenericPassword).as_CFType(),
            );
            dict.add(
                &CFString::wrap_under_get_rule(kSecAttrService),
                &CFString::new(&self.service).as_CFType(),
            );
            dict.add(
                &CFString::wrap_under_get_rule(kSecAttrAccount),
                &CFString::new(account).as_CFType(),
            );
            dict.add(
                &CFString::wrap_under_get_rule(kSecAttrSynchronizable),
                &CFBoolean::from(synced).as_CFType(),
            );
            dict.add(
                &CFString::wrap_under_get_rule(kSecUseDataProtectionKeychain),
                &CFBoolean::true_value().as_CFType(),
            );
        }
        dict
    }
}

fn accessible_value(accessibility: Accessibility) -> CFString {
    // SAFETY: static CFStrings owned by the Security framework.
    unsafe {
        CFString::wrap_under_get_rule(match accessibility {
            Accessibility::WhenUnlockedThisDeviceOnly => {
                kSecAttrAccessibleWhenUnlockedThisDeviceOnly
            }
            Accessibility::WhenUnlocked => kSecAttrAccessibleWhenUnlocked,
            Accessibility::AfterFirstUnlockThisDeviceOnly => {
                kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
            }
        })
    }
}

impl SecureItems for KeychainItems {
    fn read(&self, account: &str, synced: bool) -> Result<Option<Vec<u8>>, ItemError> {
        let mut query = self.query(account, synced);
        // SAFETY: static CFString key; the result is checked and released by wrap_under_create_rule.
        unsafe {
            query.add(
                &CFString::wrap_under_get_rule(kSecReturnData),
                &CFBoolean::true_value().as_CFType(),
            );
            let mut result: CFTypeRef = std::ptr::null();
            let status = SecItemCopyMatching(query.as_concrete_TypeRef(), &mut result);
            if status == errSecItemNotFound {
                return Ok(None);
            }
            if status != errSecSuccess {
                return Err(ItemError::Status(status));
            }
            if result.is_null() {
                return Err(ItemError::Other(
                    "the Keychain returned no data".to_string(),
                ));
            }
            let data = CFData::wrap_under_create_rule(result as _);
            Ok(Some(data.bytes().to_vec()))
        }
    }

    fn add(
        &self,
        account: &str,
        synced: bool,
        accessibility: Accessibility,
        data: &[u8],
    ) -> Result<(), ItemError> {
        if synced && accessibility != Accessibility::WhenUnlocked {
            return Err(ItemError::Other(
                "a synced item can't be ThisDeviceOnly".to_string(),
            ));
        }
        let mut attributes = self.query(account, synced);
        // SAFETY: static CFString keys; SecItemAdd is given a null result pointer.
        let status = unsafe {
            attributes.add(
                &CFString::wrap_under_get_rule(kSecAttrAccessible),
                &accessible_value(accessibility).as_CFType(),
            );
            attributes.add(
                &CFString::wrap_under_get_rule(kSecValueData),
                &CFData::from_buffer(data).as_CFType(),
            );
            SecItemAdd(attributes.as_concrete_TypeRef(), std::ptr::null_mut())
        };
        match status {
            s if s == errSecSuccess => Ok(()),
            s if s == errSecDuplicateItem => Err(ItemError::Duplicate),
            s => Err(ItemError::Status(s)),
        }
    }

    fn update(&self, account: &str, synced: bool, data: &[u8]) -> Result<(), ItemError> {
        let query = self.query(account, synced);
        let mut changes: CFMutableDictionary<CFString, CFType> = CFMutableDictionary::new();
        // SAFETY: static CFString key.
        let status = unsafe {
            changes.add(
                &CFString::wrap_under_get_rule(kSecValueData),
                &CFData::from_buffer(data).as_CFType(),
            );
            SecItemUpdate(query.as_concrete_TypeRef(), changes.as_concrete_TypeRef())
        };
        if status == errSecSuccess {
            Ok(())
        } else {
            Err(ItemError::Status(status))
        }
    }

    fn delete(&self, account: &str, synced: bool) -> Result<(), ItemError> {
        let query = self.query(account, synced);
        // SAFETY: the query dictionary outlives the call.
        let status = unsafe { SecItemDelete(query.as_concrete_TypeRef()) };
        if status == errSecSuccess || status == errSecItemNotFound {
            Ok(())
        } else {
            Err(ItemError::Status(status))
        }
    }
}
