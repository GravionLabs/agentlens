//! The analyses as plain functions with one filter and one answer shape, shared by the desktop
//! application's commands and the MCP server (and usable by any other tool that embeds the core).
//!
//! Each function takes the same filter as the CLI (period, sources, project)
//! and returns the report types of the core, so the application shows the
//! numbers the CLI prints. The functions do not depend on Tauri and are tested
//! against fixture homes.

use std::path::PathBuf;
use std::time::Duration;

use crate::adapter::{Registry, SessionFilter};
use crate::adapters::default_registry;
use crate::analyzers::advice::AdviceReport;
use crate::analyzers::compare::CompareReport;
use crate::analyzers::context::ContextReport;
use crate::analyzers::cost::{CostReport, Grouping, WhatIfReport};
use crate::analyzers::efficiency::{CacheBy, EfficiencyReport};
use crate::analyzers::flow::{ErrorReport, GrowthReport, HooksReport};
use crate::analyzers::skills::SkillReport;
use crate::analyzers::timeline::{SessionSummary, Timeline};
use crate::analyzers::unused::UnusedReport;
use crate::env::Env;
use crate::export::{self, csv as csv_export};
use crate::inventory::Duplicate;
use crate::model::Source;
use crate::pipeline;
use crate::prices::{self, PriceTable};
use crate::report::Report;
use crate::since::parse_since;
use crate::tokens::CharsPerToken;
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
    /// The version of the layout of `data` (`report::SCHEMA_VERSION`): raised when a field of a report is
    /// renamed, removed or retyped, not when one is added.
    pub schema_version: u32,
    pub data: T,
    pub warnings: u64,
    pub errors: Vec<String>,
}

fn answer<T>(data: T, warnings: u64, errors: Vec<String>) -> Result<Answer<T>, String> {
    Ok(Answer {
        schema_version: crate::report::SCHEMA_VERSION,
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

pub fn advice(ctx: &Context, f: &Filters) -> Result<Answer<AdviceReport>, String> {
    let (filter, only) = f.resolve(Timestamp::now())?;
    let run = pipeline::advice_run(&ctx.registry, &ctx.env, &filter, &only, &CharsPerToken);
    answer(run.report, run.warnings, run.errors)
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
    /// The unused view lists items that have been used too, like `--all`.
    pub all: Option<bool>,
    /// The tab of the flow view: `growth`, `errors` or `hooks`.
    pub section: Option<String>,
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

fn csv_file(
    view: &str,
    content: Result<String, csv_export::CsvError>,
    warnings: u64,
    errors: Vec<String>,
) -> Result<Export, String> {
    Ok(Export {
        content: content.map_err(|e| e.to_string())?,
        suggested_name: file_name(view, "csv"),
        warnings,
        errors,
    })
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
/// `format`: `csv` for the views whose CLI command has `--format csv` (the same columns), `json` for every view (what `--format json` of the CLI prints, for flow and cost the
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
        ("advice", "json") => exported(view, format, advice(ctx, f)?),
        ("advice", "csv") => {
            let a = advice(ctx, f)?;
            csv_file(view, csv_export::advice(&a.data), a.warnings, a.errors)
        }
        ("skills", "csv") => {
            let a = skills(ctx, f)?;
            let mut list: Vec<_> = a.data.report.skills.iter().collect();
            list.sort_by(|x, y| {
                y.overhead_tokens()
                    .cmp(&x.overhead_tokens())
                    .then_with(|| x.name.cmp(&y.name))
            });
            csv_file(view, csv_export::skills(&list), a.warnings, a.errors)
        }
        ("context", "csv") => {
            let a = context(ctx, f)?;
            let rows: Vec<_> = a.data.rows.iter().collect();
            csv_file(view, csv_export::context(&rows), a.warnings, a.errors)
        }
        ("unused", "csv") => {
            let a = unused(ctx, f)?;
            let all = options.all.unwrap_or(false);
            let items: Vec<_> = a.data.items.iter().filter(|i| all || i.uses == 0).collect();
            csv_file(view, csv_export::unused(&items), a.warnings, a.errors)
        }
        ("compare", "csv") => {
            let a = compare(ctx, f)?;
            csv_file(
                view,
                csv_export::compare(&a.data.sources),
                a.warnings,
                a.errors,
            )
        }
        ("cost", "csv") => {
            let by = options.by.as_deref().unwrap_or("model");
            let a = cost(ctx, f, by, options.what_if.as_deref())?;
            let content = match &a.data.what_if {
                Some(w) => csv_export::what_if(w),
                None => csv_export::cost(&a.data.report.rows, by),
            };
            csv_file(view, content, a.warnings, a.errors)
        }
        ("efficiency", "csv") => {
            let by = options.by.as_deref().unwrap_or("session");
            let a = efficiency(ctx, f, by)?;
            csv_file(view, csv_export::cache(&a.data.cache), a.warnings, a.errors)
        }
        ("flow", "csv") => {
            let a = flow(ctx, f)?;
            let content = match options.section.as_deref().unwrap_or("growth") {
                "growth" => csv_export::growth(&a.data.growth.sessions),
                "errors" => csv_export::errors(&a.data.errors.tools),
                "hooks" => csv_export::hooks(&a.data.hooks.hooks),
                other => return Err(format!("unknown section `{other}`")),
            };
            csv_file(view, content, a.warnings, a.errors)
        }
        (view, "csv") => Err(format!("the view `{view}` cannot be exported as CSV")),
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
