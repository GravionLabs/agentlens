use agentlens_core::analyzers::trend::{Delta, FigureRow, TrendReport, Unit};
use agentlens_core::api::{self, Context, Filters};
use agentlens_core::export;
use anyhow::{Result, anyhow};
use jiff::Timestamp;

use crate::args::Format;
use crate::output;

const NA: &str = "n/a";

fn value(unit: Unit, v: f64, per_day: bool) -> String {
    match unit {
        Unit::Rate => format!("{:.1}%", v * 100.0),
        Unit::Usd => format!("${v:.2}"),
        _ if per_day => format!("{v:.1}"),
        _ => output::number(v.round() as u64),
    }
}

fn signed(unit: Unit, v: f64, per_day: bool) -> String {
    let sign = if v < 0.0 { "-" } else { "+" };
    match unit {
        // A difference of two shares is in percentage points.
        Unit::Rate => format!("{sign}{:.1} pp", v.abs() * 100.0),
        _ => format!("{sign}{}", value(unit, v.abs(), per_day)),
    }
}

/// Estimated values carry the tilde; a missing one is `n/a`.
fn cell(d: &Delta, v: Option<f64>, f: impl Fn(f64) -> String) -> String {
    match v {
        Some(v) if d.measured => f(v),
        Some(v) => format!("~{}", f(v)),
        None => NA.into(),
    }
}

fn row(r: &FigureRow<'_>, per_day: bool) -> Vec<String> {
    let d = r.delta;
    let name = match r.skill {
        Some(s) => format!("{} {} ({})", s.name, r.figure, status(s)),
        None => r.figure.to_owned(),
    };
    let pct = match d.change_pct {
        Some(p) if d.measured => format!("{p:+.1}%"),
        Some(p) => format!("~{p:+.1}%"),
        None => NA.into(),
    };
    vec![
        r.group.to_owned(),
        name,
        cell(d, d.before, |v| value(r.unit, v, per_day)),
        cell(d, d.after, |v| value(r.unit, v, per_day)),
        cell(d, d.change, |v| signed(r.unit, v, per_day)),
        pct,
    ]
}

fn status(s: &agentlens_core::analyzers::trend::SkillTrend) -> String {
    serde_json::to_value(s.status)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

fn print_table(report: &TrendReport) {
    let w = &report.windows;
    let day = |t: Timestamp| t.to_string()[..10].to_owned();
    println!(
        "period {}..{} ({} sessions) against {}..{} ({} sessions)",
        day(w.current.since),
        day(w.current.until),
        w.current.sessions,
        day(w.against.since),
        day(w.against.until),
        w.against.sessions,
    );
    let rows = report
        .figures()
        .iter()
        .map(|r| row(r, w.normalized))
        .collect();
    println!(
        "{}",
        output::table(
            &["group", "figure", "before", "after", "change", "change %"],
            rows,
            2
        )
    );
    if w.normalized {
        println!(
            "The windows differ in length: counts, tokens and amounts are per day (normalized); rates are not."
        );
    }
    println!(
        "n/a: no sessions in one of the windows, or the earlier value is 0. `~` is estimated; everything else comes from client data."
    );
}

pub fn run(
    ctx: &Context,
    filters: &Filters,
    against: Option<&str>,
    display: output::Display,
) -> Result<()> {
    let against = against
        .map(|a| api::parse_against(a, "--against"))
        .transpose()
        .map_err(|e| anyhow!(e))?;
    let answer = api::trend(ctx, filters, against).map_err(|e| anyhow!(e))?;
    match display.format {
        Format::Json => println!("{}", serde_json::to_string_pretty(&answer.data)?),
        Format::Csv => print!("{}", export::csv::trend(&answer.data)?),
        Format::Table => print_table(&answer.data),
    }
    if answer.warnings > 0 {
        eprintln!(
            "{} warnings while reading sessions (run `agentlens scan` for details)",
            answer.warnings
        );
    }
    for e in &answer.errors {
        eprintln!("error: {e}");
    }
    Ok(())
}
