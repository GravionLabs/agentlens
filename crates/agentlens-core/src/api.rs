//! The analyses as plain functions with one filter and one answer shape, shared by the desktop
//! application's commands and the MCP server (and usable by any other tool that embeds the core).
//!
//! Each function takes the same filter as the CLI (period, sources, project)
//! and returns the report types of the core, so the application shows the
//! numbers the CLI prints. The functions do not depend on Tauri and are tested
//! against fixture homes.

use std::path::{Path, PathBuf};
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
use crate::cli_tools::{self, CliTools};
use crate::env::Env;
use crate::export::{self, csv as csv_export};
use crate::inventory::Duplicate;
use crate::model::Source;
use crate::pipeline;
use crate::prices::{self, PriceTable};
use crate::report::Report;
use crate::since::{parse_window, window_start};
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
    /// The end of the period (exclusive), in the same forms as `since`.
    pub until: Option<String>,
    /// Source ids (`claude-code`, `opencode`, ...); empty means all.
    pub sources: Vec<String>,
    /// Only sessions that ran in this directory or below it.
    pub project: Option<PathBuf>,
    /// Only sessions with one of these ids; empty means all.
    pub session: Vec<String>,
}

/// Length in days of the period the unused view looks at when no `since` is given.
pub const DEFAULT_UNUSED_DAYS: u32 = 30;

impl Filters {
    fn resolve(&self, now: Timestamp) -> Result<(SessionFilter, Vec<Source>), String> {
        self.resolve_named(now, ("since", "until"))
    }

    /// Like the resolution every analysis does, with the labels `names` (`since`, `until`) in the errors
    /// about the two bounds; a command line passes `--since` and `--until`.
    pub fn resolve_named(
        &self,
        now: Timestamp,
        names: (&str, &str),
    ) -> Result<(SessionFilter, Vec<Source>), String> {
        // Relative periods count from a five-minute step, so that the views asking one
        // after the other share the sessions the registry cache holds for that selection.
        let now = Timestamp::from_second(now.as_second() - now.as_second().rem_euclid(300))
            .unwrap_or(now);
        // Both bounds count from the same rounded `now`, so `until` steps with `since`.
        let (since, until) =
            parse_window(self.since.as_deref(), self.until.as_deref(), now, names)?;
        let mut sources = Vec::new();
        for id in &self.sources {
            sources.push(Source::from_id(id).ok_or_else(|| format!("unknown source `{id}`"))?);
        }
        Ok((
            SessionFilter {
                since,
                until,
                project: self.project.clone().filter(|p| !p.as_os_str().is_empty()),
                session: self
                    .session
                    .iter()
                    .filter(|s| !s.trim().is_empty())
                    .cloned()
                    .collect(),
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

/// Whether the command line tools are in `~/.local/bin` (writes nothing). `bin_dir` is the directory of
/// the binaries shipped with the application (the directory of its executable).
pub fn cli_tools_status(ctx: &Context, bin_dir: &Path) -> Result<Answer<CliTools>, String> {
    let data = cli_tools::status(&ctx.env, bin_dir);
    let errors = data.errors();
    answer(data, 0, errors)
}

/// Links (or, in an AppImage, copies) the command line tools into `~/.local/bin`, on the user's click.
/// An error where the action is not offered (Linux packages, Windows): nothing is written there.
pub fn install_cli_tools(ctx: &Context, bin_dir: &Path) -> Result<Answer<CliTools>, String> {
    if !cli_tools::offered(&ctx.env) {
        return Err(cli_tools::not_offered_reason(&ctx.env).to_string());
    }
    let data = cli_tools::install(&ctx.env, bin_dir);
    let errors = data.errors();
    answer(data, 0, errors)
}

/// Removes the links (or copies) [`install_cli_tools`] made; anything else in `~/.local/bin` stays.
/// An error where the action is not offered, like [`install_cli_tools`].
pub fn remove_cli_tools(ctx: &Context, bin_dir: &Path) -> Result<Answer<CliTools>, String> {
    if !cli_tools::offered(&ctx.env) {
        return Err(cli_tools::not_offered_reason(&ctx.env).to_string());
    }
    let data = cli_tools::remove(&ctx.env, bin_dir);
    let errors = data.errors();
    answer(data, 0, errors)
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
    // Advice only ranks by price: a broken price file falls back to the bundled table (fail soft).
    let (prices, price_warning) = prices::load_or_bundled(&ctx.env, None);
    let mut run = pipeline::advice_run(
        &ctx.registry,
        &ctx.env,
        &filter,
        &only,
        &CharsPerToken,
        &prices,
    );
    run.errors.extend(price_warning);
    answer(run.report, run.warnings, run.errors)
}

pub fn context(ctx: &Context, f: &Filters) -> Result<Answer<ContextReport>, String> {
    let (filter, only) = f.resolve(Timestamp::now())?;
    let run = pipeline::context_run(&ctx.registry, &ctx.env, &filter, &only, &CharsPerToken);
    answer(run.report, run.warnings, run.errors)
}

/// Defaults to the last [`DEFAULT_UNUSED_DAYS`] days if the filter has no period.
pub fn unused(ctx: &Context, f: &Filters) -> Result<Answer<UnusedReport>, String> {
    let now = Timestamp::now();
    let (filter, only) = f.resolve(now)?;
    let filter = with_unused_window(filter, DEFAULT_UNUSED_DAYS, now)?;
    let run = pipeline::unused_run(&ctx.registry, &ctx.env, &filter, &only, &CharsPerToken);
    answer(run.report, run.warnings, run.errors)
}

/// Gives a filter without `since` the start of a period of `days` days that ends at its `until`, or now.
pub fn with_unused_window(
    mut filter: SessionFilter,
    days: u32,
    now: Timestamp,
) -> Result<SessionFilter, String> {
    if filter.since.is_none() {
        filter.since = Some(window_start(days, filter.until, now)?);
    }
    Ok(filter)
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

/// `by` is `model`, `project`, `branch`, `issue` or `session`; `what_if` is a model id prefix.
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
        "session" => Grouping::Session,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> Timestamp {
        "2026-10-10T12:03:20Z".parse().unwrap()
    }

    fn filters(since: Option<&str>, until: Option<&str>) -> Filters {
        Filters {
            since: since.map(Into::into),
            until: until.map(Into::into),
            ..Filters::default()
        }
    }

    fn bounds(f: &Filters) -> (Option<String>, Option<String>) {
        let (filter, _) = f.resolve(now()).unwrap();
        (
            filter.since.map(|t| t.to_string()),
            filter.until.map(|t| t.to_string()),
        )
    }

    #[test]
    fn advice_does_not_fail_on_a_broken_price_file() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = Context {
            env: Env::for_home(dir.path()),
            registry: default_registry(),
        };
        let file = prices::user_file(&ctx.env);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, "{ nope").unwrap();
        assert!(
            cost(&ctx, &Filters::default(), "model", None).is_err(),
            "cost fails hard"
        );
        let a = advice(&ctx, &Filters::default()).expect("advice falls back to the bundled prices");
        assert!(
            a.errors
                .iter()
                .any(|e| e.starts_with("cannot load prices: ")
                    && e.contains(&file.display().to_string())
                    && e.ends_with("; the bundled prices are used")),
            "{:?}",
            a.errors
        );
    }

    #[test]
    fn until_is_resolved_like_since() {
        assert_eq!(
            bounds(&filters(Some("2026-10-01"), Some("2026-10-08"))),
            (
                Some("2026-10-01T00:00:00Z".into()),
                Some("2026-10-08T00:00:00Z".into())
            )
        );
        assert_eq!(
            bounds(&filters(None, Some("2026-10-08T06:30:00Z"))),
            (None, Some("2026-10-08T06:30:00Z".into()))
        );
    }

    #[test]
    fn a_relative_until_counts_from_the_same_five_minute_step_as_since() {
        // 12:03:20 rounds down to 12:00:00 for both bounds.
        assert_eq!(
            bounds(&filters(Some("2d"), Some("1h"))),
            (
                Some("2026-10-08T12:00:00Z".into()),
                Some("2026-10-10T11:00:00Z".into())
            )
        );
    }

    #[test]
    fn an_empty_until_means_no_upper_bound() {
        assert_eq!(bounds(&filters(None, Some("  "))), (None, None));
        assert_eq!(bounds(&Filters::default()), (None, None));
    }

    #[test]
    fn until_not_after_since_is_an_error_naming_until() {
        for until in ["2026-10-01", "2026-09-30"] {
            let err = filters(Some("2026-10-01"), Some(until))
                .resolve(now())
                .unwrap_err();
            assert!(err.contains("until"), "{err}");
        }
    }

    #[test]
    fn an_unparsable_until_is_an_error_naming_until() {
        let err = filters(None, Some("soon")).resolve(now()).unwrap_err();
        assert!(
            err.contains("until") && err.contains("cannot parse"),
            "{err}"
        );
    }

    #[test]
    fn resolve_named_labels_the_errors_with_the_given_names() {
        let err = filters(Some("soon"), None)
            .resolve_named(now(), ("--since", "--until"))
            .unwrap_err();
        assert!(err.starts_with("--since: cannot parse"), "{err}");
        let err = filters(Some("2026-10-02"), Some("2026-10-01"))
            .resolve_named(now(), ("--since", "--until"))
            .unwrap_err();
        assert!(err.starts_with("--until ("), "{err}");
    }

    #[test]
    fn an_unknown_source_is_an_error() {
        let f = Filters {
            sources: vec!["nope".into()],
            ..Filters::default()
        };
        assert_eq!(f.resolve(now()).unwrap_err(), "unknown source `nope`");
    }

    #[test]
    fn the_unused_window_defaults_to_the_shared_thirty_days_and_keeps_a_given_since() {
        assert_eq!(DEFAULT_UNUSED_DAYS, 30);
        let (open, _) = filters(None, None).resolve(now()).unwrap();
        let w = with_unused_window(open, DEFAULT_UNUSED_DAYS, now()).unwrap();
        assert_eq!(w.since.unwrap().to_string(), "2026-09-10T12:03:20Z");
        let (given, _) = filters(Some("2026-10-01"), None).resolve(now()).unwrap();
        let w = with_unused_window(given, 7, now()).unwrap();
        assert_eq!(w.since.unwrap().to_string(), "2026-10-01T00:00:00Z");
    }

    #[test]
    fn filters_deserialize_with_or_without_until() {
        let f: Filters =
            serde_json::from_str(r#"{"since":"7d","until":"1d","sources":[]}"#).unwrap();
        assert_eq!(f.until.as_deref(), Some("1d"));
        let f: Filters =
            serde_json::from_str(r#"{"since":"7d","sources":[],"project":null}"#).unwrap();
        assert_eq!(f.until, None);
        let f: Filters = serde_json::from_str(r#"{"until":null}"#).unwrap();
        assert_eq!(f.until, None);
    }

    /// A home with one Claude Code session on 2026-01-10 (long before any `now` of a test run).
    fn old_session_context() -> (tempfile::TempDir, Context) {
        let dir = tempfile::tempdir().unwrap();
        let projects = dir.path().join(".claude/projects/-work-demo");
        std::fs::create_dir_all(&projects).unwrap();
        let line = |ts: &str, body: &str| format!("{{\"timestamp\":\"{ts}\",{body}}}\n");
        let text = line(
            "2026-01-10T10:00:00Z",
            r#""type":"user","sessionId":"old","cwd":"/work/demo","message":{"role":"user","content":"hi"}"#,
        );
        std::fs::write(projects.join("old.jsonl"), text).unwrap();
        let ctx = Context {
            env: Env::for_home(dir.path()),
            registry: default_registry(),
        };
        (dir, ctx)
    }

    #[test]
    fn the_default_unused_window_is_thirty_days_before_until() {
        let (_dir, ctx) = old_session_context();
        let f = filters(None, Some("2026-01-20"));
        let a = unused(&ctx, &f).unwrap();
        assert_eq!(a.data.sessions, 1);
        // Counted from now, the default window would end long after `until` and be empty.
        let f = filters(None, Some("2026-01-05"));
        assert_eq!(unused(&ctx, &f).unwrap().data.sessions, 0);
    }
}
