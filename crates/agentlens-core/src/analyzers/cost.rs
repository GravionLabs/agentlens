//! What the sessions cost.
//!
//! Cost reported by the client is measured; cost computed from the price table
//! is estimated; requests of models without a price are counted as unpriced and
//! never contribute zero dollars silently.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use jiff::Timestamp;
use serde::{Deserialize, Serialize};

use crate::model::{EventKind, SessionEvents, Source};
use crate::prices::PriceTable;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CostRow {
    pub key: String,
    pub requests: u64,
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    /// Cost the client reported.
    pub measured_usd: f64,
    /// Cost computed from the price table.
    pub estimated_usd: f64,
    pub unpriced_requests: u64,
    /// Premium requests (GitHub Copilot); a plan allowance, not money.
    pub premium_requests: f64,
    /// How the issue of the row was found; only grouped by issue. With sessions of several
    /// sources, the weakest of them (`commit` before `branch`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issue_source: Option<IssueSource>,
}

impl CostRow {
    fn new(key: &str) -> CostRow {
        CostRow {
            key: key.to_owned(),
            ..CostRow::default()
        }
    }

    pub fn total_usd(&self) -> f64 {
        self.measured_usd + self.estimated_usd
    }

    fn add(&mut self, other: &CostRow) {
        self.requests += other.requests;
        self.input += other.input;
        self.output += other.output;
        self.cache_read += other.cache_read;
        self.cache_write += other.cache_write;
        self.measured_usd += other.measured_usd;
        self.estimated_usd += other.estimated_usd;
        self.unpriced_requests += other.unpriced_requests;
        self.premium_requests += other.premium_requests;
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CostReport {
    /// Sorted by total cost, largest first; unpriced-only rows last.
    pub rows: Vec<CostRow>,
    pub total: CostRow,
    /// Models with unpriced requests and how many.
    pub unpriced_models: BTreeMap<String, u64>,
    /// One row per session; only filled when grouped by session.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sessions: Vec<SessionCost>,
}

/// What one session cost.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionCost {
    pub source: Source,
    pub session: String,
    pub project: Option<PathBuf>,
    pub started_at: Option<Timestamp>,
    pub duration_ms: Option<u64>,
    /// `#123` as `--by issue` resolves it ([`session_issue`]).
    pub issue: Option<String>,
    /// How `issue` was found.
    pub issue_source: Option<IssueSource>,
    pub models: Vec<String>,
    pub requests: u64,
    /// Requests of the main agent (not made by subagents).
    pub main_requests: u64,
    /// Largest `input + cache_read + cache_write` of a main-agent request.
    pub peak_context: u64,
    pub input: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub output: u64,
    /// Cost the client reported.
    pub measured_usd: f64,
    /// Cost computed from the price table.
    pub estimated_usd: f64,
    pub unpriced_requests: u64,
    /// Cost of the requests made by subagents (measured and estimated).
    pub subagent_usd: f64,
}

impl SessionCost {
    pub fn total_usd(&self) -> f64 {
        self.measured_usd + self.estimated_usd
    }

    /// No request has a price: the cost is unknown, not zero.
    pub fn cost_known(&self) -> bool {
        self.requests > self.unpriced_requests
    }

    /// The part of the cost that subagents caused (0 to 1); `None` if the cost is unknown or zero.
    pub fn subagent_share(&self) -> Option<f64> {
        (self.cost_known() && self.total_usd() > 0.0).then(|| self.subagent_usd / self.total_usd())
    }
}

/// What the cost is grouped by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Grouping {
    Model,
    Project,
    Branch,
    Issue,
    Session,
}

pub const NO_PROJECT: &str = "(no project)";
pub const NO_BRANCH: &str = "(no branch)";
pub const UNASSIGNED: &str = "(unassigned)";

/// Where the issue number of a session comes from, in the order they are tried.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum IssueSource {
    /// The branch name ([`issue_number`]).
    Branch,
    /// The message of a `git commit -m` the session ran ([`crate::commit::commit_issue`]).
    Commit,
}

/// The issue a session worked on: from the branch name, else from the first commit message of
/// the session that refers to an issue.
pub fn session_issue(se: &SessionEvents) -> Option<(u32, IssueSource)> {
    if let Some(n) = se.session.branch.as_deref().and_then(issue_number) {
        return Some((n, IssueSource::Branch));
    }
    se.events.iter().find_map(|e| match e.kind {
        EventKind::CommitIssue { issue } => Some((issue, IssueSource::Commit)),
        _ => None,
    })
}

/// The issue number a branch name refers to, if any.
///
/// A number of 2 to 6 digits counts if it starts a `/` segment (`feat/123-title`,
/// `123-title`) or follows `issue`, `issues` or `gh` (`issue-123`). A four-digit
/// year followed by a two-digit month or day is a date, not an issue.
pub fn issue_number(branch: &str) -> Option<u32> {
    for segment in branch.split('/') {
        let tokens: Vec<&str> = segment.split(['-', '_']).collect();
        for (i, token) in tokens.iter().enumerate() {
            let is_number =
                (2..=6).contains(&token.len()) && token.bytes().all(|b| b.is_ascii_digit());
            if !is_number {
                continue;
            }
            let after_keyword = i > 0
                && matches!(
                    tokens[i - 1].to_lowercase().as_str(),
                    "issue" | "issues" | "gh"
                );
            if i != 0 && !after_keyword {
                continue;
            }
            let date = token.len() == 4
                && (token.starts_with("19") || token.starts_with("20"))
                && tokens
                    .get(i + 1)
                    .is_some_and(|n| n.len() == 2 && n.bytes().all(|b| b.is_ascii_digit()));
            if !date {
                return token.parse().ok();
            }
        }
    }
    None
}

fn group_key(se: &SessionEvents, group: Grouping) -> Option<String> {
    match group {
        Grouping::Model => None,
        Grouping::Project => Some(
            se.session
                .project_dir
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| NO_PROJECT.into()),
        ),
        Grouping::Branch => Some(
            se.session
                .branch
                .clone()
                .unwrap_or_else(|| NO_BRANCH.into()),
        ),
        Grouping::Session => Some(se.session.id.clone()),
        Grouping::Issue => Some(
            session_issue(se)
                .map(|(n, _)| format!("#{n}"))
                .unwrap_or_else(|| UNASSIGNED.into()),
        ),
    }
}

/// Cost per model.
pub fn by_model(sessions: &[SessionEvents], prices: &PriceTable) -> CostReport {
    analyze(sessions, prices, Grouping::Model)
}

/// Cost grouped by model, project, branch, issue or session.
pub fn analyze(sessions: &[SessionEvents], prices: &PriceTable, group: Grouping) -> CostReport {
    let mut rows: BTreeMap<String, CostRow> = BTreeMap::new();
    let mut unpriced: BTreeMap<String, u64> = BTreeMap::new();
    let mut per_session: Vec<SessionCost> = Vec::new();
    for se in sessions {
        let mut sc = (group == Grouping::Session).then(|| session_cost(se));
        let mut models: BTreeSet<String> = BTreeSet::new();
        let key = group_key(se, group);
        let issue_source = (group == Grouping::Issue)
            .then(|| session_issue(se).map(|(_, source)| source))
            .flatten();
        for e in &se.events {
            let EventKind::Usage(u) = &e.kind else {
                continue;
            };
            let model = u
                .model
                .clone()
                .or_else(|| se.session.model.clone())
                .unwrap_or_else(|| "unknown".into());
            let key = key.clone().unwrap_or_else(|| model.clone());
            let row = rows
                .entry(key.clone())
                .or_insert_with(|| CostRow::new(&key));
            row.issue_source = row.issue_source.max(issue_source);
            row.requests += 1;
            row.input += u.input;
            row.output += u.output;
            row.cache_read += u.cache_read;
            row.cache_write += u.cache_write;
            row.premium_requests += u.premium_requests.unwrap_or(0.0);
            let mut usage = u.clone();
            usage.model = Some(model.clone());
            let cost = prices.cost(&usage);
            match &cost {
                Some(c) if c.measured => row.measured_usd += c.usd,
                Some(c) => row.estimated_usd += c.usd,
                None => {
                    row.unpriced_requests += 1;
                    *unpriced.entry(model.clone()).or_default() += 1;
                }
            }
            if let Some(sc) = sc.as_mut() {
                models.insert(model);
                sc.requests += 1;
                sc.input += u.input;
                sc.output += u.output;
                sc.cache_read += u.cache_read;
                sc.cache_write += u.cache_write;
                if !u.subagent {
                    sc.main_requests += 1;
                    sc.peak_context = sc.peak_context.max(u.total_input());
                }
                match cost {
                    Some(c) => {
                        if c.measured {
                            sc.measured_usd += c.usd;
                        } else {
                            sc.estimated_usd += c.usd;
                        }
                        if u.subagent {
                            sc.subagent_usd += c.usd;
                        }
                    }
                    None => sc.unpriced_requests += 1,
                }
            }
        }
        if let Some(mut sc) = sc {
            sc.models = models.into_iter().collect();
            per_session.push(sc);
        }
    }
    per_session.sort_by(|a, b| {
        b.cost_known()
            .cmp(&a.cost_known())
            .then_with(|| {
                b.total_usd()
                    .partial_cmp(&a.total_usd())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| a.session.cmp(&b.session))
    });
    let mut total = CostRow::new("total");
    for r in rows.values() {
        total.add(r);
    }
    let mut rows: Vec<CostRow> = rows.into_values().collect();
    rows.sort_by(|a, b| {
        b.total_usd()
            .partial_cmp(&a.total_usd())
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.key.cmp(&b.key))
    });
    CostReport {
        rows,
        total,
        unpriced_models: unpriced,
        sessions: per_session,
    }
}

fn session_cost(se: &SessionEvents) -> SessionCost {
    let s = &se.session;
    let issue = session_issue(se);
    SessionCost {
        source: s.source,
        session: s.id.clone(),
        project: s.project_dir.clone(),
        started_at: s.started_at,
        duration_ms: match (s.started_at, s.ended_at) {
            (Some(a), Some(b)) => u64::try_from(b.as_millisecond() - a.as_millisecond()).ok(),
            _ => None,
        },
        issue: issue.map(|(n, _)| format!("#{n}")),
        issue_source: issue.map(|(_, source)| source),
        models: Vec::new(),
        requests: 0,
        main_requests: 0,
        peak_context: 0,
        input: 0,
        cache_read: 0,
        cache_write: 0,
        output: 0,
        measured_usd: 0.0,
        estimated_usd: 0.0,
        unpriced_requests: 0,
        subagent_usd: 0.0,
    }
}

/// Actual cost of one model next to what the same tokens would cost with another model.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct WhatIfRow {
    pub model: String,
    /// Requests that went into the comparison.
    pub requests: u64,
    pub actual_usd: f64,
    pub what_if_usd: f64,
    /// Requests left out: no actual price or no price for the what-if model.
    pub excluded_requests: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WhatIfReport {
    /// The model whose prices were applied.
    pub target: String,
    /// Sorted by actual cost, largest first.
    pub rows: Vec<WhatIfRow>,
    pub total: WhatIfRow,
}

/// Recomputes the usage with the prices of `target`. The caller checks that
/// `target` has a price; without one every request is excluded.
pub fn what_if(sessions: &[SessionEvents], prices: &PriceTable, target: &str) -> WhatIfReport {
    let mut rows: BTreeMap<String, WhatIfRow> = BTreeMap::new();
    for se in sessions {
        for e in &se.events {
            let EventKind::Usage(u) = &e.kind else {
                continue;
            };
            let model = u
                .model
                .clone()
                .or_else(|| se.session.model.clone())
                .unwrap_or_else(|| "unknown".into());
            let row = rows.entry(model.clone()).or_insert_with(|| WhatIfRow {
                model: model.clone(),
                ..WhatIfRow::default()
            });
            let mut usage = u.clone();
            usage.model = Some(model);
            match (prices.cost(&usage), prices.compute(target, &usage)) {
                (Some(actual), Some(alt)) => {
                    row.requests += 1;
                    row.actual_usd += actual.usd;
                    row.what_if_usd += alt;
                }
                _ => row.excluded_requests += 1,
            }
        }
    }
    let mut total = WhatIfRow {
        model: "total".into(),
        ..WhatIfRow::default()
    };
    for r in rows.values() {
        total.requests += r.requests;
        total.actual_usd += r.actual_usd;
        total.what_if_usd += r.what_if_usd;
        total.excluded_requests += r.excluded_requests;
    }
    let mut rows: Vec<WhatIfRow> = rows.into_values().collect();
    rows.sort_by(|a, b| {
        b.actual_usd
            .partial_cmp(&a.actual_usd)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.model.cmp(&b.model))
    });
    WhatIfReport {
        target: target.to_owned(),
        rows,
        total,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Session, Source, Usage};

    fn usage(model: Option<&str>, input: u64, output: u64, cost: Option<f64>) -> EventKind {
        EventKind::Usage(Usage {
            model: model.map(Into::into),
            input,
            output,
            cost_usd: cost,
            premium_requests: Some(1.0),
            ..Usage::default()
        })
    }

    fn session(kinds: Vec<EventKind>) -> SessionEvents {
        let mut se = SessionEvents::new(Session::new("s", Source::ClaudeCode));
        for k in kinds {
            se.push(None, k);
        }
        se
    }

    #[test]
    fn cost_is_split_into_measured_estimated_and_unpriced() {
        let se = session(vec![
            usage(Some("claude-sonnet-5-5"), 1_000_000, 100_000, None), // 2.0 + 1.0 estimated
            usage(Some("claude-sonnet-5-5"), 1_000_000, 0, Some(0.25)), // measured
            usage(Some("mystery"), 10, 10, None),
            usage(Some("free-model"), 10, 10, Some(0.0)), // reported zero is a price
        ]);
        let r = by_model(&[se], &PriceTable::bundled());
        let sonnet = r
            .rows
            .iter()
            .find(|x| x.key == "claude-sonnet-5-5")
            .unwrap();
        assert_eq!(sonnet.requests, 2);
        assert!((sonnet.estimated_usd - 3.0).abs() < 1e-9);
        assert!((sonnet.measured_usd - 0.25).abs() < 1e-9);
        assert_eq!(r.unpriced_models.get("mystery"), Some(&1));
        assert!(!r.unpriced_models.contains_key("free-model"));
        assert!((r.total.total_usd() - 3.25).abs() < 1e-9);
        assert_eq!(r.total.unpriced_requests, 1);
        assert_eq!(r.total.requests, 4);
        assert_eq!(r.total.premium_requests, 4.0);
        assert_eq!(r.rows[0].key, "claude-sonnet-5-5", "sorted by cost");
    }

    #[test]
    fn session_grouping_has_a_row_per_session_with_peak_context_and_subagent_share() {
        let sub = |input, cost| {
            EventKind::Usage(Usage {
                model: Some("claude-sonnet-5-5".into()),
                input,
                cost_usd: Some(cost),
                subagent: true,
                ..Usage::default()
            })
        };
        let mut a = session(vec![
            usage(Some("claude-sonnet-5-5"), 500, 0, Some(0.75)),
            usage(Some("claude-sonnet-5-5"), 900, 0, Some(0.25)),
            sub(5_000, 1.0),
        ]);
        a.session.branch = Some("feat/12-x".into());
        let mut b = SessionEvents::new(Session::new("b", Source::OpenCode));
        b.push(None, usage(Some("mystery"), 10, 10, None));
        let idle = SessionEvents::new(Session::new("c", Source::CopilotCli));
        let r = analyze(&[b, idle, a], &PriceTable::bundled(), Grouping::Session);
        let ids: Vec<&str> = r.sessions.iter().map(|s| s.session.as_str()).collect();
        assert_eq!(ids, ["s", "b", "c"], "known cost first, then unknown");
        let s = &r.sessions[0];
        assert_eq!((s.requests, s.main_requests), (3, 2));
        assert_eq!(s.peak_context, 900, "subagent requests do not count");
        assert_eq!(s.issue.as_deref(), Some("#12"));
        assert!((s.total_usd() - 2.0).abs() < 1e-9);
        assert_eq!(s.subagent_share(), Some(0.5));
        for unknown in &r.sessions[1..] {
            assert!(!unknown.cost_known(), "{} is n/a, not 0", unknown.session);
            assert_eq!(unknown.subagent_share(), None);
        }
    }

    #[test]
    fn requests_without_a_model_use_the_session_model_or_are_unknown() {
        let mut se = session(vec![usage(None, 1_000_000, 0, None)]);
        se.session.model = Some("claude-sonnet-5-5".into());
        let unknown = session(vec![usage(None, 5, 5, None)]);
        let r = by_model(&[se, unknown], &PriceTable::bundled());
        assert!((r.rows[0].estimated_usd - 2.0).abs() < 1e-9);
        assert_eq!(r.unpriced_models.get("unknown"), Some(&1));
    }

    #[test]
    fn issue_numbers_are_read_from_branch_names() {
        for (branch, want) in [
            ("feat/123-title", Some(123)),
            ("123-title", Some(123)),
            ("issue-45", Some(45)),
            ("fix/gh-7890-crash", Some(7890)),
            ("bugfix/Issues_88", Some(88)),
            ("main", None),
            ("feat/oauth2-login", None),
            ("feat/v2", None),
            ("release/2026-10-10", None),
            ("feat/9-too-short", None),
            ("feat/1234567-too-long", None),
            ("topic-123", None),
            ("HEAD", None),
        ] {
            assert_eq!(issue_number(branch), want, "{branch}");
        }
    }

    #[test]
    fn the_issue_comes_from_the_branch_then_the_first_commit() {
        let commit = |issue| EventKind::CommitIssue { issue };
        let mut branch_only = session(vec![]);
        branch_only.session.branch = Some("feat/12-x".into());
        assert_eq!(session_issue(&branch_only), Some((12, IssueSource::Branch)));
        let mut commit_only = session(vec![commit(502), commit(503)]);
        commit_only.session.branch = Some("release/next".into());
        assert_eq!(
            session_issue(&commit_only),
            Some((502, IssueSource::Commit)),
            "the first commit wins"
        );
        let mut both = session(vec![commit(502)]);
        both.session.branch = Some("feat/12-x".into());
        assert_eq!(session_issue(&both), Some((12, IssueSource::Branch)));
        assert_eq!(session_issue(&session(vec![])), None);
    }

    #[test]
    fn an_issue_row_names_its_weakest_source() {
        let u = || usage(Some("claude-sonnet-5-5"), 1_000, 0, None);
        let sessions = [
            on("/a", Some("feat/12-x"), vec![u()]),
            on(
                "/a",
                Some("release/next"),
                vec![EventKind::CommitIssue { issue: 12 }, u()],
            ),
            on("/a", Some("feat/13-y"), vec![u()]),
            on("/a", None, vec![u()]),
        ];
        let r = analyze(&sessions, &PriceTable::bundled(), Grouping::Issue);
        let source = |key: &str| r.rows.iter().find(|r| r.key == key).unwrap().issue_source;
        assert_eq!(source("#12"), Some(IssueSource::Commit));
        assert_eq!(source("#13"), Some(IssueSource::Branch));
        assert_eq!(source(UNASSIGNED), None);
        assert_eq!(r.total.issue_source, None);
        let by_branch = analyze(&sessions, &PriceTable::bundled(), Grouping::Branch);
        assert!(by_branch.rows.iter().all(|r| r.issue_source.is_none()));
    }

    fn on(project: &str, branch: Option<&str>, kinds: Vec<EventKind>) -> SessionEvents {
        let mut se = session(kinds);
        se.session.project_dir = Some(project.into());
        se.session.branch = branch.map(Into::into);
        se
    }

    #[test]
    fn groupings_partition_the_total() {
        let u = || usage(Some("claude-sonnet-5-5"), 1_000_000, 0, None); // $2.00
        let sessions = [
            on("/a", Some("feat/12-x"), vec![u(), u()]),
            on("/a", Some("feat/12-y"), vec![u()]),
            on("/b", Some("main"), vec![u()]),
            on("/b", None, vec![u()]),
        ];
        let prices = PriceTable::bundled();
        let total = analyze(&sessions, &prices, Grouping::Model)
            .total
            .total_usd();
        assert!((total - 10.0).abs() < 1e-9);
        for g in [Grouping::Project, Grouping::Branch, Grouping::Issue] {
            let r = analyze(&sessions, &prices, g);
            let sum: f64 = r.rows.iter().map(CostRow::total_usd).sum();
            assert!((sum - total).abs() < 1e-9, "{g:?}");
            assert!((r.total.total_usd() - total).abs() < 1e-9);
        }
        let issues = analyze(&sessions, &prices, Grouping::Issue);
        assert_eq!(issues.rows[0].key, "#12");
        assert!((issues.rows[0].total_usd() - 6.0).abs() < 1e-9);
        assert_eq!(issues.rows[1].key, UNASSIGNED);
        let projects = analyze(&sessions, &prices, Grouping::Project);
        assert_eq!(
            projects
                .rows
                .iter()
                .map(|r| r.key.as_str())
                .collect::<Vec<_>>(),
            ["/a", "/b"]
        );
        let branches = analyze(&sessions, &prices, Grouping::Branch);
        assert!(branches.rows.iter().any(|r| r.key == NO_BRANCH));
    }

    #[test]
    fn what_if_applies_the_other_models_prices_to_the_same_tokens() {
        let se = session(vec![
            usage(Some("claude-sonnet-5-5"), 1_000_000, 0, None), // actual $2, haiku 4.5 $1
            usage(Some("claude-sonnet-5-5"), 0, 1_000_000, None), // actual $10, haiku 4.5 $5
            usage(Some("claude-opus-5-5"), 1_000_000, 0, Some(9.0)), // reported $9, haiku $1
            usage(Some("mystery"), 5, 5, None),                   // no actual price: excluded
        ]);
        let r = what_if(&[se], &PriceTable::bundled(), "claude-haiku-4-5");
        let sonnet = r
            .rows
            .iter()
            .find(|x| x.model == "claude-sonnet-5-5")
            .unwrap();
        assert_eq!(sonnet.requests, 2);
        assert!((sonnet.actual_usd - 12.0).abs() < 1e-9);
        assert!((sonnet.what_if_usd - 6.0).abs() < 1e-9);
        assert!((r.total.actual_usd - 21.0).abs() < 1e-9);
        assert!((r.total.what_if_usd - 7.0).abs() < 1e-9);
        assert_eq!((r.total.requests, r.total.excluded_requests), (3, 1));
        assert_eq!(
            r.rows[0].model, "claude-sonnet-5-5",
            "sorted by actual cost"
        );
    }

    #[test]
    fn a_target_without_a_price_excludes_everything() {
        let se = session(vec![usage(Some("claude-sonnet-5-5"), 1_000_000, 0, None)]);
        let r = what_if(&[se], &PriceTable::bundled(), "unknown-model");
        assert_eq!((r.total.requests, r.total.excluded_requests), (0, 1));
        assert_eq!(r.total.what_if_usd, 0.0);
    }
}
