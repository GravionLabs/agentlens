//! The AgentLens desktop application: a thin shell over `agentlens-core`.

pub mod api;

use std::sync::Arc;

use api::{Answer, Context, Filters};
use serde::Serialize;
use tauri::State;

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

/// Starts the application.
pub fn run() {
    let shared = Context::system().expect("cannot determine the home directory");
    tauri::Builder::default()
        .manage(Arc::new(shared))
        .invoke_handler(tauri::generate_handler![
            refresh, overview, skills, context, unused, flow, cost, efficiency, compare, sessions,
            timeline
        ])
        .run(tauri::generate_context!())
        .expect("error while running AgentLens");
}
