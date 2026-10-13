//! The AgentLens desktop application: a thin shell over `agentlens-core`.

pub mod api;

use std::sync::{Arc, Mutex};

use api::{Answer, Context, Filters};
use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_updater::{Update, UpdaterExt};

type Shared<'a> = State<'a, Arc<Context>>;

/// Runs a blocking analysis off the UI thread.
async fn blocking<T, F>(ctx: &Shared<'_>, f: F) -> Result<Answer<T>, String>
where
    T: Serialize + Send + 'static,
    F: FnOnce(&Context) -> Result<Answer<T>, String> + Send + 'static,
{
    let ctx = Arc::clone(ctx);
    tauri::async_runtime::spawn_blocking(move || f(&ctx))
        .await
        .map_err(|e| format!("the analysis stopped unexpectedly: {e}"))?
}

#[tauri::command]
async fn refresh(ctx: Shared<'_>) -> Result<Answer<()>, String> {
    blocking(&ctx, api::refresh).await
}

#[tauri::command]
async fn overview(
    ctx: Shared<'_>,
    filters: Filters,
) -> Result<Answer<Vec<api::SourceOverview>>, String> {
    blocking(&ctx, move |c| api::overview(c, &filters)).await
}

#[tauri::command]
async fn advice(
    ctx: Shared<'_>,
    filters: Filters,
) -> Result<Answer<agentlens_core::analyzers::advice::AdviceReport>, String> {
    blocking(&ctx, move |c| api::advice(c, &filters)).await
}

#[tauri::command]
async fn skills(ctx: Shared<'_>, filters: Filters) -> Result<Answer<api::SkillsData>, String> {
    blocking(&ctx, move |c| api::skills(c, &filters)).await
}

#[tauri::command]
async fn context(
    ctx: Shared<'_>,
    filters: Filters,
) -> Result<Answer<agentlens_core::analyzers::context::ContextReport>, String> {
    blocking(&ctx, move |c| api::context(c, &filters)).await
}

#[tauri::command]
async fn unused(
    ctx: Shared<'_>,
    filters: Filters,
) -> Result<Answer<agentlens_core::analyzers::unused::UnusedReport>, String> {
    blocking(&ctx, move |c| api::unused(c, &filters)).await
}

#[tauri::command]
async fn flow(ctx: Shared<'_>, filters: Filters) -> Result<Answer<api::FlowData>, String> {
    blocking(&ctx, move |c| api::flow(c, &filters)).await
}

#[tauri::command]
async fn cost(
    ctx: Shared<'_>,
    filters: Filters,
    by: String,
    what_if: Option<String>,
) -> Result<Answer<api::CostData>, String> {
    blocking(&ctx, move |c| {
        api::cost(c, &filters, &by, what_if.as_deref())
    })
    .await
}

#[tauri::command]
async fn efficiency(
    ctx: Shared<'_>,
    filters: Filters,
    by: String,
) -> Result<Answer<agentlens_core::analyzers::efficiency::EfficiencyReport>, String> {
    blocking(&ctx, move |c| api::efficiency(c, &filters, &by)).await
}

#[tauri::command]
async fn compare(
    ctx: Shared<'_>,
    filters: Filters,
) -> Result<Answer<agentlens_core::analyzers::compare::CompareReport>, String> {
    blocking(&ctx, move |c| api::compare(c, &filters)).await
}

#[tauri::command]
async fn sessions(
    ctx: Shared<'_>,
    filters: Filters,
) -> Result<Answer<Vec<agentlens_core::analyzers::timeline::SessionSummary>>, String> {
    blocking(&ctx, move |c| api::sessions(c, &filters)).await
}

#[tauri::command]
async fn timeline(
    ctx: Shared<'_>,
    filters: Filters,
    source: String,
    id: String,
) -> Result<Answer<agentlens_core::analyzers::timeline::Timeline>, String> {
    blocking(&ctx, move |c| api::timeline(c, &filters, &source, &id)).await
}

/// What was written by an export.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Exported {
    path: String,
    bytes: usize,
    warnings: u64,
    errors: Vec<String>,
}

/// Exports a view to the file the user chose in the save dialog. Nothing is written without a path,
/// and only that one file.
#[tauri::command]
async fn export_view(
    ctx: Shared<'_>,
    view: String,
    format: String,
    filters: Filters,
    options: Option<api::ExportOptions>,
    path: String,
) -> Result<Exported, String> {
    if path.trim().is_empty() {
        return Err("no file was chosen".into());
    }
    let options = options.unwrap_or_default();
    let ctx = Arc::clone(&ctx);
    tauri::async_runtime::spawn_blocking(move || {
        let file = api::export(&ctx, &view, &format, &filters, &options)?;
        std::fs::write(&path, &file.content).map_err(|e| format!("cannot write {path}: {e}"))?;
        Ok(Exported {
            path,
            bytes: file.content.len(),
            warnings: file.warnings,
            errors: file.errors,
        })
    })
    .await
    .map_err(|e| format!("the export stopped unexpectedly: {e}"))?
}

/// The suggested file name for the save dialog.
#[tauri::command]
fn export_name(view: String, format: String) -> String {
    api::export_file_name(&view, &format)
}

/// The version of the running application (injected into `tauri.conf.json` by the release build).
#[tauri::command]
fn app_version(app: AppHandle) -> String {
    app.package_info().version.to_string()
}

/// The update found by the last check, waiting to be installed.
#[derive(Default)]
struct PendingUpdate(Mutex<Option<Update>>);

/// A newer release than the running application.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdateInfo {
    version: String,
    current_version: String,
    notes: Option<String>,
    date: Option<String>,
}

/// Asks the release page whether a newer version exists. This is the only network
/// call of the application and it only happens when the user asks for it.
#[tauri::command]
async fn update_check(
    app: AppHandle,
    pending: State<'_, PendingUpdate>,
) -> Result<Option<UpdateInfo>, String> {
    let update = app
        .updater()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| format!("Could not check for updates: {e}"))?;
    let info = update.as_ref().map(|u| UpdateInfo {
        version: u.version.clone(),
        current_version: u.current_version.clone(),
        notes: u.body.clone(),
        date: u.date.map(|d| d.to_string()),
    });
    *pending.0.lock().unwrap() = update;
    Ok(info)
}

/// Downloads and installs the update found by the last check, then restarts.
#[tauri::command]
async fn update_install(app: AppHandle, pending: State<'_, PendingUpdate>) -> Result<(), String> {
    let update = pending.0.lock().unwrap().take();
    let Some(update) = update else {
        return Err("There is no update to install; check for updates first".into());
    };
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|e| format!("Could not install the update: {e}"))?;
    app.restart()
}

/// The directory that holds the running application's binaries (the CLI and the MCP server sit next to it).
fn bin_dir() -> Result<std::path::PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| format!("cannot find the application: {e}"))?;
    exe.parent()
        .map(std::path::Path::to_path_buf)
        .ok_or_else(|| "cannot find the folder of the application".to_string())
}

/// The state of the command line tool links; writes nothing.
#[tauri::command]
async fn cli_tools_status(ctx: Shared<'_>) -> Result<Answer<api::CliTools>, String> {
    let dir = bin_dir()?;
    blocking(&ctx, move |c| api::cli_tools_status(c, &dir)).await
}

/// Links (or copies) the CLI and the MCP server into `~/.local/bin`.
#[tauri::command]
async fn cli_tools_install(ctx: Shared<'_>) -> Result<Answer<api::CliTools>, String> {
    let dir = bin_dir()?;
    blocking(&ctx, move |c| api::install_cli_tools(c, &dir)).await
}

/// Removes the links (or copies) that `cli_tools_install` made.
#[tauri::command]
async fn cli_tools_remove(ctx: Shared<'_>) -> Result<Answer<api::CliTools>, String> {
    let dir = bin_dir()?;
    blocking(&ctx, move |c| api::remove_cli_tools(c, &dir)).await
}

/// Starts the application.
pub fn run() {
    let shared = Context::system().expect("cannot determine the home directory");
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .manage(Arc::new(shared))
        .manage(PendingUpdate::default())
        .invoke_handler(tauri::generate_handler![
            app_version,
            export_view,
            export_name,
            refresh,
            overview,
            skills,
            advice,
            context,
            unused,
            flow,
            cost,
            efficiency,
            compare,
            sessions,
            timeline,
            update_check,
            update_install,
            cli_tools_status,
            cli_tools_install,
            cli_tools_remove
        ])
        .run(tauri::generate_context!())
        .expect("error while running AgentLens");
}
