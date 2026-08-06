pub mod tests_common;
pub mod temp_file_helpers;

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

#[cfg(target_os = "ios")]
#[path = "../../tests/api/mod.rs"]
pub mod api;

#[cfg(target_os = "ios")]
#[path = "../../tests/blockchain/mod.rs"]
pub mod blockchain;

#[cfg(target_os = "ios")]
#[path = "../../tests/dtls/mod.rs"]
pub mod dtls;

#[cfg(target_os = "ios")]
#[path = "../../tests/engine/mod.rs"]
pub mod engine;

#[cfg(target_os = "ios")]
#[path = "../../tests/protocol/mod.rs"]
pub mod protocol;

#[cfg(target_os = "ios")]
#[path = "../../tests/relay/mod.rs"]
pub mod relay;

#[cfg(target_os = "ios")]
#[path = "../../tests/stun/mod.rs"]
pub mod stun;

#[cfg(target_os = "ios")]
#[path = "../../tests/cli/mod.rs"]
pub mod cli;

#[cfg(target_os = "ios")]
#[macro_use]
#[path = "../../tests/util/mod.rs"]
pub mod util;

#[cfg(target_os = "ios")]
#[path = "../../tests/ddb.rs"]
pub mod ddb;

#[cfg(target_os = "ios")]
#[path = "../../tests/turn/mod.rs"]
pub mod turn;

#[cfg(target_os = "ios")]
#[path = "../../tests/distributed_mutex/mod.rs"]
pub mod distributed_mutex;

#[cfg(target_os = "ios")]
#[path = "../../tests/integration/mod.rs"]
pub mod integration;

#[cfg(target_os = "ios")]
#[path = "../../tests/setup_localnet.rs"]
pub mod setup_localnet;

#[cfg(target_os = "ios")]
#[path = "../../tests/messages.rs"]
pub mod messages;

#[cfg(target_os = "ios")]
mod all_tests_ffi;

#[cfg(target_os = "ios")]
#[unsafe(no_mangle)]
pub extern "C" fn bingle_core_run_all_unit_tests() -> u32 {
    157
}

#[cfg(target_os = "ios")]
#[unsafe(no_mangle)]
pub extern "C" fn bingle_core_run_named_test(name: *const libc::c_char) -> u8 {
    use std::ffi::CStr;
    if name.is_null() {
        return 0;
    }
    let cstr = unsafe { CStr::from_ptr(name) };
    let test_name = match cstr.to_str() {
        Ok(s) => s,
        Err(_) => return 0,
    };
    if all_tests_ffi::run_named_test(test_name) {
        1
    } else {
        0
    }
}
