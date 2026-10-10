use agentlens_core::adapter::{Registry, SessionFilter};
use agentlens_core::analyzers::compare::SourceStats;
use agentlens_core::env::Env;
use agentlens_core::model::Source;
use agentlens_core::pipeline::compare_run;
use agentlens_core::tokens::CharsPerToken;
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
                .map(|t| format!("~{}", output::number(t)))
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
    format: Format,
) -> Result<()> {
    let run = compare_run(registry, env, filter, only, &CharsPerToken);
    let sources = &run.report.sources;

    match format {
        Format::Json => println!("{}", serde_json::to_string_pretty(&run.report)?),
        Format::Csv => {
            let header = [
                "source",
                "sessions",
                "requests",
                "input_tokens",
                "output_tokens",
                "cache_hit_rate",
                "tool_calls",
                "tool_errors",
                "tool_error_rate",
                "skill_invocations",
                "skill_overhead_tokens",
                "total_duration_ms",
                "mean_session_ms",
            ];
            let opt = |v: Option<String>| v.unwrap_or_default();
            let rows = sources
                .iter()
                .map(|s| {
                    vec![
                        s.source.id().to_owned(),
                        s.sessions.to_string(),
                        opt(s.requests.map(|v| v.to_string())),
                        opt(s.input_tokens.map(|v| v.to_string())),
                        opt(s.output_tokens.map(|v| v.to_string())),
                        opt(s.cache_hit_rate.map(|v| format!("{v:.6}"))),
                        s.tool_calls.to_string(),
                        s.tool_errors.to_string(),
                        opt(s.tool_error_rate.map(|v| format!("{v:.6}"))),
                        s.skill_invocations.to_string(),
                        opt(s.skill_overhead_tokens.map(|v| v.to_string())),
                        opt(s.total_duration_ms.map(|v| v.to_string())),
                        opt(s.mean_session_ms.map(|v| v.to_string())),
                    ]
                })
                .collect();
            print!("{}", output::csv(&header, rows)?);
        }
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
