use crate::get_module_version;
use crate::util::version::VersionInfo;

pub fn get_version() -> VersionInfo {
    get_module_version!()
}
