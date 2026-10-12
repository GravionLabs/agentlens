//! Skill usage and what it costs.
//!
//! Four kinds of cost per skill:
//!
//! - **baseline**: name and description are part of every request of the
//!   clients that load the skill, used or not (estimated from the inventory);
//! - **load**: the skill content entering the context. Where the client reports
//!   usage, this is the growth of the request size across the load (an upper
//!   bound: other content added in the same step is included), otherwise it is
//!   estimated from the content size;
//! - **carry**: the loaded content stays in the context and is sent again with
//!   every later request until the context is compacted (estimated: load size ×
//!   later requests, split into cache read and uncached by each request's
//!   cache share);
//! - **follow-on**: what the requests after the load cost until the next user
//!   turn, the next skill or a compaction. The usage numbers are measured, but
//!   attributing them to the skill is an assumption.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use jiff::Timestamp;
use serde::{Deserialize, Serialize};

use crate::inventory::{Inventory, SkillEntry};
use crate::model::{EventKind, SessionEvents, SkillInvoked, SkillTrigger, Source, Usage};
use crate::tokens::{TokenEstimator, Tokens};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TriggerCounts {
    pub model: u32,
    pub user: u32,
    pub nested: u32,
}

/// Sum of the usage of several requests.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageSum {
    pub requests: u64,
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
}

impl UsageSum {
    fn add(&mut self, u: &Usage) {
        self.requests += 1;
        self.input += u.input;
        self.output += u.output;
        self.cache_read += u.cache_read;
        self.cache_write += u.cache_write;
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CarryCost {
    /// Later requests that carried the skill content.
    pub requests: u64,
    /// Skill content tokens × requests. Always an estimate: it is derived, not reported by a client.
    pub tokens: Tokens,
    /// The part of `tokens` served from the cache.
    pub cache_read_tokens: u64,
    pub uncached_tokens: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BaselineCost {
    /// Requests of the clients (and projects) that list the skill.
    pub requests: u64,
    /// Listing tokens × requests, estimated.
    pub tokens: Tokens,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillStats {
    pub name: String,
    pub in_inventory: bool,
    /// Clients that load the skill according to the inventory.
    pub clients: Vec<Source>,
    pub invocations: u32,
    /// Tool calls that read or searched the files of the skill, or ran a script from its folder, without
    /// loading it. A skill used only like this is not unused. Counted for installed skills only, and not
    /// in a session that edits the skill (that is working on it).
    #[serde(default)]
    pub file_uses: u32,
    /// Sessions in which the skill was loaded.
    pub sessions: u32,
    pub projects: Vec<PathBuf>,
    pub first_used: Option<Timestamp>,
    pub last_used: Option<Timestamp>,
    pub triggers: TriggerCounts,
    pub by_source: BTreeMap<Source, u32>,
    /// Invocations per UTC day (`YYYY-MM-DD`).
    #[serde(default)]
    pub uses_by_day: BTreeMap<String, u32>,
    /// Sum of the load cost; `measured` only if every load was measured.
    pub load: Tokens,
    /// Invocations whose load size could not be determined (counted as 0).
    pub unknown_loads: u32,
    pub carry: CarryCost,
    pub follow_on: UsageSum,
    /// Usage the client itself attributes to the skill (OpenTelemetry
    /// `skill.name`): measured, unlike the follow-on attribution.
    #[serde(default)]
    pub attributed: UsageSum,
    pub baseline: BaselineCost,
}

impl SkillStats {
    /// Tokens the skill adds on top of the work itself: baseline + load + carry.
    pub fn overhead_tokens(&self) -> u64 {
        self.baseline.tokens.count + self.load.count + self.carry.tokens.count
    }
}

/// A user-typed command that is not an installed skill (e.g. a custom command).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IgnoredCommand {
    pub name: String,
    pub count: u32,
}

/// A request with at least this many input tokens (cached or not) counts as running on a large context.
pub const LARGE_CONTEXT_TOKENS: u64 = 200_000;
/// Sessions kept in `PeriodTotals::largest_sessions`.
pub const LARGEST_SESSIONS: usize = 5;

/// One session in `PeriodTotals::largest_sessions`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionContext {
    pub id: String,
    pub project: Option<PathBuf>,
    pub requests: u64,
    /// The average input size (cached or not) of its requests.
    pub average_context: u64,
}

/// What all requests of the selected sessions amount to: the scale to put findings against.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PeriodTotals {
    pub requests: u64,
    /// Requests made by subagents.
    pub subagent_requests: u64,
    /// Input, output and cache tokens of all requests.
    pub tokens: u64,
    /// The input size (cached or not) of all requests added up.
    pub context_tokens: u64,
    /// Requests with at least `LARGE_CONTEXT_TOKENS` of input.
    pub large_context_requests: u64,
    /// `false` if any request's numbers were estimated rather than reported.
    pub measured: bool,
    /// The sessions with the largest average context, largest first.
    pub largest_sessions: Vec<SessionContext>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SkillReport {
    #[serde(default)]
    pub totals: PeriodTotals,
    pub sessions: u32,
    pub first_event: Option<Timestamp>,
    pub last_event: Option<Timestamp>,
    pub requests_by_source: BTreeMap<Source, u64>,
    /// Sorted by name.
    pub skills: Vec<SkillStats>,
    pub ignored_commands: Vec<IgnoredCommand>,
}

struct Acc {
    stats: SkillStats,
    sessions: BTreeSet<String>,
    projects: BTreeSet<PathBuf>,
    all_loads_measured: bool,
}

pub fn analyze(
    inventory: &Inventory,
    sessions: &[SessionEvents],
    estimator: &dyn TokenEstimator,
) -> SkillReport {
    let mut report = SkillReport::default();
    let mut accs: BTreeMap<String, Acc> = BTreeMap::new();
    let mut ignored: BTreeMap<String, u32> = BTreeMap::new();

    for entry in &inventory.skills {
        let acc = accs
            .entry(entry.name.clone())
            .or_insert_with(|| Acc::new(&entry.name, true, Vec::new()));
        for c in &entry.clients {
            if !acc.stats.clients.contains(c) {
                acc.stats.clients.push(*c);
            }
        }
        acc.stats.clients.sort();
    }

    let mut all_measured = true;
    let mut session_contexts: Vec<SessionContext> = Vec::new();
    for se in sessions {
        report.sessions += 1;
        let (mut session_requests, mut session_context) = (0u64, 0u64);
        for e in &se.events {
            if let EventKind::Usage(u) = &e.kind {
                let totals = &mut report.totals;
                totals.requests += 1;
                totals.subagent_requests += u64::from(u.subagent);
                totals.tokens = totals.tokens.saturating_add(u.total_input() + u.output);
                totals.context_tokens = totals.context_tokens.saturating_add(u.total_input());
                totals.large_context_requests += u64::from(u.total_input() >= LARGE_CONTEXT_TOKENS);
                all_measured &= u.measured;
                session_requests += 1;
                session_context += u.total_input();
            }
        }
        if let Some(average) = session_context.checked_div(session_requests) {
            session_contexts.push(SessionContext {
                id: se.session.id.clone(),
                project: se.session.project_dir.clone(),
                requests: session_requests,
                average_context: average,
            });
        }
        let requests = se
            .events
            .iter()
            .filter(|e| matches!(e.kind, EventKind::Usage(_)))
            .count() as u64;
        *report
            .requests_by_source
            .entry(se.session.source)
            .or_default() += requests;
        for e in &se.events {
            if let Some(at) = e.at {
                report.first_event = Some(report.first_event.map_or(at, |f| f.min(at)));
                report.last_event = Some(report.last_event.map_or(at, |l| l.max(at)));
            }
        }

        for (i, e) in se.events.iter().enumerate() {
            let EventKind::SkillInvoked(skill) = &e.kind else {
                continue;
            };
            let known = inventory.skills.iter().any(|s| s.name == skill.name);
            if skill.trigger == SkillTrigger::User && !known {
                *ignored.entry(skill.name.clone()).or_default() += 1;
                continue;
            }
            let acc = accs
                .entry(skill.name.clone())
                .or_insert_with(|| Acc::new(&skill.name, false, Vec::new()));
            acc.record(se, i, e.at, skill, inventory, estimator);
        }

        // Tool calls that read or run files in the folder of an installed skill: use without loading.
        // A session that also edits the skill is working on it, and what it reads or runs there is
        // part of that work.
        let edited: BTreeSet<&str> = se
            .events
            .iter()
            .filter_map(|e| match &e.kind {
                EventKind::SkillFileEdit(name) => Some(name.as_str()),
                _ => None,
            })
            .collect();
        for e in &se.events {
            if let EventKind::SkillFileUse(name) = &e.kind
                && !edited.contains(name.as_str())
                && let Some(acc) = accs.get_mut(name)
                && acc.stats.in_inventory
            {
                acc.stats.file_uses += 1;
            }
        }

        for e in &se.events {
            if let EventKind::Usage(u) = &e.kind
                && let Some(name) = &u.skill
            {
                accs.entry(name.clone())
                    .or_insert_with(|| Acc::new(name, false, Vec::new()))
                    .stats
                    .attributed
                    .add(u);
            }
        }
    }

    // Several files may share a name; the skill is listed wherever any copy is visible.
    for (name, acc) in accs.iter_mut() {
        let entries: Vec<&SkillEntry> = inventory
            .skills
            .iter()
            .filter(|s| &s.name == name)
            .collect();
        acc.stats.baseline = baseline(&entries, sessions);
    }

    report.totals.measured = report.totals.requests > 0 && all_measured;
    session_contexts.sort_by(|a, b| {
        b.average_context
            .cmp(&a.average_context)
            .then_with(|| a.id.cmp(&b.id))
    });
    session_contexts.truncate(LARGEST_SESSIONS);
    report.totals.largest_sessions = session_contexts;
    report.skills = accs.into_values().map(Acc::finish).collect();
    report.ignored_commands = ignored
        .into_iter()
        .map(|(name, count)| IgnoredCommand { name, count })
        .collect();
    report
}

impl Acc {
    fn new(name: &str, in_inventory: bool, clients: Vec<Source>) -> Acc {
        Acc {
            stats: SkillStats {
                name: name.to_owned(),
                in_inventory,
                clients,
                invocations: 0,
                file_uses: 0,
                sessions: 0,
                projects: Vec::new(),
                first_used: None,
                last_used: None,
                triggers: TriggerCounts::default(),
                by_source: BTreeMap::new(),
                uses_by_day: BTreeMap::new(),
                load: Tokens::measured(0),
                unknown_loads: 0,
                carry: CarryCost::default(),
                follow_on: UsageSum::default(),
                attributed: UsageSum::default(),
                baseline: BaselineCost {
                    requests: 0,
                    tokens: Tokens::estimated(0),
                },
            },
            sessions: BTreeSet::new(),
            projects: BTreeSet::new(),
            all_loads_measured: true,
        }
    }

    fn record(
        &mut self,
        se: &SessionEvents,
        i: usize,
        at: Option<Timestamp>,
        skill: &SkillInvoked,
        inventory: &Inventory,
        estimator: &dyn TokenEstimator,
    ) {
        let s = &mut self.stats;
        s.invocations += 1;
        match skill.trigger {
            SkillTrigger::Model => s.triggers.model += 1,
            SkillTrigger::User => s.triggers.user += 1,
            SkillTrigger::Nested => s.triggers.nested += 1,
        }
        *s.by_source.entry(se.session.source).or_default() += 1;
        self.sessions
            .insert(format!("{}:{}", se.session.source.id(), se.session.id));
        if let Some(dir) = &se.session.project_dir {
            self.projects.insert(dir.clone());
        }
        if let Some(at) = at {
            s.first_used = Some(s.first_used.map_or(at, |f| f.min(at)));
            s.last_used = Some(s.last_used.map_or(at, |l| l.max(at)));
            let day = at.to_zoned(jiff::tz::TimeZone::UTC).date().to_string();
            *s.uses_by_day.entry(day).or_default() += 1;
        }

        let window_end = se.events[i + 1..]
            .iter()
            .position(|e| matches!(e.kind, EventKind::Compaction))
            .map_or(se.events.len(), |p| i + 1 + p);
        let main_usage = |range: std::ops::Range<usize>| {
            se.events[range].iter().filter_map(|e| match &e.kind {
                EventKind::Usage(u) if !u.subagent => Some(u),
                _ => None,
            })
        };
        let window_start = se.events[..i]
            .iter()
            .rposition(|e| matches!(e.kind, EventKind::Compaction))
            .map_or(0, |p| p + 1);
        let before = main_usage(window_start..i).next_back();
        let mut after = main_usage(i + 1..window_end);
        let first_after = after.next();

        // Load: measured growth of the request size, else estimated from size.
        let measured = match (before, first_after) {
            (Some(b), Some(a)) => {
                Some(a.total_input().saturating_sub(b.total_input())).filter(|d| *d > 0)
            }
            _ => None,
        };
        let load = match (
            measured,
            skill.content_bytes,
            inventory.find(&skill.name).first(),
        ) {
            (Some(d), _, _) => Tokens::measured(d),
            (None, Some(bytes), _) => estimator.count_bytes(bytes),
            (None, None, Some(entry)) => entry.body_tokens,
            (None, None, None) => {
                s.unknown_loads += 1;
                Tokens::estimated(0)
            }
        };
        s.load.count += load.count;
        self.all_loads_measured &= load.measured;

        // Carry: every request after the first one that saw the content.
        for u in after {
            let total = u.total_input();
            let cached = if total == 0 {
                0
            } else {
                (load.count as u128 * u.cache_read as u128 / total as u128) as u64
            };
            s.carry.requests += 1;
            s.carry.tokens.count += load.count;
            s.carry.cache_read_tokens += cached;
            s.carry.uncached_tokens += load.count - cached;
        }

        // Follow-on: until the next turn, skill or compaction.
        let end = se.events[i + 1..]
            .iter()
            .position(|e| {
                matches!(
                    e.kind,
                    EventKind::UserTurn | EventKind::SkillInvoked(_) | EventKind::Compaction
                )
            })
            .map_or(se.events.len(), |p| i + 1 + p);
        for e in &se.events[i + 1..end] {
            if let EventKind::Usage(u) = &e.kind {
                s.follow_on.add(u);
            }
        }
    }

    fn finish(mut self) -> SkillStats {
        self.stats.sessions = self.sessions.len() as u32;
        self.stats.projects = self.projects.into_iter().collect();
        self.stats.load.measured = self.all_loads_measured;
        self.stats
    }
}

/// Listing tokens × the requests of every session in which the skill is listed.
/// Per session the first visible copy (sorted by file) decides the listing size.
/// The result is measured only if every listing size that went into it was.
fn baseline(entries: &[&SkillEntry], sessions: &[SessionEvents]) -> BaselineCost {
    let (mut requests, mut tokens) = (0u64, 0u64);
    let (mut counted, mut all_measured) = (false, true);
    for se in sessions {
        let visible = entries.iter().find(|entry| {
            entry.locations.iter().any(|loc| {
                loc.kind.clients().contains(&se.session.source)
                    && loc.project.as_deref().is_none_or(|root| {
                        se.session
                            .project_dir
                            .as_deref()
                            .is_some_and(|d| d.starts_with(root))
                    })
            })
        });
        if let Some(entry) = visible {
            let n = se
                .events
                .iter()
                .filter(|e| matches!(e.kind, EventKind::Usage(_)))
                .count() as u64;
            requests += n;
            tokens += entry.baseline_tokens.count * n;
            counted = true;
            all_measured &= entry.baseline_tokens.measured;
        }
    }
    BaselineCost {
        requests,
        tokens: Tokens {
            count: tokens,
            measured: counted && all_measured,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::{Location, LocationKind, SkillEntry};
    use crate::model::{Event, Session};
    use crate::tokens::CharsPerToken;

    fn usage(
        input: u64,
        output: u64,
        cache_read: u64,
        cache_write: u64,
        subagent: bool,
    ) -> EventKind {
        EventKind::Usage(Usage {
            input,
            output,
            cache_read,
            cache_write,
            subagent,
            ..Usage::default()
        })
    }

    fn skill(name: &str, trigger: SkillTrigger, bytes: Option<u64>) -> EventKind {
        let mut s = SkillInvoked::new(name, trigger);
        s.content_bytes = bytes;
        EventKind::SkillInvoked(s)
    }

    fn session(id: &str, source: Source, project: &str, kinds: Vec<EventKind>) -> SessionEvents {
        let mut s = Session::new(id, source);
        s.project_dir = Some(project.into());
        let events = kinds
            .into_iter()
            .enumerate()
            .map(|(i, kind)| Event {
                at: Some(Timestamp::from_second(1_790_000_000 + i as i64).unwrap()),
                kind,
            })
            .collect();
        SessionEvents {
            session: s,
            events,
            warnings: Vec::new(),
        }
    }

    fn entry(name: &str, baseline: u64, body: u64, kind: LocationKind, dir: &str) -> SkillEntry {
        let project = matches!(
            kind,
            LocationKind::ProjectGithub | LocationKind::ProjectClaude
        )
        .then(|| {
            std::path::Path::new(dir)
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .to_path_buf()
        });
        let loc = Location {
            kind,
            dir: dir.into(),
            project,
        };
        SkillEntry {
            name: name.into(),
            description: None,
            file: format!("{dir}/{name}/SKILL.md").into(),
            clients: kind.clients().to_vec(),
            locations: vec![loc],
            baseline_tokens: Tokens::estimated(baseline),
            body_tokens: Tokens::estimated(body),
            body_bytes: body * 4,
        }
    }

    fn inventory(skills: Vec<SkillEntry>) -> Inventory {
        Inventory {
            skills,
            ..Inventory::default()
        }
    }

    fn find<'a>(r: &'a SkillReport, name: &str) -> &'a SkillStats {
        r.skills.iter().find(|s| s.name == name).unwrap()
    }

    /// One skill load with known numbers, computed by hand:
    ///
    /// load = 2410 - 1010 = 1400 (measured)
    /// carry = requests U3 and U5 (the subagent request U4 is not part of the main context):
    ///   cache_read = 1400*2400/2405 + 1400*2000/2001 = 1397 + 1399 = 2796
    /// follow-on = U2, U3, U4 (up to the next user turn)
    fn measured_session() -> SessionEvents {
        session(
            "s1",
            Source::ClaudeCode,
            "/work/demo",
            vec![
                EventKind::UserTurn,
                usage(10, 0, 1000, 0, false),
                skill("demo", SkillTrigger::Model, Some(4000)),
                usage(10, 100, 1900, 500, false),
                usage(5, 50, 2400, 0, false),
                usage(7, 3, 0, 0, true),
                EventKind::UserTurn,
                usage(1, 0, 2000, 0, false),
            ],
        )
    }

    #[test]
    fn load_is_the_measured_growth_of_the_request_size() {
        let r = analyze(&inventory(vec![]), &[measured_session()], &CharsPerToken);
        let s = find(&r, "demo");
        assert_eq!(s.load, Tokens::measured(1400));
        assert_eq!(s.unknown_loads, 0);
    }

    #[test]
    fn files_used_without_loading_count_for_installed_skills_only() {
        let inv = inventory(vec![
            entry(
                "tool",
                10,
                100,
                LocationKind::UserClaude,
                "/home/u/.claude/skills",
            ),
            entry(
                "idle",
                10,
                100,
                LocationKind::UserClaude,
                "/home/u/.claude/skills",
            ),
        ]);
        let se = session(
            "s",
            Source::ClaudeCode,
            "/work/demo",
            vec![
                EventKind::SkillFileUse("tool".into()),
                EventKind::SkillFileUse("tool".into()),
                // A folder called `skills` in a project, not an installed skill.
                EventKind::SkillFileUse("features".into()),
                usage(1, 1, 0, 0, false),
            ],
        );
        let r = analyze(&inv, &[se], &CharsPerToken);
        assert_eq!(find(&r, "tool").file_uses, 2);
        assert_eq!(find(&r, "tool").invocations, 0, "it was never loaded");
        assert_eq!(find(&r, "idle").file_uses, 0);
        assert!(r.skills.iter().all(|s| s.name != "features"));
    }

    #[test]
    fn a_session_that_edits_a_skill_does_not_use_it_through_its_files() {
        let inv = inventory(vec![
            entry(
                "tool",
                10,
                100,
                LocationKind::UserClaude,
                "/home/u/.claude/skills",
            ),
            entry(
                "other",
                10,
                100,
                LocationKind::UserClaude,
                "/home/u/.claude/skills",
            ),
        ]);
        let using = session(
            "using",
            Source::ClaudeCode,
            "/work/demo",
            vec![
                EventKind::SkillFileUse("tool".into()),
                usage(1, 1, 0, 0, false),
            ],
        );
        // This session reads `tool` and `other`, but it also edits `tool`.
        let working = session(
            "working",
            Source::ClaudeCode,
            "/work/demo",
            vec![
                EventKind::SkillFileUse("tool".into()),
                EventKind::SkillFileUse("other".into()),
                EventKind::SkillFileEdit("tool".into()),
                usage(1, 1, 0, 0, false),
            ],
        );
        let r = analyze(&inv, &[using, working], &CharsPerToken);
        assert_eq!(
            find(&r, "tool").file_uses,
            1,
            "only the session that does not edit it"
        );
        assert_eq!(find(&r, "other").file_uses, 1);
    }

    #[test]
    fn carry_counts_later_main_requests_until_the_end_of_the_window() {
        let r = analyze(&inventory(vec![]), &[measured_session()], &CharsPerToken);
        let c = &find(&r, "demo").carry;
        assert_eq!(c.requests, 2);
        assert_eq!(c.tokens, Tokens::estimated(2800));
        assert_eq!(c.cache_read_tokens, 2796);
        assert_eq!(c.uncached_tokens, 4);
    }

    #[test]
    fn follow_on_stops_at_the_next_user_turn_and_includes_subagents() {
        let r = analyze(&inventory(vec![]), &[measured_session()], &CharsPerToken);
        let f = &find(&r, "demo").follow_on;
        assert_eq!(
            (f.requests, f.input, f.output, f.cache_read, f.cache_write),
            (3, 22, 153, 4300, 500)
        );
    }

    #[test]
    fn load_falls_back_to_content_size_then_inventory_body_size() {
        // No request before the load: no measured growth.
        let first = session(
            "a",
            Source::ClaudeCode,
            "/p",
            vec![
                skill("demo", SkillTrigger::Model, Some(4000)),
                usage(1, 1, 1, 1, false),
            ],
        );
        let r = analyze(&inventory(vec![]), &[first], &CharsPerToken);
        assert_eq!(find(&r, "demo").load, Tokens::estimated(1000));

        let no_size = session(
            "b",
            Source::ClaudeCode,
            "/p",
            vec![skill("demo", SkillTrigger::Model, None)],
        );
        let inv = inventory(vec![entry(
            "demo",
            7,
            321,
            LocationKind::UserClaude,
            "/h/.claude/skills",
        )]);
        let r = analyze(&inv, std::slice::from_ref(&no_size), &CharsPerToken);
        assert_eq!(find(&r, "demo").load, Tokens::estimated(321));

        let r = analyze(&inventory(vec![]), &[no_size], &CharsPerToken);
        assert_eq!(find(&r, "demo").unknown_loads, 1);
    }

    #[test]
    fn a_compaction_ends_the_context_window() {
        let s = session(
            "c",
            Source::ClaudeCode,
            "/p",
            vec![
                usage(10, 0, 1000, 0, false),
                skill("demo", SkillTrigger::Model, Some(400)),
                usage(10, 0, 1100, 0, false),
                usage(10, 0, 1200, 0, false),
                EventKind::Compaction,
                usage(10, 0, 100, 0, false),
                usage(10, 0, 100, 0, false),
            ],
        );
        let r = analyze(&inventory(vec![]), &[s], &CharsPerToken);
        let d = find(&r, "demo");
        assert_eq!(d.load, Tokens::measured(100));
        assert_eq!(
            d.carry.requests, 1,
            "only the request before the compaction carries the content"
        );
        assert_eq!(d.follow_on.requests, 2);
    }

    #[test]
    fn statistics_cover_triggers_sessions_projects_and_time() {
        let a = session(
            "a",
            Source::ClaudeCode,
            "/work/demo",
            vec![
                skill("demo", SkillTrigger::Model, None),
                skill("demo", SkillTrigger::User, None),
            ],
        );
        let b = session(
            "b",
            Source::OpenCode,
            "/work/other",
            vec![skill("demo", SkillTrigger::Model, None)],
        );
        let inv = inventory(vec![entry(
            "demo",
            7,
            10,
            LocationKind::UserClaude,
            "/h/.claude/skills",
        )]);
        let r = analyze(&inv, &[a, b], &CharsPerToken);
        let d = find(&r, "demo");
        assert_eq!((d.invocations, d.sessions), (3, 2));
        assert_eq!(
            (d.triggers.model, d.triggers.user, d.triggers.nested),
            (2, 1, 0)
        );
        assert_eq!(d.by_source[&Source::ClaudeCode], 2);
        assert_eq!(
            d.projects,
            [PathBuf::from("/work/demo"), PathBuf::from("/work/other")]
        );
        assert!(d.first_used.unwrap() < d.last_used.unwrap());
        assert!(d.in_inventory);
        assert_eq!(r.sessions, 2);
    }

    #[test]
    fn user_commands_that_are_no_installed_skill_are_ignored() {
        let s = session(
            "a",
            Source::ClaudeCode,
            "/p",
            vec![
                skill("my-command", SkillTrigger::User, None),
                skill("demo", SkillTrigger::User, None),
            ],
        );
        let inv = inventory(vec![entry(
            "demo",
            7,
            10,
            LocationKind::UserClaude,
            "/h/.claude/skills",
        )]);
        let r = analyze(&inv, &[s], &CharsPerToken);
        assert_eq!(
            r.ignored_commands,
            [IgnoredCommand {
                name: "my-command".into(),
                count: 1
            }]
        );
        assert_eq!(find(&r, "demo").invocations, 1);
        assert!(r.skills.iter().all(|s| s.name != "my-command"));
    }

    #[test]
    fn period_totals_count_requests_subagents_and_large_contexts() {
        let a = session(
            "a",
            Source::ClaudeCode,
            "/work/demo",
            vec![
                usage(100, 10, 199_000, 0, false),
                usage(100, 10, 100_000, 99_900, false),
                usage(50, 5, 0, 0, true),
                EventKind::UserTurn,
            ],
        );
        let r = analyze(&inventory(vec![]), std::slice::from_ref(&a), &CharsPerToken);
        let t = &r.totals;
        assert_eq!(t.requests, 3);
        assert_eq!(t.subagent_requests, 1);
        assert_eq!(t.context_tokens, 199_100 + 200_000 + 50);
        assert_eq!(t.tokens, t.context_tokens + 25);
        assert_eq!(
            t.large_context_requests, 1,
            "199,100 is below, 200,000 is at the limit"
        );
        assert_eq!(t.largest_sessions.len(), 1);
        assert_eq!(t.largest_sessions[0].requests, 3);
        assert_eq!(t.largest_sessions[0].average_context, 399_150 / 3);

        // No requests: not measured, no sessions listed.
        let empty = session(
            "e",
            Source::ClaudeCode,
            "/work/demo",
            vec![EventKind::UserTurn],
        );
        let r = analyze(&inventory(vec![]), &[empty], &CharsPerToken);
        assert_eq!(r.totals.requests, 0);
        assert!(!r.totals.measured);
        assert!(r.totals.largest_sessions.is_empty());
    }

    #[test]
    fn period_totals_are_measured_only_if_every_request_was() {
        let estimated = Usage {
            measured: false,
            ..Usage::default()
        };
        let measured = Usage {
            measured: true,
            ..Usage::default()
        };
        let one = |u: Usage| {
            session(
                "m",
                Source::ClaudeCode,
                "/work/demo",
                vec![EventKind::Usage(u)],
            )
        };
        let r = analyze(&inventory(vec![]), &[one(measured.clone())], &CharsPerToken);
        assert!(r.totals.measured);
        let r = analyze(
            &inventory(vec![]),
            &[one(measured), one(estimated)],
            &CharsPerToken,
        );
        assert!(!r.totals.measured);
    }

    #[test]
    fn largest_sessions_are_sorted_by_average_context_and_cut() {
        let sessions: Vec<SessionEvents> = (0..7u64)
            .map(|i| {
                session(
                    &format!("s{i}"),
                    Source::ClaudeCode,
                    "/work/demo",
                    vec![usage(1_000 * (i + 1), 0, 0, 0, false)],
                )
            })
            .collect();
        let r = analyze(&inventory(vec![]), &sessions, &CharsPerToken);
        let ids: Vec<&str> = r
            .totals
            .largest_sessions
            .iter()
            .map(|c| c.id.as_str())
            .collect();
        assert_eq!(ids, ["s6", "s5", "s4", "s3", "s2"]);

        // Equal averages: by id.
        let tie: Vec<SessionEvents> = ["b", "a"]
            .iter()
            .map(|id| {
                session(
                    id,
                    Source::ClaudeCode,
                    "/work/demo",
                    vec![usage(5, 0, 0, 0, false)],
                )
            })
            .collect();
        let r = analyze(&inventory(vec![]), &tie, &CharsPerToken);
        assert_eq!(r.totals.largest_sessions[0].id, "a");
    }

    #[test]
    fn baseline_counts_requests_of_the_clients_and_projects_that_list_the_skill() {
        let user = entry(
            "global",
            7,
            0,
            LocationKind::UserClaude,
            "/h/.claude/skills",
        );
        let project = entry(
            "local",
            10,
            0,
            LocationKind::ProjectGithub,
            "/work/demo/.github/skills",
        );
        let reqs = |n: usize| (0..n).map(|_| usage(1, 1, 1, 1, false)).collect::<Vec<_>>();
        let sessions = [
            session("c", Source::ClaudeCode, "/work/demo", reqs(5)),
            session("o", Source::OpenCode, "/work/demo", reqs(3)),
            session("p1", Source::CopilotCli, "/work/demo", reqs(4)),
            session("p2", Source::CopilotCli, "/work/other", reqs(6)),
        ];
        let r = analyze(&inventory(vec![user, project]), &sessions, &CharsPerToken);

        // Claude Code, OpenCode and both Copilot sessions all load ~/.claude/skills.
        let g = find(&r, "global");
        assert_eq!(g.baseline.requests, 18);
        assert_eq!(g.baseline.tokens, Tokens::estimated(7 * 18));
        // Only Copilot sessions inside /work/demo list the project skill.
        let l = find(&r, "local");
        assert_eq!(l.baseline.requests, 4);
        assert_eq!(l.baseline.tokens, Tokens::estimated(40));
        assert_eq!(
            l.invocations, 0,
            "unused skills are reported with their baseline cost"
        );
        assert_eq!(r.requests_by_source[&Source::CopilotCli], 10);
    }

    #[test]
    fn baseline_is_measured_only_if_the_listing_size_was() {
        let reqs = (0..3).map(|_| usage(1, 1, 1, 1, false)).collect::<Vec<_>>();
        let sessions = [session("c", Source::ClaudeCode, "/work/demo", reqs)];

        let mut exact = entry("e", 7, 0, LocationKind::UserClaude, "/h/.claude/skills");
        exact.baseline_tokens = Tokens::measured(7);
        let r = analyze(&inventory(vec![exact]), &sessions, &CharsPerToken);
        assert_eq!(find(&r, "e").baseline.tokens, Tokens::measured(21));

        let guessed = entry("g", 7, 0, LocationKind::UserClaude, "/h/.claude/skills");
        let r = analyze(&inventory(vec![guessed]), &sessions, &CharsPerToken);
        assert_eq!(find(&r, "g").baseline.tokens, Tokens::estimated(21));

        let unlisted = entry(
            "u",
            7,
            0,
            LocationKind::ProjectGithub,
            "/other/.github/skills",
        );
        let r = analyze(&inventory(vec![unlisted]), &sessions, &CharsPerToken);
        assert_eq!(
            find(&r, "u").baseline.tokens,
            Tokens::estimated(0),
            "nothing counted stays an estimate"
        );
    }
}

#[cfg(test)]
mod duplicate_tests {
    use super::*;
    use crate::inventory::{Location, LocationKind};
    use crate::model::{Event, Session};

    fn entry(file: &str, kind: LocationKind, project: Option<&str>, baseline: u64) -> SkillEntry {
        SkillEntry {
            name: "same".into(),
            description: None,
            file: file.into(),
            clients: kind.clients().to_vec(),
            locations: vec![Location {
                kind,
                dir: file.into(),
                project: project.map(Into::into),
            }],
            baseline_tokens: Tokens::estimated(baseline),
            body_tokens: Tokens::estimated(0),
            body_bytes: 0,
        }
    }

    fn session(source: Source, project: &str, requests: usize) -> SessionEvents {
        let mut s = Session::new(format!("{source:?}"), source);
        s.project_dir = Some(project.into());
        let events = (0..requests)
            .map(|_| Event {
                at: None,
                kind: EventKind::Usage(Usage::default()),
            })
            .collect();
        SessionEvents {
            session: s,
            events,
            warnings: Vec::new(),
        }
    }

    #[test]
    fn a_skill_with_copies_for_different_clients_is_listed_for_each_client() {
        // ~/.agents (not Claude Code) sorts before ~/.claude; both copies exist.
        let inv = Inventory {
            skills: vec![
                entry(
                    "/h/.agents/skills/same/SKILL.md",
                    LocationKind::UserAgents,
                    None,
                    10,
                ),
                entry(
                    "/h/.claude/skills/same/SKILL.md",
                    LocationKind::UserClaude,
                    None,
                    20,
                ),
            ],
            ..Inventory::default()
        };
        let sessions = [
            session(Source::ClaudeCode, "/p", 3),
            session(Source::OpenCode, "/p", 2),
        ];
        let r = analyze(&inv, &sessions, &crate::tokens::CharsPerToken);
        let b = &r.skills[0].baseline;
        assert_eq!(b.requests, 5);
        // OpenCode sees the first copy (10), Claude Code only the second (20).
        assert_eq!(b.tokens.count, 2 * 10 + 3 * 20);
    }

    #[test]
    fn project_scoped_plugins_only_count_for_sessions_in_that_project() {
        let inv = Inventory {
            skills: vec![entry(
                "/plugins/x/skills/same/SKILL.md",
                LocationKind::Plugin,
                Some("/work/helix"),
                10,
            )],
            ..Inventory::default()
        };
        let sessions = [
            session(Source::ClaudeCode, "/work/helix/app", 4),
            session(Source::ClaudeCode, "/work/other", 6),
        ];
        let r = analyze(&inv, &sessions, &crate::tokens::CharsPerToken);
        assert_eq!(r.skills[0].baseline.requests, 4);
    }
}
