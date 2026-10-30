//! Core of AgentLens: adapters turn the session data of a coding agent into
//! normalized events, analyzers turn those events into reports.

pub mod adapter;
pub mod adapters;
pub mod analyzers;
pub mod api;
pub mod catalog;
pub mod cli_tools;
pub mod commit;
pub mod env;
pub mod exact;
pub mod export;
pub mod instructions;
pub mod inventory;
pub mod mcp_setup;
pub mod model;
pub mod pipeline;
pub mod prices;
pub mod references;
pub mod report;
pub mod since;
pub mod tokens;
pub mod worktree;

/// Version of this crate, for report metadata.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
