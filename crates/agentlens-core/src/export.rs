//! Files made from the analyses: the self-contained HTML report. The CLI and the application both
//! call these functions, so a file is the same wherever it comes from.

use crate::report::Report;

pub mod csv;

const TEMPLATE: &str = include_str!("../assets/report.html");
const PLACEHOLDER: &str = "__REPORT_DATA__";

/// Embeds the report as JSON in the HTML template. `<` is escaped so that the data can never end the
/// script element.
pub fn render_html(report: &Report) -> Result<String, serde_json::Error> {
    let json = serde_json::to_string(report)?.replace('<', "\\u003c");
    Ok(TEMPLATE.replacen(PLACEHOLDER, &json, 1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzers::skills::SkillReport;
    use crate::report::{ReportMeta, SCHEMA_VERSION};

    fn report(session_count: u32) -> Report {
        Report {
            meta: ReportMeta {
                schema_version: SCHEMA_VERSION,
                tool_version: "0.0.0".into(),
                generated_at: "2026-10-10T00:00:00Z".parse().unwrap(),
                since: None,
                until: None,
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
