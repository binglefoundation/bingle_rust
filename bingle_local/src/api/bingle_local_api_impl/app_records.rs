//! The app's own records in the state file (issue #471): named JSON values the engine keeps with
//! the rest of the state, sealed when the state file is sealed, but never reads. The app uses them
//! for data that belongs to the account, such as its read markers and settings, so that data gets
//! the state file's protection and goes with the account when it is deleted.

use super::BingleApiLocalImpl;
use bingle_core::api::bingle_api::BingleError;

/// The longest app record name, in characters.
pub const APP_RECORD_NAME_MAX: usize = 64;
/// The largest app record value, in bytes.
pub const APP_RECORD_VALUE_MAX: usize = 256 * 1024;

/// Checks an app record's name and, when given, its value: a name is 1 to
/// [`APP_RECORD_NAME_MAX`] characters from `a-z`, `0-9`, `_`, `.` and `-`, and a value is at most
/// [`APP_RECORD_VALUE_MAX`] bytes. The value isn't parsed. Returns why it is refused.
pub fn validate_app_record(name: &str, value: Option<&str>) -> Result<(), String> {
    if name.is_empty() || name.len() > APP_RECORD_NAME_MAX {
        return Err(format!(
            "app record name must be 1 to {APP_RECORD_NAME_MAX} characters"
        ));
    }
    if !name
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'_' | b'.' | b'-'))
    {
        return Err(format!(
            "app record name '{name}' may use only a-z, 0-9, '_', '.' and '-'"
        ));
    }
    if let Some(value) = value
        && value.len() > APP_RECORD_VALUE_MAX
    {
        return Err(format!(
            "app record '{name}' is {} bytes; the limit is {APP_RECORD_VALUE_MAX}",
            value.len()
        ));
    }
    Ok(())
}

fn lock_failed(e: impl std::fmt::Display) -> BingleError {
    let msg = format!("mutex poisoned: {e}");
    tracing::error!("[app_records] Failed to lock app records: {}", msg);
    BingleError::Other(msg)
}

impl BingleApiLocalImpl {
    /// The core of [`BingleLocalApi::app_record`](crate::api::BingleLocalApi::app_record).
    pub(super) fn app_record_inner(&self, name: &str) -> Result<Option<String>, BingleError> {
        validate_app_record(name, None).map_err(BingleError::Other)?;
        let records = self.app_records.lock().map_err(lock_failed)?;
        Ok(records.get(name).cloned())
    }

    /// The core of [`BingleLocalApi::set_app_record`](crate::api::BingleLocalApi::set_app_record).
    pub(super) fn set_app_record_inner(&self, name: &str, json: String) -> Result<(), BingleError> {
        validate_app_record(name, Some(&json)).map_err(BingleError::Other)?;
        let mut records = self.app_records.lock().map_err(lock_failed)?;
        records.insert(name.to_string(), json);
        Ok(())
    }

    /// `&self` core of [`BingleLocalApi::set_app_record`](crate::api::BingleLocalApi::set_app_record),
    /// for a caller holding the store behind an `Arc`, like the other `*_shared` cores (bingle_cli's
    /// `logclient` keeps its spam-limit counts this way, issue #66).
    pub fn set_app_record_shared(&self, name: &str, json: String) -> Result<(), BingleError> {
        self.set_app_record_inner(name, json)
    }

    /// The core of
    /// [`BingleLocalApi::remove_app_record`](crate::api::BingleLocalApi::remove_app_record).
    pub(super) fn remove_app_record_inner(&self, name: &str) -> Result<(), BingleError> {
        validate_app_record(name, None).map_err(BingleError::Other)?;
        let mut records = self.app_records.lock().map_err(lock_failed)?;
        records.remove(name);
        Ok(())
    }
}
