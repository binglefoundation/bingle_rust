use bingle_test::tests_common::{self, backends::real::RealBackend};
#[path = "../test_util.rs"]
pub mod test_util;

#[test]
#[cfg(not(target_os = "ios"))]
pub fn algo_ops_basic_suite() {
    assert!(tests_common::algo_ops_basic::<RealBackend>(
        test_util::PASSPHRASE_SPEND
    ));
}
