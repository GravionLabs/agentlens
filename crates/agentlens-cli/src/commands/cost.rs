use std::path::Path;

use agentlens_core::adapter::{Registry, SessionFilter};
use agentlens_core::analyzers::cost::{CostRow, Grouping, SessionCost, WhatIfReport};
use agentlens_core::env::Env;
use agentlens_core::export;
use agentlens_core::model::Source;
use agentlens_core::pipeline::cost_run;
use agentlens_core::prices;
use anyhow::{Result, anyhow, bail};

use crate::output;

use crate::args::{CostBy, Format};

fn usd(v: f64) -> String {
    format!("${v:.2}")
}

/// `$1.50`, `~$1.50` for computed values, `$1.50 + ~$2.00` for a mix.
fn money(r: &CostRow) -> String {
    match (r.measured_usd > 0.0, r.estimated_usd > 0.0) {
        (true, true) => format!("{} + ~{}", usd(r.measured_usd), usd(r.estimated_usd)),
        (false, true) => format!("~{}", usd(r.estimated_usd)),
        (true, false) => usd(r.measured_usd),
        (false, false) if r.unpriced_requests == r.requests && r.requests > 0 => "unpriced".into(),
        (false, false) => usd(0.0),
    }
}

fn share(r: &CostRow, total: f64) -> String {
    if total <= 0.0 {
        return "-".into();
    }
    format!("{:.0}%", r.total_usd() * 100.0 / total)
}

pub struct Params<'a> {
    pub price_file: Option<&'a Path>,
    pub by: CostBy,
    pub what_if: Option<&'a str>,
    pub top: usize,
}

fn what_if_table(w: &WhatIfReport) -> Vec<Vec<String>> {
    w.rows
        .iter()
        .chain(std::iter::once(&w.total))
        .map(|x| {
            let diff = x.what_if_usd - x.actual_usd;
            vec![
                x.model.clone(),
                output::number(x.requests),
                format!("~{}", usd(x.actual_usd)),
                format!("~{}", usd(x.what_if_usd)),
                format!("{}{}", if diff < 0.0 { "-" } else { "+" }, usd(diff.abs())),
                if x.actual_usd > 0.0 {
                    format!("{:+.0}%", diff * 100.0 / x.actual_usd)
                } else {
                    "-".into()
                },
                output::number(x.excluded_requests),
            ]
        })
        .collect()
}

const WHAT_IF_HEADER: [&str; 7] = [
    "model",
    "requests",
    "actual",
    "what-if",
    "difference",
    "change",
    "excluded",
];

fn session_money(s: &SessionCost) -> String {
    if !s.cost_known() {
        return if s.requests == 0 { "n/a" } else { "unpriced" }.into();
    }
    match (s.measured_usd > 0.0, s.estimated_usd > 0.0) {
        (true, true) => format!("{} + ~{}", usd(s.measured_usd), usd(s.estimated_usd)),
        (false, true) => format!("~{}", usd(s.estimated_usd)),
        (true, false) => usd(s.measured_usd),
        (false, false) => usd(0.0),
    }
}

fn duration(ms: Option<u64>) -> String {
    let Some(ms) = ms else {
        return "n/a".into();
    };
    let minutes = ms / 60_000;
    if minutes >= 60 {
        format!("{}h{:02}m", minutes / 60, minutes % 60)
    } else {
        format!("{minutes}m")
    }
}

fn session_table(
    r: &agentlens_core::analyzers::cost::CostReport,
    top: usize,
    display: output::Display,
) {
    let shown = if top == 0 {
        r.sessions.len()
    } else {
        top.min(r.sessions.len())
    };
    let rows: Vec<Vec<String>> = r
        .sessions
        .iter()
        .take(shown)
        .map(|s| {
            vec![
                s.session.clone(),
                s.source.id().to_owned(),
                s.project
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "n/a".into()),
                s.started_at
                    .map(|t| t.strftime("%Y-%m-%d %H:%M").to_string())
                    .unwrap_or_else(|| "n/a".into()),
                s.issue.clone().unwrap_or_default(),
                s.models.join(", "),
                duration(s.duration_ms),
                output::number(s.requests),
                output::number(s.main_requests),
                display.count(s.peak_context),
                display.count(s.input),
                display.count(s.cache_read),
                display.count(s.cache_write),
                display.count(s.output),
                session_money(s),
                s.subagent_share()
                    .map(|v| format!("{:.0}%", v * 100.0))
                    .unwrap_or_else(|| "n/a".into()),
            ]
        })
        .collect();
    println!(
        "{}",
        output::table(
            &[
                "session",
                "source",
                "project",
                "started",
                "issue",
                "models",
                "duration",
                "requests",
                "main",
                "peak context",
                "input",
                "cache read",
                "cache write",
                "output",
                "cost",
                "subagent share",
            ],
            rows,
            6
        )
    );
    if shown < r.sessions.len() {
        println!(
            "{} more rows; use --top 0, --format json or csv to see them all.",
            r.sessions.len() - shown
        );
    }
    println!(
        "Total {} over {} sessions. Cost without `~` was reported by the client; `~` was computed from the price table (ADR 0002). `n/a` means the cost is unknown, not zero.",
        money(&r.total),
        r.sessions.len()
    );
    if !r.unpriced_models.is_empty() {
        let list: Vec<String> = r
            .unpriced_models
            .iter()
            .map(|(m, n)| format!("{m} ({n})"))
            .collect();
        println!("No price for: {}.", list.join(", "));
    }
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
        what_if,
        top,
    } = params;
    let (group, label) = match by {
        CostBy::Model => (Grouping::Model, "model"),
        CostBy::Project => (Grouping::Project, "project"),
        CostBy::Branch => (Grouping::Branch, "branch"),
        CostBy::Issue => (Grouping::Issue, "issue"),
        CostBy::Session => (Grouping::Session, "session"),
    };
    let table = prices::load(env, price_file).map_err(|e| anyhow!("cannot load prices: {e}"))?;
    if let Some(model) = what_if
        && table.lookup(model).is_none()
    {
        bail!("no price for the what-if model `{model}`; add it to a price file (--prices FILE)");
    }
    let run = cost_run(registry, env, filter, only, roles, &table, group, what_if);
    let r = &run.report;

    match display.format {
        Format::Json => {
            let mut v = serde_json::to_value(r)?;
            if let (Some(w), Some(obj)) = (&run.what_if, v.as_object_mut()) {
                obj.insert("what_if".into(), serde_json::to_value(w)?);
            }
            println!("{}", serde_json::to_string_pretty(&v)?);
        }
        Format::Csv if run.what_if.is_some() => {
            let w = run.what_if.as_ref().expect("checked");
            print!("{}", export::csv::what_if(w)?);
        }
        Format::Csv if group == Grouping::Session => {
            print!("{}", export::csv::session_cost(&r.sessions)?)
        }
        Format::Csv => print!("{}", export::csv::cost(&r.rows, label)?),
        Format::Table if group == Grouping::Session => {
            session_table(r, top, display);
            if let Some(w) = &run.what_if {
                println!("\nWhat if everything ran on {}?", w.target);
                println!("{}", output::table(&WHAT_IF_HEADER, what_if_table(w), 1));
            }
        }
        Format::Table => {
            let total = r.total.total_usd();
            let mut rows: Vec<Vec<String>> = r
                .rows
                .iter()
                .map(|x| {
                    vec![
                        x.key.clone(),
                        output::number(x.requests),
                        display.count(x.input + x.cache_read + x.cache_write),
                        display.count(x.output),
                        money(x),
                        share(x, total),
                        if x.premium_requests > 0.0 {
                            format!("{:.1}", x.premium_requests)
                        } else {
                            String::new()
                        },
                    ]
                })
                .collect();
            rows.push(vec![
                "total".into(),
                output::number(r.total.requests),
                display.count(r.total.input + r.total.cache_read + r.total.cache_write),
                display.count(r.total.output),
                money(&r.total),
                String::new(),
                if r.total.premium_requests > 0.0 {
                    format!("{:.1}", r.total.premium_requests)
                } else {
                    String::new()
                },
            ]);
            println!(
                "{}",
                output::table(
                    &[
                        label,
                        "requests",
                        "input",
                        "output",
                        "cost",
                        "share",
                        "premium requests"
                    ],
                    rows,
                    1
                )
            );
            println!(
                "Cost without `~` was reported by the client; `~` was computed from the price table (ADR 0002). Cache writes are priced as 5-minute cache writes."
            );
            if !r.unpriced_models.is_empty() {
                let list: Vec<String> = r
                    .unpriced_models
                    .iter()
                    .map(|(m, n)| format!("{m} ({n})"))
                    .collect();
                println!(
                    "No price for: {}. Add them to {} or pass --prices FILE.",
                    list.join(", "),
                    prices::user_file(env).display()
                );
            }
            if let Some(w) = &run.what_if {
                println!("\nWhat if everything ran on {}?", w.target);
                println!("{}", output::table(&WHAT_IF_HEADER, what_if_table(w), 1));
                println!(
                    "Same tokens, priced with the prices of {}. `excluded` requests have no actual price and are left out of both sides.",
                    w.target
                );
            }
            if r.total.premium_requests > 0.0 {
                println!(
                    "Premium requests are a plan allowance, not money, and are not part of the cost."
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
