//! The analyses of `agentlens-core` as plain functions the Tauri commands wrap.
//!
//! Each function takes the same filter as the CLI (period, sources, project)
//! and returns the report types of the core, so the application shows the
//! numbers the CLI prints. The functions do not depend on Tauri and are tested
//! against fixture homes.

use std::path::PathBuf;
use std::time::Duration;

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
use agentlens_core::export;
use agentlens_core::inventory::Duplicate;
use agentlens_core::model::Source;
use agentlens_core::pipeline;
use agentlens_core::prices::{self, PriceTable};
use agentlens_core::report::Report;
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
            // Every view asks for its own analysis; they share the sessions read for
            // the same filters for a few minutes. Refresh forgets them.
            registry: default_registry().with_cache(Duration::from_secs(300)),
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
        // Relative periods count from a five-minute step, so that the views asking one
        // after the other share the sessions the registry cache holds for that selection.
        let now = Timestamp::from_second(now.as_second() - now.as_second().rem_euclid(300))
            .unwrap_or(now);
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

/// Forgets the sessions read so far, so the next analysis reads the files again.
pub fn refresh(ctx: &Context) -> Result<Answer<()>, String> {
    ctx.registry.clear_cache();
    answer((), 0, Vec::new())
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
    // One read of all selected sources (shared with the other views through the
    // registry cache), then counted per source.
    let mut rows: Vec<SourceOverview> = ctx
        .registry
        .detect_all(&ctx.env)
        .into_iter()
        .filter(|(source, _)| only.is_empty() || only.contains(source))
        .map(|(source, d)| SourceOverview {
            source,
            found: d.found,
            roots: d.roots,
            note: d.note,
            sessions: 0,
            events: 0,
            warnings: 0,
            errors: Vec::new(),
        })
        .collect();
    let mut errors = Vec::new();
    for r in ctx.registry.sessions(&ctx.env, &filter, &only) {
        match r {
            Ok(se) => {
                if let Some(row) = rows.iter_mut().find(|row| row.source == se.session.source) {
                    row.sessions += 1;
                    row.events += se.events.len() as u64;
                    row.warnings += se.warnings.len() as u64;
                }
            }
            Err(e) => errors.push(e.to_string()),
        }
    }
    let warnings = rows.iter().map(|r| r.warnings).sum();
    answer(rows, warnings, errors)
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

/// The options of the views that have them, as the views pass them to their analysis.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ExportOptions {
    /// Grouping of the cost (`model`, ...) and efficiency (`session`, ...) views.
    pub by: Option<String>,
    /// The what-if model of the cost view.
    pub what_if: Option<String>,
}

/// A file made from an analysis, not yet written.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Export {
    pub content: String,
    /// A file name that says what is in it, for the save dialog.
    pub suggested_name: String,
    pub warnings: u64,
    pub errors: Vec<String>,
}

fn json_of<T: Serialize>(value: &T) -> Result<String, String> {
    serde_json::to_string_pretty(value).map_err(|e| format!("cannot write the JSON: {e}"))
}

fn exported<T: Serialize>(view: &str, format: &str, answer: Answer<T>) -> Result<Export, String> {
    Ok(Export {
        content: json_of(&answer.data)?,
        suggested_name: file_name(view, format),
        warnings: answer.warnings,
        errors: answer.errors,
    })
}

/// The suggested name of an export, for the save dialog.
pub fn export_file_name(view: &str, format: &str) -> String {
    let view = if view == "skills" && format == "html" {
        "report"
    } else {
        view
    };
    file_name(view, format)
}

fn file_name(view: &str, format: &str) -> String {
    let day = Timestamp::now().to_string();
    format!("agentlens-{view}-{}.{format}", &day[..10])
}

/// Runs the analysis of `view` with the same filters and options as the view and returns it in
/// `format`: `json` for every view (what `--format json` of the CLI prints, for flow and cost the
/// whole data of the view) and `html` for the skills view (the file of `agentlens report`).
pub fn export(
    ctx: &Context,
    view: &str,
    format: &str,
    f: &Filters,
    options: &ExportOptions,
) -> Result<Export, String> {
    match (view, format) {
        ("skills", "json") => {
            let (filter, only) = f.resolve(Timestamp::now())?;
            let run = pipeline::skill_run(&ctx.registry, &ctx.env, &filter, &only, &CharsPerToken);
            Ok(Export {
                content: json_of(&run.report)?,
                suggested_name: file_name(view, format),
                warnings: run.warnings,
                errors: run.errors,
            })
        }
        ("skills", "html") => {
            let now = Timestamp::now();
            let (filter, only) = f.resolve(now)?;
            let run = pipeline::skill_run(&ctx.registry, &ctx.env, &filter, &only, &CharsPerToken);
            let (warnings, errors) = (run.warnings, run.errors.clone());
            let report = Report::from_skill_run(run, &filter, &only, now);
            let content = export::render_html(&report)
                .map_err(|e| format!("cannot write the report: {e}"))?;
            Ok(Export {
                content,
                suggested_name: file_name("report", format),
                warnings,
                errors,
            })
        }
        (_, "html") => Err("only the skills view can be exported as HTML".into()),
        ("overview", "json") => exported(view, format, overview(ctx, f)?),
        ("context", "json") => exported(view, format, context(ctx, f)?),
        ("unused", "json") => exported(view, format, unused(ctx, f)?),
        ("flow", "json") => exported(view, format, flow(ctx, f)?),
        ("compare", "json") => exported(view, format, compare(ctx, f)?),
        ("cost", "json") => {
            let by = options.by.as_deref().unwrap_or("model");
            exported(view, format, cost(ctx, f, by, options.what_if.as_deref())?)
        }
        ("efficiency", "json") => {
            let by = options.by.as_deref().unwrap_or("session");
            exported(view, format, efficiency(ctx, f, by)?)
        }
        (view, "json") => Err(format!("the view `{view}` cannot be exported")),
        (_, other) => Err(format!("unknown format `{other}`")),
    }
}
