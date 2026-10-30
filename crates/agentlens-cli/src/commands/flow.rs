use agentlens_core::adapter::{Registry, SessionFilter};
use agentlens_core::analyzers::flow::{HooksReport, RetryLoop, SessionGrowth, ToolErrors};
use agentlens_core::env::Env;
use agentlens_core::export;
use agentlens_core::model::Source;
use agentlens_core::pipeline::{FlowRun, flow_run};
use anyhow::{Result, bail};
use serde_json::json;

use crate::args::{FlowSection, Format};
use crate::output;

fn project(p: &Option<std::path::PathBuf>) -> String {
    p.as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "-".into())
}

fn short(id: &str) -> String {
    id.chars().take(8).collect()
}

fn shown(top: usize, len: usize) -> usize {
    if top == 0 { len } else { top.min(len) }
}

fn more(top: usize, len: usize) {
    if shown(top, len) < len {
        println!(
            "{} more rows; use --top 0, --format json or csv to see them all.",
            len - shown(top, len)
        );
    }
}

fn rate(t: &ToolErrors) -> String {
    if t.calls == 0 {
        return "-".into();
    }
    format!("{:.1}%", t.errors as f64 * 100.0 / t.calls as f64)
}

fn growth_table(growth: &[SessionGrowth], top: usize, display: output::Display) {
    let rows = growth
        .iter()
        .take(shown(top, growth.len()))
        .map(|g| {
            vec![
                g.source.id().to_owned(),
                short(&g.session),
                project(&g.project),
                g.model.clone().unwrap_or_else(|| "-".into()),
                g.requests.to_string(),
                display.count(g.first_tokens),
                display.count(g.last_tokens),
                display.count(g.peak_tokens),
                g.compactions
                    .iter()
                    .map(|c| display.count(*c))
                    .collect::<Vec<_>>()
                    .join(" "),
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
                "near limit",
            ],
            rows,
            4
        )
    );
    println!(
        "Measured context size (input + cache) of main-agent requests; sorted by peak. `near limit` = at least 90% of the model's documented window, or of the largest size at which that model was compacted if the window does not fit."
    );
    more(top, growth.len());
}

fn errors_table(tools: &[ToolErrors], loops: &[RetryLoop], top: usize) {
    let rows = tools
        .iter()
        .take(shown(top, tools.len()))
        .map(|t| {
            vec![
                t.source.id().to_owned(),
                t.tool.clone(),
                output::number(t.calls),
                output::number(t.errors),
                rate(t),
                output::number(t.unknown),
                t.loops.to_string(),
            ]
        })
        .collect();
    println!("Tool errors");
    println!(
        "{}",
        output::table(
            &[
                "source",
                "tool",
                "calls",
                "errors",
                "error rate",
                "unknown",
                "retry loops"
            ],
            rows,
            2
        )
    );
    println!("`unknown` calls have no recorded outcome and are not counted as errors.");
    more(top, tools.len());
    if !loops.is_empty() {
        let rows = loops
            .iter()
            .take(shown(top, loops.len()))
            .map(|l| {
                vec![
                    l.source.id().to_owned(),
                    short(&l.session),
                    l.tool.clone(),
                    l.input.clone().unwrap_or_else(|| "-".into()),
                    l.calls.to_string(),
                    l.errors.to_string(),
                ]
            })
            .collect();
        println!("\nRetry loops (3 or more adjacent identical calls, the first two failed)");
        println!(
            "{}",
            output::table(
                &["source", "session", "tool", "input", "calls", "errors"],
                rows,
                4
            )
        );
        more(top, loops.len());
    }
}

fn seconds(ms: u64) -> String {
    format!("{:.1}", ms as f64 / 1000.0)
}

fn hooks_table(r: &HooksReport, top: usize) {
    println!("Hooks and permission prompts");
    if !r.hooks.is_empty() {
        let rows = r
            .hooks
            .iter()
            .take(shown(top, r.hooks.len()))
            .map(|h| {
                vec![
                    h.source.id().to_owned(),
                    h.hook.clone(),
                    output::number(h.runs),
                    output::number(h.failures),
                    output::number(h.unknown),
                    seconds(h.total_ms),
                ]
            })
            .collect();
        println!(
            "{}",
            output::table(
                &[
                    "source",
                    "hook",
                    "runs",
                    "failures",
                    "no outcome",
                    "runtime (s)"
                ],
                rows,
                2
            )
        );
        more(top, r.hooks.len());
    }
    if !r.permissions.is_empty() {
        let rows = r
            .permissions
            .iter()
            .map(|p| {
                vec![
                    p.source.id().to_owned(),
                    output::number(p.prompts),
                    output::number(p.answered),
                    seconds(p.total_wait_ms),
                ]
            })
            .collect();
        println!(
            "{}",
            output::table(
                &["source", "permission prompts", "answered", "wait (s)"],
                rows,
                1
            )
        );
    }
    if !r.permissions.is_empty() {
        println!(
            "Permission wait is the time until the answer, including the time the user was away."
        );
    }
    for (source, n) in &r.unavailable {
        println!(
            "{}: not available, the client records neither hooks nor permission prompts ({n} sessions).",
            source.id()
        );
    }
}

pub fn run(
    registry: &Registry,
    env: &Env,
    filter: &SessionFilter,
    only: &[Source],
    roles: &[String],
    section: FlowSection,
    top: usize,
    display: output::Display,
) -> Result<()> {
    if display.format == Format::Csv && section == FlowSection::All {
        bail!("--format csv holds one table; choose it with --section growth|errors|hooks");
    }
    let run: FlowRun = flow_run(registry, env, filter, only, roles);
    let want = |s: FlowSection| section == FlowSection::All || section == s;

    match display.format {
        Format::Json => {
            let mut v = serde_json::Map::new();
            if want(FlowSection::Growth) {
                v.insert("growth".into(), json!(run.growth));
            }
            if want(FlowSection::Errors) {
                v.insert("errors".into(), json!(run.tool_errors));
            }
            if want(FlowSection::Hooks) {
                v.insert("hooks".into(), json!(run.hooks));
            }
            println!("{}", serde_json::to_string_pretty(&v)?);
        }
        Format::Csv => match section {
            FlowSection::Growth => print!("{}", export::csv::growth(&run.growth.sessions)?),
            FlowSection::Errors => print!("{}", export::csv::errors(&run.tool_errors.tools)?),
            _ => print!("{}", export::csv::hooks(&run.hooks.hooks)?),
        },
        Format::Table => {
            if want(FlowSection::Growth) {
                growth_table(&run.growth.sessions, top, display);
            }
            if want(FlowSection::All) {
                println!();
            }
            if want(FlowSection::Errors) {
                errors_table(&run.tool_errors.tools, &run.tool_errors.loops, top);
            }
            if want(FlowSection::All) {
                println!();
            }
            if want(FlowSection::Hooks) {
                hooks_table(&run.hooks, top);
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
