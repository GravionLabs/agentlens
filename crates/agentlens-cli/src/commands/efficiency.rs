use std::path::Path;

use agentlens_core::adapter::{Registry, SessionFilter};
use agentlens_core::analyzers::efficiency::{CacheBy, CacheRow, RoleRow, SubagentShare};
use agentlens_core::env::Env;
use agentlens_core::export;
use agentlens_core::model::Source;
use agentlens_core::pipeline::efficiency_run;
use agentlens_core::prices;
use anyhow::{Result, anyhow, bail};
use serde_json::json;

use crate::args::{EfficiencyBy, EfficiencySection, Format};
use crate::output;

fn percent(part: f64, whole: f64) -> String {
    if whole <= 0.0 {
        return "-".into();
    }
    format!("{:.1}%", part * 100.0 / whole)
}

fn hit_rate(r: &CacheRow) -> String {
    r.hit_rate
        .map(|h| format!("{:.1}%", h * 100.0))
        .unwrap_or_else(|| "n/a".into())
}

fn share_row(s: &SubagentShare) -> Vec<String> {
    vec![
        s.source.id().to_owned(),
        output::number(s.requests),
        percent(s.subagent_requests as f64, s.requests as f64),
        percent(s.subagent_tokens as f64, s.tokens as f64),
        if s.usd > 0.0 {
            percent(s.subagent_usd, s.usd)
        } else {
            "-".into()
        },
        output::number(s.unpriced_requests),
    ]
}

fn role_row(r: &RoleRow, display: output::Display) -> Vec<String> {
    let usd = |v: Option<f64>| match v {
        Some(v) => format!("{}${v:.4}", if r.cost_measured { "" } else { "~" }),
        None => "n/a".into(),
    };
    vec![
        r.role.clone(),
        serde_json::to_value(r.kind)
            .ok()
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_default(),
        output::number(r.runs),
        format!("{:.1}", r.requests_per_run),
        display.count(r.input),
        display.count(r.cache_read),
        display.count(r.cache_write),
        display.count(r.output),
        usd(r.usd_per_run),
        r.models.join(", "),
        display.tokens(r.first_request_context),
        display.tokens(r.uncached_after_first),
        r.hit_rate
            .map(|h| format!("{:.1}%", h * 100.0))
            .unwrap_or_else(|| "n/a".into()),
    ]
}

pub struct Params<'a> {
    pub price_file: Option<&'a Path>,
    pub by: EfficiencyBy,
    pub top: usize,
    pub section: EfficiencySection,
}

pub fn run(
    registry: &Registry,
    env: &Env,
    filter: &SessionFilter,
    only: &[Source],
    roles: &[String],
    params: Params,
    display: output::Display,
) -> Result<()> {
    let Params {
        price_file,
        by,
        top,
        section,
    } = params;
    if display.format == Format::Csv && section == EfficiencySection::All {
        bail!("--format csv holds one table; choose it with --section cache|roles");
    }
    let want = |s: EfficiencySection| section == EfficiencySection::All || section == s;
    let table = prices::load(env, price_file).map_err(|e| anyhow!("cannot load prices: {e}"))?;
    let cache_by = match by {
        EfficiencyBy::Session => CacheBy::Session,
        EfficiencyBy::Project => CacheBy::Project,
        EfficiencyBy::Source => CacheBy::Source,
    };
    let run = efficiency_run(registry, env, filter, only, roles, &table, cache_by);
    let r = &run.report;

    match display.format {
        Format::Json => println!("{}", serde_json::to_string_pretty(&json!(r))?),
        Format::Csv => match section {
            EfficiencySection::Roles => print!("{}", export::csv::roles(&r.roles)?),
            _ => print!("{}", export::csv::cache(&r.cache)?),
        },
        Format::Table => {
            if want(EfficiencySection::Cache) {
                let shown = if top == 0 {
                    r.cache.len()
                } else {
                    top.min(r.cache.len())
                };
                let rows = r
                    .cache
                    .iter()
                    .take(shown)
                    .map(|c| {
                        vec![
                            c.key.clone(),
                            c.source.id().to_owned(),
                            output::number(c.requests),
                            display.count(c.input),
                            display.count(c.cache_read),
                            display.count(c.cache_write),
                            hit_rate(c),
                        ]
                    })
                    .collect();
                println!("Cache hit rate");
                println!(
                    "{}",
                    output::table(
                        &[
                            "group",
                            "source",
                            "requests",
                            "input",
                            "cache read",
                            "cache write",
                            "hit rate"
                        ],
                        rows,
                        2
                    )
                );
                println!(
                    "Hit rate = cache read / all input tokens (measured). `n/a`: the client reports no cache tokens."
                );
                if shown < r.cache.len() {
                    println!(
                        "{} more rows; use --top 0, --format json or csv to see them all.",
                        r.cache.len() - shown
                    );
                }
                let rows = r.subagents.iter().map(share_row).collect();
                println!("\nSubagent share");
                println!(
                    "{}",
                    output::table(
                        &[
                            "source",
                            "requests",
                            "subagent requests",
                            "subagent tokens",
                            "subagent cost",
                            "unpriced requests"
                        ],
                        rows,
                        1
                    )
                );
                println!(
                    "Cost shares only cover requests with a price (reported or from the price table)."
                );
            }
            if want(EfficiencySection::All) {
                println!();
            }
            if want(EfficiencySection::Roles) {
                let rows = r.roles.iter().map(|x| role_row(x, display)).collect();
                println!("Roles");
                println!(
                    "{}",
                    output::table(
                        &[
                            "role",
                            "kind",
                            "runs",
                            "requests/run",
                            "input",
                            "cache read",
                            "cache write",
                            "output",
                            "cost/run",
                            "models",
                            "first request context",
                            "uncached after first",
                            "hit rate"
                        ],
                        rows,
                        2
                    )
                );
                println!(
                    "Role = forked skill name, else workflow phase, else agent type; `main` is the main agent. Context columns are means per run; `~` marks estimates."
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
