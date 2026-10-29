//! Ready-made runs over a registry: what the CLI and the reports call.

use std::collections::BTreeSet;
use std::path::PathBuf;

use crate::adapter::{Detection, Registry, SessionFilter};
use crate::analyzers::advice::{self, AdviceReport};
use crate::analyzers::compare::{self, CompareReport};
use crate::analyzers::context::{self, ContextReport};
use crate::analyzers::cost::{self, CostReport, Grouping, WhatIfReport};
use crate::analyzers::efficiency::{self, CacheBy, EfficiencyReport};
use crate::analyzers::flow::{self, ErrorReport, GrowthReport, HooksReport};
use crate::analyzers::skills::{self, SkillReport};
use crate::analyzers::timeline::{self, SessionSummary, Timeline};
use crate::analyzers::unused::{self, UnusedReport};
use crate::catalog;
use crate::env::Env;
use crate::instructions;
use crate::inventory::{self, Inventory};
use crate::model::{SessionEvents, Source};
use crate::references;
use crate::tokens::TokenEstimator;
use crate::worktree::WorktreeResolver;

/// What one source holds on this machine.
#[derive(Debug, Clone)]
pub struct SourceScan {
    pub source: Source,
    pub detection: Detection,
    pub sessions: u64,
    pub events: u64,
    pub warnings: u64,
    /// Sessions that could not be read at all.
    pub errors: Vec<String>,
}

/// Counts what every selected adapter finds (all adapters if `only` is empty).
pub fn scan_sources(
    registry: &Registry,
    env: &Env,
    filter: &SessionFilter,
    only: &[Source],
) -> Vec<SourceScan> {
    registry
        .adapters()
        .filter(|a| only.is_empty() || only.contains(&a.id()))
        .map(|a| {
            let mut scan = SourceScan {
                source: a.id(),
                detection: a.detect(env),
                sessions: 0,
                events: 0,
                warnings: 0,
                errors: Vec::new(),
            };
            if scan.detection.found {
                let mut resolver = WorktreeResolver::new();
                for r in a.sessions(env, filter) {
                    match r {
                        Ok(mut se) => {
                            resolver.apply(&mut se.session);
                            if !filter.matches(&se.session) {
                                continue;
                            }
                            scan.sessions += 1;
                            scan.events += se.events.len() as u64;
                            scan.warnings += se.warnings.len() as u64;
                        }
                        Err(e) => scan.errors.push(e.to_string()),
                    }
                }
            }
            scan
        })
        .collect()
}

#[derive(Debug, Clone)]
pub struct SkillRun {
    pub report: SkillReport,
    pub inventory: Inventory,
    /// The projects the sessions ran in.
    pub projects: Vec<PathBuf>,
    pub warnings: u64,
    pub errors: Vec<String>,
}

pub struct AdviceRun {
    pub report: AdviceReport,
    pub warnings: u64,
    pub errors: Vec<String>,
}

/// The findings about the skills of the selected sessions: what to change to save tokens, ranked by
/// what each would have saved, priced with `prices`.
pub fn advice_run(
    registry: &Registry,
    env: &Env,
    filter: &SessionFilter,
    only: &[Source],
    estimator: &dyn TokenEstimator,
    prices: &crate::prices::PriceTable,
) -> AdviceRun {
    let (sessions, errors) = read_sessions(registry, env, filter, only);
    let run = skill_run_from(env, &sessions, errors, estimator);
    let catalog = catalog::scan(env, &run.projects, estimator);
    let references = references::scan(
        env,
        &run.projects,
        &idle_skills(&run.report),
        &run.inventory,
        &catalog,
    );
    let instructions = instructions::scan(env, &run.projects, estimator);
    let context_report = context::analyze(&sessions, &instructions.files, estimator);
    let mcp = advice::mcp_servers(&context_report, &sessions);
    let context_saving = advice::context_saving(&sessions, prices);
    let pricing = advice::Pricing::from_sessions(&sessions, prices);
    AdviceRun {
        report: advice::analyze_with(&run.report, &references, &mcp, &context_saving, &pricing),
        warnings: run.warnings + catalog.warnings.len() as u64 + instructions.warnings.len() as u64,
        errors: run.errors,
    }
}

/// The installed skills that were neither loaded nor used through their files.
fn idle_skills(report: &SkillReport) -> Vec<&str> {
    report
        .skills
        .iter()
        .filter(|s| s.in_inventory && s.invocations == 0 && s.file_uses == 0)
        .map(|s| s.name.as_str())
        .collect()
}

/// Reads the sessions, inventories the skills (user level plus the projects the
/// sessions ran in) and analyzes them.
pub fn skill_run(
    registry: &Registry,
    env: &Env,
    filter: &SessionFilter,
    only: &[Source],
    estimator: &dyn TokenEstimator,
) -> SkillRun {
    let (sessions, errors) = read_sessions(registry, env, filter, only);
    skill_run_from(env, &sessions, errors, estimator)
}

fn read_sessions(
    registry: &Registry,
    env: &Env,
    filter: &SessionFilter,
    only: &[Source],
) -> (Vec<SessionEvents>, Vec<String>) {
    let mut sessions: Vec<SessionEvents> = Vec::new();
    let mut errors = Vec::new();
    for r in registry.sessions(env, filter, only) {
        match r {
            Ok(se) => sessions.push(se),
            Err(e) => errors.push(e.to_string()),
        }
    }
    (sessions, errors)
}

fn skill_run_from(
    env: &Env,
    sessions: &[SessionEvents],
    errors: Vec<String>,
    estimator: &dyn TokenEstimator,
) -> SkillRun {
    let warnings = sessions.iter().map(|s| s.warnings.len() as u64).sum();
    let projects: BTreeSet<PathBuf> = sessions
        .iter()
        .filter_map(|s| s.session.project_dir.clone())
        .collect();
    let projects: Vec<PathBuf> = projects.into_iter().collect();
    let inventory = inventory::scan(&inventory::locations(env, &projects), estimator);
    let report = skills::analyze(&inventory, sessions, estimator);
    SkillRun {
        report,
        inventory,
        projects,
        warnings,
        errors,
    }
}

#[derive(Debug, Clone)]
pub struct ContextRun {
    pub report: ContextReport,
    pub warnings: u64,
    pub errors: Vec<String>,
}

/// Reads the sessions and analyzes the static context of their requests.
pub fn context_run(
    registry: &Registry,
    env: &Env,
    filter: &SessionFilter,
    only: &[Source],
    estimator: &dyn TokenEstimator,
) -> ContextRun {
    let mut sessions: Vec<SessionEvents> = Vec::new();
    let mut errors = Vec::new();
    for r in registry.sessions(env, filter, only) {
        match r {
            Ok(se) => sessions.push(se),
            Err(e) => errors.push(e.to_string()),
        }
    }
    let projects: BTreeSet<PathBuf> = sessions
        .iter()
        .filter_map(|s| s.session.project_dir.clone())
        .collect();
    let projects: Vec<PathBuf> = projects.into_iter().collect();
    let instructions = instructions::scan(env, &projects, estimator);
    ContextRun {
        warnings: sessions
            .iter()
            .map(|s| s.warnings.len() as u64)
            .sum::<u64>()
            + instructions.warnings.len() as u64,
        report: context::analyze(&sessions, &instructions.files, estimator),
        errors,
    }
}

#[derive(Debug, Clone)]
pub struct UnusedRun {
    pub report: UnusedReport,
    pub warnings: u64,
    pub errors: Vec<String>,
}

/// Which skills, agents and MCP servers were used in the selected sessions.
pub fn unused_run(
    registry: &Registry,
    env: &Env,
    filter: &SessionFilter,
    only: &[Source],
    estimator: &dyn TokenEstimator,
) -> UnusedRun {
    let mut sessions: Vec<SessionEvents> = Vec::new();
    let mut errors = Vec::new();
    for r in registry.sessions(env, filter, only) {
        match r {
            Ok(se) => sessions.push(se),
            Err(e) => errors.push(e.to_string()),
        }
    }
    let projects: BTreeSet<PathBuf> = sessions
        .iter()
        .filter_map(|s| s.session.project_dir.clone())
        .collect();
    let projects: Vec<PathBuf> = projects.into_iter().collect();
    let inventory = inventory::scan(&inventory::locations(env, &projects), estimator);
    let instructions = instructions::scan(env, &projects, estimator);
    let catalog = catalog::scan(env, &projects, estimator);
    let skill_report = skills::analyze(&inventory, &sessions, estimator);
    let context_report = context::analyze(&sessions, &instructions.files, estimator);
    let warnings = sessions
        .iter()
        .map(|s| s.warnings.len() as u64)
        .sum::<u64>()
        + inventory.warnings.len() as u64
        + catalog.warnings.len() as u64;
    let references = references::scan(
        env,
        &projects,
        &idle_skills(&skill_report),
        &inventory,
        &catalog,
    );
    UnusedRun {
        report: unused::analyze(
            &skill_report,
            &catalog,
            &context_report,
            &sessions,
            &references,
        ),
        warnings,
        errors,
    }
}

#[derive(Debug, Clone)]
pub struct FlowRun {
    pub growth: GrowthReport,
    pub tool_errors: ErrorReport,
    pub hooks: HooksReport,
    pub warnings: u64,
    pub errors: Vec<String>,
}

/// Reads the sessions and analyzes how they unfold.
pub fn flow_run(
    registry: &Registry,
    env: &Env,
    filter: &SessionFilter,
    only: &[Source],
) -> FlowRun {
    let mut sessions: Vec<SessionEvents> = Vec::new();
    let mut errors = Vec::new();
    for r in registry.sessions(env, filter, only) {
        match r {
            Ok(se) => sessions.push(se),
            Err(e) => errors.push(e.to_string()),
        }
    }
    FlowRun {
        growth: flow::growth(&sessions),
        tool_errors: flow::errors(&sessions),
        hooks: flow::hooks(&sessions),
        warnings: sessions.iter().map(|s| s.warnings.len() as u64).sum(),
        errors,
    }
}

#[derive(Debug, Clone)]
pub struct CostRun {
    pub report: CostReport,
    /// Set if a what-if model was requested.
    pub what_if: Option<WhatIfReport>,
    pub warnings: u64,
    pub errors: Vec<String>,
}

/// Reads the sessions and prices their usage.
pub fn cost_run(
    registry: &Registry,
    env: &Env,
    filter: &SessionFilter,
    only: &[Source],
    prices: &crate::prices::PriceTable,
    group: Grouping,
    what_if: Option<&str>,
) -> CostRun {
    let mut sessions: Vec<SessionEvents> = Vec::new();
    let mut errors = Vec::new();
    for r in registry.sessions(env, filter, only) {
        match r {
            Ok(se) => sessions.push(se),
            Err(e) => errors.push(e.to_string()),
        }
    }
    CostRun {
        warnings: sessions.iter().map(|s| s.warnings.len() as u64).sum(),
        report: cost::analyze(&sessions, prices, group),
        what_if: what_if.map(|m| cost::what_if(&sessions, prices, m)),
        errors,
    }
}

#[derive(Debug, Clone)]
pub struct EfficiencyRun {
    pub report: EfficiencyReport,
    pub warnings: u64,
    pub errors: Vec<String>,
}

/// Reads the sessions and analyzes cache reuse and subagent share.
pub fn efficiency_run(
    registry: &Registry,
    env: &Env,
    filter: &SessionFilter,
    only: &[Source],
    prices: &crate::prices::PriceTable,
    by: CacheBy,
) -> EfficiencyRun {
    let mut sessions: Vec<SessionEvents> = Vec::new();
    let mut errors = Vec::new();
    for r in registry.sessions(env, filter, only) {
        match r {
            Ok(se) => sessions.push(se),
            Err(e) => errors.push(e.to_string()),
        }
    }
    EfficiencyRun {
        warnings: sessions.iter().map(|s| s.warnings.len() as u64).sum(),
        report: efficiency::analyze(&sessions, prices, by),
        errors,
    }
}

#[derive(Debug, Clone)]
pub struct CompareRun {
    pub report: CompareReport,
    pub warnings: u64,
    pub errors: Vec<String>,
}

/// Reads the sessions and compares the sources.
pub fn compare_run(
    registry: &Registry,
    env: &Env,
    filter: &SessionFilter,
    only: &[Source],
    estimator: &dyn TokenEstimator,
) -> CompareRun {
    let mut sessions: Vec<SessionEvents> = Vec::new();
    let mut errors = Vec::new();
    for r in registry.sessions(env, filter, only) {
        match r {
            Ok(se) => sessions.push(se),
            Err(e) => errors.push(e.to_string()),
        }
    }
    let projects: BTreeSet<PathBuf> = sessions
        .iter()
        .filter_map(|s| s.session.project_dir.clone())
        .collect();
    let projects: Vec<PathBuf> = projects.into_iter().collect();
    let inventory = inventory::scan(&inventory::locations(env, &projects), estimator);
    // The skill analysis per source: baseline, load and carry depend on the client.
    let mut skill_overhead = std::collections::BTreeMap::new();
    for source in sessions
        .iter()
        .map(|s| s.session.source)
        .collect::<BTreeSet<_>>()
    {
        let of_source: Vec<SessionEvents> = sessions
            .iter()
            .filter(|s| s.session.source == source)
            .cloned()
            .collect();
        let report = skills::analyze(&inventory, &of_source, estimator);
        skill_overhead.insert(
            source,
            report
                .skills
                .iter()
                .map(|s| s.overhead_tokens())
                .sum::<u64>(),
        );
    }
    CompareRun {
        warnings: sessions.iter().map(|s| s.warnings.len() as u64).sum(),
        report: compare::analyze(&sessions, &skill_overhead),
        errors,
    }
}

/// The sessions of the selection, most recent first, for a session picker.
pub fn session_list(
    registry: &Registry,
    env: &Env,
    filter: &SessionFilter,
    only: &[Source],
) -> (Vec<SessionSummary>, Vec<String>) {
    let mut sessions = Vec::new();
    let mut errors = Vec::new();
    for r in registry.sessions(env, filter, only) {
        match r {
            Ok(se) => sessions.push(se),
            Err(e) => errors.push(e.to_string()),
        }
    }
    (timeline::list(&sessions), errors)
}

/// The timeline of one session, or `None` if it is not found.
pub fn session_timeline(
    registry: &Registry,
    env: &Env,
    filter: &SessionFilter,
    source: Source,
    id: &str,
    prices: &crate::prices::PriceTable,
) -> Option<Timeline> {
    registry
        .sessions(env, filter, &[source])
        .filter_map(Result::ok)
        .find(|s| s.session.id == id && s.session.source == source)
        .map(|s| timeline::timeline(&s, prices))
}
