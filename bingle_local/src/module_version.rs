//! Build and version information for the `bingle_local` crate.

use bingle_core::get_module_version;
use bingle_core::util::version::VersionInfo;

/// Return build and version information for this crate.
///
/// The returned [`VersionInfo`] captures the crate version and build metadata recorded at compile
/// time by the `get_module_version!` macro.
pub fn get_version() -> VersionInfo {
    get_module_version!()
}
