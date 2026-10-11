//! OpenTelemetry exports (OTLP JSON, one export per line) of Claude Code.
//!
//! Claude Code has no file exporter; the lines come from an OpenTelemetry
//! Collector `file` exporter fed with the logs signal. Only the documented log
//! events are read: `claude_code.user_prompt`, `.api_request`, `.skill_activated`
//! and `.tool_result`. Prompt text, tool input and commands are never kept.
//! Metrics and traces lines are counted and skipped.
//!
//! The Copilot CLI file exporter (`COPILOT_OTEL_FILE_EXPORTER_PATH`) writes one
//! JSON object per line, either a `span` or a `metric`. Spans of the GenAI
//! conventions are read: `chat` (one model request with its tokens),
//! `execute_tool` and `invoke_agent` (the user turns). Prompts, responses and
//! tool arguments are only present with `OTEL_INSTRUMENTATION_GENAI_CAPTURE_MESSAGE_CONTENT`
//! and are never read. The session is `gen_ai.conversation.id`, the id of the
//! local Copilot CLI session, so the export merges into it.

use std::collections::{BTreeMap, HashMap};
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
        vec![Source::Otel, Source::ClaudeCode, Source::CopilotCli]
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
    let mut sessions: Sessions = BTreeMap::new();
    let mut skipped: BTreeMap<String, u64> = BTreeMap::new();
    let mut spans: Vec<Value> = Vec::new();
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
            match v.get("type").and_then(Value::as_str) {
                Some("span") => {
                    spans.push(v);
                    continue;
                }
                Some("metric") => {
                    *skipped.entry("metrics".into()).or_default() += 1;
                    continue;
                }
                _ => {}
            }
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
    copilot_spans(&spans, &mut sessions, &mut skipped);
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

/// Sessions of all clients in one file, keyed by source and session id.
type Sessions = BTreeMap<(&'static str, String), SessionEvents>;

fn handle(r: &Record, sessions: &mut Sessions, skipped: &mut BTreeMap<String, u64>) {
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
        .entry((Source::ClaudeCode.id(), id.to_owned()))
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

/// `[seconds, nanoseconds]` as the file exporter writes times.
fn span_time(v: Option<&Value>) -> Option<Timestamp> {
    let pair = v?.as_array()?;
    let secs = i128::from(pair.first()?.as_i64()?);
    let nanos = i128::from(pair.get(1)?.as_i64()?);
    Timestamp::from_nanosecond(secs * 1_000_000_000 + nanos).ok()
}

fn count(attrs: &Value, key: &str) -> u64 {
    attrs.get(key).and_then(Value::as_u64).unwrap_or(0)
}

/// The number of `invoke_agent` spans on the way from `id` to the root, itself included.
fn agent_depth(id: &str, spans: &HashMap<&str, (&str, Option<&str>)>) -> usize {
    let mut depth = 0;
    let mut cur = Some(id);
    for _ in 0..64 {
        let Some((name, parent)) = cur.and_then(|c| spans.get(c)) else {
            break;
        };
        if *name == "invoke_agent" {
            depth += 1;
        }
        cur = *parent;
    }
    depth
}

/// Turns the spans of the Copilot CLI file exporter into events of a Copilot CLI session.
fn copilot_spans(spans: &[Value], sessions: &mut Sessions, skipped: &mut BTreeMap<String, u64>) {
    let index: HashMap<&str, (&str, Option<&str>)> = spans
        .iter()
        .filter_map(|s| {
            Some((
                s.get("spanId")?.as_str()?,
                (
                    s.get("name")?.as_str()?,
                    s.get("parentSpanId").and_then(Value::as_str),
                ),
            ))
        })
        .collect();
    for span in spans {
        let attrs = span.get("attributes").unwrap_or(&Value::Null);
        let operation = attrs.get("gen_ai.operation.name").and_then(Value::as_str);
        if !matches!(operation, Some("chat" | "execute_tool" | "invoke_agent")) {
            *skipped.entry("other spans".into()).or_default() += 1;
            continue;
        }
        let Some(id) = attrs
            .get("gen_ai.conversation.id")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        else {
            *skipped
                .entry("spans without gen_ai.conversation.id".into())
                .or_default() += 1;
            continue;
        };
        let se = sessions
            .entry((Source::CopilotCli.id(), id.to_owned()))
            .or_insert_with(|| SessionEvents::new(Session::new(id, Source::CopilotCli)));
        let start = span_time(span.get("startTime"));
        let end = span_time(span.get("endTime"));
        match operation {
            Some("chat") => {
                let model = ["gen_ai.response.model", "gen_ai.request.model"]
                    .iter()
                    .find_map(|k| attrs.get(*k).and_then(Value::as_str))
                    .map(str::to_owned);
                if se.session.model.is_none() {
                    se.session.model = model.clone();
                }
                let span_id = span.get("spanId").and_then(Value::as_str).unwrap_or("");
                let (cache_read, cache_write) = (
                    count(attrs, "gen_ai.usage.cache_read.input_tokens"),
                    count(attrs, "gen_ai.usage.cache_write.input_tokens"),
                );
                se.push(
                    // The local session store stamps a request when it ends.
                    end.or(start),
                    EventKind::Usage(Usage {
                        model,
                        // `input_tokens` includes everything read from or written to the cache.
                        input: count(attrs, "gen_ai.usage.input_tokens")
                            .saturating_sub(cache_read + cache_write),
                        output: count(attrs, "gen_ai.usage.output_tokens"),
                        cache_read,
                        cache_write,
                        reasoning: count(attrs, "gen_ai.usage.reasoning.output_tokens"),
                        cost_usd: None,
                        measured: true,
                        // An agent started by the main agent is nested in it.
                        subagent: agent_depth(span_id, &index) >= 2,
                        premium_requests: attrs.get("github.copilot.cost").and_then(Value::as_f64),
                        skill: None,
                    }),
                );
            }
            Some("execute_tool") => {
                let Some(name) = attrs.get("gen_ai.tool.name").and_then(Value::as_str) else {
                    *skipped
                        .entry("execute_tool without gen_ai.tool.name".into())
                        .or_default() += 1;
                    continue;
                };
                // Status 0 is "unset": the span does not say whether the call worked.
                let status = match span.pointer("/status/code").and_then(Value::as_i64) {
                    Some(2) => ToolStatus::Error,
                    Some(1) => ToolStatus::Ok,
                    _ => ToolStatus::Unknown,
                };
                let duration_ms = match (start, end) {
                    (Some(s), Some(e)) if e >= s => {
                        Some(((e.as_nanosecond() - s.as_nanosecond()) / 1_000_000) as u64)
                    }
                    _ => None,
                };
                se.push(
                    start,
                    EventKind::ToolCall(ToolCall {
                        name: name.to_owned(),
                        input_summary: None,
                        status,
                        duration_ms,
                    }),
                );
            }
            Some("invoke_agent") => {
                for event in span
                    .get("events")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    if event.get("name").and_then(Value::as_str)
                        == Some("github.copilot.user.message")
                    {
                        se.push(span_time(event.get("time")), EventKind::UserTurn);
                    }
                }
            }
            _ => {}
        }
    }
}
