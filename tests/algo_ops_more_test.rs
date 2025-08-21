use rust_comms::tests_common::{self, backends::real::RealBackend};

#[test]
fn algo_ops_more_suite() {
    assert!(tests_common::algo_ops_more::<RealBackend>());
}
