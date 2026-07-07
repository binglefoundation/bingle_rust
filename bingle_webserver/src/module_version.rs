use bingle_core::get_module_version;
use bingle_core::util::version::VersionInfo;

pub fn get_version() -> VersionInfo {
    get_module_version!()
}
