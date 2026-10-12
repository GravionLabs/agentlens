use agentlens_core::adapter::{Registry, SessionFilter};
use agentlens_core::analyzers::advice::{AdviceReport, Finding, FindingKind};
use agentlens_core::env::Env;
use agentlens_core::export;
use agentlens_core::model::Source;
use agentlens_core::pipeline::advice_run;
use agentlens_core::tokens::TokenEstimator;
use anyhow::Result;

use crate::args::Format;
use crate::output;

fn title(f: &Finding) -> String {
    match f.kind {
        FindingKind::UnusedSkills => format!("Unused skills ({})", f.count),
        FindingKind::ListedButLoadedByHand => format!("Listed but loaded by hand ({})", f.count),
        FindingKind::CostlyDescriptions => format!("Costly descriptions ({})", f.count),
        FindingKind::UsedButNotInstalled => format!("Used but not installed ({})", f.count),
        FindingKind::LargeBodies => format!("Large skill bodies ({})", f.count),
        FindingKind::LargeContext => format!("Work on a large context ({})", f.count),
        FindingKind::CostlyMcpServers => format!("Costly MCP servers ({})", f.count),
        FindingKind::ContextSize => format!("Context size ({})", f.count),
    }
}

fn print(report: &AdviceReport) {
    println!(
        "{} sessions, {} installed skills; their names and descriptions cost {} tokens in the period.",
        report.sessions,
        report.installed_skills,
        output::tokens(report.baseline_total)
    );
    if report.total_tokens.count > 0 {
        println!(
            "All requests of the period together carried {} tokens.",
            output::tokens(report.total_tokens)
        );
    }
    if report.findings.is_empty() {
        println!("\nNothing to suggest for these sessions.");
        return;
    }
    for f in &report.findings {
        println!("\n{}", title(f));
        println!("{}", f.explanation);
        if let Some(total) = f.total {
            let baseline = f
                .share_of_baseline
                .map(|s| format!(", {:.0}% of the baseline", s * 100.0))
                .unwrap_or_default();
            let overall = f
                .share_of_total_tokens
                .map(|s| {
                    let tilde = if total.measured { "" } else { "~" };
                    format!(", {tilde}{:.2}% of all tokens", s * 100.0)
                })
                .unwrap_or_default();
            let share = format!("{baseline}{overall}");
            let what = if f.kind == FindingKind::CostlyDescriptions {
                "could save"
            } else {
                "cost"
            };
            println!(
                "Together they {what} {} tokens{share}.",
                output::tokens(total)
            );
        }
        let rows = f
            .items
            .iter()
            .map(|i| {
                vec![
                    i.name.clone(),
                    output::tokens(i.tokens),
                    output::number(i.requests),
                    i.detail.clone(),
                ]
            })
            .collect();
        println!(
            "{}",
            output::table(&["skill", "tokens", "requests", "detail"], rows, 1)
        );
        if (f.items.len() as u32) < f.count {
            println!(
                "{} more; --format json lists the top ones only.",
                f.count as usize - f.items.len()
            );
        }
        println!("What to do: {}", f.action);
        for note in &f.notes {
            println!("Note: {note}");
        }
    }
    println!("\n~ estimated; no ~ means measured.");
}

pub fn run(
    registry: &Registry,
    env: &Env,
    filter: &SessionFilter,
    only: &[Source],
    estimator: &dyn TokenEstimator,
    format: Format,
) -> Result<()> {
    let run = advice_run(registry, env, filter, only, estimator);
    match format {
        Format::Json => println!("{}", serde_json::to_string_pretty(&run.report)?),
        Format::Csv => print!("{}", export::csv::advice(&run.report)?),
        Format::Table => print(&run.report),
    }
    Ok(())
}
