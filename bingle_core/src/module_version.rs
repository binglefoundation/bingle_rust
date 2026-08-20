use crate::get_module_version;
use crate::util::version::VersionInfo;

/// Return the [`VersionInfo`] for `bingle_core`.
pub fn get_version() -> VersionInfo {
    get_module_version!()
}
