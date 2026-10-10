use std::collections::BTreeSet;
use std::path::Path;

use agentlens_core::adapter::{Adapter, SessionFilter};
use agentlens_core::adapters::otel::OtelAdapter;
use agentlens_core::adapters::registry_with_otel;
use agentlens_core::env::Env;
use agentlens_core::model::{EventKind, Source};
use anyhow::{Result, bail};
use serde_json::json;

use crate::args::Format;
use crate::output;

pub fn run(env: &Env, file: &Path, format: Format) -> Result<()> {
    if !file.is_file() {
        bail!("{} is not a file", file.display());
    }
    let adapter = OtelAdapter::new(vec![file.to_path_buf()]);
    let (mut sessions, mut events, mut requests, mut skills, mut tools) =
        (0u64, 0u64, 0u64, 0u64, 0u64);
    let mut ids = BTreeSet::new();
    let mut warnings = Vec::new();
    for r in adapter.sessions(env, &SessionFilter::default()) {
        let se = r?;
        warnings.extend(se.warnings.iter().map(|w| w.message.clone()));
        if se.events.is_empty() {
            continue;
        }
        sessions += 1;
        events += se.events.len() as u64;
        for e in &se.events {
            match e.kind {
                EventKind::Usage(_) => requests += 1,
                EventKind::SkillInvoked(_) => skills += 1,
                EventKind::ToolCall(_) => tools += 1,
                _ => {}
            }
        }
        ids.insert(se.session.id.clone());
    }
    // Sessions that the local Claude Code data holds as well are not counted twice.
    let local = registry_with_otel(Vec::new());
    let also_local = local
        .sessions(env, &SessionFilter::default(), &[Source::ClaudeCode])
        .filter_map(Result::ok)
        .filter(|s| ids.contains(&s.session.id))
        .count();

    match format {
        Format::Json => println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "file": file,
                "sessions": sessions,
                "events": events,
                "requests": requests,
                "skill_invocations": skills,
                "tool_calls": tools,
                "also_in_local_data": also_local,
                "warnings": warnings,
            }))?
        ),
        Format::Csv => {
            let header = [
                "file",
                "sessions",
                "events",
                "requests",
                "skill_invocations",
                "tool_calls",
                "also_in_local_data",
            ];
            let row = vec![
                file.display().to_string(),
                sessions.to_string(),
                events.to_string(),
                requests.to_string(),
                skills.to_string(),
                tools.to_string(),
                also_local.to_string(),
            ];
            print!("{}", output::csv(&header, vec![row])?);
        }
        Format::Table => {
            let row = vec![
                file.display().to_string(),
                sessions.to_string(),
                events.to_string(),
                requests.to_string(),
                skills.to_string(),
                tools.to_string(),
                also_local.to_string(),
            ];
            println!(
                "{}",
                output::table(
                    &[
                        "file",
                        "sessions",
                        "events",
                        "requests",
                        "skills",
                        "tool calls",
                        "also in local data"
                    ],
                    vec![row],
                    1
                )
            );
            if also_local > 0 {
                println!(
                    "{also_local} of these sessions are also in the local Claude Code data; analyses count them once; the export's requests replace the local usage in the range the export covers."
                );
            }
            for w in &warnings {
                println!("{w}");
            }
        }
    }
    Ok(())
}
