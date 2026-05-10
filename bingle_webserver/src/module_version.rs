use rust_comms::util::version::VersionInfo;
use rust_comms::get_module_version;

pub fn get_version() -> VersionInfo {
    get_module_version!()
}
