//! One adapter per session data source.

pub mod claude_code;
pub mod copilot_cli;
pub mod opencode;
pub mod vscode_copilot;

use crate::adapter::Registry;

/// A registry with every adapter that exists.
pub fn default_registry() -> Registry {
    let mut r = Registry::new();
    r.register(Box::new(claude_code::ClaudeCodeAdapter));
    r.register(Box::new(opencode::OpenCodeAdapter));
    r.register(Box::new(copilot_cli::CopilotCliAdapter));
    r.register(Box::new(vscode_copilot::VsCodeCopilotAdapter));
    r
}
