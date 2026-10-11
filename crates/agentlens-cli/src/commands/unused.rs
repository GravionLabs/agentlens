use agentlens_core::adapter::{Registry, SessionFilter};
use agentlens_core::analyzers::unused::{UnusedItem, UnusedKind};
use agentlens_core::env::Env;
use agentlens_core::export;
use agentlens_core::model::Source;
use agentlens_core::pipeline::unused_run;
use agentlens_core::tokens::TokenEstimator;
use anyhow::Result;

use crate::args::Format;
use crate::output;

fn kind(k: UnusedKind) -> &'static str {
    match k {
        UnusedKind::Skill => "skill",
        UnusedKind::Agent => "agent",
        UnusedKind::McpServer => "mcp-server",
    }
}

fn project(i: &UnusedItem) -> String {
    i.project
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "-".into())
}

fn clients(i: &UnusedItem) -> String {
    i.clients
        .iter()
        .map(|c| c.id())
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn run(
    registry: &Registry,
    env: &Env,
    filter: &SessionFilter,
    only: &[Source],
    estimator: &dyn TokenEstimator,
    all: bool,
    format: Format,
) -> Result<()> {
    let run = unused_run(registry, env, filter, only, estimator);
    let items: Vec<&UnusedItem> = run
        .report
        .items
        .iter()
        .filter(|i| all || i.uses == 0)
        .collect();

    match format {
        Format::Json => {
            let mut report = run.report.clone();
            report.items.retain(|i| all || i.uses == 0);
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        Format::Csv => print!("{}", export::csv::unused(&items)?),
        Format::Table => {
            let rows = items
                .iter()
                .map(|i| {
                    vec![
                        kind(i.kind).to_owned(),
                        i.name.clone(),
                        project(i),
                        clients(i),
                        i.uses.to_string(),
                        output::number(i.requests),
                        i.overhead_tokens
                            .map(output::tokens)
                            .unwrap_or_else(|| "unknown".into()),
                    ]
                })
                .collect();
            println!(
                "{}",
                output::table(
                    &[
                        "kind",
                        "name",
                        "project",
                        "clients",
                        "uses",
                        "requests",
                        "idle cost"
                    ],
                    rows,
                    4
                )
            );
            println!(
                "~ estimated. idle cost = baseline tokens x requests that carried the item; `unknown` means the client reports no data for MCP servers."
            );
            println!("{} sessions in the period.", run.report.sessions);
        }
    }
    if run.warnings > 0 {
        eprintln!(
            "{} warnings while reading sessions and configuration",
            run.warnings
        );
    }
    for e in &run.errors {
        eprintln!("error: {e}");
    }
    Ok(())
}
