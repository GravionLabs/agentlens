//! OpenCode: the SQLite database `<data home>/opencode/opencode.db` and, for
//! versions before 1.2.0, the JSON files below `<data home>/opencode/storage/`.
//!
//! A session present in both places is read from the database only.
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
use crate::commit::commit_issue;
use crate::env::Env;
use crate::model::{
    EventKind, Session, SessionEvents, SkillInvoked, SkillTrigger, Source, ToolCall, ToolStatus,
    Usage, Warning,
};

pub struct OpenCodeAdapter;

fn db_path(env: &Env) -> PathBuf {
    env.data_home().join("opencode").join("opencode.db")
}

/// `<data home>/opencode/storage`: one JSON file per session, message and part.
fn storage_path(env: &Env) -> PathBuf {
    env.data_home().join("opencode").join("storage")
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
        let db = db_path(env);
        let storage = storage_path(env);
        let found: Vec<PathBuf> = [db.clone(), storage.clone()]
            .into_iter()
            .filter(|p| p.exists())
            .collect();
        if found.is_empty() {
            Detection::not_found(format!(
                "{} and {} do not exist",
                db.display(),
                storage.display()
            ))
        } else {
            Detection::found(found)
        }
    }

    fn sessions<'a>(&'a self, env: &'a Env, filter: &'a SessionFilter) -> SessionIter<'a> {
        let mut out: Vec<Result<SessionEvents, AdapterError>> = Vec::new();
        let mut known = std::collections::HashSet::new();
        let db = db_path(env);
        if db.is_file() {
            match open_read_only(&db).and_then(|conn| {
                let store = Store::Db(&conn, &db);
                read_all(&store, filter)
            }) {
                Ok(sessions) => {
                    known.extend(sessions.iter().map(|s| s.session.id.clone()));
                    out.extend(sessions.into_iter().map(Ok));
                }
                Err(e) => out.push(Err(e)),
            }
        }
        let storage = storage_path(env);
        if storage.is_dir() {
            match read_all(&Store::Files(&storage), filter) {
                Ok(sessions) => out.extend(
                    sessions
                        .into_iter()
                        .filter(|s| !known.contains(&s.session.id))
                        .map(Ok),
                ),
                Err(e) => out.push(Err(e)),
            }
        }
        Box::new(out.into_iter())
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

fn read_all(store: &Store, filter: &SessionFilter) -> Result<Vec<SessionEvents>, AdapterError> {
    let rows = store.session_rows()?;
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
        read_session(store, root, false, &mut se);
        for kid in kids {
            read_session(store, kid, true, &mut se);
        }
        se.events.sort_by_key(|e| e.at);
        if let Some(last) = se.events.iter().filter_map(|e| e.at).max() {
            se.session.ended_at = Some(last);
        }
        out.push(se);
    }
    Ok(out)
}

/// Where the sessions come from: the database or the legacy JSON files.
enum Store<'a> {
    Db(&'a Connection, &'a Path),
    Files(&'a Path),
}

/// A record of a session: its id, creation time in milliseconds and JSON.
struct Raw {
    id: String,
    created: Option<i64>,
    data: Value,
}

impl Store<'_> {
    fn session_rows(&self) -> Result<Vec<SessionRow>, AdapterError> {
        match self {
            Store::Db(conn, path) => db_session_rows(conn, path),
            Store::Files(root) => Ok(file_session_rows(root)),
        }
    }

    /// Messages of a session with their creation time and parsed data;
    /// records that are not JSON are left out.
    fn messages(&self, session: &str) -> Result<Vec<Raw>, String> {
        match self {
            Store::Db(conn, _) => db_messages(conn, session).map_err(|e| e.to_string()),
            Store::Files(root) => Ok(file_messages(root, session)),
        }
    }

    /// Parts of a session in order. `warnings` receives records that cannot be read.
    fn parts(
        &self,
        session: &str,
        messages: &[Raw],
        warnings: &mut Vec<String>,
    ) -> Result<Vec<Part>, String> {
        match self {
            Store::Db(conn, _) => db_parts(conn, session, warnings).map_err(|e| e.to_string()),
            Store::Files(root) => Ok(file_parts(root, messages, warnings)),
        }
    }
}

struct Part {
    message_id: String,
    at: Option<Timestamp>,
    data: Value,
}

fn db_session_rows(conn: &Connection, path: &Path) -> Result<Vec<SessionRow>, AdapterError> {
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

fn db_messages(conn: &Connection, session: &str) -> rusqlite::Result<Vec<Raw>> {
    let mut stmt =
        conn.prepare("SELECT id, time_created, data FROM message WHERE session_id = ?1")?;
    let rows = stmt.query_map([session], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, String>(2)?,
        ))
    })?;
    Ok(rows
        .filter_map(Result::ok)
        .filter_map(|(id, created, data)| {
            let data = serde_json::from_str::<Value>(&data).ok()?;
            Some(Raw {
                id,
                created: Some(created),
                data,
            })
        })
        .collect())
}

fn db_parts(
    conn: &Connection,
    session: &str,
    warnings: &mut Vec<String>,
) -> rusqlite::Result<Vec<Part>> {
    let mut stmt = conn.prepare(
        "SELECT message_id, time_created, data FROM part WHERE session_id = ?1 \
         ORDER BY time_created, id",
    )?;
    let rows = stmt.query_map([session], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, String>(2)?,
        ))
    })?;
    let mut out = Vec::new();
    for (message_id, created, data) in rows.filter_map(Result::ok) {
        match serde_json::from_str::<Value>(&data) {
            Ok(data) => out.push(Part {
                message_id,
                at: millis(created),
                data,
            }),
            Err(_) => warnings.push("part with invalid JSON skipped".into()),
        }
    }
    Ok(out)
}

/// The `*.json` files directly in `dir`, sorted by name (ids sort by creation).
fn json_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|e| e == "json"))
        .collect();
    files.sort();
    files
}

fn read_json(path: &Path) -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

fn i64_at(v: &Value, path: &[&str]) -> Option<i64> {
    path.iter()
        .try_fold(v, |v, k| v.get(*k))
        .and_then(Value::as_i64)
}

/// `storage/session/<project>/<session>.json`; the model is the one of the
/// first assistant message, because the legacy session file has none.
fn file_session_rows(root: &Path) -> Vec<SessionRow> {
    let Ok(projects) = std::fs::read_dir(root.join("session")) else {
        return Vec::new();
    };
    let mut dirs: Vec<PathBuf> = projects
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    let mut rows = Vec::new();
    for dir in dirs {
        for file in json_files(&dir) {
            let Some(v) = read_json(&file) else { continue };
            let Some(id) = str_of(&v, "id").map(str::to_owned) else {
                continue;
            };
            let model = file_messages(root, &id)
                .iter()
                .filter(|m| str_of(&m.data, "role") == Some("assistant"))
                .find_map(|m| str_of(&m.data, "modelID").map(str::to_owned));
            rows.push(SessionRow {
                parent: str_of(&v, "parentID").map(str::to_owned),
                directory: str_of(&v, "directory").map(str::to_owned),
                model,
                created: i64_at(&v, &["time", "created"]).and_then(millis),
                updated: i64_at(&v, &["time", "updated"]).and_then(millis),
                id,
            });
        }
    }
    rows.sort_by(|a, b| (a.created, &a.id).cmp(&(b.created, &b.id)));
    rows
}

/// `storage/message/<session>/<message>.json`
fn file_messages(root: &Path, session: &str) -> Vec<Raw> {
    json_files(&root.join("message").join(session))
        .into_iter()
        .filter_map(|file| {
            let data = read_json(&file)?;
            let id = str_of(&data, "id")
                .map(str::to_owned)
                .or_else(|| file.file_stem().map(|s| s.to_string_lossy().into_owned()))?;
            Some(Raw {
                id,
                created: i64_at(&data, &["time", "created"]),
                data,
            })
        })
        .collect()
}

/// `storage/part/<message>/<part>.json`. A part has no time of its own except
/// tools and steps with `time.start`; others take the time of their message.
fn file_parts(root: &Path, messages: &[Raw], warnings: &mut Vec<String>) -> Vec<Part> {
    let mut out = Vec::new();
    for m in messages {
        let fallback = i64_at(&m.data, &["time", "completed"]).or(m.created);
        for file in json_files(&root.join("part").join(&m.id)) {
            let Some(data) = read_json(&file) else {
                warnings.push("part with invalid JSON skipped".into());
                continue;
            };
            let at = i64_at(&data, &["time", "start"])
                .or_else(|| i64_at(&data, &["state", "time", "start"]))
                .or(fallback);
            out.push(Part {
                message_id: m.id.clone(),
                at: at.and_then(millis),
                data,
            });
        }
    }
    out
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
        skill: None,
        agent: None,
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
fn read_session(store: &Store, row: &SessionRow, subagent: bool, se: &mut SessionEvents) {
    let loc = |what: &str| format!("session {} {what}", row.id);
    let raw = match store.messages(&row.id) {
        Ok(list) => list,
        Err(e) => {
            se.warn(Warning::new(format!("cannot read messages: {e}")).at(loc("messages")));
            return;
        }
    };
    let mut messages: BTreeMap<String, MessageInfo> = raw
        .iter()
        .map(|m| (m.id.clone(), message_info(m)))
        .collect();

    let mut problems = Vec::new();
    let parts = match store.parts(&row.id, &raw, &mut problems) {
        Ok(parts) => parts,
        Err(e) => {
            se.warn(Warning::new(format!("cannot read parts: {e}")).at(loc("parts")));
            return;
        }
    };
    for p in problems {
        se.warn(Warning::new(p).at(loc("part")));
    }

    for part in &parts {
        if str_of(&part.data, "type") == Some("step-finish")
            && let Some(m) = messages.get_mut(&part.message_id)
        {
            m.has_steps = true;
        }
    }

    for Part {
        message_id,
        at,
        data: part,
    } in parts
    {
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

fn message_info(m: &Raw) -> MessageInfo {
    let v = &m.data;
    let model = str_of(v, "modelID")
        .or_else(|| v.get("model").and_then(|m| str_of(m, "modelID")))
        .map(str::to_owned);
    MessageInfo {
        role: str_of(v, "role").unwrap_or("").to_owned(),
        model,
        tokens: v.get("tokens").cloned(),
        cost: v.get("cost").and_then(Value::as_f64),
        at: m.created.and_then(millis),
        has_steps: false,
    }
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
    if name == "bash"
        && let Some(issue) = str_of(input, "command").and_then(commit_issue)
    {
        se.push(at, EventKind::CommitIssue { issue });
    }

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
