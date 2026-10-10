//! OpenCode: the SQLite database `<data home>/opencode/opencode.db`.
//!
//! Tables used: `session`, `message` and `part`; the interesting content sits in
//! their JSON `data` columns. The schema is not documented, so the adapter checks
//! the columns it needs and reports a clear error instead of guessing.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::time::Duration;

use jiff::Timestamp;
use rusqlite::{Connection, OpenFlags};
use serde_json::Value;

use crate::adapter::{Adapter, AdapterError, Detection, SessionFilter, SessionIter};
use crate::env::Env;
use crate::model::{
    EventKind, Session, SessionEvents, SkillInvoked, SkillTrigger, Source, ToolCall, ToolStatus,
    Usage, Warning,
};

pub struct OpenCodeAdapter;

fn db_path(env: &Env) -> PathBuf {
    env.data_home().join("opencode").join("opencode.db")
}

/// Columns the adapter reads; a database without them has an unknown schema.
const REQUIRED: &[(&str, &[&str])] = &[
    (
        "session",
        &[
            "id",
            "parent_id",
            "directory",
            "model",
            "time_created",
            "time_updated",
        ],
    ),
    ("message", &["id", "session_id", "time_created", "data"]),
    (
        "part",
        &["message_id", "session_id", "time_created", "data"],
    ),
];

impl Adapter for OpenCodeAdapter {
    fn id(&self) -> Source {
        Source::OpenCode
    }

    fn detect(&self, env: &Env) -> Detection {
        let path = db_path(env);
        if path.is_file() {
            Detection::found(vec![path])
        } else {
            Detection::not_found(format!("{} does not exist", path.display()))
        }
    }

    fn sessions<'a>(&'a self, env: &'a Env, filter: &'a SessionFilter) -> SessionIter<'a> {
        match read_all(&db_path(env), filter) {
            Ok(sessions) => Box::new(sessions.into_iter().map(Ok)),
            Err(e) => Box::new(std::iter::once(Err(e))),
        }
    }
}

fn db_err(path: &Path, e: rusqlite::Error) -> AdapterError {
    AdapterError::Other(format!("{}: {e}", path.display()))
}

/// Opens the database read-only. This also works while OpenCode runs and holds
/// the write-ahead log, because readers never block on the writer.
pub fn open_read_only(path: &Path) -> Result<Connection, AdapterError> {
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| db_err(path, e))?;
    conn.busy_timeout(Duration::from_secs(2))
        .map_err(|e| db_err(path, e))?;
    check_schema(&conn, path)?;
    Ok(conn)
}

fn check_schema(conn: &Connection, path: &Path) -> Result<(), AdapterError> {
    for (table, columns) in REQUIRED {
        let mut stmt = conn
            .prepare(&format!("PRAGMA table_info({table})"))
            .map_err(|e| db_err(path, e))?;
        let have: Vec<String> = stmt
            .query_map([], |r| r.get::<_, String>(1))
            .map_err(|e| db_err(path, e))?
            .filter_map(Result::ok)
            .collect();
        if let Some(missing) = columns.iter().find(|c| !have.iter().any(|h| h == *c)) {
            return Err(AdapterError::Other(format!(
                "{}: unsupported OpenCode schema, table `{table}` has no column `{missing}`; \
                 this OpenCode version is not supported yet",
                path.display()
            )));
        }
    }
    Ok(())
}

struct SessionRow {
    id: String,
    parent: Option<String>,
    directory: Option<String>,
    model: Option<String>,
    created: Option<Timestamp>,
    updated: Option<Timestamp>,
}

fn millis(ms: i64) -> Option<Timestamp> {
    Timestamp::from_millisecond(ms).ok()
}

fn read_all(path: &Path, filter: &SessionFilter) -> Result<Vec<SessionEvents>, AdapterError> {
    let conn = open_read_only(path)?;
    let rows = session_rows(&conn, path)?;
    let by_id: HashMap<&str, &SessionRow> = rows.iter().map(|r| (r.id.as_str(), r)).collect();

    // Subagent sessions have a parent; their usage belongs to the root session.
    let mut children: BTreeMap<String, Vec<&SessionRow>> = BTreeMap::new();
    let mut roots = Vec::new();
    for row in &rows {
        match root_of(row, &by_id) {
            Some(root) if root != row.id => children.entry(root).or_default().push(row),
            _ => roots.push(row),
        }
    }

    let mut out = Vec::new();
    for root in roots {
        let kids = children.remove(&root.id).unwrap_or_default();
        let last = kids
            .iter()
            .filter_map(|k| k.updated)
            .chain(root.updated)
            .max();
        if let (Some(since), Some(last)) = (filter.since, last)
            && last < since
        {
            continue;
        }
        let mut se = SessionEvents::new(session_of(root));
        read_session(&conn, root, false, &mut se);
        for kid in kids {
            read_session(&conn, kid, true, &mut se);
        }
        se.events.sort_by_key(|e| e.at);
        if let Some(last) = se.events.iter().filter_map(|e| e.at).max() {
            se.session.ended_at = Some(last);
        }
        out.push(se);
    }
    Ok(out)
}

fn session_rows(conn: &Connection, path: &Path) -> Result<Vec<SessionRow>, AdapterError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, parent_id, directory, model, time_created, time_updated \
             FROM session ORDER BY time_created, id",
        )
        .map_err(|e| db_err(path, e))?;
    let rows = stmt
        .query_map([], |r| {
            Ok(SessionRow {
                id: r.get(0)?,
                parent: r.get(1)?,
                directory: r.get(2)?,
                model: r.get::<_, Option<String>>(3)?.and_then(|m| model_id(&m)),
                created: r.get::<_, Option<i64>>(4)?.and_then(millis),
                updated: r.get::<_, Option<i64>>(5)?.and_then(millis),
            })
        })
        .map_err(|e| db_err(path, e))?
        .filter_map(Result::ok)
        .collect();
    Ok(rows)
}

/// `session.model` holds a JSON object such as `{"id":"…","providerID":"…"}`.
fn model_id(raw: &str) -> Option<String> {
    let v: Value = serde_json::from_str(raw).ok()?;
    str_of(&v, "id").map(str::to_owned)
}

/// The topmost ancestor, or `None` on a cycle.
fn root_of(row: &SessionRow, by_id: &HashMap<&str, &SessionRow>) -> Option<String> {
    let mut cur = row;
    for _ in 0..64 {
        match cur.parent.as_deref().and_then(|p| by_id.get(p)) {
            Some(parent) => cur = parent,
            None => return Some(cur.id.clone()),
        }
    }
    None
}

fn session_of(row: &SessionRow) -> Session {
    let mut s = Session::new(row.id.clone(), Source::OpenCode);
    s.project_dir = row
        .directory
        .clone()
        .filter(|d| !d.is_empty())
        .map(PathBuf::from);
    s.model = row.model.clone();
    s.started_at = row.created;
    s.ended_at = row.updated;
    s
}

fn str_of<'v>(v: &'v Value, key: &str) -> Option<&'v str> {
    v.get(key).and_then(Value::as_str).filter(|s| !s.is_empty())
}

fn u64_of(v: &Value, key: &str) -> u64 {
    v.get(key).and_then(Value::as_u64).unwrap_or(0)
}

fn usage_of(tokens: &Value, cost: Option<f64>, model: Option<String>, subagent: bool) -> Usage {
    let cache = tokens.get("cache").unwrap_or(&Value::Null);
    Usage {
        model,
        input: u64_of(tokens, "input"),
        output: u64_of(tokens, "output"),
        cache_read: u64_of(cache, "read"),
        cache_write: u64_of(cache, "write"),
        reasoning: u64_of(tokens, "reasoning"),
        cost_usd: cost,
        measured: true,
        subagent,
        premium_requests: None,
    }
}

struct MessageInfo {
    role: String,
    model: Option<String>,
    tokens: Option<Value>,
    cost: Option<f64>,
    at: Option<Timestamp>,
    has_steps: bool,
}

/// Appends the events of one session (the root or a subagent) to `se`.
fn read_session(conn: &Connection, row: &SessionRow, subagent: bool, se: &mut SessionEvents) {
    let loc = |what: &str| format!("session {} {what}", row.id);
    let mut messages: BTreeMap<String, MessageInfo> = BTreeMap::new();
    match load_messages(conn, &row.id) {
        Ok(list) => {
            for (id, info) in list {
                messages.insert(id, info);
            }
        }
        Err(e) => {
            se.warn(Warning::new(format!("cannot read messages: {e}")).at(loc("messages")));
            return;
        }
    }

    let mut parts = match conn.prepare(
        "SELECT message_id, time_created, data FROM part WHERE session_id = ?1 \
         ORDER BY time_created, id",
    ) {
        Ok(p) => p,
        Err(e) => {
            se.warn(Warning::new(format!("cannot read parts: {e}")).at(loc("parts")));
            return;
        }
    };
    let rows = match parts.query_map([&row.id], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, String>(2)?,
        ))
    }) {
        Ok(rows) => rows,
        Err(e) => {
            se.warn(Warning::new(format!("cannot read parts: {e}")).at(loc("parts")));
            return;
        }
    };

    let mut decoded = Vec::new();
    for (message_id, created, data) in rows.filter_map(Result::ok) {
        let Ok(part) = serde_json::from_str::<Value>(&data) else {
            se.warn(Warning::new("part with invalid JSON skipped").at(loc("part")));
            continue;
        };
        if str_of(&part, "type") == Some("step-finish")
            && let Some(m) = messages.get_mut(&message_id)
        {
            m.has_steps = true;
        }
        decoded.push((message_id, millis(created), part));
    }

    for (message_id, at, part) in decoded {
        let msg = messages.get(&message_id);
        match str_of(&part, "type") {
            Some("tool") => tool_part(&part, se),
            Some("step-finish") => {
                let model = msg.and_then(|m| m.model.clone());
                let usage = usage_of(
                    part.get("tokens").unwrap_or(&Value::Null),
                    part.get("cost").and_then(Value::as_f64),
                    model,
                    subagent,
                );
                se.push(at, EventKind::Usage(usage));
            }
            Some("compaction") if !subagent => se.push(at, EventKind::Compaction),
            _ => {}
        }
    }

    for info in messages.values() {
        match info.role.as_str() {
            "user" if !subagent => se.push(info.at, EventKind::UserTurn),
            // Older data has message totals without step records.
            "assistant" if !info.has_steps => {
                if let Some(tokens) = &info.tokens {
                    let usage = usage_of(tokens, info.cost, info.model.clone(), subagent);
                    se.push(info.at, EventKind::Usage(usage));
                }
            }
            _ => {}
        }
    }
}

fn load_messages(conn: &Connection, session: &str) -> rusqlite::Result<Vec<(String, MessageInfo)>> {
    let mut stmt =
        conn.prepare("SELECT id, time_created, data FROM message WHERE session_id = ?1")?;
    let rows = stmt.query_map([session], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, String>(2)?,
        ))
    })?;
    let mut out = Vec::new();
    for (id, created, data) in rows.filter_map(Result::ok) {
        let Ok(v) = serde_json::from_str::<Value>(&data) else {
            continue;
        };
        let model = str_of(&v, "modelID")
            .or_else(|| v.get("model").and_then(|m| str_of(m, "modelID")))
            .map(str::to_owned);
        out.push((
            id,
            MessageInfo {
                role: str_of(&v, "role").unwrap_or("").to_owned(),
                model,
                tokens: v.get("tokens").cloned(),
                cost: v.get("cost").and_then(Value::as_f64),
                at: millis(created),
                has_steps: false,
            },
        ));
    }
    Ok(out)
}

fn tool_part(part: &Value, se: &mut SessionEvents) {
    let Some(name) = str_of(part, "tool") else {
        return;
    };
    let state = part.get("state").unwrap_or(&Value::Null);
    let input = state.get("input").unwrap_or(&Value::Null);
    let time = state.get("time").unwrap_or(&Value::Null);
    let start = time.get("start").and_then(Value::as_i64);
    let at = start.and_then(millis);
    let status = match str_of(state, "status") {
        Some("completed") => ToolStatus::Ok,
        Some("error") => ToolStatus::Error,
        _ => ToolStatus::Unknown,
    };
    let duration_ms = match (start, time.get("end").and_then(Value::as_i64)) {
        (Some(s), Some(e)) if e >= s => Some((e - s) as u64),
        _ => None,
    };
    let call = ToolCall {
        name: name.to_owned(),
        input_summary: summarize(name, input),
        status,
        duration_ms,
    };
    se.push(at, EventKind::ToolCall(call));

    if name == "skill"
        && let Some(skill) = str_of(input, "name")
    {
        let mut ev = SkillInvoked::new(skill, SkillTrigger::Model);
        ev.path = state
            .get("metadata")
            .and_then(|m| str_of(m, "dir"))
            .map(|d| Path::new(d).join("SKILL.md"));
        ev.content_bytes = state
            .get("output")
            .and_then(Value::as_str)
            .map(|o| o.len() as u64);
        se.push(at, EventKind::SkillInvoked(ev));
    }
}

/// A short, privacy-conscious description of a tool input: a path, pattern or
/// skill name; for shell commands only the program, never the arguments.
fn summarize(tool: &str, input: &Value) -> Option<String> {
    if tool == "bash" {
        return str_of(input, "command")?
            .split_whitespace()
            .next()
            .map(str::to_owned);
    }
    if tool == "task"
        && let Some(kind) = str_of(input, "subagent_type")
    {
        return Some(kind.to_owned());
    }
    ["filePath", "path", "pattern", "name", "description"]
        .iter()
        .find_map(|k| str_of(input, k))
        .map(|s| s.chars().take(80).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_never_contains_shell_arguments() {
        let input = serde_json::json!({"command": "curl -H 'Authorization: x' https://e"});
        assert_eq!(summarize("bash", &input).as_deref(), Some("curl"));
    }

    #[test]
    fn model_is_read_from_the_json_column() {
        assert_eq!(
            model_id(r#"{"id":"m1","providerID":"p"}"#).as_deref(),
            Some("m1")
        );
        assert_eq!(model_id("not json"), None);
    }
}
