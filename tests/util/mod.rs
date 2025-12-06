// Grouped util tests (plus utilities module)

// This module intentionally includes the util tests tree. Although tests/test_util.rs
// contains utilities rather than tests, we include it via path so submodules that
// expect `mod test_util;` continue to compile.

#[path = "../test_util.rs"]
pub mod test_util;

#[path = "cli_parse_test.rs"]
mod cli_parse_test;

#[path = "printing_enable.rs"]
mod printing_enable;

#[path = "price_parse.rs"]
mod price_parse;

// util/cli tests as direct submodules to avoid double 'cli' in paths
#[path = "cli/debug_flag.rs"]
mod cli_debug_flag;
#[path = "cli/node_file_ids.rs"]
mod cli_node_file_ids;
#[path = "cli/node_file_null_and_missing.rs"]
mod cli_node_file_null_and_missing;
#[path = "cli/node_file_override.rs"]
mod cli_node_file_override;

// util/stun tests
#[path = "stun/comments.rs"]
mod stun_comments;
