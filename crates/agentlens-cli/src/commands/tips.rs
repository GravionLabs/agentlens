use agentlens_core::analyzers::advice::Confidence;
use agentlens_core::analyzers::tips::{
    Check, CheckUnit, TargetStatus, TipCheck, TipsReport, Verdict,
};
use agentlens_core::api::{self, Context, Filters};
use agentlens_core::export;
use anyhow::{Result, anyhow};

use crate::args::Format;
use crate::output;

fn confidence(c: Confidence) -> &'static str {
    match c {
        Confidence::Measured => "measured",
        Confidence::Estimated => "estimated",
        Confidence::Inferred => "inferred",
    }
}

fn print_table(report: &TipsReport) {
    if report.tips.is_empty() {
        println!("No tip for these sessions.");
    }
    for t in &report.tips {
        println!("{}. {} [{}]", t.rank, t.headline, confidence(t.confidence));
        for target in &t.targets {
            println!("     - {}: {}", target.name, target.detail);
            for step in &target.steps {
                let file = step
                    .file
                    .as_deref()
                    .map(|f| format!("{f}: "))
                    .unwrap_or_default();
                println!("         step: {file}{}", step.change);
            }
        }
        if t.more > 0 {
            println!("     ... and {} more", t.more);
        }
        println!("   What to do: {}", t.action);
        println!();
    }
    let threshold = report.threshold * 100.0;
    if report.below_threshold > 0 {
        println!(
            "{} more findings save less than {threshold:.1} % of the period's cost; `agentlens advise` lists them.",
            report.below_threshold
        );
    }
    println!(
        "Tips are the findings of `agentlens advise` that save at least {threshold:.1} % of the period's cost; ~ estimated."
    );
}

fn verdict(v: Verdict) -> &'static str {
    match v {
        Verdict::Improved => "improved",
        Verdict::Unchanged => "unchanged",
        Verdict::Worse => "worse",
        Verdict::TooLittleData => "too little data",
    }
}

fn status(s: TargetStatus) -> &'static str {
    match s {
        TargetStatus::Resolved => "resolved",
        TargetStatus::Open => "open",
        TargetStatus::Unknown => "unknown",
    }
}

/// The value with its unit: `~$1.20` per 1,000 requests, or tokens where a window has no price.
fn per_1k(value: Option<f64>, usd: bool) -> String {
    match (value, usd) {
        (None, _) => "n/a".into(),
        (Some(v), true) => format!("~${v:.2}"),
        (Some(v), false) => format!("~{v:.0} tokens"),
    }
}

fn print_check(c: &Check) {
    let usd = c.unit == CheckUnit::Usd;
    let change = c
        .change_pct
        .map(|p| format!("{p:+.0} %"))
        .unwrap_or_else(|| "n/a".into());
    println!("{}. {}", c.tip.rank, c.tip.headline);
    println!(
        "   per 1,000 requests: {} -> {} ({change}): {}",
        per_1k(c.before_per_1k, usd),
        per_1k(c.after_per_1k, usd),
        verdict(c.verdict)
    );
    for t in &c.targets {
        println!("     - {}: {}", t.name, status(t.status));
    }
    let summary = c.resolved_summary();
    if !summary.is_empty() {
        println!("   Targets: {summary}");
    }
    println!();
}

fn print_check_table(report: &TipCheck) {
    let w = &report.windows;
    println!(
        "Before: {} .. {} ({} sessions). After: {} .. {} ({} sessions).\n",
        w.against.since,
        w.against.until,
        w.against.sessions,
        w.current.since,
        w.current.until,
        w.current.sessions
    );
    if report.checks.is_empty() {
        println!("No tip in the window before the date.");
    }
    for c in &report.checks {
        print_check(c);
    }
    if !report.new.is_empty() {
        println!("New since the date (no verdict):");
        for t in &report.new {
            println!("  {}. {}", t.rank, t.headline);
        }
        println!();
    }
    println!(
        "Verdict: the saving per 1,000 requests dropped (improved) or rose (worse) by at least 20 %; a window with fewer than 50 requests gives too little data. ~ estimated."
    );
}

pub fn run(
    ctx: &Context,
    filters: &Filters,
    limit: usize,
    changed_at: Option<&str>,
    display: output::Display,
) -> Result<()> {
    let (warnings, errors) = match changed_at {
        Some(changed_at) => {
            let answer =
                api::tips_check(ctx, filters, changed_at, limit).map_err(|e| anyhow!(e))?;
            match display.format {
                Format::Json => println!("{}", serde_json::to_string_pretty(&answer.data)?),
                Format::Csv => print!("{}", export::csv::tips_check(&answer.data)?),
                Format::Table => print_check_table(&answer.data),
            }
            (answer.warnings, answer.errors)
        }
        None => {
            let answer = api::tips(ctx, filters, limit).map_err(|e| anyhow!(e))?;
            match display.format {
                Format::Json => println!("{}", serde_json::to_string_pretty(&answer.data)?),
                Format::Csv => print!("{}", export::csv::tips(&answer.data)?),
                Format::Table => print_table(&answer.data),
            }
            (answer.warnings, answer.errors)
        }
    };
    if warnings > 0 {
        eprintln!("{warnings} warnings while reading sessions (run `agentlens scan` for details)");
    }
    for e in &errors {
        eprintln!("warning: {e}");
    }
    Ok(())
}
