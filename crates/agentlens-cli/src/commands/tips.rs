use agentlens_core::analyzers::advice::Confidence;
use agentlens_core::analyzers::tips::TipsReport;
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

pub fn run(ctx: &Context, filters: &Filters, limit: usize, display: output::Display) -> Result<()> {
    let answer = api::tips(ctx, filters, limit).map_err(|e| anyhow!(e))?;
    match display.format {
        Format::Json => println!("{}", serde_json::to_string_pretty(&answer.data)?),
        Format::Csv => print!("{}", export::csv::tips(&answer.data)?),
        Format::Table => print_table(&answer.data),
    }
    if answer.warnings > 0 {
        eprintln!(
            "{} warnings while reading sessions (run `agentlens scan` for details)",
            answer.warnings
        );
    }
    for e in &answer.errors {
        eprintln!("warning: {e}");
    }
    Ok(())
}
