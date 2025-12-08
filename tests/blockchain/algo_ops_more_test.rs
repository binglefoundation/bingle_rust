use bingle_test::tests_common::{self, backends::real::RealBackend};
#[path = "../test_util.rs"]
mod test_util;

#[test]
fn algo_ops_more_suite() {
    assert!(tests_common::algo_ops_more::<RealBackend>(test_util::PASSPHRASE_SPEND));
}
