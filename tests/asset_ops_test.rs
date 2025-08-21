use rust_comms::tests_common::{self, backends::real::RealBackend};

#[test]
fn asset_ops_suite() {
    assert!(tests_common::asset_ops::<RealBackend>());
}
