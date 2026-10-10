//! One adapter per session data source.

pub mod claude_code;
pub mod copilot_cli;
pub mod opencode;
pub mod otel;
pub mod vscode_copilot;

use crate::adapter::Registry;

/// A registry with every adapter that exists.
pub fn default_registry() -> Registry {
    registry_with_otel(Vec::new())
}

/// All adapters plus, if files are given, the OpenTelemetry import. The import
/// is registered last, so a session that exists locally wins over its export.
pub fn registry_with_otel(otel_files: Vec<std::path::PathBuf>) -> Registry {
    let mut r = Registry::new();
    r.register(Box::new(claude_code::ClaudeCodeAdapter));
    r.register(Box::new(opencode::OpenCodeAdapter));
    r.register(Box::new(copilot_cli::CopilotCliAdapter));
    r.register(Box::new(vscode_copilot::VsCodeCopilotAdapter));
    if !otel_files.is_empty() {
        r.register(Box::new(otel::OtelAdapter::new(otel_files)));
    }
    r
}
