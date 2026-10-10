//! Ready-made runs over a registry: what the CLI and the reports call.

use std::collections::BTreeSet;
use std::path::PathBuf;

use crate::adapter::{Detection, Registry, SessionFilter};
use crate::analyzers::context::{self, ContextReport};
use crate::analyzers::cost::{self, CostReport, Grouping};
use crate::analyzers::flow::{self, ErrorReport, GrowthReport, HooksReport};
use crate::analyzers::skills::{self, SkillReport};
use crate::analyzers::unused::{self, UnusedReport};
use crate::catalog;
use crate::env::Env;
use crate::instructions;
use crate::inventory::{self, Inventory};
use crate::model::{SessionEvents, Source};
use crate::tokens::TokenEstimator;

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
                for r in a.sessions(env, filter) {
                    match r {
                        Ok(se) if filter.matches(&se.session) => {
                            scan.sessions += 1;
                            scan.events += se.events.len() as u64;
                            scan.warnings += se.warnings.len() as u64;
                        }
                        Ok(_) => {}
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
    pub warnings: u64,
    pub errors: Vec<String>,
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
    let mut sessions: Vec<SessionEvents> = Vec::new();
    let mut errors = Vec::new();
    for r in registry.sessions(env, filter, only) {
        match r {
            Ok(se) => sessions.push(se),
            Err(e) => errors.push(e.to_string()),
        }
    }
    let warnings = sessions.iter().map(|s| s.warnings.len() as u64).sum();
    let projects: BTreeSet<PathBuf> = sessions
        .iter()
        .filter_map(|s| s.session.project_dir.clone())
        .collect();
    let projects: Vec<PathBuf> = projects.into_iter().collect();
    let inventory = inventory::scan(&inventory::locations(env, &projects), estimator);
    let report = skills::analyze(&inventory, &sessions, estimator);
    SkillRun {
        report,
        inventory,
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
    UnusedRun {
        report: unused::analyze(&skill_report, &catalog, &context_report, &sessions),
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
        errors,
    }
}
