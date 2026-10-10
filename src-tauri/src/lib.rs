//! The AgentLens desktop application: a thin shell over `agentlens-core`.

/// Starts the application.
pub fn run() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("error while running AgentLens");
}
