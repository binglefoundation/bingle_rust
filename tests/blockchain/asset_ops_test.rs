use bingle_test::tests_common::{self, backends::real::RealBackend};
#[path = "../test_util.rs"]
mod test_util;

#[test]
fn asset_ops_suite() {
    assert!(tests_common::asset_ops::<RealBackend>(test_util::PASSPHRASE_SPEND));
}
