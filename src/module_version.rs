use crate::util::version::VersionInfo;
use crate::get_module_version;

pub fn get_version() -> VersionInfo {
    get_module_version!()
}
