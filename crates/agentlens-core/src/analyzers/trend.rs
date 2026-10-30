//! The trend: the figures of a period next to those of a comparison window, with their deltas.
//!
//! The analyzer reuses the others per window (`skills`, `cost`, `efficiency`, `flow`) and only adds the
//! comparison. Sessions are split by the caller (`api::windows`) with the rule of
//! `SessionFilter::matches`: a session belongs to the window in which it ends.

use std::collections::{BTreeMap, BTreeSet};

use jiff::Timestamp;
use serde::{Deserialize, Serialize};

use crate::analyzers::cost;
use crate::analyzers::efficiency::{self, CacheBy};
use crate::analyzers::flow;
use crate::analyzers::skills::{self, SkillStats};
use crate::inventory::Inventory;
use crate::model::SessionEvents;
use crate::prices::PriceTable;
use crate::since::window_start;
use crate::tokens::TokenEstimator;

const DAY_SECONDS: f64 = 86_400.0;

/// One window `[since, until)` and the number of sessions that ended in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Window {
    pub since: Timestamp,
    pub until: Timestamp,
    pub sessions: u32,
}

impl Window {
    fn days(&self) -> f64 {
        (self.until.as_second() - self.since.as_second()) as f64 / DAY_SECONDS
    }
}

/// The current window and the one it is compared against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Windows {
    pub current: Window,
    pub against: Window,
    /// The windows differ in length: every count is compared per day.
    pub normalized: bool,
}

/// Plans the two windows (session counts still 0). The current window is `[since, until)`; without
/// `since` it is the last `default_days` days before `until`, without `until` it ends at `now`. The
/// comparison window is `against` or else the window of the same length right before. Windows that
/// overlap, or an empty one, are an error.
pub fn plan(
    since: Option<Timestamp>,
    until: Option<Timestamp>,
    now: Timestamp,
    default_days: u32,
    against: Option<(Timestamp, Timestamp)>,
) -> Result<Windows, String> {
    let until = until.unwrap_or(now);
    let since = match since {
        Some(s) => s,
        None => window_start(default_days, Some(until), now)?,
    };
    if since >= until {
        return Err(format!("the period {since}..{until} is empty"));
    }
    let (a_since, a_until) = match against {
        Some((s, u)) => {
            if s >= u {
                return Err(format!("the comparison window {s}..{u} is empty"));
            }
            (s, u)
        }
        None => {
            let len = until.as_second() - since.as_second();
            let start = Timestamp::from_second(since.as_second() - len)
                .map_err(|e| format!("the comparison window starts out of range: {e}"))?;
            (start, since)
        }
    };
    if a_since < until && since < a_until {
        return Err(format!(
            "the comparison window {a_since}..{a_until} overlaps the period {since}..{until}"
        ));
    }
    let window = |since, until| Window {
        since,
        until,
        sessions: 0,
    };
    let current = window(since, until);
    let against = window(a_since, a_until);
    Ok(Windows {
        normalized: (current.days() - against.days()).abs() > 1e-9,
        current,
        against,
    })
}

/// A figure in both windows. `before` is the comparison window, `after` the current one; with
/// `Windows::normalized` counts are per-day values.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Delta {
    pub before: Option<f64>,
    pub after: Option<f64>,
    /// `after - before`; `None` when either window has no sessions (never a 100 % drop).
    pub change: Option<f64>,
    /// `change` in percent of `before`; `None` when `before` is 0.
    pub change_pct: Option<f64>,
    /// `true` only if both values were measured.
    pub measured: bool,
}

/// What the deltas need to know about the two windows.
struct Basis {
    comparable: bool,
    /// Window lengths in days (`before`, `after`) when counts are compared per day.
    per_day: Option<(f64, f64)>,
}

impl Basis {
    fn new(windows: &Windows) -> Basis {
        Basis {
            comparable: windows.current.sessions > 0 && windows.against.sessions > 0,
            per_day: windows
                .normalized
                .then(|| (windows.against.days(), windows.current.days())),
        }
    }

    /// A count or an amount: per day when the windows differ in length.
    fn count(&self, before: f64, after: f64, measured: bool) -> Delta {
        let (b, a) = match self.per_day {
            Some((bd, ad)) => (before / bd, after / ad),
            None => (before, after),
        };
        self.delta(Some(b), Some(a), measured)
    }

    /// A rate or a share: compared as it is, never per day.
    fn rate(&self, before: Option<f64>, after: Option<f64>, measured: bool) -> Delta {
        self.delta(before, after, measured)
    }

    fn delta(&self, before: Option<f64>, after: Option<f64>, measured: bool) -> Delta {
        let change = match (self.comparable, before, after) {
            (true, Some(b), Some(a)) => Some(a - b),
            _ => None,
        };
        let change_pct = match (change, before) {
            (Some(c), Some(b)) if b != 0.0 => Some(c / b.abs() * 100.0),
            _ => None,
        };
        Delta {
            before,
            after,
            change,
            change_pct,
            measured,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VolumeTrend {
    pub sessions: Delta,
    pub requests: Delta,
    pub subagent_requests: Delta,
    pub tokens: Delta,
    pub context_tokens: Delta,
    pub large_context_requests: Delta,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CostTrend {
    /// Cost the clients reported.
    pub measured_usd: Delta,
    /// Cost computed from the price table; kept apart from the measured one.
    pub estimated_usd: Delta,
    pub unpriced_requests: Delta,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EfficiencyTrend {
    /// Cache read over all input, summed over the sources that report cache tokens.
    pub cache_hit_rate: Delta,
    /// Share of the requests that subagents made.
    pub subagent_share: Delta,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ErrorTrend {
    pub tool_calls: Delta,
    pub failed_calls: Delta,
    pub retry_loops: Delta,
}

/// Whether a skill was loaded in both windows or in one only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SkillStatus {
    /// Loaded in the current window only.
    New,
    /// Loaded in the comparison window only.
    Gone,
    Both,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillTrend {
    pub name: String,
    pub status: SkillStatus,
    pub loads: Delta,
    /// Baseline, load and carry tokens of the skill.
    pub overhead_tokens: Delta,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrendReport {
    pub windows: Windows,
    pub volume: VolumeTrend,
    pub cost: CostTrend,
    pub efficiency: EfficiencyTrend,
    pub errors: ErrorTrend,
    /// Every skill with loads in either window, by the size of the change of its overhead tokens
    /// (largest first), then by name.
    pub skills: Vec<SkillTrend>,
}

/// How a figure is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    Count,
    Tokens,
    Usd,
    /// A share between 0 and 1.
    Rate,
}

/// One figure of the report with its group and name, in the order of the table and the CSV.
#[derive(Debug, Clone, Copy)]
pub struct FigureRow<'a> {
    /// `volume`, `cost`, `efficiency`, `errors` or `skills`.
    pub group: &'static str,
    /// The figure (`requests`); for a skill `loads` or `overhead_tokens`.
    pub figure: &'static str,
    /// The skill, for the rows of the group `skills`.
    pub skill: Option<&'a SkillTrend>,
    pub unit: Unit,
    pub delta: &'a Delta,
}

impl TrendReport {
    /// Every figure in the order volume, cost, efficiency, errors, then two rows per skill.
    pub fn figures(&self) -> Vec<FigureRow<'_>> {
        let row = |group, figure, unit, delta| FigureRow {
            group,
            figure,
            skill: None,
            unit,
            delta,
        };
        let (v, c, e, x) = (&self.volume, &self.cost, &self.efficiency, &self.errors);
        let mut rows = vec![
            row("volume", "sessions", Unit::Count, &v.sessions),
            row("volume", "requests", Unit::Count, &v.requests),
            row(
                "volume",
                "subagent_requests",
                Unit::Count,
                &v.subagent_requests,
            ),
            row("volume", "tokens", Unit::Tokens, &v.tokens),
            row("volume", "context_tokens", Unit::Tokens, &v.context_tokens),
            row(
                "volume",
                "large_context_requests",
                Unit::Count,
                &v.large_context_requests,
            ),
            row("cost", "measured_usd", Unit::Usd, &c.measured_usd),
            row("cost", "estimated_usd", Unit::Usd, &c.estimated_usd),
            row(
                "cost",
                "unpriced_requests",
                Unit::Count,
                &c.unpriced_requests,
            ),
            row(
                "efficiency",
                "cache_hit_rate",
                Unit::Rate,
                &e.cache_hit_rate,
            ),
            row(
                "efficiency",
                "subagent_share",
                Unit::Rate,
                &e.subagent_share,
            ),
            row("errors", "tool_calls", Unit::Count, &x.tool_calls),
            row("errors", "failed_calls", Unit::Count, &x.failed_calls),
            row("errors", "retry_loops", Unit::Count, &x.retry_loops),
        ];
        for s in &self.skills {
            rows.push(FigureRow {
                skill: Some(s),
                ..row("skills", "loads", Unit::Count, &s.loads)
            });
            rows.push(FigureRow {
                skill: Some(s),
                ..row(
                    "skills",
                    "overhead_tokens",
                    Unit::Tokens,
                    &s.overhead_tokens,
                )
            });
        }
        rows
    }
}

/// The figures of one window.
struct Figures {
    sessions: f64,
    requests: f64,
    subagent_requests: f64,
    tokens: f64,
    context_tokens: f64,
    large_context_requests: f64,
    /// `false` if any request's numbers were estimated.
    tokens_measured: bool,
    measured_usd: f64,
    estimated_usd: f64,
    unpriced: f64,
    hit_rate: Option<f64>,
    subagent_share: Option<f64>,
    tool_calls: f64,
    failed_calls: f64,
    retry_loops: f64,
    skills: BTreeMap<String, SkillStats>,
}

fn figures(
    sessions: &[SessionEvents],
    inventory: &Inventory,
    prices: &PriceTable,
    estimator: &dyn TokenEstimator,
) -> Figures {
    let skill_report = skills::analyze(inventory, sessions, estimator);
    let totals = &skill_report.totals;
    let cost = cost::by_model(sessions, prices).total;
    let eff = efficiency::analyze(sessions, prices, CacheBy::Source);
    let errors = flow::errors(sessions);

    let (mut read, mut all_input) = (0u64, 0u64);
    for row in eff.cache.iter().filter(|r| r.hit_rate.is_some()) {
        read += row.cache_read;
        all_input += row.input + row.cache_read + row.cache_write;
    }
    let (mut requests, mut by_subagents) = (0u64, 0u64);
    for share in &eff.subagents {
        requests += share.requests;
        by_subagents += share.subagent_requests;
    }
    Figures {
        sessions: f64::from(skill_report.sessions),
        requests: totals.requests as f64,
        subagent_requests: totals.subagent_requests as f64,
        tokens: totals.tokens as f64,
        context_tokens: totals.context_tokens as f64,
        large_context_requests: totals.large_context_requests as f64,
        // A window without requests has nothing estimated.
        tokens_measured: totals.measured || totals.requests == 0,
        measured_usd: cost.measured_usd,
        estimated_usd: cost.estimated_usd,
        unpriced: cost.unpriced_requests as f64,
        hit_rate: (all_input > 0).then(|| read as f64 / all_input as f64),
        subagent_share: (requests > 0).then(|| by_subagents as f64 / requests as f64),
        tool_calls: errors.tools.iter().map(|t| t.calls).sum::<u64>() as f64,
        failed_calls: errors.tools.iter().map(|t| t.errors).sum::<u64>() as f64,
        retry_loops: errors.loops.len() as f64,
        skills: skill_report
            .skills
            .into_iter()
            .filter(|s| s.invocations > 0)
            .map(|s| (s.name.clone(), s))
            .collect(),
    }
}

fn overhead_measured(s: Option<&SkillStats>) -> bool {
    s.is_none_or(|s| s.baseline.tokens.measured && s.load.measured && s.carry.tokens.measured)
}

/// Compares the sessions of the current window with those of the comparison window. `windows` comes
/// from [`plan`] with the session counts filled in; `inventory` is the one skill inventory of both.
pub fn analyze(
    windows: Windows,
    current: &[SessionEvents],
    against: &[SessionEvents],
    inventory: &Inventory,
    prices: &PriceTable,
    estimator: &dyn TokenEstimator,
) -> TrendReport {
    let before = figures(against, inventory, prices, estimator);
    let after = figures(current, inventory, prices, estimator);
    let basis = Basis::new(&windows);
    let tokens_measured = before.tokens_measured && after.tokens_measured;
    let c = |b: f64, a: f64| basis.count(b, a, true);

    let mut names: BTreeSet<&String> = before.skills.keys().collect();
    names.extend(after.skills.keys());
    let mut skills: Vec<SkillTrend> = names
        .into_iter()
        .map(|name| {
            let (b, a) = (before.skills.get(name), after.skills.get(name));
            let status = match (b, a) {
                (Some(_), Some(_)) => SkillStatus::Both,
                (None, _) => SkillStatus::New,
                (_, None) => SkillStatus::Gone,
            };
            let measured = overhead_measured(b) && overhead_measured(a);
            SkillTrend {
                name: name.clone(),
                status,
                loads: c(
                    b.map_or(0.0, |s| f64::from(s.invocations)),
                    a.map_or(0.0, |s| f64::from(s.invocations)),
                ),
                overhead_tokens: basis.count(
                    b.map_or(0.0, |s| s.overhead_tokens() as f64),
                    a.map_or(0.0, |s| s.overhead_tokens() as f64),
                    measured,
                ),
            }
        })
        .collect();
    let size = |s: &SkillTrend| {
        let o = &s.overhead_tokens;
        (o.after.unwrap_or(0.0) - o.before.unwrap_or(0.0)).abs()
    };
    skills.sort_by(|x, y| {
        size(y)
            .total_cmp(&size(x))
            .then_with(|| x.name.cmp(&y.name))
    });

    TrendReport {
        windows,
        volume: VolumeTrend {
            sessions: c(before.sessions, after.sessions),
            requests: c(before.requests, after.requests),
            subagent_requests: c(before.subagent_requests, after.subagent_requests),
            tokens: basis.count(before.tokens, after.tokens, tokens_measured),
            context_tokens: basis.count(
                before.context_tokens,
                after.context_tokens,
                tokens_measured,
            ),
            large_context_requests: basis.count(
                before.large_context_requests,
                after.large_context_requests,
                tokens_measured,
            ),
        },
        cost: CostTrend {
            measured_usd: c(before.measured_usd, after.measured_usd),
            estimated_usd: basis.count(before.estimated_usd, after.estimated_usd, false),
            unpriced_requests: c(before.unpriced, after.unpriced),
        },
        efficiency: EfficiencyTrend {
            cache_hit_rate: basis.rate(before.hit_rate, after.hit_rate, true),
            subagent_share: basis.rate(before.subagent_share, after.subagent_share, true),
        },
        errors: ErrorTrend {
            tool_calls: c(before.tool_calls, after.tool_calls),
            failed_calls: c(before.failed_calls, after.failed_calls),
            retry_loops: c(before.retry_loops, after.retry_loops),
        },
        skills,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Source;
    use crate::model::{
        Event, EventKind, Session, SkillInvoked, SkillTrigger, ToolCall, ToolStatus, Usage,
    };
    use crate::tokens::CharsPerToken;

    fn ts(s: &str) -> Timestamp {
        s.parse().unwrap()
    }

    fn now() -> Timestamp {
        ts("2026-10-31T00:00:00Z")
    }

    /// Ten days: 2026-10-21..2026-10-31 against 2026-10-11..2026-10-21.
    fn equal() -> Windows {
        let mut w = plan(Some(ts("2026-10-21T00:00:00Z")), None, now(), 30, None).unwrap();
        w.current.sessions = 1;
        w.against.sessions = 1;
        w
    }

    fn usage(input: u64, measured: bool, cost: Option<f64>) -> EventKind {
        EventKind::Usage(Usage {
            model: Some("claude-sonnet-4-5".into()),
            input,
            output: 10,
            cost_usd: cost,
            measured,
            ..Usage::default()
        })
    }

    fn session(id: &str, end: &str, kinds: Vec<EventKind>) -> SessionEvents {
        let mut s = Session::new(id, Source::ClaudeCode);
        s.ended_at = Some(ts(end));
        SessionEvents {
            session: s,
            events: kinds
                .into_iter()
                .map(|kind| Event { at: None, kind })
                .collect(),
            warnings: Vec::new(),
        }
    }

    fn load(name: &str, bytes: u64) -> EventKind {
        let mut s = SkillInvoked::new(name, SkillTrigger::Model);
        s.content_bytes = Some(bytes);
        EventKind::SkillInvoked(s)
    }

    fn run(windows: Windows, current: &[SessionEvents], against: &[SessionEvents]) -> TrendReport {
        analyze(
            windows,
            current,
            against,
            &Inventory::default(),
            &PriceTable::bundled(),
            &CharsPerToken,
        )
    }

    #[test]
    fn two_equal_windows_compare_absolute_values() {
        let w = equal();
        assert!(!w.normalized);
        let now = [session(
            "n",
            "2026-10-25T00:00:00Z",
            vec![usage(300, true, Some(3.0)); 1],
        )];
        let old = [session(
            "o",
            "2026-10-15T00:00:00Z",
            vec![usage(100, true, Some(1.0)); 1],
        )];
        let r = run(w, &now, &old);
        let req = &r.volume.requests;
        assert_eq!(
            (req.before, req.after, req.change),
            (Some(1.0), Some(1.0), Some(0.0))
        );
        let tokens = &r.volume.tokens;
        assert_eq!((tokens.before, tokens.after), (Some(110.0), Some(310.0)));
        assert_eq!(tokens.change, Some(200.0));
        assert!((tokens.change_pct.unwrap() - 181.818_181).abs() < 1e-3);
        assert!(tokens.measured);
        let usd = &r.cost.measured_usd;
        assert_eq!(
            (usd.before, usd.after, usd.change),
            (Some(1.0), Some(3.0), Some(2.0))
        );
        assert_eq!(usd.change_pct, Some(200.0));
    }

    #[test]
    fn the_boundary_session_belongs_to_the_current_window() {
        // The split is the caller's: the rule is the one of the filter, [since, until).
        let w = equal();
        let f = crate::adapter::SessionFilter {
            since: Some(w.current.since),
            until: Some(w.current.until),
            ..Default::default()
        };
        let at_boundary = session("b", "2026-10-21T00:00:00Z", vec![]);
        let at_until = session("u", "2026-10-31T00:00:00Z", vec![]);
        assert!(f.matches(&at_boundary.session));
        assert!(!f.matches(&at_until.session));
        let r = run(w, &[at_boundary], &[]);
        assert_eq!(r.volume.sessions.after, Some(1.0));
        assert_eq!(r.volume.sessions.before, Some(0.0));
    }

    #[test]
    fn an_empty_window_gives_no_change() {
        let mut w = equal();
        w.against.sessions = 0;
        let now = [session(
            "n",
            "2026-10-25T00:00:00Z",
            vec![usage(300, true, None)],
        )];
        let r = run(w, &now, &[]);
        let req = &r.volume.requests;
        assert_eq!((req.before, req.after), (Some(0.0), Some(1.0)));
        assert_eq!((req.change, req.change_pct), (None, None));
        assert_eq!(r.volume.sessions.change, None);
        assert_eq!(r.efficiency.cache_hit_rate.change, None);
    }

    #[test]
    fn a_zero_base_has_no_percentage() {
        let w = equal();
        let now = [session(
            "n",
            "2026-10-25T00:00:00Z",
            vec![usage(300, true, Some(2.0))],
        )];
        let old = [session("o", "2026-10-15T00:00:00Z", vec![])];
        let r = run(w, &now, &old);
        let usd = &r.cost.measured_usd;
        assert_eq!(usd.before, Some(0.0));
        assert_eq!(usd.change, Some(2.0));
        assert_eq!(usd.change_pct, None);
    }

    #[test]
    fn windows_of_different_length_compare_per_day() {
        // 10 days against the 5 days right before.
        let mut w = plan(
            Some(ts("2026-10-21T00:00:00Z")),
            None,
            now(),
            30,
            Some((ts("2026-10-16T00:00:00Z"), ts("2026-10-21T00:00:00Z"))),
        )
        .unwrap();
        assert!(w.normalized);
        w.current.sessions = 1;
        w.against.sessions = 1;
        let kinds = |n: usize| vec![usage(100, true, None); n];
        let now = [session("n", "2026-10-25T00:00:00Z", kinds(20))];
        let old = [session("o", "2026-10-18T00:00:00Z", kinds(20))];
        let r = run(w, &now, &old);
        let req = &r.volume.requests;
        // 20 requests in 5 days is 4 a day; 20 in 10 days is 2 a day.
        assert_eq!((req.before, req.after), (Some(4.0), Some(2.0)));
        assert_eq!(req.change_pct, Some(-50.0));
    }

    #[test]
    fn rates_are_not_normalized() {
        let mut w = plan(
            Some(ts("2026-10-21T00:00:00Z")),
            None,
            now(),
            30,
            Some((ts("2026-10-16T00:00:00Z"), ts("2026-10-21T00:00:00Z"))),
        )
        .unwrap();
        w.current.sessions = 1;
        w.against.sessions = 1;
        let cached = |read| {
            EventKind::Usage(Usage {
                input: 100,
                cache_read: read,
                measured: true,
                ..Usage::default()
            })
        };
        let now = [session("n", "2026-10-25T00:00:00Z", vec![cached(300)])];
        let old = [session("o", "2026-10-18T00:00:00Z", vec![cached(100)])];
        let r = run(w, &now, &old);
        let rate = &r.efficiency.cache_hit_rate;
        assert_eq!((rate.before, rate.after), (Some(0.5), Some(0.75)));
    }

    #[test]
    fn a_skill_in_one_window_only_is_new_or_gone() {
        let w = equal();
        let now = [session(
            "n",
            "2026-10-25T00:00:00Z",
            vec![load("fresh", 4000), load("both", 400)],
        )];
        let old = [session(
            "o",
            "2026-10-15T00:00:00Z",
            vec![load("stale", 400), load("both", 400)],
        )];
        let r = run(w, &now, &old);
        let got: Vec<(&str, SkillStatus)> = r
            .skills
            .iter()
            .map(|s| (s.name.as_str(), s.status))
            .collect();
        // fresh: +1000 tokens, stale: -100, both: 0.
        assert_eq!(
            got,
            vec![
                ("fresh", SkillStatus::New),
                ("stale", SkillStatus::Gone),
                ("both", SkillStatus::Both)
            ]
        );
        assert_eq!(r.skills[0].loads.before, Some(0.0));
        assert_eq!(r.skills[0].loads.after, Some(1.0));
        assert_eq!(r.skills[0].overhead_tokens.change, Some(1000.0));
    }

    #[test]
    fn figures_come_in_groups_then_two_rows_per_skill() {
        let now = [session(
            "n",
            "2026-10-25T00:00:00Z",
            vec![load("fresh", 400)],
        )];
        let r = run(equal(), &now, &[]);
        let rows = r.figures();
        let groups: Vec<&str> = rows.iter().map(|f| f.group).collect();
        assert_eq!(groups[0], "volume");
        assert_eq!(groups.iter().filter(|g| **g == "cost").count(), 3);
        assert_eq!(&groups[groups.len() - 2..], ["skills", "skills"]);
        assert_eq!(rows.len(), 14 + 2);
        let last = rows.last().unwrap();
        assert_eq!(
            (last.figure, last.skill.unwrap().name.as_str()),
            ("overhead_tokens", "fresh")
        );
    }

    #[test]
    fn the_csv_has_a_row_per_figure_and_empty_cells_for_missing_values() {
        let now = [session(
            "n",
            "2026-10-25T00:00:00Z",
            vec![load("fresh", 400)],
        )];
        let mut w = equal();
        w.against.sessions = 0;
        let r = run(w, &now, &[]);
        let csv = crate::export::csv::trend(&r).unwrap();
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(
            lines[0],
            "group,figure,skill,status,before,after,change,change_pct,measured"
        );
        assert_eq!(lines.len(), 1 + 16);
        // No sessions before: no change, and the cache hit rate has no value at all.
        assert_eq!(lines[1], "volume,sessions,,,0.000000,1.000000,,,true");
        assert!(
            lines
                .iter()
                .any(|l| l.starts_with("skills,loads,fresh,new,0.000000,1.000000,,")),
            "{csv}"
        );
        assert!(
            lines
                .iter()
                .any(|l| l.starts_with("efficiency,cache_hit_rate,,,,")),
            "{csv}"
        );
    }

    #[test]
    fn skills_with_the_same_change_are_sorted_by_name() {
        let w = equal();
        let now = [session(
            "n",
            "2026-10-25T00:00:00Z",
            vec![load("b", 400), load("a", 400)],
        )];
        let r = run(w, &now, &[session("o", "2026-10-15T00:00:00Z", vec![])]);
        let names: Vec<&str> = r.skills.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["a", "b"]);
    }

    #[test]
    fn an_estimated_input_makes_the_delta_estimated() {
        let w = equal();
        let now = [session(
            "n",
            "2026-10-25T00:00:00Z",
            vec![usage(300, false, None)],
        )];
        let old = [session(
            "o",
            "2026-10-15T00:00:00Z",
            vec![usage(100, true, None)],
        )];
        let r = run(w, &now, &old);
        assert!(!r.volume.tokens.measured);
        assert!(!r.volume.context_tokens.measured);
        assert!(r.volume.requests.measured);
        // The price table's cost is an estimate, the reported one is not.
        assert!(!r.cost.estimated_usd.measured);
        assert!(r.cost.measured_usd.measured);
    }

    #[test]
    fn failed_calls_and_loops_are_counted_per_window() {
        let call = |status| {
            EventKind::ToolCall(ToolCall {
                name: "Bash".into(),
                input_summary: Some("make".into()),
                status,
                duration_ms: None,
            })
        };
        let w = equal();
        let now = [session(
            "n",
            "2026-10-25T00:00:00Z",
            vec![
                call(ToolStatus::Error),
                call(ToolStatus::Error),
                call(ToolStatus::Ok),
            ],
        )];
        let old = [session(
            "o",
            "2026-10-15T00:00:00Z",
            vec![call(ToolStatus::Ok)],
        )];
        let r = run(w, &now, &old);
        assert_eq!(r.errors.tool_calls.after, Some(3.0));
        assert_eq!(r.errors.failed_calls.before, Some(0.0));
        assert_eq!(r.errors.failed_calls.after, Some(2.0));
        assert_eq!(r.errors.retry_loops.after, Some(1.0));
    }

    #[test]
    fn an_overlapping_comparison_window_is_an_error() {
        let err = plan(
            Some(ts("2026-10-21T00:00:00Z")),
            None,
            now(),
            30,
            Some((ts("2026-10-15T00:00:00Z"), ts("2026-10-22T00:00:00Z"))),
        )
        .unwrap_err();
        assert!(err.contains("overlaps"), "{err}");
        let err = plan(
            Some(ts("2026-10-21T00:00:00Z")),
            None,
            now(),
            30,
            Some((ts("2026-10-15T00:00:00Z"), ts("2026-10-15T00:00:00Z"))),
        )
        .unwrap_err();
        assert!(err.contains("empty"), "{err}");
    }

    #[test]
    fn a_window_that_ends_where_the_other_starts_does_not_overlap() {
        let w = plan(
            Some(ts("2026-10-21T00:00:00Z")),
            None,
            now(),
            30,
            Some((ts("2026-11-05T00:00:00Z"), ts("2026-11-10T00:00:00Z"))),
        );
        assert!(w.is_ok());
    }

    #[test]
    fn the_default_windows_are_thirty_days_and_the_thirty_before() {
        let w = plan(None, None, now(), 30, None).unwrap();
        assert_eq!(w.current.until, now());
        assert_eq!(w.current.since, ts("2026-10-01T00:00:00Z"));
        assert_eq!(w.against.since, ts("2026-09-01T00:00:00Z"));
        assert_eq!(w.against.until, ts("2026-10-01T00:00:00Z"));
        assert!(!w.normalized);
    }
}
