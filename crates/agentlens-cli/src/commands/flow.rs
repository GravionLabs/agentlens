use agentlens_core::adapter::{Registry, SessionFilter};
use agentlens_core::analyzers::flow::SessionGrowth;
use agentlens_core::env::Env;
use agentlens_core::model::Source;
use agentlens_core::pipeline::flow_run;
use anyhow::Result;
use serde_json::json;

use crate::args::{FlowSection, Format};
use crate::output;

fn project(g: &SessionGrowth) -> String {
    g.project
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "-".into())
}

fn short(id: &str) -> String {
    id.chars().take(8).collect()
}

fn compactions(g: &SessionGrowth) -> String {
    g.compactions
        .iter()
        .map(|c| output::number(*c))
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn run(
    registry: &Registry,
    env: &Env,
    filter: &SessionFilter,
    only: &[Source],
    _section: FlowSection,
    top: usize,
    format: Format,
) -> Result<()> {
    let run = flow_run(registry, env, filter, only);
    let growth = &run.growth.sessions;

    match format {
        Format::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({ "growth": run.growth }))?
            );
        }
        Format::Csv => {
            let header = [
                "source",
                "session",
                "project",
                "model",
                "requests",
                "first_tokens",
                "last_tokens",
                "peak_tokens",
                "compactions",
                "compaction_tokens",
                "near_limit",
            ];
            let rows = growth
                .iter()
                .map(|g| {
                    vec![
                        g.source.id().to_owned(),
                        g.session.clone(),
                        project(g),
                        g.model.clone().unwrap_or_default(),
                        g.requests.to_string(),
                        g.first_tokens.to_string(),
                        g.last_tokens.to_string(),
                        g.peak_tokens.to_string(),
                        g.compactions.len().to_string(),
                        g.compactions
                            .iter()
                            .map(u64::to_string)
                            .collect::<Vec<_>>()
                            .join(" "),
                        g.near_limit.to_string(),
                    ]
                })
                .collect();
            print!("{}", output::csv(&header, rows)?);
        }
        Format::Table => {
            let shown = if top == 0 {
                growth.len()
            } else {
                top.min(growth.len())
            };
            let rows = growth
                .iter()
                .take(shown)
                .map(|g| {
                    vec![
                        g.source.id().to_owned(),
                        short(&g.session),
                        project(g),
                        g.model.clone().unwrap_or_else(|| "-".into()),
                        g.requests.to_string(),
                        output::number(g.first_tokens),
                        output::number(g.last_tokens),
                        output::number(g.peak_tokens),
                        compactions(g),
                        if g.near_limit { "yes" } else { "" }.to_owned(),
                    ]
                })
                .collect();
            println!("Context growth");
            println!(
                "{}",
                output::table(
                    &[
                        "source",
                        "session",
                        "project",
                        "model",
                        "requests",
                        "first",
                        "last",
                        "peak",
                        "size before compaction",
                        "near limit"
                    ],
                    rows,
                    4
                )
            );
            println!(
                "Measured context size (input + cache) of main-agent requests; sorted by peak. `near limit` = at least 90% of the model's documented window, or of the largest size at which that model was compacted if the window does not fit."
            );
            if shown < growth.len() {
                println!(
                    "{} more sessions; use --top 0, --format json or csv to see them all.",
                    growth.len() - shown
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
