pub mod tests_common;

mod ffi_tests;

#[unsafe(no_mangle)]
pub extern "C" fn rust_comms_run_algo_ops_tests() -> u8 {
    if ffi_tests::run_algo_ops_tests() { 1 } else { 0 }
}

#[unsafe(no_mangle)]
pub extern "C" fn rust_comms_run_algo_ops_more_tests() -> u8 {
    if ffi_tests::run_algo_ops_more_tests() { 1 } else { 0 }
}

#[unsafe(no_mangle)]
pub extern "C" fn rust_comms_run_asset_ops_tests() -> u8 {
    if ffi_tests::run_asset_ops_tests() { 1 } else { 0 }
}
