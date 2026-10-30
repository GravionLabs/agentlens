use agentlens_core::adapter::{Registry, SessionFilter};
use agentlens_core::analyzers::advice::{AdviceReport, Confidence, Finding, FindingKind};
use agentlens_core::env::Env;
use agentlens_core::export;
use agentlens_core::model::Source;
use agentlens_core::pipeline::advice_run;
use agentlens_core::prices;
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
        FindingKind::RetryLoops => format!("Retry loops ({})", f.count),
        FindingKind::ModelMix => format!("Model mix ({})", f.count),
    }
}

/// "Could save ~$1.23 (~45,678 tokens, estimated)." or `None` for a finding without a saving.
fn saving(f: &Finding, display: output::Display) -> Option<String> {
    let impact = &f.impact;
    if impact.saving.count == 0 {
        return None;
    }
    let confidence = match impact.confidence {
        Confidence::Measured => "measured",
        Confidence::Estimated => "estimated",
        Confidence::Inferred => "inferred",
    };
    let tokens = display.tokens(impact.saving);
    Some(match impact.saving_usd {
        // Always computed from the price table: an estimate.
        Some(usd) => format!("Could save ~${usd:.2} ({tokens} tokens, {confidence})."),
        None => format!("Could save {tokens} tokens ({confidence}; no price known)."),
    })
}

fn print(report: &AdviceReport, display: output::Display) {
    println!(
        "{} sessions, {} installed skills; their names and descriptions cost {} tokens in the period.",
        report.sessions,
        report.installed_skills,
        display.tokens(report.baseline_total)
    );
    if report.total_tokens.count > 0 {
        println!(
            "All requests of the period together carried {} tokens.",
            display.tokens(report.total_tokens)
        );
    }
    if report.findings.is_empty() {
        println!("\nNothing to suggest for these sessions.");
        return;
    }
    for f in &report.findings {
        println!("\n{}. {}", f.rank, title(f));
        if let Some(saving) = saving(f, display) {
            println!("{saving}");
        }
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
                display.tokens(total)
            );
        }
        let rows = f
            .items
            .iter()
            .map(|i| {
                vec![
                    i.name.clone(),
                    display.tokens(i.tokens),
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
    println!(
        "\nFindings are ranked by what they could save; ~ estimated, no ~ means measured; inferred savings rest on an assumption."
    );
}

pub fn run(
    registry: &Registry,
    env: &Env,
    filter: &SessionFilter,
    only: &[Source],
    estimator: &dyn TokenEstimator,
    display: output::Display,
) -> Result<()> {
    // Advice only ranks by price: a broken price file falls back to the bundled table (fail soft).
    let (prices, price_warning) = prices::load_or_bundled(env, None);
    if let Some(w) = &price_warning {
        eprintln!("warning: {w}");
    }
    let run = advice_run(registry, env, filter, only, estimator, &prices);
    match display.format {
        Format::Json => println!("{}", serde_json::to_string_pretty(&run.report)?),
        Format::Csv => print!("{}", export::csv::advice(&run.report)?),
        Format::Table => print(&run.report, display),
    }
    Ok(())
}
