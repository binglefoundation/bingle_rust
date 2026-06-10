// This util mod groups utilities and tests under util/ namespace for the integration test crate.
// Expose common test utilities as a submodule so imports like crate::util::test_util::X work.
#[macro_use]
#[path = "../test_util.rs"]
pub mod test_util;

pub mod net_det;
pub mod mock_bingle_api;
pub mod reusable_mock_api;
pub mod version;

#[path = "cli_parse_test.rs"]
pub mod cli_parse_test;

#[path = "cli/debug_flag.rs"]
pub mod cli_debug_flag;

#[path = "cli/node_file_ids.rs"]
pub mod cli_node_file_ids;

#[path = "cli/node_file_null_and_missing.rs"]
pub mod cli_node_file_null_and_missing;

#[path = "cli/node_file_override.rs"]
pub mod cli_node_file_override;

#[path = "price_parse.rs"]
pub mod price_parse;

#[path = "printing_enable.rs"]
pub mod printing_enable;

#[path = "stun/comments.rs"]
pub mod stun_comments;

pub mod relay_test_util;
#[path = "parse_algos_test.rs"]
pub mod parse_algos_test;
