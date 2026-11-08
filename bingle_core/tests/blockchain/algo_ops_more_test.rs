use crate::util::test_util;
use bingle_test::tests_common::{self, backends::real::RealBackend};

#[test]
#[cfg(not(target_os = "ios"))]
pub fn algo_ops_more_suite() {
    assert!(tests_common::algo_ops_more::<RealBackend>(
        test_util::PASSPHRASE_SPEND
    ));
}
