use std::path::Path;

use agentlens_core::adapter::{Registry, SessionFilter};
use agentlens_core::analyzers::cost::CostRow;
use agentlens_core::env::Env;
use agentlens_core::model::Source;
use agentlens_core::pipeline::cost_run;
use agentlens_core::prices;
use anyhow::{Result, anyhow};

use crate::output;

use crate::args::Format;

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

pub fn run(
    registry: &Registry,
    env: &Env,
    filter: &SessionFilter,
    only: &[Source],
    price_file: Option<&Path>,
    format: Format,
) -> Result<()> {
    let table = prices::load(env, price_file).map_err(|e| anyhow!("cannot load prices: {e}"))?;
    let run = cost_run(registry, env, filter, only, &table);
    let r = &run.report;

    match format {
        Format::Json => println!("{}", serde_json::to_string_pretty(r)?),
        Format::Csv => {
            let header = [
                "model",
                "requests",
                "input_tokens",
                "output_tokens",
                "cache_read_tokens",
                "cache_write_tokens",
                "measured_usd",
                "estimated_usd",
                "unpriced_requests",
                "premium_requests",
            ];
            let rows = r
                .rows
                .iter()
                .map(|x| {
                    vec![
                        x.key.clone(),
                        x.requests.to_string(),
                        x.input.to_string(),
                        x.output.to_string(),
                        x.cache_read.to_string(),
                        x.cache_write.to_string(),
                        format!("{:.6}", x.measured_usd),
                        format!("{:.6}", x.estimated_usd),
                        x.unpriced_requests.to_string(),
                        x.premium_requests.to_string(),
                    ]
                })
                .collect();
            print!("{}", output::csv(&header, rows)?);
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
                        "model",
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
