use std::path::Path;

use agentlens_core::adapter::{Registry, SessionFilter};
use agentlens_core::env::Env;
use agentlens_core::model::Source;
use agentlens_core::pipeline::skill_run;
use agentlens_core::report::Report;
use agentlens_core::tokens::TokenEstimator;
use anyhow::{Context, Result};
use jiff::Timestamp;

const TEMPLATE: &str = include_str!("../../assets/report.html");
const PLACEHOLDER: &str = "__REPORT_DATA__";

/// Embeds the report as JSON in the HTML template. `<` is escaped so that the
/// data can never end the script element.
pub fn render_html(report: &Report) -> Result<String> {
    let json = serde_json::to_string(report)?.replace('<', "\\u003c");
    Ok(TEMPLATE.replacen(PLACEHOLDER, &json, 1))
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use agentlens_core::analyzers::skills::SkillReport;
    use agentlens_core::report::{ReportMeta, SCHEMA_VERSION};

    fn report(session_count: u32) -> Report {
        Report {
            meta: ReportMeta {
                schema_version: SCHEMA_VERSION,
                tool_version: "0.0.0".into(),
                generated_at: "2026-10-10T00:00:00Z".parse().unwrap(),
                since: None,
                sources: vec![],
                installed_skills: 0,
                warnings: 0,
                errors: vec!["</script><b>x".into()],
            },
            skills: SkillReport {
                sessions: session_count,
                ..SkillReport::default()
            },
        }
    }

    #[test]
    fn data_cannot_break_out_of_the_script_element() {
        let page = render_html(&report(1)).unwrap();
        assert!(!page.contains("</script><b>x"));
        assert!(page.contains("\\u003c/script>\\u003cb>x"));
        assert!(!page.contains(PLACEHOLDER));
    }

    #[test]
    fn template_has_no_external_references() {
        for needle in ["http://", "https://", "src=\"", "href=\""] {
            let hits = TEMPLATE.matches(needle).count();
            // The only allowed URL is the SVG namespace string.
            let allowed = usize::from(needle == "http://");
            assert_eq!(hits, allowed, "unexpected `{needle}` in template");
        }
    }
}
