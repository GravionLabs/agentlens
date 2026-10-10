//! Ready-made runs over a registry: what the CLI and the reports call.

use std::collections::BTreeSet;
use std::path::PathBuf;

use crate::adapter::{Detection, Registry, SessionFilter};
use crate::analyzers::skills::{self, SkillReport};
use crate::env::Env;
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
