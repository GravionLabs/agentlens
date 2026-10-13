use agentlens_core::adapter::{Registry, SessionFilter};
use agentlens_core::env::Env;
use agentlens_core::model::Source;
use agentlens_core::pipeline::scan_sources;
use anyhow::Result;
use serde_json::json;

use crate::args::Format;
use crate::output;

pub fn run(
    registry: &Registry,
    env: &Env,
    filter: &SessionFilter,
    only: &[Source],
    display: output::Display,
) -> Result<()> {
    let scans = scan_sources(registry, env, filter, only);
    let header = [
        "source", "found", "sessions", "events", "warnings", "errors", "roots", "note",
    ];
    let rows: Vec<Vec<String>> = scans
        .iter()
        .map(|s| {
            vec![
                s.source.id().to_owned(),
                if s.detection.found { "yes" } else { "no" }.to_owned(),
                s.sessions.to_string(),
                s.events.to_string(),
                s.warnings.to_string(),
                s.errors.len().to_string(),
                s.detection
                    .roots
                    .iter()
                    .map(|r| r.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", "),
                s.detection.note.clone().unwrap_or_default(),
            ]
        })
        .collect();
    match display.format {
        Format::Table => {
            let shown: Vec<Vec<String>> = rows.iter().map(|r| r[..7].to_vec()).collect();
            println!("{}", output::table(&header[..7], shown, 2));
            for s in scans.iter().filter(|s| !s.detection.found) {
                if let Some(note) = &s.detection.note {
                    println!("{}: {note}", s.source.id());
                }
            }
            for e in scans.iter().flat_map(|s| &s.errors) {
                eprintln!("error: {e}");
            }
        }
        Format::Csv => print!("{}", output::csv(&header, rows)?),
        Format::Json => {
            let items: Vec<_> = scans
                .iter()
                .map(|s| {
                    json!({
                        "source": s.source,
                        "found": s.detection.found,
                        "roots": s.detection.roots,
                        "note": s.detection.note,
                        "sessions": s.sessions,
                        "events": s.events,
                        "warnings": s.warnings,
                        "errors": s.errors,
                    })
                })
                .collect();
            println!("{}", serde_json::to_string_pretty(&items)?);
        }
    }
    Ok(())
}
