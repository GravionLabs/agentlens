//! Claude Code: `<config dir>/projects/<slug>/<session>.jsonl`, plus subagent
//! transcripts in `<slug>/<session>/subagents/*.jsonl`.
//!
//! The format is not documented; the parser only relies on a handful of fields
//! and skips everything it does not understand.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use jiff::Timestamp;
use serde_json::Value;

use crate::adapter::{Adapter, AdapterError, Detection, SessionFilter, SessionIter};
use crate::env::Env;
use crate::model::{
    ContextItem, ContextKind, Event, EventKind, Session, SessionEvents, SkillInvoked, SkillTrigger,
    Source, ToolCall, ToolStatus, Usage, Warning,
};

/// Slash commands that control the client itself and never load a skill.
const BUILTIN_COMMANDS: &[&str] = &[
    "add-dir",
    "agents",
    "bug",
    "clear",
    "compact",
    "config",
    "context",
    "cost",
    "doctor",
    "exit",
    "export",
    "help",
    "hooks",
    "ide",
    "install-github-app",
    "login",
    "logout",
    "mcp",
    "memory",
    "model",
    "output-style",
    "permissions",
    "plan",
    "privacy-settings",
    "quit",
    "release-notes",
    "resume",
    "rewind",
    "status",
    "statusline",
    "terminal-setup",
    "todos",
    "upgrade",
    "usage",
    "vim",
    "web-setup",
];

pub struct ClaudeCodeAdapter;

pub(crate) fn claude_home(env: &Env) -> PathBuf {
    env.var("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| env.home().join(".claude"))
}

fn projects_dir(env: &Env) -> PathBuf {
    env.var("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| env.home().join(".claude"))
        .join("projects")
}

impl Adapter for ClaudeCodeAdapter {
    fn id(&self) -> Source {
        Source::ClaudeCode
    }

    fn detect(&self, env: &Env) -> Detection {
        let dir = projects_dir(env);
        if dir.is_dir() {
            Detection::found(vec![dir])
        } else {
            Detection::not_found(format!("{} does not exist", dir.display()))
        }
    }

    fn sessions<'a>(&'a self, env: &'a Env, filter: &'a SessionFilter) -> SessionIter<'a> {
        let files = match discover(&projects_dir(env), filter) {
            Ok(files) => files,
            Err(e) => return Box::new(std::iter::once(Err(e))),
        };
        Box::new(
            dedupe(parse_all(&files))
                .into_iter()
                .filter(|r| !matches!(r, Ok(se) if se.events.is_empty() && se.warnings.is_empty())),
        )
    }
}

/// The same session can sit under two project directories (a moved or renamed
/// project, a resumed session). It is one session: the copy with the most events
/// is kept (the first one on a tie).
fn dedupe(
    results: Vec<Result<SessionEvents, AdapterError>>,
) -> Vec<Result<SessionEvents, AdapterError>> {
    let mut best: HashMap<String, (usize, usize)> = HashMap::new();
    for (i, r) in results.iter().enumerate() {
        if let Ok(se) = r {
            let events = se.events.len();
            let entry = best.entry(se.session.id.clone()).or_insert((i, events));
            if events > entry.1 {
                *entry = (i, events);
            }
        }
    }
    results
        .into_iter()
        .enumerate()
        .filter(|(i, r)| match r {
            Ok(se) => best[&se.session.id].0 == *i,
            Err(_) => true,
        })
        .map(|(_, r)| r)
        .collect()
}

/// Parses the session files on all cores; the order of `files` is kept.
fn parse_all(files: &[SessionFile]) -> Vec<Result<SessionEvents, AdapterError>> {
    let workers = std::thread::available_parallelism()
        .map_or(1, |n| n.get())
        .min(files.len().max(1));
    if workers <= 1 {
        return files
            .iter()
            .map(|f| parse_session(&f.path, &f.subagents))
            .collect();
    }
    let next = std::sync::atomic::AtomicUsize::new(0);
    let mut slots: Vec<Option<Result<SessionEvents, AdapterError>>> =
        (0..files.len()).map(|_| None).collect();
    let results = std::sync::Mutex::new(&mut slots);
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                loop {
                    let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let Some(f) = files.get(i) else { break };
                    let parsed = parse_session(&f.path, &f.subagents);
                    results.lock().expect("results lock")[i] = Some(parsed);
                }
            });
        }
    });
    slots.into_iter().flatten().collect()
}

struct SessionFile {
    path: PathBuf,
    subagents: Vec<PathBuf>,
}

fn io_err(path: &Path, source: std::io::Error) -> AdapterError {
    AdapterError::Io {
        path: path.to_path_buf(),
        source,
    }
}

fn jsonl_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(rd) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = rd
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "jsonl"))
        .collect();
    files.sort();
    files
}

fn discover(projects: &Path, filter: &SessionFilter) -> Result<Vec<SessionFile>, AdapterError> {
    let mut slugs: Vec<PathBuf> = fs::read_dir(projects)
        .map_err(|e| io_err(projects, e))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    slugs.sort();
    let mut out = Vec::new();
    for slug in slugs {
        for path in jsonl_files(&slug) {
            if untouched_since(&path, filter.since) {
                continue;
            }
            let stem = path.file_stem().map(PathBuf::from).unwrap_or_default();
            let subagents = jsonl_files(&slug.join(stem).join("subagents"));
            out.push(SessionFile { path, subagents });
        }
    }
    Ok(out)
}

/// A file last modified before `since` cannot contain newer events.
pub(crate) fn untouched_since(path: &Path, since: Option<Timestamp>) -> bool {
    let Some(since) = since else { return false };
    fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| Timestamp::try_from(t).ok())
        .is_some_and(|modified| modified < since)
}

#[derive(Default)]
struct Meta {
    cwd: Option<String>,
    branch: Option<String>,
    model: Option<String>,
    first: Option<Timestamp>,
    last: Option<Timestamp>,
}

impl Meta {
    fn seen_at(&mut self, ts: Timestamp) {
        self.first = Some(self.first.map_or(ts, |f| f.min(ts)));
        self.last = Some(self.last.map_or(ts, |l| l.max(ts)));
    }
}

/// Reads one session: the main transcript and its subagent transcripts.
pub fn parse_session(path: &Path, subagents: &[PathBuf]) -> Result<SessionEvents, AdapterError> {
    let id = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut warnings = Vec::new();
    let mut meta = Meta::default();
    let mut events = Vec::new();
    parse_file(path, false, &mut events, &mut warnings, &mut meta)?;
    for sub in subagents {
        let mut sub_events = Vec::new();
        match parse_file(sub, true, &mut sub_events, &mut warnings, &mut meta) {
            Ok(()) => events.append(&mut sub_events),
            Err(e) => warnings.push(Warning::new(e.to_string())),
        }
    }
    // Interleave subagent events by time, but only if that is well defined.
    if !subagents.is_empty() && events.iter().all(|e| e.at.is_some()) {
        events.sort_by_key(|e| e.at);
    }
    let mut session = Session::new(id, Source::ClaudeCode);
    session.project_dir = meta.cwd.map(PathBuf::from);
    session.branch = meta.branch;
    session.model = meta.model;
    session.started_at = meta.first;
    session.ended_at = meta.last;
    Ok(SessionEvents {
        session,
        events,
        warnings,
    })
}

struct Parser<'a> {
    file: String,
    force_subagent: bool,
    events: &'a mut Vec<Event>,
    warnings: &'a mut Vec<Warning>,
    meta: &'a mut Meta,
    /// `message.id:requestId` -> index of its `Usage` event.
    usage_idx: HashMap<String, usize>,
    /// `tool_use` id -> (index of its `ToolCall`, start time).
    tool_idx: HashMap<String, (usize, Option<Timestamp>)>,
    /// `tool_use` id -> index of its `SkillInvoked`.
    skill_idx: HashMap<String, usize>,
    /// A user-typed skill waiting for the content message that follows.
    pending_slash: Option<usize>,
}

fn parse_file(
    path: &Path,
    subagent: bool,
    events: &mut Vec<Event>,
    warnings: &mut Vec<Warning>,
    meta: &mut Meta,
) -> Result<(), AdapterError> {
    let reader = BufReader::new(File::open(path).map_err(|e| io_err(path, e))?);
    let mut p = Parser {
        file: path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        force_subagent: subagent,
        events,
        warnings,
        meta,
        usage_idx: HashMap::new(),
        tool_idx: HashMap::new(),
        skill_idx: HashMap::new(),
        pending_slash: None,
    };
    for (n, line) in reader.lines().enumerate() {
        let line = line.map_err(|e| io_err(path, e))?;
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<Value>(&line) {
            Ok(v) => p.line(&v),
            Err(e) => p
                .warnings
                .push(Warning::new(format!("invalid JSON: {e}")).at(format!(
                    "{}:{}",
                    p.file,
                    n + 1
                ))),
        }
    }
    Ok(())
}

fn str_of<'v>(v: &'v Value, key: &str) -> Option<&'v str> {
    v.get(key).and_then(Value::as_str).filter(|s| !s.is_empty())
}

fn u64_of(v: &Value, key: &str) -> u64 {
    v.get(key).and_then(Value::as_u64).unwrap_or(0)
}

/// All text of a message `content` (a string or an array of blocks).
fn text_of(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .filter(|b| b.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|b| b.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

impl Parser<'_> {
    fn push(&mut self, at: Option<Timestamp>, kind: EventKind) -> usize {
        self.events.push(Event { at, kind });
        self.events.len() - 1
    }

    fn line(&mut self, v: &Value) {
        let at = str_of(v, "timestamp").and_then(|s| s.parse::<Timestamp>().ok());
        if let Some(ts) = at {
            self.meta.seen_at(ts);
        }
        if self.meta.cwd.is_none() {
            self.meta.cwd = str_of(v, "cwd").map(str::to_owned);
        }
        if self.meta.branch.is_none() {
            self.meta.branch = str_of(v, "gitBranch")
                .filter(|b| *b != "HEAD")
                .map(str::to_owned);
        }
        let subagent =
            self.force_subagent || v.get("isSidechain").and_then(Value::as_bool) == Some(true);
        match str_of(v, "type") {
            Some("assistant") => self.assistant(v, at, subagent),
            Some("user") => self.user(v, at, subagent),
            Some("attachment") if !subagent => {
                if let Some(a) = v.get("attachment") {
                    self.attachment(a, at);
                }
            }
            Some("system") if str_of(v, "subtype") == Some("compact_boundary") && !subagent => {
                self.push(at, EventKind::Compaction);
            }
            _ => {}
        }
    }

    fn context(
        &mut self,
        at: Option<Timestamp>,
        kind: ContextKind,
        name: Option<&str>,
        bytes: usize,
    ) {
        self.push(
            at,
            EventKind::ContextItem(ContextItem {
                kind,
                name: name.map(str::to_owned),
                bytes: bytes as u64,
                tokens: None,
            }),
        );
    }

    /// Attachments announce static context. Only names and sizes are read.
    fn attachment(&mut self, a: &Value, at: Option<Timestamp>) {
        let list = |key: &str| a.get(key).and_then(Value::as_array);
        let len = |v: &Value| v.as_str().map_or_else(|| v.to_string().len(), str::len);
        match str_of(a, "type") {
            Some("deferred_tools_delta") => {
                let names = list("addedNames").map(Vec::as_slice).unwrap_or_default();
                let lines = list("addedLines").map(Vec::as_slice).unwrap_or_default();
                for (i, name) in names.iter().enumerate() {
                    let bytes = lines.get(i).map_or(0, len);
                    self.context(at, ContextKind::ToolListing, name.as_str(), bytes);
                }
                for d in list("surfacedDefinitions")
                    .map(Vec::as_slice)
                    .unwrap_or_default()
                {
                    let bytes = d.get("definition").map_or(0, len);
                    self.context(at, ContextKind::ToolDefinition, str_of(d, "name"), bytes);
                }
            }
            Some("mcp_instructions_delta") => {
                let names = list("addedNames").map(Vec::as_slice).unwrap_or_default();
                let blocks = list("addedBlocks").map(Vec::as_slice).unwrap_or_default();
                for (i, block) in blocks.iter().enumerate() {
                    let name = names.get(i).and_then(Value::as_str);
                    self.context(at, ContextKind::McpInstructions, name, len(block));
                }
            }
            Some("agent_listing_delta") => {
                let names = list("addedTypes").map(Vec::as_slice).unwrap_or_default();
                let lines = list("addedLines").map(Vec::as_slice).unwrap_or_default();
                for (i, line) in lines.iter().enumerate() {
                    let name = names.get(i).and_then(Value::as_str);
                    self.context(at, ContextKind::AgentListing, name, len(line));
                }
            }
            Some("skill_listing") => {
                let bytes = a.get("content").map_or(0, len);
                self.context(at, ContextKind::SkillListing, None, bytes);
            }
            Some("instructions") => {
                for f in list("files").map(Vec::as_slice).unwrap_or_default() {
                    let bytes = f.get("content").map_or(0, len);
                    self.context(at, ContextKind::InstructionFile, str_of(f, "path"), bytes);
                }
            }
            Some("nested_memory") => {
                let bytes = a.get("content").map_or(0, len);
                self.context(at, ContextKind::InstructionFile, str_of(a, "path"), bytes);
            }
            _ => {}
        }
    }

    fn assistant(&mut self, v: &Value, at: Option<Timestamp>, subagent: bool) {
        let Some(msg) = v.get("message") else { return };
        let model = str_of(msg, "model").map(str::to_owned);
        if let Some(usage) = msg.get("usage").filter(|u| u.is_object())
            && model.as_deref() != Some("<synthetic>")
        {
            if self.meta.model.is_none() && !subagent {
                self.meta.model = model.clone();
            }
            self.usage(msg, v, usage, model, at, subagent);
        }
        let Some(blocks) = msg.get("content").and_then(Value::as_array) else {
            return;
        };
        for block in blocks {
            if str_of(block, "type") != Some("tool_use") {
                continue;
            }
            let name = str_of(block, "name").unwrap_or("unknown").to_owned();
            let input = block.get("input").unwrap_or(&Value::Null);
            let call = ToolCall {
                input_summary: summarize(&name, input),
                name: name.clone(),
                status: ToolStatus::Unknown,
                duration_ms: None,
            };
            let idx = self.push(at, EventKind::ToolCall(call));
            let tool_id = str_of(block, "id").map(str::to_owned);
            if let Some(id) = &tool_id {
                self.tool_idx.insert(id.clone(), (idx, at));
            }
            for skill in skill_folders_touched(&name, input) {
                self.push(at, EventKind::SkillFileUse(skill));
            }
            for skill in skill_folders_edited(&name, input) {
                self.push(at, EventKind::SkillFileEdit(skill));
            }
            if name == "Skill"
                && let Some(skill) = str_of(input, "skill")
            {
                let i = self.push(
                    at,
                    EventKind::SkillInvoked(SkillInvoked::new(skill, SkillTrigger::Model)),
                );
                if let Some(id) = tool_id {
                    self.skill_idx.insert(id, i);
                }
            }
        }
    }

    /// One API response is spread over several lines that repeat the same
    /// `usage`; count it once, keeping the largest values (streaming updates).
    fn usage(
        &mut self,
        msg: &Value,
        line: &Value,
        usage: &Value,
        model: Option<String>,
        at: Option<Timestamp>,
        subagent: bool,
    ) {
        let new = Usage {
            model,
            input: u64_of(usage, "input_tokens"),
            output: u64_of(usage, "output_tokens"),
            cache_read: u64_of(usage, "cache_read_input_tokens"),
            cache_write: u64_of(usage, "cache_creation_input_tokens"),
            reasoning: usage
                .get("output_tokens_details")
                .map_or(0, |d| u64_of(d, "thinking_tokens")),
            cost_usd: None,
            measured: true,
            subagent,
            premium_requests: None,
            skill: None,
        };
        let key =
            str_of(msg, "id").map(|m| format!("{m}:{}", str_of(line, "requestId").unwrap_or("")));
        if let Some(&i) = key.as_ref().and_then(|k| self.usage_idx.get(k))
            && let EventKind::Usage(old) = &mut self.events[i].kind
        {
            old.input = old.input.max(new.input);
            old.output = old.output.max(new.output);
            old.cache_read = old.cache_read.max(new.cache_read);
            old.cache_write = old.cache_write.max(new.cache_write);
            old.reasoning = old.reasoning.max(new.reasoning);
            return;
        }
        let i = self.push(at, EventKind::Usage(new));
        if let Some(k) = key {
            self.usage_idx.insert(k, i);
        }
    }

    fn user(&mut self, v: &Value, at: Option<Timestamp>, subagent: bool) {
        if v.get("isCompactSummary").and_then(Value::as_bool) == Some(true) {
            return;
        }
        let content = v
            .get("message")
            .and_then(|m| m.get("content"))
            .unwrap_or(&Value::Null);
        let is_meta = v.get("isMeta").and_then(Value::as_bool) == Some(true);
        if let Some(blocks) = content.as_array() {
            for b in blocks
                .iter()
                .filter(|b| str_of(b, "type") == Some("tool_result"))
            {
                self.tool_result(b, at);
            }
        }
        let text = text_of(content);
        if is_meta {
            self.meta_message(v, &text);
            return;
        }
        let trimmed = text.trim_start();
        if trimmed.is_empty() || trimmed.starts_with("<local-command-") {
            return;
        }
        if let Some(command) = command_name(trimmed) {
            if !subagent {
                self.push(at, EventKind::UserTurn);
            }
            if !BUILTIN_COMMANDS.contains(&command) {
                let i = self.push(
                    at,
                    EventKind::SkillInvoked(SkillInvoked::new(command, SkillTrigger::User)),
                );
                self.pending_slash = Some(i);
            }
            return;
        }
        self.pending_slash = None;
        if !subagent {
            self.push(at, EventKind::UserTurn);
        }
    }

    fn tool_result(&mut self, block: &Value, at: Option<Timestamp>) {
        let Some((idx, started)) =
            str_of(block, "tool_use_id").and_then(|id| self.tool_idx.get(id).copied())
        else {
            return;
        };
        if let EventKind::ToolCall(call) = &mut self.events[idx].kind {
            let failed = block.get("is_error").and_then(Value::as_bool) == Some(true);
            call.status = if failed {
                ToolStatus::Error
            } else {
                ToolStatus::Ok
            };
            if let (Some(s), Some(e)) = (started, at) {
                call.duration_ms = u64::try_from(e.as_millisecond() - s.as_millisecond()).ok();
            }
        }
    }

    /// A meta message carries the text the client injected, e.g. a skill body.
    fn meta_message(&mut self, v: &Value, text: &str) {
        if text.trim_start().starts_with("<local-command-") {
            return;
        }
        let target = str_of(v, "sourceToolUseID")
            .and_then(|id| self.skill_idx.get(id).copied())
            .or_else(|| self.pending_slash.take());
        if let Some(i) = target
            && let EventKind::SkillInvoked(skill) = &mut self.events[i].kind
        {
            skill.content_bytes = Some(text.len() as u64);
            skill.path = text
                .lines()
                .find_map(|l| l.strip_prefix("Base directory for this skill: "))
                .map(|p| PathBuf::from(p.trim()));
        }
    }
}

/// The names after `skills/` in a piece of text: `.../skills/<name>/...` or `skills/<name>`. A word that
/// only ends in `skills` (`myskills/x`) is no folder, and a bare `skills/` names nothing.
fn skill_names_in(text: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut from = 0;
    while let Some(found) = text[from..].find("skills/") {
        let start = from + found;
        from = start + "skills/".len();
        let before = text[..start].chars().next_back();
        if before.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '-') {
            continue;
        }
        let name: String = text[from..]
            .chars()
            .take_while(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.'))
            .collect();
        if !name.is_empty() && name != "." && name != ".." && !names.contains(&name) {
            names.push(name);
        }
    }
    names
}

/// Programs that run the file they are given: `bash <path>`, `python3 <path>`, ...
const INTERPRETERS: &[&str] = &[
    "bash",
    "sh",
    "zsh",
    "python",
    "python3",
    "node",
    "deno",
    "bun",
    "pwsh",
    "powershell",
    "ruby",
    "perl",
    "uv",
    "npx",
    "tsx",
    "source",
    ".",
    "dotnet",
];

/// The skills whose scripts a shell command runs: a path in a skill's folder given to an interpreter, or
/// run as a program. Listing, copying, printing, searching or committing files there is looking after
/// the skill and does not count.
fn skills_run_by(command: &str) -> Vec<String> {
    let mut names = Vec::new();
    for segment in command.split(['\n', ';', '|', '&']) {
        let mut words = segment
            .split_whitespace()
            .map(|w| w.trim_matches(['"', '\'']));
        let mut first = words.next();
        // `FOO=1 sudo time ./run.sh`
        while first.is_some_and(|w| {
            (w.contains('=') && !w.contains('/')) || matches!(w, "sudo" | "time" | "env" | "exec")
        }) {
            first = words.next();
        }
        let Some(first) = first else { continue };
        let program = first.rsplit('/').next().unwrap_or(first);
        let candidates: Vec<&str> = if INTERPRETERS.contains(&program) {
            words.collect()
        } else {
            vec![first]
        };
        for word in candidates {
            for name in skill_names_in(word) {
                if !names.contains(&name) {
                    names.push(name);
                }
            }
        }
    }
    names
}

/// The skills whose folder a tool call reads or searches, or whose script it runs. Only the name is
/// returned; nothing else of the call is kept. A folder called `skills` in some other project matches
/// too, so the analysis counts only names it knows as installed skills.
fn skill_folders_touched(tool: &str, input: &Value) -> Vec<String> {
    let fields: &[&str] = match tool {
        "Read" => &["file_path"],
        "Glob" => &["pattern", "path"],
        "Grep" => &["path", "glob"],
        "Bash" => {
            return str_of(input, "command")
                .map(skills_run_by)
                .unwrap_or_default();
        }
        _ => return Vec::new(),
    };
    let mut names = Vec::new();
    for field in fields {
        for name in str_of(input, field).map(skill_names_in).unwrap_or_default() {
            if !names.contains(&name) {
                names.push(name);
            }
        }
    }
    names
}

/// The skills whose files a tool call edits or writes.
fn skill_folders_edited(tool: &str, input: &Value) -> Vec<String> {
    match tool {
        "Edit" | "Write" | "MultiEdit" => str_of(input, "file_path")
            .map(skill_names_in)
            .unwrap_or_default(),
        "NotebookEdit" => str_of(input, "notebook_path")
            .map(skill_names_in)
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// The name of a typed slash command: `<command-name>/foo</command-name>` -> `foo`. A built-in command
/// starts with the name tag; a command that runs a skill or a prompt starts with the message tag
/// (`<command-message>foo</command-message>`, then the name, then the arguments), so the name is looked
/// for in the tags at the start of the text, in whatever order they come.
fn command_name(text: &str) -> Option<&str> {
    if !text.starts_with("<command-") {
        return None;
    }
    let rest = &text[text.find("<command-name>")? + "<command-name>".len()..];
    let name = rest.split("</command-name>").next()?;
    Some(name.trim().trim_start_matches('/')).filter(|n| !n.is_empty())
}

/// A short, privacy-conscious description of a tool input: a path, pattern or
/// skill name; for shell commands only the program, never the arguments.
fn summarize(tool: &str, input: &Value) -> Option<String> {
    if tool == "Bash" {
        return str_of(input, "command")?
            .split_whitespace()
            .next()
            .map(str::to_owned);
    }
    if matches!(tool, "Agent" | "Task")
        && let Some(kind) = str_of(input, "subagent_type")
    {
        return Some(kind.to_owned());
    }
    [
        "file_path",
        "path",
        "notebook_path",
        "pattern",
        "skill",
        "description",
    ]
    .iter()
    .find_map(|k| str_of(input, k))
    .map(|s| s.chars().take(80).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reading_and_running_in_a_skill_folder_names_the_skill() {
        let touched = |tool: &str, input: Value| skill_folders_touched(tool, &input);
        assert_eq!(
            touched(
                "Read",
                json!({"file_path": "/home/u/.claude/skills/ng-developer/SKILL.md"})
            ),
            ["ng-developer"]
        );
        assert_eq!(
            touched("Glob", json!({"pattern": "skills/ts-test/**/*.md"})),
            ["ts-test"]
        );
        assert_eq!(
            touched(
                "Grep",
                json!({"path": "/p/.claude/skills/a", "pattern": "x"})
            ),
            ["a"]
        );
        // A script run by an interpreter, by its path, or with a variable set in front.
        for command in [
            "bash ~/.claude/skills/github-issue/scripts/x.sh --issue 3",
            "python3 -u ./.claude/skills/github-issue/run.py",
            "FOO=1 sh \"$HOME/.claude/skills/github-issue/x.sh\"",
            "~/.claude/skills/github-issue/scripts/x.sh arg",
            "cd /p && bash skills/github-issue/x.sh | tee out",
        ] {
            assert_eq!(
                touched("Bash", json!({"command": command})),
                ["github-issue"],
                "{command}"
            );
        }
    }

    #[test]
    fn looking_after_a_skill_is_not_using_it() {
        // Listing, printing, copying, searching, committing and removing are maintenance.
        for command in [
            "ls ~/.claude/skills/a/",
            "cat skills/a/SKILL.md",
            "cp -r ~/.claude/skills/a /tmp/a",
            "git add .claude/skills/a && git commit -m x",
            "grep -rn foo ~/.claude/skills/a",
            "rm -rf skills/a",
            "echo skills/a",
        ] {
            assert!(
                skill_folders_touched("Bash", &json!({"command": command})).is_empty(),
                "{command}"
            );
        }
        // Editing is not a use, and the other tools name nothing.
        for tool in ["Edit", "Write", "Task", "WebFetch"] {
            assert!(
                skill_folders_touched(tool, &json!({"file_path": "/h/.claude/skills/x/SKILL.md"}))
                    .is_empty(),
                "{tool}"
            );
        }
        // A listing of the whole folder, or a word that only ends in `skills`, names no skill.
        assert!(
            skill_folders_touched("Read", &json!({"file_path": "/h/.claude/skills/"})).is_empty()
        );
        assert!(skill_folders_touched("Glob", &json!({"pattern": "myskills/x"})).is_empty());
        // The same skill twice in one call is one name.
        assert_eq!(
            skill_folders_touched(
                "Glob",
                &json!({"pattern": "skills/a/x", "path": "skills/a/y"})
            ),
            ["a"]
        );
    }

    #[test]
    fn writing_in_a_skill_folder_names_the_skill_as_edited() {
        let edited = |tool: &str, input: Value| skill_folders_edited(tool, &input);
        for tool in ["Edit", "Write", "MultiEdit"] {
            assert_eq!(
                edited(tool, json!({"file_path": "/h/.claude/skills/a/SKILL.md"})),
                ["a"],
                "{tool}"
            );
        }
        assert_eq!(
            edited(
                "NotebookEdit",
                json!({"notebook_path": "/h/.claude/skills/b/n.ipynb"})
            ),
            ["b"]
        );
        assert!(edited("Read", json!({"file_path": "/h/.claude/skills/a/SKILL.md"})).is_empty());
    }

    #[test]
    fn command_name_is_extracted() {
        assert_eq!(
            command_name("<command-name>/orch</command-name>\n<command-message>x"),
            Some("orch")
        );
        assert_eq!(command_name("hello"), None);
    }

    #[test]
    fn a_command_that_runs_a_skill_starts_with_the_message_tag() {
        // What Claude Code writes for a typed skill: the message, the name, then the arguments.
        let text = "<command-message>cliff-setup</command-message>\n<command-name>/cliff-setup</command-name>\n<command-args>0.8</command-args>";
        assert_eq!(command_name(text), Some("cliff-setup"));
        // A user who merely writes the tag later in a sentence typed no command.
        assert_eq!(command_name("see <command-name>/x</command-name>"), None);
        // A message tag without a name names nothing.
        assert_eq!(command_name("<command-message>x</command-message>"), None);
    }

    #[test]
    fn subagent_calls_are_summarized_by_agent_type() {
        let input = serde_json::json!({"subagent_type": "Explore", "description": "find things", "prompt": "secret"});
        assert_eq!(summarize("Agent", &input).as_deref(), Some("Explore"));
        assert_eq!(summarize("Other", &input).as_deref(), Some("find things"));
    }

    #[test]
    fn summary_never_contains_shell_arguments() {
        let input: Value =
            serde_json::json!({"command": "curl -H 'Authorization: secret' https://x"});
        assert_eq!(summarize("Bash", &input).as_deref(), Some("curl"));
        let input: Value = serde_json::json!({"file_path": "src/main.rs"});
        assert_eq!(summarize("Read", &input).as_deref(), Some("src/main.rs"));
    }
}
