//! GitHub Copilot Chat in VS Code: `<user dir>/workspaceStorage/<hash>/chatSessions/*.json(l)`
//! and `<user dir>/globalStorage/emptyWindowChatSessions/*`.
//!
//! A `.jsonl` file is an operation log: kind 0 is the initial snapshot, kind 1
//! sets the value at path `k`, kind 2 truncates the array at `k` from index `i`
//! (if given) and appends `v`. Replaying it gives the same object as the
//! `.json` form. The format is not documented; unknown shapes are skipped.

use std::fs;
use std::path::{Path, PathBuf};

use jiff::Timestamp;
use serde_json::Value;

use crate::adapter::{Adapter, AdapterError, Detection, SessionFilter, SessionIter};
use crate::adapters::claude_code::untouched_since;
use crate::env::Env;
use crate::model::{
    ContextItem, ContextKind, EventKind, Session, SessionEvents, SkillInvoked, SkillTrigger,
    Source, ToolCall, ToolStatus, Usage, Warning,
};

pub struct VsCodeCopilotAdapter;

/// The `User` directories of VS Code and VS Code Insiders that exist.
pub(crate) fn user_dirs(env: &Env) -> Vec<PathBuf> {
    let mut bases = vec![
        crate::inventory::config_home(env),
        env.home().join("Library/Application Support"),
    ];
    if let Some(appdata) = env.var("APPDATA") {
        bases.push(PathBuf::from(appdata));
    }
    let mut out = Vec::new();
    for base in bases {
        for edition in ["Code", "Code - Insiders"] {
            let dir = base.join(edition).join("User");
            if dir.is_dir() && !out.contains(&dir) {
                out.push(dir);
            }
        }
    }
    out
}

impl Adapter for VsCodeCopilotAdapter {
    fn id(&self) -> Source {
        Source::VsCodeCopilot
    }

    fn detect(&self, env: &Env) -> Detection {
        let roots: Vec<PathBuf> = user_dirs(env)
            .into_iter()
            .filter(|d| {
                d.join("workspaceStorage").is_dir()
                    || d.join("globalStorage/emptyWindowChatSessions").is_dir()
            })
            .collect();
        if roots.is_empty() {
            Detection::not_found("no VS Code user directory with chat storage found")
        } else {
            Detection::found(roots)
        }
    }

    fn sessions<'a>(&'a self, env: &'a Env, filter: &'a SessionFilter) -> SessionIter<'a> {
        let mut files: Vec<(PathBuf, Option<PathBuf>)> = Vec::new();
        for user in user_dirs(env) {
            let ws = user.join("workspaceStorage");
            if let Ok(rd) = fs::read_dir(&ws) {
                let mut dirs: Vec<PathBuf> = rd.filter_map(Result::ok).map(|e| e.path()).collect();
                dirs.sort();
                for dir in dirs {
                    let project = workspace_folder(&dir.join("workspace.json"));
                    for f in session_files(&dir.join("chatSessions")) {
                        files.push((f, project.clone()));
                    }
                }
            }
            for f in session_files(&user.join("globalStorage/emptyWindowChatSessions")) {
                files.push((f, None));
            }
        }
        Box::new(
            files
                .into_iter()
                .filter(|(f, _)| !untouched_since(f, filter.since))
                .filter_map(|(f, project)| match parse_file(&f, project) {
                    Ok(se) if se.events.is_empty() && se.warnings.is_empty() => None,
                    other => Some(other),
                }),
        )
    }
}

fn session_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(rd) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = rd
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "json" || e == "jsonl"))
        .collect();
    files.sort();
    files
}

/// The folder of a workspace from `workspace.json`; multi-root workspaces have none.
fn workspace_folder(file: &Path) -> Option<PathBuf> {
    let v: Value = serde_json::from_str(&fs::read_to_string(file).ok()?).ok()?;
    uri_to_path(v.get("folder")?.as_str()?)
}

/// `file:///home/u/x%20y` -> `/home/u/x y`; `file:///c%3A/Users/u` -> `c:/Users/u`.
fn uri_to_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let bytes = rest.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(b) = u8::from_str_radix(&rest[i + 1..i + 3], 16)
        {
            out.push(b);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    let mut path = String::from_utf8(out).ok()?;
    let drive = path.len() > 2 && path.as_bytes()[2] == b':' && path.starts_with('/');
    if drive {
        path.remove(0);
    }
    Some(PathBuf::from(path))
}

/// Replays an operation log into the final object.
fn replay(text: &str) -> Result<Value, String> {
    let mut state: Option<Value> = None;
    for (n, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let op: Value = serde_json::from_str(line).map_err(|e| format!("line {}: {e}", n + 1))?;
        let kind = op.get("kind").and_then(Value::as_u64);
        if kind == Some(0) {
            state = op.get("v").cloned();
            continue;
        }
        let root = state
            .as_mut()
            .ok_or_else(|| format!("line {}: operation before the snapshot", n + 1))?;
        let path = op
            .get("k")
            .and_then(Value::as_array)
            .ok_or_else(|| format!("line {}: operation without a path", n + 1))?;
        let (last, parents) = path
            .split_last()
            .ok_or_else(|| format!("line {}: empty path", n + 1))?;
        let mut cur = root;
        for step in parents {
            cur = step_into(cur, step).ok_or_else(|| format!("line {}: path not found", n + 1))?;
        }
        match kind {
            Some(1) => {
                let v = op.get("v").cloned().unwrap_or(Value::Null);
                match (cur, last) {
                    (Value::Array(a), Value::Number(i)) => {
                        let i = i.as_u64().unwrap_or(0) as usize;
                        if i < a.len() {
                            a[i] = v;
                        } else {
                            a.push(v);
                        }
                    }
                    (Value::Object(m), Value::String(k)) => {
                        m.insert(k.clone(), v);
                    }
                    _ => return Err(format!("line {}: cannot set the path", n + 1)),
                }
            }
            Some(2) => {
                let target = step_into(cur, last)
                    .and_then(Value::as_array_mut)
                    .ok_or_else(|| format!("line {}: splice target is not an array", n + 1))?;
                if let Some(i) = op.get("i").and_then(Value::as_u64) {
                    target.truncate(i as usize);
                }
                if let Some(Value::Array(items)) = op.get("v") {
                    target.extend(items.iter().cloned());
                }
            }
            _ => return Err(format!("line {}: unknown operation", n + 1)),
        }
    }
    state.ok_or_else(|| "empty log".to_owned())
}

fn step_into<'v>(v: &'v mut Value, step: &Value) -> Option<&'v mut Value> {
    match (v, step) {
        (Value::Object(m), Value::String(k)) => m.get_mut(k),
        (Value::Array(a), Value::Number(i)) => a.get_mut(i.as_u64()? as usize),
        _ => None,
    }
}

fn str_of<'v>(v: &'v Value, key: &str) -> Option<&'v str> {
    v.get(key).and_then(Value::as_str).filter(|s| !s.is_empty())
}

fn millis(v: &Value, key: &str) -> Option<Timestamp> {
    Timestamp::from_millisecond(v.get(key)?.as_i64()?).ok()
}

fn parse_file(path: &Path, project: Option<PathBuf>) -> Result<SessionEvents, AdapterError> {
    let text = fs::read_to_string(path).map_err(|source| AdapterError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let fallback_id = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("session")
        .to_owned();
    let mut se = SessionEvents::new(Session::new(fallback_id, Source::VsCodeCopilot));
    se.session.project_dir = project;
    let state = if path.extension().is_some_and(|e| e == "jsonl") {
        replay(&text)
    } else {
        serde_json::from_str(&text).map_err(|e| e.to_string())
    };
    let state = match state {
        Ok(v) => v,
        Err(e) => {
            se.warn(
                Warning::new(format!("cannot read chat session: {e}"))
                    .at(path.display().to_string()),
            );
            return Ok(se);
        }
    };
    if let Some(id) = str_of(&state, "sessionId") {
        se.session.id = id.to_owned();
    }
    se.session.started_at = millis(&state, "creationDate");
    let requests = state
        .get("requests")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let mut last_model = None;
    for r in requests {
        let at = millis(r, "timestamp");
        let cancelled = r.get("isCanceled").and_then(Value::as_bool) == Some(true);
        se.push(at, EventKind::UserTurn);
        for part in r
            .get("response")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default()
        {
            if str_of(part, "kind") == Some("toolInvocationSerialized") {
                tool(part, at, cancelled, &mut se);
            }
        }
        let model =
            str_of(r, "modelId").map(|m| m.strip_prefix("copilot/").unwrap_or(m).to_owned());
        if model.is_some() {
            last_model = model.clone();
        }
        if let Some(result) = r.get("result") {
            usage(result, r, at, model, &mut se);
        }
    }
    se.session.model = last_model;
    se.session.ended_at = se.events.iter().filter_map(|e| e.at).max();
    se.events.sort_by_key(|e| e.at);
    Ok(se)
}

/// File paths a tool invocation refers to (`invocationMessage.uris` keys).
fn tool_paths(part: &Value) -> Vec<PathBuf> {
    ["invocationMessage", "pastTenseMessage"]
        .iter()
        .filter_map(|k| part.get(*k)?.get("uris")?.as_object())
        .flat_map(|uris| uris.keys())
        .filter_map(|u| uri_to_path(u))
        .collect()
}

fn tool(part: &Value, at: Option<Timestamp>, cancelled: bool, se: &mut SessionEvents) {
    let Some(name) = str_of(part, "toolId") else {
        return;
    };
    let paths = tool_paths(part);
    let status = if part.get("isComplete").and_then(Value::as_bool) == Some(true) && !cancelled {
        ToolStatus::Ok
    } else {
        ToolStatus::Unknown
    };
    let summary = paths
        .first()
        .map(|p| p.display().to_string().chars().take(80).collect());
    se.push(
        at,
        EventKind::ToolCall(ToolCall {
            name: name.to_owned(),
            input_summary: summary,
            status,
            duration_ms: None,
        }),
    );

    let skill_file = paths
        .iter()
        .find(|p| p.file_name().is_some_and(|n| n == "SKILL.md"));
    if let Some(file) = skill_file
        && let Some(skill) = file
            .parent()
            .and_then(Path::file_name)
            .and_then(|n| n.to_str())
    {
        let mut ev = SkillInvoked::new(skill, SkillTrigger::Model);
        ev.path = Some(file.clone());
        se.push(at, EventKind::SkillInvoked(ev));
    } else if name == "skill" {
        let data = part.get("toolSpecificData").unwrap_or(&Value::Null);
        match str_of(data, "skillName").or_else(|| str_of(data, "name")) {
            Some(skill) => se.push(
                at,
                EventKind::SkillInvoked(SkillInvoked::new(skill, SkillTrigger::Model)),
            ),
            None => se.warn(Warning::new("skill tool call without a skill name")),
        }
    }
}

/// `Claude Sonnet 4.6 \u{2022} 1x` -> 1.0; `GPT-4.1 \u{2022} 0.33x` -> 0.33.
fn premium_multiplier(details: &str) -> Option<f64> {
    let tail = details.rsplit('\u{2022}').next()?.trim();
    tail.strip_suffix('x')?.trim().parse().ok()
}

fn usage(
    result: &Value,
    request: &Value,
    at: Option<Timestamp>,
    model: Option<String>,
    se: &mut SessionEvents,
) {
    let Some(u) = result.get("usage") else { return };
    let prompt = u.get("promptTokens").and_then(Value::as_u64);
    let completion = u.get("completionTokens").and_then(Value::as_u64);
    if prompt.is_none() && completion.is_none() {
        return;
    }
    let elapsed = result
        .get("timings")
        .and_then(|t| t.get("totalElapsed"))
        .and_then(Value::as_i64);
    let end = match (at, elapsed) {
        (Some(a), Some(ms)) => Timestamp::from_millisecond(a.as_millisecond() + ms).ok(),
        _ => at,
    };
    // The prompt breakdown tells how much of the prompt is static context.
    for d in u
        .get("promptTokenDetails")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
    {
        let kind = match str_of(d, "label") {
            Some("System Instructions") => ContextKind::SystemPrompt,
            Some("Tool Definitions") => ContextKind::ToolDefinition,
            _ => continue,
        };
        if let (Some(p), Some(pct)) = (prompt, d.get("percentageOfPrompt").and_then(Value::as_u64))
        {
            se.push(
                at,
                EventKind::ContextItem(ContextItem {
                    kind,
                    name: None,
                    bytes: 0,
                    tokens: Some(p * pct / 100),
                }),
            );
        }
    }
    let usage = Usage {
        model,
        input: prompt.unwrap_or(0),
        output: completion.unwrap_or(0),
        measured: true,
        premium_requests: result
            .get("details")
            .and_then(Value::as_str)
            .or_else(|| str_of(request, "details"))
            .and_then(premium_multiplier),
        ..Usage::default()
    };
    se.push(end, EventKind::Usage(usage));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uris_become_paths() {
        assert_eq!(
            uri_to_path("file:///home/u/a%20b.md"),
            Some(PathBuf::from("/home/u/a b.md"))
        );
        assert_eq!(
            uri_to_path("file:///c%3A/Users/u/x"),
            Some(PathBuf::from("c:/Users/u/x"))
        );
        assert_eq!(uri_to_path("https://example.test"), None);
    }

    #[test]
    fn premium_multiplier_is_parsed_from_the_label() {
        assert_eq!(
            premium_multiplier("Claude Sonnet 4.6 \u{2022} 1x"),
            Some(1.0)
        );
        assert_eq!(premium_multiplier("GPT-4.1 \u{2022} 0.33x"), Some(0.33));
        assert_eq!(premium_multiplier("GPT-4.1"), None);
    }

    #[test]
    fn splice_truncates_from_the_index_before_appending() {
        let log = [
            r#"{"kind":0,"v":{"requests":[]}}"#,
            r#"{"kind":2,"k":["requests"],"v":[{"a":1},{"a":2}]}"#,
            r#"{"kind":1,"k":["requests",0,"a"],"v":9}"#,
            r#"{"kind":2,"k":["requests"],"i":1,"v":[{"a":3}]}"#,
        ]
        .join("\n");
        let v = replay(&log).unwrap();
        assert_eq!(v["requests"], serde_json::json!([{"a": 9}, {"a": 3}]));
        assert!(replay("{\"kind\":1,\"k\":[\"x\"],\"v\":1}").is_err());
    }
}
