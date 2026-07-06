use rust_comms::get_module_version;
use rust_comms::util::version::VersionInfo;

pub fn get_version() -> VersionInfo {
    get_module_version!()
}
