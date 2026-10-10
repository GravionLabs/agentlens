//! Core of AgentLens: adapters turn the session data of a coding agent into
//! normalized events, analyzers turn those events into reports.

pub mod adapter;
pub mod env;
pub mod model;

/// Version of this crate, for report metadata.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
