pub mod temp_file_helpers;
pub mod tests_common;

// Reusable algokit-localnet integration harness, enabled with the `localnet` feature.
#[cfg(feature = "localnet")]
pub mod localnet;

extern crate self as bingle_test;

mod ffi_tests;

#[unsafe(no_mangle)]
pub extern "C" fn bingle_core_run_algo_ops_tests() -> u8 {
    if ffi_tests::run_algo_ops_tests() {
        1
    } else {
        0
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bingle_core_run_algo_ops_more_tests() -> u8 {
    if ffi_tests::run_algo_ops_more_tests() {
        1
    } else {
        0
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bingle_core_run_asset_ops_tests() -> u8 {
    if ffi_tests::run_asset_ops_tests() {
        1
    } else {
        0
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bingle_core_run_stun_tests() -> u8 {
    if ffi_tests::run_stun_tests() { 1 } else { 0 }
}
