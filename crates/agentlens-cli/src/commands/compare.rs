use agentlens_core::adapter::{Registry, SessionFilter};
use agentlens_core::analyzers::compare::SourceStats;
use agentlens_core::env::Env;
use agentlens_core::export;
use agentlens_core::model::Source;
use agentlens_core::pipeline::compare_run;
use agentlens_core::tokens::TokenEstimator;
use anyhow::Result;

use crate::output;

use crate::args::Format;

const NA: &str = "n/a";

fn percent(v: Option<f64>) -> String {
    v.map(|v| format!("{:.1}%", v * 100.0))
        .unwrap_or_else(|| NA.into())
}

fn count(v: Option<u64>) -> String {
    v.map(output::number).unwrap_or_else(|| NA.into())
}

fn minutes(ms: Option<u64>) -> String {
    ms.map(|ms| format!("{:.1}", ms as f64 / 60_000.0))
        .unwrap_or_else(|| NA.into())
}

/// One metric per row, one source per column.
fn metrics(s: &SourceStats) -> Vec<(&'static str, String)> {
    vec![
        ("sessions", output::number(s.sessions)),
        ("requests", count(s.requests)),
        ("input tokens", count(s.input_tokens)),
        ("output tokens", count(s.output_tokens)),
        ("cache hit rate", percent(s.cache_hit_rate)),
        ("tool calls", output::number(s.tool_calls)),
        ("tool error rate", percent(s.tool_error_rate)),
        ("skill invocations", output::number(s.skill_invocations)),
        (
            "skill overhead (est.)",
            s.skill_overhead_tokens
                .map(output::tokens)
                .unwrap_or_else(|| NA.into()),
        ),
        ("mean session (min)", minutes(s.mean_session_ms)),
        ("total time (min)", minutes(s.total_duration_ms)),
    ]
}

pub fn run(
    registry: &Registry,
    env: &Env,
    filter: &SessionFilter,
    only: &[Source],
    estimator: &dyn TokenEstimator,
    format: Format,
) -> Result<()> {
    let run = compare_run(registry, env, filter, only, estimator);
    let sources = &run.report.sources;

    match format {
        Format::Json => println!("{}", serde_json::to_string_pretty(&run.report)?),
        Format::Csv => print!("{}", export::csv::compare(sources)?),
        Format::Table => {
            let per_source: Vec<Vec<(&str, String)>> = sources.iter().map(metrics).collect();
            let names: Vec<&str> = per_source
                .first()
                .map(|m| m.iter().map(|(n, _)| *n).collect())
                .unwrap_or_default();
            let rows = names
                .iter()
                .enumerate()
                .map(|(i, name)| {
                    let mut row = vec![(*name).to_owned()];
                    row.extend(per_source.iter().map(|m| m[i].1.clone()));
                    row
                })
                .collect();
            let mut header = vec!["metric"];
            header.extend(sources.iter().map(|s| s.source.id()));
            println!("{}", output::table(&header, rows, 1));
            println!(
                "n/a: the client does not record it. `~` is estimated; everything else comes from client data. Session time is first to last event."
            );
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
