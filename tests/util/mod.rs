// This util mod groups utilities and tests under util/ namespace for the integration test crate.
// Expose common test utilities as a submodule so imports like crate::util::test_util::X work.
#[path = "../test_util.rs"]
pub mod test_util;

pub mod net_det;
pub mod mock_bingle_api;
