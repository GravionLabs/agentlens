//! The analyses of `agentlens-core` as plain functions the Tauri commands wrap.
//!
//! Each function takes the same filter as the CLI (period, sources, project)
//! and returns the report types of the core, so the application shows the
//! numbers the CLI prints. The functions do not depend on Tauri and are tested
//! against fixture homes.

use std::path::PathBuf;

use agentlens_core::adapter::{Registry, SessionFilter};
use agentlens_core::adapters::default_registry;
use agentlens_core::analyzers::compare::CompareReport;
use agentlens_core::analyzers::context::ContextReport;
use agentlens_core::analyzers::cost::{CostReport, Grouping, WhatIfReport};
use agentlens_core::analyzers::efficiency::{CacheBy, EfficiencyReport};
use agentlens_core::analyzers::flow::{ErrorReport, GrowthReport, HooksReport};
use agentlens_core::analyzers::skills::SkillReport;
use agentlens_core::analyzers::timeline::{SessionSummary, Timeline};
use agentlens_core::analyzers::unused::UnusedReport;
use agentlens_core::env::Env;
use agentlens_core::inventory::Duplicate;
use agentlens_core::model::Source;
use agentlens_core::pipeline;
use agentlens_core::prices::{self, PriceTable};
use agentlens_core::since::parse_since;
use agentlens_core::tokens::CharsPerToken;
use jiff::Timestamp;
use serde::{Deserialize, Serialize};

/// Where the data comes from: the home directory and the adapters.
pub struct Context {
    pub env: Env,
    pub registry: Registry,
}

impl Context {
    pub fn system() -> Result<Context, String> {
        let env = Env::from_system()
            .ok_or("cannot determine the home directory (HOME or USERPROFILE)")?;
        Ok(Context {
            env,
            registry: default_registry(),
        })
    }
}

/// The filter bar of the application.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Filters {
    /// A duration (`30d`, `12h`, `4w`), a date or an RFC 3339 instant.
    pub since: Option<String>,
    /// Source ids (`claude-code`, `opencode`, ...); empty means all.
    pub sources: Vec<String>,
    /// Only sessions that ran in this directory or below it.
    pub project: Option<PathBuf>,
}

impl Filters {
    fn resolve(&self, now: Timestamp) -> Result<(SessionFilter, Vec<Source>), String> {
        let since = match self
            .since
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            Some(s) => Some(parse_since(s, now)?),
            None => None,
        };
        let mut sources = Vec::new();
        for id in &self.sources {
            sources.push(Source::from_id(id).ok_or_else(|| format!("unknown source `{id}`"))?);
        }
        Ok((
            SessionFilter {
                since,
                project: self.project.clone().filter(|p| !p.as_os_str().is_empty()),
            },
            sources,
        ))
    }
}

/// A result plus what went wrong while reading the sessions.
#[derive(Debug, Serialize)]
pub struct Answer<T> {
    pub data: T,
    pub warnings: u64,
    pub errors: Vec<String>,
}

fn answer<T>(data: T, warnings: u64, errors: Vec<String>) -> Result<Answer<T>, String> {
    Ok(Answer {
        data,
        warnings,
        errors,
    })
}

#[derive(Debug, Serialize)]
pub struct SourceOverview {
    pub source: Source,
    pub found: bool,
    pub roots: Vec<PathBuf>,
    pub note: Option<String>,
    pub sessions: u64,
    pub events: u64,
    pub warnings: u64,
    pub errors: Vec<String>,
}

pub fn overview(ctx: &Context, f: &Filters) -> Result<Answer<Vec<SourceOverview>>, String> {
    let (filter, only) = f.resolve(Timestamp::now())?;
    let scans = pipeline::scan_sources(&ctx.registry, &ctx.env, &filter, &only);
    let warnings = scans.iter().map(|s| s.warnings).sum();
    let data = scans
        .into_iter()
        .map(|s| SourceOverview {
            source: s.source,
            found: s.detection.found,
            roots: s.detection.roots,
            note: s.detection.note,
            sessions: s.sessions,
            events: s.events,
            warnings: s.warnings,
            errors: s.errors,
        })
        .collect();
    answer(data, warnings, Vec::new())
}

#[derive(Debug, Serialize)]
pub struct SkillsData {
    pub report: SkillReport,
    pub installed_skills: usize,
    pub duplicates: Vec<Duplicate>,
}

pub fn skills(ctx: &Context, f: &Filters) -> Result<Answer<SkillsData>, String> {
    let (filter, only) = f.resolve(Timestamp::now())?;
    let run = pipeline::skill_run(&ctx.registry, &ctx.env, &filter, &only, &CharsPerToken);
    let data = SkillsData {
        installed_skills: run.inventory.skills.len(),
        duplicates: run.inventory.duplicates,
        report: run.report,
    };
    answer(data, run.warnings, run.errors)
}

pub fn context(ctx: &Context, f: &Filters) -> Result<Answer<ContextReport>, String> {
    let (filter, only) = f.resolve(Timestamp::now())?;
    let run = pipeline::context_run(&ctx.registry, &ctx.env, &filter, &only, &CharsPerToken);
    answer(run.report, run.warnings, run.errors)
}

/// Defaults to the last 30 days if the filter has no period.
pub fn unused(ctx: &Context, f: &Filters) -> Result<Answer<UnusedReport>, String> {
    let now = Timestamp::now();
    let (mut filter, only) = f.resolve(now)?;
    if filter.since.is_none() {
        filter.since = Some(parse_since("30d", now)?);
    }
    let run = pipeline::unused_run(&ctx.registry, &ctx.env, &filter, &only, &CharsPerToken);
    answer(run.report, run.warnings, run.errors)
}

#[derive(Debug, Serialize)]
pub struct FlowData {
    pub growth: GrowthReport,
    pub errors: ErrorReport,
    pub hooks: HooksReport,
}

pub fn flow(ctx: &Context, f: &Filters) -> Result<Answer<FlowData>, String> {
    let (filter, only) = f.resolve(Timestamp::now())?;
    let run = pipeline::flow_run(&ctx.registry, &ctx.env, &filter, &only);
    let data = FlowData {
        growth: run.growth,
        errors: run.tool_errors,
        hooks: run.hooks,
    };
    answer(data, run.warnings, run.errors)
}

fn load_prices(ctx: &Context) -> Result<PriceTable, String> {
    prices::load(&ctx.env, None).map_err(|e| format!("cannot load prices: {e}"))
}

#[derive(Debug, Serialize)]
pub struct CostData {
    pub report: CostReport,
    pub what_if: Option<WhatIfReport>,
}

/// `by` is `model`, `project`, `branch` or `issue`; `what_if` is a model id prefix.
pub fn cost(
    ctx: &Context,
    f: &Filters,
    by: &str,
    what_if: Option<&str>,
) -> Result<Answer<CostData>, String> {
    let group = match by {
        "model" => Grouping::Model,
        "project" => Grouping::Project,
        "branch" => Grouping::Branch,
        "issue" => Grouping::Issue,
        other => return Err(format!("unknown grouping `{other}`")),
    };
    let (filter, only) = f.resolve(Timestamp::now())?;
    let prices = load_prices(ctx)?;
    let what_if = what_if.map(str::trim).filter(|m| !m.is_empty());
    if let Some(model) = what_if
        && prices.lookup(model).is_none()
    {
        return Err(format!("no price for the what-if model `{model}`"));
    }
    let run = pipeline::cost_run(
        &ctx.registry,
        &ctx.env,
        &filter,
        &only,
        &prices,
        group,
        what_if,
    );
    let data = CostData {
        report: run.report,
        what_if: run.what_if,
    };
    answer(data, run.warnings, run.errors)
}

/// `by` is `session`, `project` or `source`.
pub fn efficiency(
    ctx: &Context,
    f: &Filters,
    by: &str,
) -> Result<Answer<EfficiencyReport>, String> {
    let cache_by = match by {
        "session" => CacheBy::Session,
        "project" => CacheBy::Project,
        "source" => CacheBy::Source,
        other => return Err(format!("unknown grouping `{other}`")),
    };
    let (filter, only) = f.resolve(Timestamp::now())?;
    let prices = load_prices(ctx)?;
    let run = pipeline::efficiency_run(&ctx.registry, &ctx.env, &filter, &only, &prices, cache_by);
    answer(run.report, run.warnings, run.errors)
}

pub fn compare(ctx: &Context, f: &Filters) -> Result<Answer<CompareReport>, String> {
    let (filter, only) = f.resolve(Timestamp::now())?;
    let run = pipeline::compare_run(&ctx.registry, &ctx.env, &filter, &only, &CharsPerToken);
    answer(run.report, run.warnings, run.errors)
}

pub fn sessions(ctx: &Context, f: &Filters) -> Result<Answer<Vec<SessionSummary>>, String> {
    let (filter, only) = f.resolve(Timestamp::now())?;
    let (list, errors) = pipeline::session_list(&ctx.registry, &ctx.env, &filter, &only);
    answer(list, 0, errors)
}

pub fn timeline(
    ctx: &Context,
    f: &Filters,
    source: &str,
    id: &str,
) -> Result<Answer<Timeline>, String> {
    let source = Source::from_id(source).ok_or_else(|| format!("unknown source `{source}`"))?;
    let (filter, _) = f.resolve(Timestamp::now())?;
    let prices = load_prices(ctx)?;
    let timeline =
        pipeline::session_timeline(&ctx.registry, &ctx.env, &filter, source, id, &prices)
            .ok_or_else(|| format!("session `{id}` not found"))?;
    answer(timeline, 0, Vec::new())
}
