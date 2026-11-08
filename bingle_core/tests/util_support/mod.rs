// Support-only utilities for use in integration, flaky and all test targets.
// This module contains only helper code (no test functions), so it can be included
// in targets where util test files (cli_parse_test, price_parse, etc.) must not appear.
// test_util and pki are declared here only, once per test target, and used elsewhere as
// `crate::util::test_util` / `crate::util::pki` (issue #11): tooling that rewrites source, such as
// `clippy --fix`, corrupts a file that is `#[path]`-included more than once.
#[macro_use]
#[path = "../test_util.rs"]
pub mod test_util;
#[path = "../util/mock_bingle_api.rs"]
pub mod mock_bingle_api;
#[path = "../dtls/pki.rs"]
pub mod pki;
#[path = "../util/relay_test_util.rs"]
pub mod relay_test_util;
#[path = "../util/reusable_mock_api.rs"]
pub mod reusable_mock_api;
