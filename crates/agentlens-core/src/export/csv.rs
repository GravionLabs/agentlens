//! The CSV of each analysis: the columns and the formatting of `--format csv`. The CLI prints the
//! result and the application saves it, so both produce the same file.
//!
//! Counts are plain integers, money and rates have six decimals, an unknown value is an empty
//! cell. Measured and estimated values are separate columns where a report distinguishes them.

use std::fmt;
use std::path::PathBuf;

use crate::analyzers::advice::AdviceReport;
use crate::analyzers::compare::SourceStats;
use crate::analyzers::context::ContextRow;
use crate::analyzers::cost::{CostRow, WhatIfReport};
use crate::analyzers::efficiency::CacheRow;
use crate::analyzers::flow::{HookStats, SessionGrowth, ToolErrors};
use crate::analyzers::skills::SkillStats;
use crate::analyzers::unused::{UnusedItem, UnusedKind};
use crate::model::ContextKind;

/// Writing the CSV failed (only an I/O or encoding problem of the writer).
#[derive(Debug)]
pub struct CsvError(String);

impl fmt::Display for CsvError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "cannot write the CSV: {}", self.0)
    }
}

impl std::error::Error for CsvError {}

/// A header and rows as CSV text.
pub fn write(header: &[&str], rows: Vec<Vec<String>>) -> Result<String, CsvError> {
    let err = |e: &dyn fmt::Display| CsvError(e.to_string());
    let mut w = csv::Writer::from_writer(Vec::new());
    w.write_record(header).map_err(|e| err(&e))?;
    for row in rows {
        w.write_record(row).map_err(|e| err(&e))?;
    }
    let bytes = w.into_inner().map_err(|e| err(&e))?;
    String::from_utf8(bytes).map_err(|e| err(&e))
}

fn project(p: &Option<PathBuf>) -> String {
    p.as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "-".into())
}

fn kind_id(kind: ContextKind) -> String {
    serde_json::to_value(kind)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

fn unused_kind(k: UnusedKind) -> &'static str {
    match k {
        UnusedKind::Skill => "skill",
        UnusedKind::Agent => "agent",
        UnusedKind::McpServer => "mcp-server",
    }
}

fn attributed_tokens(s: &SkillStats) -> u64 {
    let a = &s.attributed;
    a.input + a.output + a.cache_read + a.cache_write
}

/// `skills`: one row per skill, in the order given.
pub fn skills(skills: &[&SkillStats]) -> Result<String, CsvError> {
    let header = [
        "skill",
        "in_inventory",
        "uses",
        "sessions",
        "load_tokens",
        "load_measured",
        "carry_tokens",
        "baseline_tokens",
        "overhead_tokens",
        "follow_on_requests",
        "follow_on_input",
        "follow_on_output",
        "attributed_requests",
        "attributed_tokens",
        "baseline_measured",
        "carry_measured",
        "overhead_measured",
        "file_uses",
    ];
    let rows = skills
        .iter()
        .map(|s| {
            vec![
                s.name.clone(),
                s.in_inventory.to_string(),
                s.invocations.to_string(),
                s.sessions.to_string(),
                s.load.count.to_string(),
                s.load.measured.to_string(),
                s.carry.tokens.count.to_string(),
                s.baseline.tokens.count.to_string(),
                s.overhead_tokens().to_string(),
                s.follow_on.requests.to_string(),
                (s.follow_on.input + s.follow_on.cache_read + s.follow_on.cache_write).to_string(),
                s.follow_on.output.to_string(),
                s.attributed.requests.to_string(),
                attributed_tokens(s).to_string(),
                s.baseline.tokens.measured.to_string(),
                s.carry.tokens.measured.to_string(),
                (s.baseline.tokens.measured && s.load.measured && s.carry.tokens.measured)
                    .to_string(),
                s.file_uses.to_string(),
            ]
        })
        .collect();
    write(&header, rows)
}

/// `context`: one row per source, project, kind and owner.
pub fn context(rows: &[&ContextRow]) -> Result<String, CsvError> {
    let header = [
        "source",
        "project",
        "kind",
        "owner",
        "items",
        "bytes",
        "tokens",
        "requests",
        "overhead_tokens",
        "tokens_measured",
    ];
    let body = rows
        .iter()
        .map(|r| {
            vec![
                r.source.id().to_owned(),
                project(&r.project),
                kind_id(r.kind),
                r.owner.clone(),
                r.items.to_string(),
                r.bytes.to_string(),
                r.tokens.count.to_string(),
                r.requests.to_string(),
                r.overhead_tokens.count.to_string(),
                r.tokens.measured.to_string(),
            ]
        })
        .collect();
    write(&header, body)
}

/// `unused`: one row per skill, agent or MCP server.
pub fn unused(items: &[&UnusedItem]) -> Result<String, CsvError> {
    let header = [
        "kind",
        "name",
        "project",
        "clients",
        "uses",
        "requests",
        "overhead_tokens",
        "overhead_measured",
    ];
    let rows = items
        .iter()
        .map(|i| {
            vec![
                unused_kind(i.kind).to_owned(),
                i.name.clone(),
                project(&i.project),
                i.clients
                    .iter()
                    .map(|c| c.id())
                    .collect::<Vec<_>>()
                    .join(" "),
                i.uses.to_string(),
                i.requests.to_string(),
                i.overhead_tokens
                    .map(|t| t.count.to_string())
                    .unwrap_or_default(),
                i.overhead_tokens
                    .map(|t| t.measured.to_string())
                    .unwrap_or_default(),
            ]
        })
        .collect();
    write(&header, rows)
}

/// `compare`: one row per source; a value the client does not record is an empty cell.
pub fn compare(sources: &[SourceStats]) -> Result<String, CsvError> {
    let header = [
        "source",
        "sessions",
        "requests",
        "input_tokens",
        "output_tokens",
        "cache_hit_rate",
        "tool_calls",
        "tool_errors",
        "tool_error_rate",
        "skill_invocations",
        "skill_overhead_tokens",
        "total_duration_ms",
        "mean_session_ms",
        "skill_overhead_measured",
    ];
    let opt = |v: Option<String>| v.unwrap_or_default();
    let rows = sources
        .iter()
        .map(|s| {
            vec![
                s.source.id().to_owned(),
                s.sessions.to_string(),
                opt(s.requests.map(|v| v.to_string())),
                opt(s.input_tokens.map(|v| v.to_string())),
                opt(s.output_tokens.map(|v| v.to_string())),
                opt(s.cache_hit_rate.map(|v| format!("{v:.6}"))),
                s.tool_calls.to_string(),
                s.tool_errors.to_string(),
                opt(s.tool_error_rate.map(|v| format!("{v:.6}"))),
                s.skill_invocations.to_string(),
                opt(s.skill_overhead_tokens.map(|v| v.count.to_string())),
                opt(s.total_duration_ms.map(|v| v.to_string())),
                opt(s.mean_session_ms.map(|v| v.to_string())),
                opt(s.skill_overhead_tokens.map(|v| v.measured.to_string())),
            ]
        })
        .collect();
    write(&header, rows)
}

/// `cost`: one row per group; `label` names the first column (`model`, `project`, ...).
pub fn cost(rows: &[CostRow], label: &str) -> Result<String, CsvError> {
    let header = [
        label,
        "requests",
        "input_tokens",
        "output_tokens",
        "cache_read_tokens",
        "cache_write_tokens",
        "measured_usd",
        "estimated_usd",
        "unpriced_requests",
        "premium_requests",
    ];
    let body = rows
        .iter()
        .map(|x| {
            vec![
                x.key.clone(),
                x.requests.to_string(),
                x.input.to_string(),
                x.output.to_string(),
                x.cache_read.to_string(),
                x.cache_write.to_string(),
                format!("{:.6}", x.measured_usd),
                format!("{:.6}", x.estimated_usd),
                x.unpriced_requests.to_string(),
                x.premium_requests.to_string(),
            ]
        })
        .collect();
    write(&header, body)
}

/// `cost --what-if`: the actual and the what-if price per model.
pub fn what_if(w: &WhatIfReport) -> Result<String, CsvError> {
    let header = [
        "model",
        "requests",
        "actual_usd",
        "what_if_usd",
        "excluded_requests",
    ];
    let rows = w
        .rows
        .iter()
        .map(|x| {
            vec![
                x.model.clone(),
                x.requests.to_string(),
                format!("{:.6}", x.actual_usd),
                format!("{:.6}", x.what_if_usd),
                x.excluded_requests.to_string(),
            ]
        })
        .collect();
    write(&header, rows)
}

/// `efficiency`: the cache table.
pub fn cache(rows: &[CacheRow]) -> Result<String, CsvError> {
    let header = [
        "key",
        "source",
        "requests",
        "input_tokens",
        "cache_read_tokens",
        "cache_write_tokens",
        "hit_rate",
    ];
    let body = rows
        .iter()
        .map(|r| {
            vec![
                r.key.clone(),
                r.source.id().to_owned(),
                r.requests.to_string(),
                r.input.to_string(),
                r.cache_read.to_string(),
                r.cache_write.to_string(),
                r.hit_rate.map(|h| format!("{h:.6}")).unwrap_or_default(),
            ]
        })
        .collect();
    write(&header, body)
}

/// `flow --section hooks`.
pub fn hooks(hooks: &[HookStats]) -> Result<String, CsvError> {
    let header = [
        "source",
        "hook",
        "runs",
        "failures",
        "no_outcome",
        "total_ms",
    ];
    let rows = hooks
        .iter()
        .map(|h| {
            vec![
                h.source.id().to_owned(),
                h.hook.clone(),
                h.runs.to_string(),
                h.failures.to_string(),
                h.unknown.to_string(),
                h.total_ms.to_string(),
            ]
        })
        .collect();
    write(&header, rows)
}

/// `flow --section growth`.
pub fn growth(growth: &[SessionGrowth]) -> Result<String, CsvError> {
    let header = [
        "source",
        "session",
        "project",
        "model",
        "requests",
        "first_tokens",
        "last_tokens",
        "peak_tokens",
        "compactions",
        "compaction_tokens",
        "near_limit",
    ];
    let rows = growth
        .iter()
        .map(|g| {
            vec![
                g.source.id().to_owned(),
                g.session.clone(),
                project(&g.project),
                g.model.clone().unwrap_or_default(),
                g.requests.to_string(),
                g.first_tokens.to_string(),
                g.last_tokens.to_string(),
                g.peak_tokens.to_string(),
                g.compactions.len().to_string(),
                g.compactions
                    .iter()
                    .map(u64::to_string)
                    .collect::<Vec<_>>()
                    .join(" "),
                g.near_limit.to_string(),
            ]
        })
        .collect();
    write(&header, rows)
}

/// `flow --section errors`.
pub fn errors(tools: &[ToolErrors]) -> Result<String, CsvError> {
    let header = [
        "source",
        "tool",
        "calls",
        "errors",
        "unknown",
        "retry_loops",
    ];
    let rows = tools
        .iter()
        .map(|t| {
            vec![
                t.source.id().to_owned(),
                t.tool.clone(),
                t.calls.to_string(),
                t.errors.to_string(),
                t.unknown.to_string(),
                t.loops.to_string(),
            ]
        })
        .collect();
    write(&header, rows)
}

/// `advise`: one row per skill of each finding.
pub fn advice(report: &AdviceReport) -> Result<String, CsvError> {
    let header = [
        "finding",
        "skill",
        "tokens",
        "tokens_measured",
        "requests",
        "detail",
    ];
    let mut rows = Vec::new();
    for f in &report.findings {
        let kind = serde_json::to_value(f.kind)
            .ok()
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_default();
        for i in &f.items {
            rows.push(vec![
                kind.clone(),
                i.name.clone(),
                i.tokens.count.to_string(),
                i.tokens.measured.to_string(),
                i.requests.to_string(),
                i.detail.clone(),
            ]);
        }
    }
    write(&header, rows)
}
