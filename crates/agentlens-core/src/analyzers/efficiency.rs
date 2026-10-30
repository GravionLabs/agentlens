//! How well the context is reused and how much work subagents do.
//!
//! Cache hit rate = cache read tokens / all input tokens of a request
//! (uncached input + cache read + cache write), measured from client usage.
//! A source that reports no cache tokens at all has no hit rate (not 0%).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::model::{EventKind, SessionEvents, Source, Usage};
use crate::prices::PriceTable;
use crate::tokens::Tokens;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CacheBy {
    Session,
    Project,
    Source,
}

pub const NO_PROJECT: &str = "(no project)";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CacheRow {
    pub key: String,
    pub source: Source,
    pub requests: u64,
    /// Uncached input tokens.
    pub input: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    /// `None` if the source reports no cache tokens at all.
    pub hit_rate: Option<f64>,
}

/// Share of the work done by subagents, per source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubagentShare {
    pub source: Source,
    pub requests: u64,
    pub subagent_requests: u64,
    /// Input and output tokens.
    pub tokens: u64,
    pub subagent_tokens: u64,
    /// Cost of the priced requests.
    pub usd: f64,
    pub subagent_usd: f64,
    /// Requests without a price; they are left out of the cost shares.
    pub unpriced_requests: u64,
}

/// Which field of a run named its role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RoleKind {
    /// The main agent (requests without a run).
    Main,
    /// The name of a forked skill.
    Skill,
    /// The workflow phase of a workflow subagent.
    WorkflowPhase,
    /// The agent type (`Explore`, `general-purpose`, ...).
    AgentType,
    /// The run names none of the three.
    Unknown,
}

pub const MAIN_ROLE: &str = "main";
pub const UNKNOWN_ROLE: &str = "(unknown)";

/// One role over the period: the main agent or all runs with the same skill, phase or type.
/// A run without a request counts in `runs` (requests 0, no cost).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoleRow {
    pub role: String,
    pub kind: RoleKind,
    /// Runs of the role; the main agent counts one run per session with requests.
    pub runs: u64,
    pub requests: u64,
    pub requests_per_run: f64,
    /// Uncached input tokens.
    pub input: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub output: u64,
    /// Cost of the priced requests; `None` if no request has a price.
    pub usd: Option<f64>,
    /// Mean cost per run (`usd / runs`).
    pub usd_per_run: Option<f64>,
    /// `true` if there is a cost and every priced request carries one the client reported.
    pub cost_measured: bool,
    pub unpriced_requests: u64,
    /// Sorted, without duplicates.
    pub models: Vec<String>,
    /// Per run, mean rounded to an integer, over the runs with requests: `input + cache_read +
    /// cache_write` of the first request, the static part every run pays.
    pub first_request_context: Tokens,
    /// Per run, mean rounded to an integer, over the runs with requests: `input + cache_write` of
    /// the later requests, what the run read.
    pub uncached_after_first: Tokens,
    /// `None` without any input.
    pub hit_rate: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EfficiencyReport {
    /// Sorted by requests, largest first.
    pub cache: Vec<CacheRow>,
    pub subagents: Vec<SubagentShare>,
    /// Sorted by cost, largest first (rows without a price last).
    #[serde(default)]
    pub roles: Vec<RoleRow>,
}

#[derive(Default)]
struct RunAcc {
    requests: u64,
    first_context: u64,
    later_uncached: u64,
    measured: bool,
}

#[derive(Default)]
struct RoleAcc {
    runs: u64,
    /// Runs with at least one request; the denominator of the two context means.
    runs_with_requests: u64,
    requests: u64,
    input: u64,
    cache_read: u64,
    cache_write: u64,
    output: u64,
    usd: f64,
    priced: u64,
    unmeasured_cost: bool,
    unpriced: u64,
    models: std::collections::BTreeSet<String>,
    first_context: u64,
    later_uncached: u64,
    all_measured: bool,
}

/// What a run says about itself, merged from its `AgentRun` event and the info on its requests,
/// whichever comes first.
#[derive(Default)]
struct RunInfo {
    name: Option<String>,
    phase: Option<String>,
    /// From the `AgentRun` event; preferred over the one on the requests.
    agent_type: Option<String>,
    request_agent_type: Option<String>,
}

/// Keep a known value: the first non-blank one wins.
fn fill(slot: &mut Option<String>, value: &Option<String>) {
    if slot.is_none()
        && let Some(v) = value.as_ref().filter(|v| !v.trim().is_empty())
    {
        *slot = Some(v.clone());
    }
}

fn role_of(info: &RunInfo) -> (String, RoleKind) {
    if let Some(n) = &info.name {
        (n.clone(), RoleKind::Skill)
    } else if let Some(p) = &info.phase {
        (p.clone(), RoleKind::WorkflowPhase)
    } else if let Some(t) = info
        .agent_type
        .as_ref()
        .or(info.request_agent_type.as_ref())
    {
        (t.clone(), RoleKind::AgentType)
    } else {
        (UNKNOWN_ROLE.into(), RoleKind::Unknown)
    }
}

fn roles(sessions: &[SessionEvents], prices: &PriceTable) -> Vec<RoleRow> {
    let mut acc: BTreeMap<(RoleKind, String), RoleAcc> = BTreeMap::new();
    let fresh = || RoleAcc {
        all_measured: true,
        ..RoleAcc::default()
    };
    for se in sessions {
        let mut infos: BTreeMap<&str, RunInfo> = BTreeMap::new();
        // Requests per run in event order; `None` is the main agent.
        let mut runs: BTreeMap<Option<&str>, RunAcc> = BTreeMap::new();
        let mut seen: Vec<(Option<&str>, &Usage)> = Vec::new();
        for e in &se.events {
            match &e.kind {
                EventKind::AgentRun {
                    agent_id,
                    agent_type,
                    name,
                    ..
                } => {
                    let i = infos.entry(agent_id).or_default();
                    fill(&mut i.name, name);
                    fill(&mut i.agent_type, agent_type);
                    // A run without requests still counts.
                    runs.entry(Some(agent_id)).or_insert_with(|| RunAcc {
                        measured: true,
                        ..RunAcc::default()
                    });
                }
                EventKind::Usage(u) => {
                    let id = u.agent.as_ref().and_then(|a| a.agent_id.as_deref());
                    if let (Some(a), Some(id)) = (&u.agent, id) {
                        let i = infos.entry(id).or_default();
                        fill(&mut i.phase, &a.workflow_phase);
                        fill(&mut i.request_agent_type, &a.agent_type);
                    }
                    seen.push((id, u));
                }
                _ => {}
            }
        }
        for (id, u) in &seen {
            let r = runs.entry(*id).or_insert_with(|| RunAcc {
                measured: true,
                ..RunAcc::default()
            });
            if r.requests == 0 {
                r.first_context = u.total_input();
            } else {
                r.later_uncached += u.input + u.cache_write;
            }
            r.requests += 1;
            r.measured &= u.measured;
        }
        let key_of = |id: Option<&str>| match id {
            None => (RoleKind::Main, MAIN_ROLE.to_owned()),
            Some(id) => {
                let (name, kind) = role_of(&infos[id]);
                (kind, name)
            }
        };
        for (id, run) in runs {
            let a = acc.entry(key_of(id)).or_insert_with(fresh);
            a.runs += 1;
            if run.requests > 0 {
                a.runs_with_requests += 1;
                a.first_context += run.first_context;
                a.later_uncached += run.later_uncached;
                a.all_measured &= run.measured;
            }
        }
        for (id, u) in &seen {
            let a = acc.get_mut(&key_of(*id)).expect("role created above");
            a.requests += 1;
            a.input += u.input;
            a.cache_read += u.cache_read;
            a.cache_write += u.cache_write;
            a.output += u.output;
            let mut usage = (*u).clone();
            if usage.model.is_none() {
                usage.model = se.session.model.clone();
            }
            if let Some(m) = &usage.model {
                a.models.insert(m.clone());
            }
            match prices.cost(&usage) {
                Some(c) => {
                    a.usd += c.usd;
                    a.priced += 1;
                    a.unmeasured_cost |= !c.measured;
                }
                None => a.unpriced += 1,
            }
        }
    }
    let mut rows: Vec<RoleRow> = acc
        .into_iter()
        .map(|((kind, role), a)| {
            let runs = a.runs.max(1) as f64;
            let with_requests = a.runs_with_requests.max(1) as f64;
            let mean = |n: u64| (n as f64 / with_requests).round() as u64;
            let total_input = a.input + a.cache_read + a.cache_write;
            let usd = (a.priced > 0).then_some(a.usd);
            let measured = a.runs_with_requests > 0 && a.all_measured;
            RoleRow {
                role,
                kind,
                runs: a.runs,
                requests: a.requests,
                requests_per_run: a.requests as f64 / runs,
                input: a.input,
                cache_read: a.cache_read,
                cache_write: a.cache_write,
                output: a.output,
                usd,
                usd_per_run: usd.map(|u| u / runs),
                cost_measured: usd.is_some() && !a.unmeasured_cost,
                unpriced_requests: a.unpriced,
                models: a.models.into_iter().collect(),
                first_request_context: Tokens {
                    count: mean(a.first_context),
                    measured,
                },
                uncached_after_first: Tokens {
                    count: mean(a.later_uncached),
                    measured,
                },
                hit_rate: (total_input > 0).then(|| a.cache_read as f64 / total_input as f64),
            }
        })
        .collect();
    rows.sort_by(|a, b| {
        b.usd
            .unwrap_or(-1.0)
            .total_cmp(&a.usd.unwrap_or(-1.0))
            .then_with(|| b.requests.cmp(&a.requests))
            .then_with(|| a.role.cmp(&b.role))
    });
    rows
}

pub fn analyze(sessions: &[SessionEvents], prices: &PriceTable, by: CacheBy) -> EfficiencyReport {
    let mut rows: BTreeMap<(Source, String), CacheRow> = BTreeMap::new();
    let mut shares: BTreeMap<Source, SubagentShare> = BTreeMap::new();
    for se in sessions {
        let source = se.session.source;
        let key = match by {
            CacheBy::Session => se.session.id.clone(),
            CacheBy::Project => se
                .session
                .project_dir
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| NO_PROJECT.into()),
            CacheBy::Source => source.id().to_owned(),
        };
        for e in &se.events {
            let EventKind::Usage(u) = &e.kind else {
                continue;
            };
            let row = rows
                .entry((source, key.clone()))
                .or_insert_with(|| CacheRow {
                    key: key.clone(),
                    source,
                    requests: 0,
                    input: 0,
                    cache_read: 0,
                    cache_write: 0,
                    hit_rate: None,
                });
            row.requests += 1;
            row.input += u.input;
            row.cache_read += u.cache_read;
            row.cache_write += u.cache_write;

            let share = shares.entry(source).or_insert_with(|| SubagentShare {
                source,
                requests: 0,
                subagent_requests: 0,
                tokens: 0,
                subagent_tokens: 0,
                usd: 0.0,
                subagent_usd: 0.0,
                unpriced_requests: 0,
            });
            let tokens = u.total_input() + u.output;
            share.requests += 1;
            share.tokens += tokens;
            if u.subagent {
                share.subagent_requests += 1;
                share.subagent_tokens += tokens;
            }
            let mut usage = u.clone();
            if usage.model.is_none() {
                usage.model = se.session.model.clone();
            }
            match prices.cost(&usage) {
                Some(c) => {
                    share.usd += c.usd;
                    if u.subagent {
                        share.subagent_usd += c.usd;
                    }
                }
                None => share.unpriced_requests += 1,
            }
        }
    }

    // A source without any cache token has no meaningful hit rate.
    let mut has_cache: BTreeMap<Source, bool> = BTreeMap::new();
    for r in rows.values() {
        *has_cache.entry(r.source).or_default() |= r.cache_read + r.cache_write > 0;
    }
    let mut cache: Vec<CacheRow> = rows.into_values().collect();
    for r in &mut cache {
        let total = r.input + r.cache_read + r.cache_write;
        r.hit_rate =
            (has_cache[&r.source] && total > 0).then(|| r.cache_read as f64 / total as f64);
    }
    cache.sort_by(|a, b| b.requests.cmp(&a.requests).then_with(|| a.key.cmp(&b.key)));
    EfficiencyReport {
        cache,
        subagents: shares.into_values().collect(),
        roles: roles(sessions, prices),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Session, Usage};

    fn request(input: u64, read: u64, write: u64, subagent: bool) -> EventKind {
        EventKind::Usage(Usage {
            model: Some("claude-sonnet-5-5".into()),
            input,
            output: 0,
            cache_read: read,
            cache_write: write,
            subagent,
            ..Usage::default()
        })
    }

    fn session(id: &str, source: Source, project: &str, kinds: Vec<EventKind>) -> SessionEvents {
        let mut s = Session::new(id, source);
        s.project_dir = Some(project.into());
        let mut se = SessionEvents::new(s);
        for k in kinds {
            se.push(None, k);
        }
        se
    }

    #[test]
    fn hit_rate_is_cache_read_over_all_input() {
        let a = session(
            "a",
            Source::ClaudeCode,
            "/p",
            vec![request(100, 600, 300, false), request(0, 1000, 0, false)],
        );
        let b = session(
            "b",
            Source::ClaudeCode,
            "/q",
            vec![request(500, 0, 500, false)],
        );
        let r = analyze(
            &[a.clone(), b.clone()],
            &PriceTable::bundled(),
            CacheBy::Session,
        );
        let get = |k: &str| r.cache.iter().find(|x| x.key == k).unwrap();
        assert!((get("a").hit_rate.unwrap() - 1600.0 / 2000.0).abs() < 1e-9);
        assert_eq!(
            get("b").hit_rate,
            Some(0.0),
            "a source with caching and no hits has 0%"
        );
        let by_project = analyze(
            &[a.clone(), b.clone()],
            &PriceTable::bundled(),
            CacheBy::Project,
        );
        assert_eq!(by_project.cache.len(), 2);
        let by_source = analyze(&[a, b], &PriceTable::bundled(), CacheBy::Source);
        assert_eq!(by_source.cache.len(), 1);
        assert_eq!(by_source.cache[0].requests, 3);
        assert!((by_source.cache[0].hit_rate.unwrap() - 1600.0 / 3000.0).abs() < 1e-9);
    }

    #[test]
    fn sources_without_cache_tokens_have_no_hit_rate() {
        let a = session("a", Source::OpenCode, "/p", vec![request(100, 0, 0, false)]);
        let r = analyze(&[a], &PriceTable::bundled(), CacheBy::Source);
        assert_eq!(r.cache[0].hit_rate, None);
    }

    #[test]
    fn subagent_share_covers_requests_tokens_and_cost() {
        let s = session(
            "a",
            Source::ClaudeCode,
            "/p",
            vec![
                request(1_000_000, 0, 0, false), // $2.00
                request(1_000_000, 0, 0, true),  // $2.00
                request(1_000_000, 0, 0, true),  // $2.00
                EventKind::Usage(Usage {
                    model: Some("mystery".into()),
                    input: 5,
                    subagent: true,
                    ..Usage::default()
                }),
            ],
        );
        let r = analyze(&[s], &PriceTable::bundled(), CacheBy::Source);
        let sh = &r.subagents[0];
        assert_eq!((sh.requests, sh.subagent_requests), (4, 3));
        assert_eq!((sh.tokens, sh.subagent_tokens), (3_000_005, 2_000_005));
        assert!((sh.usd - 6.0).abs() < 1e-9 && (sh.subagent_usd - 4.0).abs() < 1e-9);
        assert_eq!(sh.unpriced_requests, 1);
    }

    fn run_event(id: &str, ty: Option<&str>, name: Option<&str>) -> EventKind {
        EventKind::AgentRun {
            agent_id: id.into(),
            parent_agent_id: None,
            tool_use_id: None,
            agent_type: ty.map(Into::into),
            name: name.map(Into::into),
            description: Some("free text that is never a role".into()),
            model: None,
            depth: Some(1),
            started_at: "2026-10-01T12:00:00Z".parse().unwrap(),
            ended_at: None,
        }
    }

    fn agent_request(
        id: &str,
        phase: Option<&str>,
        input: u64,
        read: u64,
        write: u64,
    ) -> EventKind {
        EventKind::Usage(Usage {
            model: Some("claude-sonnet-5-5".into()),
            input,
            output: 1,
            cache_read: read,
            cache_write: write,
            measured: true,
            subagent: true,
            agent: Some(crate::model::AgentInfo {
                agent_id: Some(id.into()),
                workflow_phase: phase.map(Into::into),
                ..Default::default()
            }),
            ..Usage::default()
        })
    }

    #[test]
    fn roles_rank_name_over_phase_over_type_and_keep_unknown() {
        let s = session(
            "a",
            Source::ClaudeCode,
            "/p",
            vec![
                request(10, 0, 0, false), // main, no agent_id
                run_event("r1", Some("general-purpose"), Some("review")),
                agent_request("r1", Some("Implement"), 1, 0, 0),
                run_event("r2", Some("workflow-subagent"), None),
                agent_request("r2", Some("Implement"), 1, 0, 0),
                run_event("r3", Some("Explore"), None),
                agent_request("r3", None, 1, 0, 0),
                run_event("r4", None, None),
                agent_request("r4", None, 1, 0, 0),
                agent_request("r5", None, 1, 0, 0), // no AgentRun event at all
            ],
        );
        let r = analyze(&[s], &PriceTable::bundled(), CacheBy::Source);
        let got: Vec<(&str, RoleKind)> =
            r.roles.iter().map(|r| (r.role.as_str(), r.kind)).collect();
        for want in [
            ("main", RoleKind::Main),
            ("review", RoleKind::Skill),
            ("Implement", RoleKind::WorkflowPhase),
            ("Explore", RoleKind::AgentType),
            ("(unknown)", RoleKind::Unknown),
        ] {
            assert!(got.contains(&want), "{want:?} missing in {got:?}");
        }
        assert_eq!(got.len(), 5, "{got:?}");
        let unknown = r
            .roles
            .iter()
            .find(|r| r.kind == RoleKind::Unknown)
            .unwrap();
        assert_eq!((unknown.runs, unknown.requests), (2, 2));
    }

    #[test]
    fn first_request_context_and_uncached_after_first_per_run() {
        let s = session(
            "a",
            Source::ClaudeCode,
            "/p",
            vec![
                run_event("r1", Some("Explore"), None),
                agent_request("r1", None, 5, 100, 20), // first: 125
                agent_request("r1", None, 3, 200, 7),  // later: 3 + 7
                agent_request("r1", None, 2, 300, 0),  // later: 2
                run_event("r2", Some("Explore"), None),
                agent_request("r2", None, 1, 0, 59), // first: 60
            ],
        );
        let r = analyze(&[s], &PriceTable::bundled(), CacheBy::Source);
        let row = &r.roles[0];
        assert_eq!(
            (row.role.as_str(), row.runs, row.requests),
            ("Explore", 2, 4)
        );
        assert!((row.requests_per_run - 2.0).abs() < 1e-9);
        // Means over two runs: (125 + 60) / 2 = 92.5 -> 93; (12 + 0) / 2 = 6.
        assert_eq!(row.first_request_context.count, 93);
        assert_eq!(row.uncached_after_first.count, 6);
        assert!(row.first_request_context.measured);
        assert_eq!(row.models, ["claude-sonnet-5-5"]);
    }

    #[test]
    fn estimated_requests_make_the_context_columns_estimates() {
        let mut u = match agent_request("r1", None, 5, 0, 0) {
            EventKind::Usage(u) => u,
            _ => unreachable!(),
        };
        u.measured = false;
        let s = session("a", Source::ClaudeCode, "/p", vec![EventKind::Usage(u)]);
        let r = analyze(&[s], &PriceTable::bundled(), CacheBy::Source);
        assert!(!r.roles[0].first_request_context.measured);
        assert!(!r.roles[0].cost_measured);
    }

    #[test]
    fn source_without_runs_has_the_main_row_only_and_roles_sort_by_cost() {
        let s = session(
            "a",
            Source::OpenCode,
            "/p",
            vec![
                request(1_000_000, 0, 0, false),
                request(1_000_000, 0, 0, false),
            ],
        );
        let r = analyze(&[s], &PriceTable::bundled(), CacheBy::Source);
        assert_eq!(r.roles.len(), 1);
        assert_eq!(r.roles[0].role, "main");
        assert_eq!((r.roles[0].runs, r.roles[0].requests), (1, 2));
        assert!((r.roles[0].usd.unwrap() - 4.0).abs() < 1e-9);
        assert!((r.roles[0].usd_per_run.unwrap() - 4.0).abs() < 1e-9);

        let s = session(
            "b",
            Source::ClaudeCode,
            "/p",
            vec![
                request(1_000_000, 0, 0, false),
                run_event("r1", Some("Explore"), None),
                agent_request("r1", None, 3_000_000, 0, 0),
            ],
        );
        let r = analyze(&[s], &PriceTable::bundled(), CacheBy::Source);
        assert_eq!(r.roles[0].role, "Explore", "largest cost first");
    }

    fn events_in_both_orders(first_run: bool) -> Vec<EventKind> {
        let run = run_event("r1", Some("Explore"), Some("review"));
        let req = agent_request("r1", Some("Implement"), 5, 100, 20);
        if first_run {
            vec![run, req]
        } else {
            vec![req, run]
        }
    }

    #[test]
    fn the_order_of_run_and_usage_events_does_not_matter() {
        for first_run in [true, false] {
            let s = session(
                "a",
                Source::ClaudeCode,
                "/p",
                events_in_both_orders(first_run),
            );
            let r = analyze(&[s], &PriceTable::bundled(), CacheBy::Source);
            assert_eq!(r.roles.len(), 1, "{first_run}: {:?}", r.roles);
            assert_eq!(
                (r.roles[0].role.as_str(), r.roles[0].kind),
                ("review", RoleKind::Skill),
                "{first_run}"
            );
        }
        // Without a skill name the type from the run must survive a request that comes first.
        for first_run in [true, false] {
            let run = run_event("r1", Some("Explore"), None);
            let req = agent_request("r1", None, 5, 0, 0);
            let evs = if first_run {
                vec![run, req]
            } else {
                vec![req, run]
            };
            let s = session("a", Source::ClaudeCode, "/p", evs);
            let r = analyze(&[s], &PriceTable::bundled(), CacheBy::Source);
            assert_eq!(
                (r.roles[0].role.as_str(), r.roles[0].kind),
                ("Explore", RoleKind::AgentType),
                "{first_run}"
            );
        }
    }

    #[test]
    fn a_run_without_requests_counts_but_stays_out_of_the_context_means() {
        let s = session(
            "a",
            Source::ClaudeCode,
            "/p",
            vec![
                run_event("r1", Some("Explore"), None),
                agent_request("r1", None, 10, 0, 0),
                run_event("r2", Some("Explore"), None), // no request
            ],
        );
        let r = analyze(&[s], &PriceTable::bundled(), CacheBy::Source);
        let row = &r.roles[0];
        assert_eq!((row.runs, row.requests), (2, 1));
        assert!((row.requests_per_run - 0.5).abs() < 1e-9);
        assert_eq!(row.first_request_context.count, 10);

        let s = session(
            "b",
            Source::ClaudeCode,
            "/p",
            vec![run_event("w", Some("workflow"), None)],
        );
        let r = analyze(&[s], &PriceTable::bundled(), CacheBy::Source);
        let row = &r.roles[0];
        assert_eq!((row.runs, row.requests), (1, 0));
        assert_eq!((row.usd, row.usd_per_run), (None, None));
        assert!(!row.cost_measured);
    }

    #[test]
    fn cost_is_never_measured_without_a_value() {
        let mut u = match agent_request("r1", None, 5, 0, 0) {
            EventKind::Usage(u) => u,
            _ => unreachable!(),
        };
        u.model = Some("model-without-a-price".into());
        let s = session("a", Source::ClaudeCode, "/p", vec![EventKind::Usage(u)]);
        let r = analyze(&[s], &PriceTable::bundled(), CacheBy::Source);
        assert_eq!(r.roles[0].usd, None);
        assert!(!r.roles[0].cost_measured);
    }

    #[test]
    fn empty_input_has_no_roles() {
        let r = analyze(&[], &PriceTable::bundled(), CacheBy::Source);
        assert!(r.roles.is_empty());
    }
}
