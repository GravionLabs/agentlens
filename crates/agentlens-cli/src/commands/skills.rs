use agentlens_core::adapter::{Registry, SessionFilter};
use agentlens_core::analyzers::skills::SkillStats;
use agentlens_core::env::Env;
use agentlens_core::export;
use agentlens_core::model::Source;
use agentlens_core::pipeline::skill_run;
use agentlens_core::tokens::{TokenEstimator, Tokens};
use anyhow::Result;

use crate::args::Format;
use crate::output;

fn attributed_tokens(s: &SkillStats) -> u64 {
    let a = &s.attributed;
    a.input + a.output + a.cache_read + a.cache_write
}

pub fn run(
    registry: &Registry,
    env: &Env,
    filter: &SessionFilter,
    only: &[Source],
    estimator: &dyn TokenEstimator,
    display: output::Display,
) -> Result<()> {
    let run = skill_run(registry, env, filter, only, estimator);
    let mut skills: Vec<&SkillStats> = run.report.skills.iter().collect();
    skills.sort_by(|a, b| {
        b.overhead_tokens()
            .cmp(&a.overhead_tokens())
            .then_with(|| a.name.cmp(&b.name))
    });

    match display.format {
        Format::Json => println!("{}", serde_json::to_string_pretty(&run.report)?),
        Format::Csv => print!("{}", export::csv::skills(&skills)?),
        Format::Table => {
            // The measured column only appears where a client attributes usage to skills.
            let with_attribution = skills.iter().any(|s| s.attributed.requests > 0);
            // The file column only appears where a skill was used without being loaded.
            let with_files = skills.iter().any(|s| s.file_uses > 0);
            let rows = skills
                .iter()
                .map(|s| {
                    let mut row = vec![
                        s.name.clone(),
                        s.invocations.to_string(),
                        s.sessions.to_string(),
                        display.tokens(s.load),
                        display.tokens(s.carry.tokens),
                        display.tokens(s.baseline.tokens),
                        display.tokens(Tokens::estimated(s.overhead_tokens())),
                    ];
                    if with_files {
                        row.insert(2, s.file_uses.to_string());
                    }
                    if with_attribution {
                        row.push(if s.attributed.requests > 0 {
                            display.tokens(Tokens::measured(attributed_tokens(s)))
                        } else {
                            "-".into()
                        });
                    }
                    row
                })
                .collect();
            let mut header = vec![
                "skill", "uses", "sessions", "load", "carry", "baseline", "overhead",
            ];
            if with_files {
                header.insert(2, "file uses");
            }
            if with_attribution {
                header.push("attributed");
            }
            println!("{}", output::table(&header, rows, 1));
            println!("~ estimated; no ~ means measured. overhead = baseline + load + carry.");
            if with_files {
                println!(
                    "file uses = tool calls that read or ran files in the skill's folder without loading it; such a skill is in use."
                );
            }
            if with_attribution {
                println!(
                    "attributed = all tokens of the requests the client (OpenTelemetry `skill.name`) attributes to the skill; measured."
                );
            }
            println!(
                "{} sessions, {} skills installed.",
                run.report.sessions,
                run.inventory.skills.len()
            );
            if !run.report.ignored_commands.is_empty() {
                let n: u32 = run.report.ignored_commands.iter().map(|c| c.count).sum();
                println!("{n} typed commands are no installed skill and were ignored.");
            }
            for d in &run.inventory.duplicates {
                println!(
                    "duplicate skill `{}` in {} files{}",
                    d.name,
                    d.files.len(),
                    if d.identical { " (identical)" } else { "" }
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
