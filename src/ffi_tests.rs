use crate::tests_common::{self, backends::simple::SimpleBackend};

pub(crate) fn run_algo_ops_tests() -> bool {
    tests_common::algo_ops_basic::<SimpleBackend>()
}

pub(crate) fn run_algo_ops_more_tests() -> bool {
    tests_common::algo_ops_more::<SimpleBackend>()
}

pub(crate) fn run_asset_ops_tests() -> bool {
    tests_common::asset_ops::<SimpleBackend>()
}
