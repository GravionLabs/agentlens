//! GitHub Copilot CLI: `<copilot home>/session-state/<id>/events.jsonl` for
//! events, `<copilot home>/session-store.db` for per-request token usage.
//!
//! Only some sessions have rows in `assistant_usage_events`. For the others the
//! `session.usage_record` events (newer versions) carry the requests, and failing that the
//! `session.shutdown` event carries per-model totals, which are used instead.

use std::collections::{BTreeMap, HashMap};
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use jiff::Timestamp;
use rusqlite::{Connection, OpenFlags};
use serde_json::Value;

use crate::adapter::{Adapter, AdapterError, Detection, SessionFilter, SessionIter};
use crate::env::Env;
use crate::model::{
    EventKind, Hook, PermissionPrompt, Session, SessionEvents, SkillInvoked, SkillTrigger, Source,
    ToolCall, ToolStatus, Usage, Warning,
};

pub struct CopilotCliAdapter;

pub(crate) fn copilot_home(env: &Env) -> PathBuf {
    env.var("COPILOT_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| env.home().join(".copilot"))
}

fn state_dir(env: &Env) -> PathBuf {
    copilot_home(env).join("session-state")
}

/// Event types that are known but carry nothing the analyzers need.
///
/// Newer versions of the CLI also write the requests into the event log: `session.usage_record` (one
/// per request, with the tokens, the model and the cost), `session.usage_checkpoint` (running totals)
/// and the `model.*` events of each model call. On real sessions the usage records matched the rows of
/// `session-store.db` request by request (the same tokens, cache read and cache write), so the store
/// stays the source and the records are read only for a session that has no rows in it (the store is
/// missing or was cleaned); reading both would count a request twice. They are known, not unknown: a
/// warning for them only made every session of a new CLI look broken.
const IGNORED: &[&str] = &[
    "abort",
    "model.captured_assignment_context",
    "model.message",
    "model.messages_snapshot",
    "model.model_call_started",
    "model.model_call_success",
    "model.response",
    "model.turn_ended",
    "model.turn_started",
    "session.auto_mode_resolved",
    "session.usage_checkpoint",
    "assistant.message",
    "assistant.turn_end",
    "assistant.turn_start",
    "session.context_changed",
    "session.error",
    "session.info",
    "session.mode_changed",
    "session.model_change",
    "session.permissions_changed",
    "session.plan_changed",
    "session.resume",
    "session.task_complete",
    "session.truncation",
    "session.warning",
    "session.compaction_start",
    "subagent.completed",
    "subagent.failed",
    "subagent.started",
    "system.message",
    "system.notification",
    "tool.user_requested",
];

impl Adapter for CopilotCliAdapter {
    fn id(&self) -> Source {
        Source::CopilotCli
    }

    fn detect(&self, env: &Env) -> Detection {
        let dir = state_dir(env);
        if dir.is_dir() {
            Detection::found(vec![dir])
        } else {
            Detection::not_found(format!("{} does not exist", dir.display()))
        }
    }

    fn sessions<'a>(&'a self, env: &'a Env, filter: &'a SessionFilter) -> SessionIter<'a> {
        let dir = state_dir(env);
        let entries = match fs::read_dir(&dir) {
            Ok(rd) => rd,
            Err(source) => {
                return Box::new(std::iter::once(Err(AdapterError::Io { path: dir, source })));
            }
        };
        let mut files: Vec<(String, PathBuf)> = entries
            .filter_map(Result::ok)
            .filter_map(|e| {
                let events = e.path().join("events.jsonl");
                events
                    .is_file()
                    .then(|| (e.file_name().to_string_lossy().into_owned(), events))
            })
            .filter(|(_, p)| !untouched_since(p, filter.since))
            .collect();
        files.sort();

        let (mut usage, db_warning) = read_usage_rows(&copilot_home(env).join("session-store.db"));
        Box::new(files.into_iter().map(move |(id, path)| {
            let mut se = parse_session(&id, &path, usage.remove(&id).unwrap_or_default())?;
            if let Some(w) = &db_warning {
                se.warn(Warning::new(w.clone()));
            }
            Ok(se)
        }))
    }
}

/// A file last modified before `since` cannot contain newer events.
fn untouched_since(path: &Path, since: Option<Timestamp>) -> bool {
    let Some(since) = since else { return false };
    fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| Timestamp::try_from(t).ok())
        .is_some_and(|modified| modified < since)
}

struct UsageRow {
    at: Option<Timestamp>,
    usage: Usage,
}

const USAGE_COLUMNS: &[&str] = &[
    "session_id",
    "model",
    "input_tokens",
    "output_tokens",
    "cache_read_tokens",
    "cache_write_tokens",
    "reasoning_tokens",
    "created_at",
];

/// Per-request usage rows by session id. A missing database or table is not an
/// error (the shutdown totals are used then); a table with an unknown shape
/// yields a warning.
fn read_usage_rows(path: &Path) -> (HashMap<String, Vec<UsageRow>>, Option<String>) {
    let mut out: HashMap<String, Vec<UsageRow>> = HashMap::new();
    if !path.is_file() {
        return (out, None);
    }
    let conn = match Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) {
        Ok(c) => c,
        Err(e) => return (out, Some(format!("{}: {e}", path.display()))),
    };
    let _ = conn.busy_timeout(std::time::Duration::from_secs(2));
    let have: Vec<String> = match conn.prepare("PRAGMA table_info(assistant_usage_events)") {
        Ok(mut s) => match s.query_map([], |r| r.get::<_, String>(1)) {
            Ok(rows) => rows.filter_map(Result::ok).collect(),
            Err(_) => Vec::new(),
        },
        Err(_) => Vec::new(),
    };
    if have.is_empty() {
        return (out, None);
    }
    if let Some(missing) = USAGE_COLUMNS.iter().find(|c| !have.iter().any(|h| h == *c)) {
        return (
            out,
            Some(format!(
                "{}: unsupported session-store schema, `assistant_usage_events` has no column \
                 `{missing}`; using session totals instead",
                path.display()
            )),
        );
    }
    let subagent_cols =
        have.iter().any(|c| c == "agent_id") && have.iter().any(|c| c == "parent_tool_call_id");
    let has = |c: &str| have.iter().any(|h| h == c);
    let sql = format!(
        "SELECT session_id, model, input_tokens, output_tokens, cache_read_tokens, \
         cache_write_tokens, reasoning_tokens, created_at, {}, {}, {} \
         FROM assistant_usage_events ORDER BY created_at, id",
        if subagent_cols {
            "(agent_id IS NOT NULL OR parent_tool_call_id IS NOT NULL)"
        } else {
            "0"
        },
        if has("request_multiplier") {
            "request_multiplier"
        } else {
            "NULL"
        },
        if has("initiator") {
            "initiator"
        } else {
            "NULL"
        },
    );
    let result = conn.prepare(&sql).and_then(|mut stmt| {
        let rows = stmt.query_map([], |r| {
            let n = |i: usize| {
                r.get::<_, Option<i64>>(i)
                    .map(|v| v.unwrap_or(0).max(0) as u64)
            };
            let (input, cache_read, cache_write) = (n(2)?, n(4)?, n(5)?);
            Ok((
                r.get::<_, String>(0)?,
                UsageRow {
                    at: r.get::<_, Option<String>>(7)?.and_then(|s| parse_time(&s)),
                    usage: Usage {
                        model: r.get(1)?,
                        // The table reports the whole prompt in `input_tokens`.
                        input: input.saturating_sub(cache_read + cache_write),
                        output: n(3)?,
                        cache_read,
                        cache_write,
                        reasoning: n(6)?,
                        cost_usd: None,
                        measured: true,
                        subagent: r.get::<_, i64>(8)? != 0,
                        // Only requests the user started are charged; agent
                        // follow-ups ride along.
                        skill: None,
                        agent: None,
                        premium_requests: r.get::<_, Option<f64>>(9)?.map(|m| {
                            match r.get::<_, Option<String>>(10).ok().flatten().as_deref() {
                                Some("user") | None => m,
                                Some(_) => 0.0,
                            }
                        }),
                    },
                },
            ))
        })?;
        Ok(rows.filter_map(Result::ok).collect::<Vec<_>>())
    });
    match result {
        Ok(rows) => {
            for (id, row) in rows {
                out.entry(id).or_default().push(row);
            }
            (out, None)
        }
        Err(e) => (out, Some(format!("{}: {e}", path.display()))),
    }
}

/// RFC 3339, or SQLite's `YYYY-MM-DD HH:MM:SS` (UTC).
fn parse_time(s: &str) -> Option<Timestamp> {
    if let Ok(t) = s.parse::<Timestamp>() {
        return Some(t);
    }
    format!("{}Z", s.replace(' ', "T")).parse().ok()
}

fn str_of<'v>(v: &'v Value, key: &str) -> Option<&'v str> {
    v.get(key).and_then(Value::as_str).filter(|s| !s.is_empty())
}

fn u64_of(v: &Value, key: &str) -> u64 {
    v.get(key).and_then(Value::as_u64).unwrap_or(0)
}

/// A short, privacy-conscious description of a tool input: a path, pattern or
/// skill name; for shell commands only the program, never the arguments.
fn summarize(tool: &str, args: &Value) -> Option<String> {
    if tool == "bash" {
        return str_of(args, "command")?
            .split_whitespace()
            .next()
            .map(str::to_owned);
    }
    if tool == "task"
        && let Some(kind) = str_of(args, "agent_type")
    {
        return Some(kind.to_owned());
    }
    ["path", "file_path", "pattern", "skill", "name", "intent"]
        .iter()
        .find_map(|k| str_of(args, k))
        .map(|s| s.chars().take(80).collect())
}

fn parse_session(
    id: &str,
    path: &Path,
    rows: Vec<UsageRow>,
) -> Result<SessionEvents, AdapterError> {
    let io = |source| AdapterError::Io {
        path: path.to_path_buf(),
        source,
    };
    let file = File::open(path).map_err(io)?;
    let mut se = SessionEvents::new(Session::new(id, Source::CopilotCli));
    let mut pending: HashMap<String, (usize, Option<Timestamp>)> = HashMap::new();
    let mut hooks: HashMap<String, (usize, Option<Timestamp>)> = HashMap::new();
    let mut prompts: HashMap<String, (usize, Option<Timestamp>)> = HashMap::new();
    let mut unknown: BTreeMap<String, u64> = BTreeMap::new();
    let mut shutdowns: Vec<(Option<Timestamp>, Vec<Usage>)> = Vec::new();
    let mut records: Vec<UsageRow> = Vec::new();
    let mut first: Option<Timestamp> = None;
    let mut last: Option<Timestamp> = None;
    let mut model: Option<String> = None;

    for (n, line) in BufReader::new(file).lines().enumerate() {
        let line = line.map_err(io)?;
        if line.trim().is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(&line) else {
            se.warn(Warning::new("line with invalid JSON skipped").at(format!(
                "{}:{}",
                path.display(),
                n + 1
            )));
            continue;
        };
        let at = str_of(&v, "timestamp").and_then(|s| s.parse::<Timestamp>().ok());
        first = first.or(at);
        last = last.max(at);
        let data = v.get("data").unwrap_or(&Value::Null);
        let Some(kind) = str_of(&v, "type") else {
            continue;
        };
        match kind {
            "session.start" => {
                let ctx = data.get("context").unwrap_or(&Value::Null);
                se.session.project_dir = str_of(ctx, "cwd").map(PathBuf::from);
                se.session.branch = str_of(ctx, "branch").map(str::to_owned);
            }
            "user.message" => se.push(at, EventKind::UserTurn),
            "skill.invoked" => {
                let Some(name) = str_of(data, "name") else {
                    continue;
                };
                // Skills the user started by hand carry no trigger.
                let trigger = match str_of(data, "trigger") {
                    Some("agent-invoked") => SkillTrigger::Model,
                    _ => SkillTrigger::User,
                };
                let mut ev = SkillInvoked::new(name, trigger);
                ev.path = str_of(data, "path").map(PathBuf::from);
                ev.content_bytes = data
                    .get("content")
                    .and_then(Value::as_str)
                    .map(|c| c.len() as u64);
                se.push(at, EventKind::SkillInvoked(ev));
            }
            "tool.execution_start" => {
                let name = str_of(data, "toolName").unwrap_or("unknown");
                let args = data.get("arguments").unwrap_or(&Value::Null);
                let call = ToolCall {
                    name: name.to_owned(),
                    input_summary: summarize(name, args),
                    status: ToolStatus::Unknown,
                    duration_ms: None,
                };
                let idx = se.events.len();
                se.push(at, EventKind::ToolCall(call));
                if let Some(call_id) = str_of(data, "toolCallId") {
                    pending.insert(call_id.to_owned(), (idx, at));
                }
            }
            "tool.execution_complete" => {
                if let Some(call_id) = str_of(data, "toolCallId")
                    && let Some((idx, started)) = pending.remove(call_id)
                    && let EventKind::ToolCall(call) = &mut se.events[idx].kind
                {
                    call.status = match data.get("success").and_then(Value::as_bool) {
                        Some(true) => ToolStatus::Ok,
                        Some(false) => ToolStatus::Error,
                        None => ToolStatus::Unknown,
                    };
                    call.duration_ms = match (started, at) {
                        (Some(s), Some(e)) => {
                            u64::try_from(e.as_millisecond() - s.as_millisecond()).ok()
                        }
                        _ => None,
                    };
                }
            }
            "hook.start" => {
                let idx = se.events.len();
                let hook = Hook {
                    kind: str_of(data, "hookType").unwrap_or("unknown").to_owned(),
                    success: None,
                    duration_ms: None,
                };
                se.push(at, EventKind::Hook(hook));
                if let Some(id) = str_of(data, "hookInvocationId") {
                    hooks.insert(id.to_owned(), (idx, at));
                }
            }
            "hook.end" => {
                if let Some(id) = str_of(data, "hookInvocationId")
                    && let Some((idx, started)) = hooks.remove(id)
                    && let EventKind::Hook(h) = &mut se.events[idx].kind
                {
                    h.success = data.get("success").and_then(Value::as_bool);
                    h.duration_ms = elapsed_ms(started, at);
                }
            }
            "permission.requested" => {
                let idx = se.events.len();
                se.push(
                    at,
                    EventKind::PermissionPrompt(PermissionPrompt { wait_ms: None }),
                );
                if let Some(id) = str_of(data, "requestId") {
                    prompts.insert(id.to_owned(), (idx, at));
                }
            }
            "permission.completed" => {
                if let Some(id) = str_of(data, "requestId")
                    && let Some((idx, started)) = prompts.remove(id)
                    && let EventKind::PermissionPrompt(p) = &mut se.events[idx].kind
                {
                    p.wait_ms = elapsed_ms(started, at);
                }
            }
            "session.compaction_complete" => se.push(at, EventKind::Compaction),
            "session.model_change" => {
                model = str_of(data, "newModel").map(str::to_owned).or(model);
            }
            "session.usage_record" => {
                if let Some(usage) = data.get("usage").and_then(record_usage) {
                    records.push(UsageRow { at, usage });
                }
            }
            "session.shutdown" => {
                model = str_of(data, "currentModel").map(str::to_owned).or(model);
                shutdowns.push((at, shutdown_usage(data)));
            }
            other if IGNORED.contains(&other) => {}
            other => *unknown.entry(other.to_owned()).or_default() += 1,
        }
    }

    // Per-request rows when the store has them, else the usage records of the event log, else the
    // shutdown totals (which the records add up to, so they are not read as well).
    // All or nothing per session: a store that has only some of the rows of a session is used as it is.
    let rows = if rows.is_empty() { records } else { rows };
    if rows.is_empty() {
        for (at, usages) in shutdowns {
            for usage in usages {
                se.push(at, EventKind::Usage(usage));
            }
        }
    } else {
        for row in rows {
            se.push(row.at, EventKind::Usage(row.usage));
        }
    }
    if !unknown.is_empty() {
        let list: Vec<String> = unknown.iter().map(|(k, n)| format!("{k} ({n})")).collect();
        se.warn(
            Warning::new(format!("ignored unknown event types: {}", list.join(", ")))
                .at(path.display().to_string()),
        );
    }

    se.events.sort_by_key(|e| e.at);
    se.session.model = model.or_else(|| {
        se.events.iter().find_map(|e| match &e.kind {
            EventKind::Usage(u) => u.model.clone(),
            _ => None,
        })
    });
    se.session.started_at = first;
    se.session.ended_at = last;
    Ok(se)
}

fn elapsed_ms(start: Option<Timestamp>, end: Option<Timestamp>) -> Option<u64> {
    let (s, e) = (start?, end?);
    u64::try_from(e.as_millisecond() - s.as_millisecond()).ok()
}

/// The request of a `session.usage_record`. Only the numbers are read, nothing of the messages.
/// `inputTokens` is the whole prompt. As in the store, only a request the user started is charged:
/// `cost` is the premium requests (the multiplier) for `initiator: user`, and 0 for agent follow-ups.
fn record_usage(u: &Value) -> Option<Usage> {
    let (input, cache_read, cache_write) = (
        u64_of(u, "inputTokens"),
        u64_of(u, "cacheReadTokens"),
        u64_of(u, "cacheWriteTokens"),
    );
    let output = u64_of(u, "outputTokens");
    if input == 0 && output == 0 && cache_read == 0 && cache_write == 0 {
        return None;
    }
    Some(Usage {
        model: str_of(u, "model").map(str::to_owned),
        input: input.saturating_sub(cache_read.saturating_add(cache_write)),
        output,
        cache_read,
        cache_write,
        reasoning: u64_of(u, "reasoningTokens"),
        cost_usd: None,
        measured: true,
        // The records seen so far carry no agent fields, so this cannot be told apart.
        subagent: false,
        skill: None,
        agent: None,
        premium_requests: u.get("cost").and_then(Value::as_f64).map(|m| {
            match str_of(u, "initiator") {
                Some("user") | None => m,
                Some(_) => 0.0,
            }
        }),
    })
}

/// One usage record per model from the `session.shutdown` metrics.
fn shutdown_usage(data: &Value) -> Vec<Usage> {
    let Some(metrics) = data.get("modelMetrics").and_then(Value::as_object) else {
        return Vec::new();
    };
    metrics
        .iter()
        .map(|(model, m)| {
            let u = m.get("usage").unwrap_or(&Value::Null);
            let (input, cache_read, cache_write) = (
                u64_of(u, "inputTokens"),
                u64_of(u, "cacheReadTokens"),
                u64_of(u, "cacheWriteTokens"),
            );
            Usage {
                model: Some(model.clone()),
                // `inputTokens` includes the cached parts.
                input: input.saturating_sub(cache_read + cache_write),
                output: u64_of(u, "outputTokens"),
                cache_read,
                cache_write,
                reasoning: u64_of(u, "reasoningTokens"),
                cost_usd: None,
                measured: true,
                subagent: false,
                skill: None,
                agent: None,
                // `requests.cost` is the number of premium requests charged.
                premium_requests: m
                    .get("requests")
                    .and_then(|r| r.get("cost"))
                    .and_then(Value::as_f64),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sqlite_and_rfc3339_times_parse() {
        assert!(parse_time("2026-07-15T04:04:38.004Z").is_some());
        assert_eq!(
            parse_time("2026-07-15 04:04:38"),
            parse_time("2026-07-15T04:04:38Z")
        );
        assert_eq!(parse_time("garbage"), None);
    }

    #[test]
    fn shell_arguments_are_never_summarized() {
        let args = serde_json::json!({"command": "curl -H secret https://e"});
        assert_eq!(summarize("bash", &args).as_deref(), Some("curl"));
    }
}
