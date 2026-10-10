//! One adapter per session data source.

pub mod claude_code;

use crate::adapter::Registry;

/// A registry with every adapter that exists.
pub fn default_registry() -> Registry {
    let mut r = Registry::new();
    r.register(Box::new(claude_code::ClaudeCodeAdapter));
    r
}
