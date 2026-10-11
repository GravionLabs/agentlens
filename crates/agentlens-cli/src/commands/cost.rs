use std::path::Path;

use agentlens_core::adapter::{Registry, SessionFilter};
use agentlens_core::analyzers::cost::{CostRow, Grouping, WhatIfReport};
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

pub fn run(
    registry: &Registry,
    env: &Env,
    filter: &SessionFilter,
    only: &[Source],
    params: Params,
    format: Format,
) -> Result<()> {
    let Params {
        price_file,
        by,
        what_if,
    } = params;
    let (group, label) = match by {
        CostBy::Model => (Grouping::Model, "model"),
        CostBy::Project => (Grouping::Project, "project"),
        CostBy::Branch => (Grouping::Branch, "branch"),
        CostBy::Issue => (Grouping::Issue, "issue"),
    };
    let table = prices::load(env, price_file).map_err(|e| anyhow!("cannot load prices: {e}"))?;
    if let Some(model) = what_if
        && table.lookup(model).is_none()
    {
        bail!("no price for the what-if model `{model}`; add it to a price file (--prices FILE)");
    }
    let run = cost_run(registry, env, filter, only, &table, group, what_if);
    let r = &run.report;

    match format {
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
        Format::Csv => print!("{}", export::csv::cost(&r.rows, label)?),
        Format::Table => {
            let total = r.total.total_usd();
            let mut rows: Vec<Vec<String>> = r
                .rows
                .iter()
                .map(|x| {
                    vec![
                        x.key.clone(),
                        output::number(x.requests),
                        output::number(x.input + x.cache_read + x.cache_write),
                        output::number(x.output),
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
                output::number(r.total.input + r.total.cache_read + r.total.cache_write),
                output::number(r.total.output),
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
