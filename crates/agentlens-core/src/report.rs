//! The data behind a report: everything the analyzers produced plus metadata.
//! One serializable value, so the same data feeds JSON output and the HTML report.

use jiff::Timestamp;
use serde::{Deserialize, Serialize};

use crate::adapter::SessionFilter;
use crate::analyzers::skills::SkillReport;
use crate::model::Source;
use crate::pipeline::SkillRun;

/// Bump when the JSON layout of the report changes incompatibly. 2: `carry.tokens` is a `Tokens`
/// (`count` and `measured`) instead of a bare number.
pub const SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReportMeta {
    pub schema_version: u32,
    /// Version of AgentLens that produced the report.
    pub tool_version: String,
    pub generated_at: Timestamp,
    /// The `--since` filter, if any.
    pub since: Option<Timestamp>,
    /// The `--until` filter, if any: the report covers `[since, until)`.
    pub until: Option<Timestamp>,
    /// Sources that were requested (empty means all).
    pub sources: Vec<Source>,
    /// Number of skills found in the inventory.
    pub installed_skills: u32,
    /// Warnings raised while reading sessions.
    pub warnings: u64,
    /// Sessions that could not be read at all.
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Report {
    pub meta: ReportMeta,
    pub skills: SkillReport,
}

impl Report {
    pub fn from_skill_run(
        run: SkillRun,
        filter: &SessionFilter,
        sources: &[Source],
        now: Timestamp,
    ) -> Report {
        Report {
            meta: ReportMeta {
                schema_version: SCHEMA_VERSION,
                tool_version: crate::VERSION.to_owned(),
                generated_at: now,
                since: filter.since,
                until: filter.until,
                sources: sources.to_vec(),
                installed_skills: run.inventory.skills.len() as u32,
                warnings: run.warnings,
                errors: run.errors,
            },
            skills: run.report,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzers::skills::{BaselineCost, SkillStats, TriggerCounts};
    use crate::tokens::Tokens;

    fn sample() -> Report {
        let mut skills = SkillReport {
            sessions: 2,
            first_event: Some("2026-10-01T10:00:00Z".parse().unwrap()),
            last_event: Some("2026-10-02T10:00:00Z".parse().unwrap()),
            ..SkillReport::default()
        };
        skills.requests_by_source.insert(Source::OpenCode, 7);
        let mut s = SkillStats {
            name: "demo".into(),
            in_inventory: true,
            clients: vec![Source::ClaudeCode],
            invocations: 2,
            file_uses: 0,
            sessions: 1,
            projects: vec!["/work/demo".into()],
            first_used: Some("2026-10-01T10:00:00Z".parse().unwrap()),
            last_used: None,
            triggers: TriggerCounts::default(),
            by_source: Default::default(),
            uses_by_day: Default::default(),
            load: Tokens::estimated(120),
            unknown_loads: 0,
            carry: Default::default(),
            follow_on: Default::default(),
            attributed: Default::default(),
            baseline: BaselineCost {
                requests: 3,
                tokens: Tokens::estimated(40),
            },
        };
        s.uses_by_day.insert("2026-10-01".into(), 2);
        skills.skills.push(s);
        Report {
            meta: ReportMeta {
                schema_version: SCHEMA_VERSION,
                tool_version: "0.0.0".into(),
                generated_at: "2026-10-10T00:00:00Z".parse().unwrap(),
                since: None,
                until: None,
                sources: vec![Source::ClaudeCode, Source::OpenCode],
                installed_skills: 3,
                warnings: 1,
                errors: vec!["boom".into()],
            },
            skills,
        }
    }

    #[test]
    fn report_round_trips_through_json() {
        let report = sample();
        let json = serde_json::to_string(&report).unwrap();
        let back: Report = serde_json::from_str(&json).unwrap();
        assert_eq!(back, report);
    }

    #[test]
    fn source_ids_in_json_match_the_cli_ids() {
        let json = serde_json::to_value(sample()).unwrap();
        assert_eq!(json["meta"]["sources"][1], "opencode");
    }
}
