//! Deprecated: use crate::util::cli_utils instead.
//! This shim re-exports parse_start_options_from_args from cli_utils to preserve older imports.
//! Supported options include --handle, --passphrase, --relay, --static-ip, --stun-servers,
//! --stun-servers-file, and --node-file.

pub use crate::util::cli_utils::parse_start_options_from_args;
