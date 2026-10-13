use agentlens_core::adapter::{Registry, SessionFilter};
use agentlens_core::analyzers::context::ContextRow;
use agentlens_core::env::Env;
use agentlens_core::export;
use agentlens_core::model::{ContextKind, Source};
use agentlens_core::pipeline::context_run;
use agentlens_core::tokens::TokenEstimator;
use anyhow::Result;

use crate::args::{ContextView, Format};
use crate::output;

fn in_view(kind: ContextKind, view: ContextView) -> bool {
    use ContextKind::*;
    match view {
        ContextView::All => true,
        ContextView::Tools => matches!(kind, ToolListing | ToolDefinition | McpInstructions),
        ContextView::Instructions => kind == InstructionFile,
        ContextView::Agents => kind == AgentListing,
        ContextView::Skills => kind == SkillListing,
    }
}

fn kind_id(kind: ContextKind) -> String {
    serde_json::to_value(kind)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

fn project(row: &ContextRow) -> String {
    row.project
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "-".into())
}

pub fn run(
    registry: &Registry,
    env: &Env,
    filter: &SessionFilter,
    only: &[Source],
    estimator: &dyn TokenEstimator,
    view: ContextView,
    display: output::Display,
) -> Result<()> {
    let run = context_run(registry, env, filter, only, estimator);
    let rows: Vec<&ContextRow> = run
        .report
        .rows
        .iter()
        .filter(|r| in_view(r.kind, view))
        .collect();

    match display.format {
        Format::Json => {
            let mut report = run.report.clone();
            report.rows.retain(|r| in_view(r.kind, view));
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        Format::Csv => print!("{}", export::csv::context(&rows)?),
        Format::Table => {
            let body = rows
                .iter()
                .map(|r| {
                    vec![
                        r.source.id().to_owned(),
                        project(r),
                        kind_id(r.kind),
                        r.owner.clone(),
                        r.items.to_string(),
                        display.tokens(r.tokens),
                        output::number(r.requests),
                        display.tokens(r.overhead_tokens),
                    ]
                })
                .collect();
            println!(
                "{}",
                output::table(
                    &[
                        "source", "project", "kind", "owner", "items", "tokens", "requests",
                        "overhead"
                    ],
                    body,
                    4
                )
            );
            println!(
                "~ estimated. overhead = tokens x later requests; static content stays in every request."
            );
            for (source, f) in &run.report.first_request {
                println!(
                    "{}: first request of a session holds {} tokens on average, at most {} (measured; upper bound of all static context).",
                    source.id(),
                    display.count(f.total_tokens / u64::from(f.sessions.max(1))),
                    display.count(f.max_tokens)
                );
            }
            for (source, n) in &run.report.unavailable {
                println!(
                    "{}: not available, the client reports no context items ({n} sessions).",
                    source.id()
                );
            }
        }
    }
    if run.warnings > 0 {
        eprintln!(
            "{} warnings while reading sessions (run `agentlens scan` for details)",
            run.warnings
        );
    }
    for e in &run.errors {
        eprintln!("error: {e}");
    }
    Ok(())
}
