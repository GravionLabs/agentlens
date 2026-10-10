//! OpenTelemetry exports (OTLP JSON, one export per line) of Claude Code.
//!
//! Claude Code has no file exporter; the lines come from an OpenTelemetry
//! Collector `file` exporter fed with the logs signal. Only the documented log
//! events are read: `claude_code.user_prompt`, `.api_request`, `.skill_activated`
//! and `.tool_result`. Prompt text, tool input and commands are never kept.
//! Metrics and traces lines are counted and skipped.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

use jiff::Timestamp;
use serde_json::Value;

use crate::adapter::{Adapter, AdapterError, Detection, SessionFilter, SessionIter};
use crate::env::Env;
use crate::model::{
    EventKind, Session, SessionEvents, SkillInvoked, SkillTrigger, Source, ToolCall, ToolStatus,
    Usage, Warning,
};

/// Reads the given files; registered only when the user names OTel files.
pub struct OtelAdapter {
    files: Vec<PathBuf>,
}

impl OtelAdapter {
    pub fn new(files: Vec<PathBuf>) -> Self {
        OtelAdapter { files }
    }
}

impl Adapter for OtelAdapter {
    fn id(&self) -> Source {
        Source::Otel
    }

    fn is_export(&self) -> bool {
        true
    }

    fn sources(&self) -> Vec<Source> {
        vec![Source::Otel, Source::ClaudeCode]
    }

    fn detect(&self, _env: &Env) -> Detection {
        let existing: Vec<PathBuf> = self.files.iter().filter(|f| f.is_file()).cloned().collect();
        if existing.is_empty() {
            Detection::not_found("no OpenTelemetry file given (use --otel FILE)")
        } else {
            Detection::found(existing)
        }
    }

    fn sessions<'a>(&'a self, _env: &'a Env, _filter: &'a SessionFilter) -> SessionIter<'a> {
        match read_files(&self.files) {
            Ok(sessions) => Box::new(sessions.into_iter().map(Ok)),
            Err(e) => Box::new(std::iter::once(Err(e))),
        }
    }
}

/// An attribute value as OTLP/JSON encodes it.
fn attr_map(list: Option<&Value>) -> BTreeMap<&str, &Value> {
    list.and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|kv| Some((kv.get("key")?.as_str()?, kv.get("value")?)))
                .collect()
        })
        .unwrap_or_default()
}

fn text(v: &Value) -> Option<&str> {
    v.get("stringValue").and_then(Value::as_str)
}

fn int(v: &Value) -> Option<u64> {
    match v.get("intValue")? {
        Value::String(s) => s.parse().ok(),
        Value::Number(n) => n.as_u64(),
        _ => None,
    }
}

fn float(v: &Value) -> Option<f64> {
    v.get("doubleValue")
        .and_then(Value::as_f64)
        .or_else(|| int(v).map(|i| i as f64))
}

/// `true` for `boolValue: true` and for the string `"true"` (Claude Code sends strings).
fn flag(v: &Value) -> Option<bool> {
    v.get("boolValue")
        .and_then(Value::as_bool)
        .or_else(|| text(v).map(|s| s == "true"))
}

struct Record<'a> {
    attrs: BTreeMap<&'a str, &'a Value>,
    resource: &'a BTreeMap<&'a str, &'a Value>,
    time_nanos: Option<&'a Value>,
    event_name: Option<&'a str>,
}

impl<'a> Record<'a> {
    fn get(&self, key: &str) -> Option<&'a Value> {
        self.attrs
            .get(key)
            .copied()
            .or_else(|| self.resource.get(key).copied())
    }

    fn text(&self, key: &str) -> Option<&'a str> {
        self.get(key).and_then(text).filter(|s| !s.is_empty())
    }

    fn at(&self) -> Option<Timestamp> {
        if let Some(t) = self.text("event.timestamp").and_then(|s| s.parse().ok()) {
            return Some(t);
        }
        let nanos: i128 = match self.time_nanos? {
            Value::String(s) => s.parse().ok()?,
            Value::Number(n) => n.as_i64()?.into(),
            _ => return None,
        };
        Timestamp::from_nanosecond(nanos).ok()
    }
}

fn read_files(files: &[PathBuf]) -> Result<Vec<SessionEvents>, AdapterError> {
    let mut sessions: BTreeMap<String, SessionEvents> = BTreeMap::new();
    let mut skipped: BTreeMap<String, u64> = BTreeMap::new();
    for path in files {
        let file = File::open(path).map_err(|source| AdapterError::Io {
            path: path.clone(),
            source,
        })?;
        for line in BufReader::new(file).lines() {
            let line = line.map_err(|source| AdapterError::Io {
                path: path.clone(),
                source,
            })?;
            if line.trim().is_empty() {
                continue;
            }
            let Ok(v) = serde_json::from_str::<Value>(&line) else {
                *skipped.entry("invalid JSON".into()).or_default() += 1;
                continue;
            };
            let Some(resources) = v.get("resourceLogs").and_then(Value::as_array) else {
                let kind = ["resourceMetrics", "resourceSpans"]
                    .into_iter()
                    .find(|k| v.get(*k).is_some())
                    .unwrap_or("unknown line");
                *skipped.entry(kind.to_owned()).or_default() += 1;
                continue;
            };
            for rl in resources {
                let resource = attr_map(rl.get("resource").and_then(|r| r.get("attributes")));
                for scope in rl
                    .get("scopeLogs")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    for rec in scope
                        .get("logRecords")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                    {
                        let attrs = attr_map(rec.get("attributes"));
                        let event_name = rec
                            .get("eventName")
                            .and_then(Value::as_str)
                            .or_else(|| attrs.get("event.name").and_then(|v| text(v)))
                            .or_else(|| rec.get("body").and_then(text));
                        let record = Record {
                            attrs,
                            resource: &resource,
                            time_nanos: rec.get("timeUnixNano"),
                            event_name,
                        };
                        handle(&record, &mut sessions, &mut skipped);
                    }
                }
            }
        }
    }
    let skipped_total: u64 = skipped.values().sum();
    let mut out: Vec<SessionEvents> = sessions.into_values().collect();
    for se in &mut out {
        se.events.sort_by_key(|e| e.at);
        se.session.started_at = se.events.iter().filter_map(|e| e.at).min();
        se.session.ended_at = se.events.iter().filter_map(|e| e.at).max();
    }
    if skipped_total > 0 {
        let list: Vec<String> = skipped.iter().map(|(k, n)| format!("{k} ({n})")).collect();
        let warning = Warning::new(format!(
            "skipped OpenTelemetry records: {}",
            list.join(", ")
        ));
        match out.first_mut() {
            Some(first) => first.warn(warning),
            None => {
                let mut se = SessionEvents::new(Session::new("otel-import", Source::Otel));
                se.warn(warning);
                out.push(se);
            }
        }
    }
    Ok(out)
}

fn handle(
    r: &Record,
    sessions: &mut BTreeMap<String, SessionEvents>,
    skipped: &mut BTreeMap<String, u64>,
) {
    let Some(name) = r.event_name.filter(|n| n.starts_with("claude_code.")) else {
        *skipped.entry("other log records".into()).or_default() += 1;
        return;
    };
    let Some(id) = r.text("session.id") else {
        *skipped
            .entry("records without session.id".into())
            .or_default() += 1;
        return;
    };
    let se = sessions
        .entry(id.to_owned())
        .or_insert_with(|| SessionEvents::new(Session::new(id, Source::ClaudeCode)));
    let at = r.at();
    let uint = |key: &str| r.get(key).and_then(int).unwrap_or(0);
    match name {
        "claude_code.user_prompt" => se.push(at, EventKind::UserTurn),
        "claude_code.api_request" => {
            let model = r.text("model").map(str::to_owned);
            if se.session.model.is_none() {
                se.session.model = model.clone();
            }
            let query_source = r.text("query_source").unwrap_or("repl_main_thread");
            se.push(
                at,
                EventKind::Usage(Usage {
                    model,
                    input: uint("input_tokens"),
                    output: uint("output_tokens"),
                    cache_read: uint("cache_read_tokens"),
                    cache_write: uint("cache_creation_tokens"),
                    cost_usd: r.get("cost_usd").and_then(float),
                    measured: true,
                    subagent: !matches!(query_source, "repl_main_thread" | "compact"),
                    skill: r.text("skill.name").map(str::to_owned),
                    ..Usage::default()
                }),
            );
        }
        "claude_code.skill_activated" => {
            let Some(skill) = r.text("skill.name") else {
                *skipped
                    .entry("skill_activated without skill.name".into())
                    .or_default() += 1;
                return;
            };
            let trigger = match r.text("invocation_trigger") {
                Some("user-slash") => SkillTrigger::User,
                Some("nested-skill") => SkillTrigger::Nested,
                _ => SkillTrigger::Model,
            };
            se.push(
                at,
                EventKind::SkillInvoked(SkillInvoked::new(skill, trigger)),
            );
        }
        "claude_code.tool_result" => {
            let tool = r.text("tool_name").unwrap_or("unknown");
            let status = match r.get("success").and_then(flag) {
                Some(true) => ToolStatus::Ok,
                Some(false) => ToolStatus::Error,
                None => ToolStatus::Unknown,
            };
            se.push(
                at,
                EventKind::ToolCall(ToolCall {
                    name: tool.to_owned(),
                    // Parameters are only exported with OTEL_LOG_TOOL_DETAILS and never kept.
                    input_summary: None,
                    status,
                    duration_ms: r.get("duration_ms").and_then(int),
                }),
            );
        }
        other => {
            *skipped.entry(format!("event {other}")).or_default() += 1;
        }
    }
}
