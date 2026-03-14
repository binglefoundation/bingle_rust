use bingle_test::tests_common::{self, backends::real::RealBackend};
#[path = "../test_util.rs"]
pub mod test_util;

#[cfg_attr(not(target_os = "ios"), test)]
pub fn asset_ops_suite() {
    assert!(tests_common::asset_ops::<RealBackend>(test_util::PASSPHRASE_SPEND));
}
