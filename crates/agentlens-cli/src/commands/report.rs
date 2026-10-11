use std::path::Path;

use agentlens_core::adapter::{Registry, SessionFilter};
use agentlens_core::env::Env;
use agentlens_core::export::render_html;
use agentlens_core::model::Source;
use agentlens_core::pipeline::skill_run;
use agentlens_core::report::Report;
use agentlens_core::tokens::TokenEstimator;
use anyhow::{Context, Result};
use jiff::Timestamp;

pub fn run(
    registry: &Registry,
    env: &Env,
    filter: &SessionFilter,
    only: &[Source],
    estimator: &dyn TokenEstimator,
    html: &Path,
) -> Result<()> {
    let run = skill_run(registry, env, filter, only, estimator);
    for e in &run.errors {
        eprintln!("error: {e}");
    }
    let report = Report::from_skill_run(run, filter, only, Timestamp::now());
    let page = render_html(&report)?;
    std::fs::write(html, page).with_context(|| format!("cannot write {}", html.display()))?;
    println!(
        "wrote {} ({} sessions, {} skills)",
        html.display(),
        report.skills.sessions,
        report.skills.skills.len()
    );
    Ok(())
}
