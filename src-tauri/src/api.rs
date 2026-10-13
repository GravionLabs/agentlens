//! The analyses of `agentlens-core` as plain functions the Tauri commands wrap. They live in the core
//! (`agentlens_core::api`) so that the CLI-equivalent numbers come from one place for every shell.

pub use agentlens_core::api::*;
// The command line tools (`cli_tools_status`, `install_cli_tools`, `remove_cli_tools` come with the glob).
pub use agentlens_core::cli_tools::CliTools;
