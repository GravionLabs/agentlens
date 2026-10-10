mod support;

use agentlens_core::adapter::{Adapter, SessionFilter};
use agentlens_core::adapters::copilot_cli::CopilotCliAdapter;
use agentlens_core::model::{EventKind, SessionEvents, SkillTrigger, ToolStatus, Usage};
use rusqlite::Connection;
use support::TempHome;

const STATE: &str = ".copilot/session-state";

/// Creates `session-store.db` with a usage table holding rows for `cp-1` only.
fn write_store(home: &TempHome) {
    let path = home.path().join(".copilot/session-store.db");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let conn = Connection::open(path).unwrap();
    conn.execute_batch(
        "CREATE TABLE assistant_usage_events (
           id INTEGER PRIMARY KEY AUTOINCREMENT, session_id TEXT NOT NULL, turn_index INTEGER,
           agent_id TEXT, parent_tool_call_id TEXT, model TEXT NOT NULL,
           input_tokens INTEGER, output_tokens INTEGER, cache_read_tokens INTEGER,
           cache_write_tokens INTEGER, reasoning_tokens INTEGER, total_nano_aiu INTEGER,
           created_at TEXT);
         INSERT INTO assistant_usage_events
           (session_id, turn_index, agent_id, parent_tool_call_id, model, input_tokens,
            output_tokens, cache_read_tokens, cache_write_tokens, reasoning_tokens, created_at)
         VALUES
           ('cp-1', 0, NULL, NULL, 'model-a', 1000, 50, 600, 100, 5, '2026-07-15T04:00:02.000Z'),
           ('cp-1', 0, NULL, NULL, 'model-a', 1200, 70, 900, 0, 0, '2026-07-15T04:00:05.500Z'),
           ('cp-1', 0, 'explore-1', 'c3', 'model-b', 400, 30, 0, 0, 0, '2026-07-15T04:00:07.500Z');",
    )
    .unwrap();
}

fn home(store: bool) -> TempHome {
    let home = TempHome::new();
    home.copy_fixture(
        "copilot_cli/with_store.jsonl",
        &format!("{STATE}/cp-1/events.jsonl"),
    );
    home.copy_fixture(
        "copilot_cli/no_store_rows.jsonl",
        &format!("{STATE}/cp-2/events.jsonl"),
    );
    if store {
        write_store(&home);
    }
    home
}

fn read_all(home: &TempHome) -> Vec<SessionEvents> {
    let env = home.env();
    let filter = SessionFilter::default();
    CopilotCliAdapter
        .sessions(&env, &filter)
        .map(|r| r.unwrap())
        .collect()
}

fn session(home: &TempHome, id: &str) -> SessionEvents {
    read_all(home)
        .into_iter()
        .find(|s| s.session.id == id)
        .unwrap()
}

fn usages(se: &SessionEvents) -> Vec<&Usage> {
    se.events
        .iter()
        .filter_map(|e| match &e.kind {
            EventKind::Usage(u) => Some(u),
            _ => None,
        })
        .collect()
}

#[test]
fn detects_the_session_state_directory() {
    let empty = TempHome::new();
    assert!(!CopilotCliAdapter.detect(&empty.env()).found);
    let h = home(false);
    let d = CopilotCliAdapter.detect(&h.env());
    assert!(d.found);
    assert_eq!(d.roots, [h.path().join(STATE)]);
}

#[test]
fn copilot_home_variable_overrides_the_default_location() {
    let h = home(false);
    let env = h.env().with_var(
        "COPILOT_HOME",
        h.path().join("elsewhere").display().to_string(),
    );
    assert!(!CopilotCliAdapter.detect(&env).found);
}

#[test]
fn session_metadata_comes_from_session_start() {
    let se = session(&home(false), "cp-1");
    assert_eq!(
        se.session.project_dir.as_deref(),
        Some(std::path::Path::new("/work/demo"))
    );
    assert_eq!(se.session.branch.as_deref(), Some("main"));
    assert_eq!(se.session.model.as_deref(), Some("model-a"));
    assert_eq!(
        se.session.started_at.unwrap().to_string(),
        "2026-07-15T04:00:00Z"
    );
}

#[test]
fn tool_calls_pair_start_and_complete() {
    let se = session(&home(false), "cp-1");
    let calls: Vec<_> = se
        .events
        .iter()
        .filter_map(|e| match &e.kind {
            EventKind::ToolCall(c) => Some(c),
            _ => None,
        })
        .collect();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].name, "view");
    assert_eq!(calls[0].status, ToolStatus::Ok);
    assert_eq!(calls[0].duration_ms, Some(500));
    assert_eq!(calls[0].input_summary.as_deref(), Some("/work/demo/a.txt"));
    assert_eq!(calls[1].status, ToolStatus::Error);
    assert_eq!(calls[1].input_summary.as_deref(), Some("curl"));
}

#[test]
fn skills_distinguish_agent_and_user_invocations() {
    let se = session(&home(false), "cp-1");
    let skills: Vec<_> = se
        .events
        .iter()
        .filter_map(|e| match &e.kind {
            EventKind::SkillInvoked(s) => Some(s),
            _ => None,
        })
        .collect();
    assert_eq!(skills.len(), 2);
    assert_eq!(
        (skills[0].name.as_str(), skills[0].trigger),
        ("demo-skill", SkillTrigger::Model)
    );
    assert_eq!(skills[0].content_bytes, Some(10));
    assert_eq!(
        (skills[1].name.as_str(), skills[1].trigger),
        ("typed-skill", SkillTrigger::User)
    );
    assert_eq!(skills[1].content_bytes, Some(20));
}

#[test]
fn user_turns_and_compactions_are_emitted() {
    let se = session(&home(false), "cp-1");
    let count = |f: fn(&EventKind) -> bool| se.events.iter().filter(|e| f(&e.kind)).count();
    assert_eq!(count(|k| matches!(k, EventKind::UserTurn)), 1);
    assert_eq!(count(|k| matches!(k, EventKind::Compaction)), 1);
}

#[test]
fn usage_comes_from_the_store_and_excludes_cached_tokens_from_input() {
    let se = session(&home(true), "cp-1");
    let u = usages(&se);
    assert_eq!(u.len(), 3, "store rows replace the shutdown totals");
    assert!(u.iter().all(|u| u.measured));
    // `input_tokens` of the store contains the cached parts.
    assert_eq!(
        (u[0].input, u[0].cache_read, u[0].cache_write),
        (300, 600, 100)
    );
    assert_eq!((u[0].output, u[0].reasoning), (50, 5));
    assert_eq!(u[0].model.as_deref(), Some("model-a"));
    assert!(!u[0].subagent);
    assert!(u[2].subagent);
}

#[test]
fn shutdown_totals_are_the_fallback_without_store_rows() {
    // The store exists, but has no rows for cp-2.
    let se = session(&home(true), "cp-2");
    let u = usages(&se);
    assert_eq!(u.len(), 2);
    let a = u
        .iter()
        .find(|u| u.model.as_deref() == Some("model-a"))
        .unwrap();
    assert_eq!(
        (a.input, a.cache_read, a.cache_write, a.output),
        (200, 700, 100, 50)
    );
    assert!(a.measured);
    assert_eq!(se.session.model.as_deref(), Some("model-b"));
    // Without a store at all, both sessions use their shutdown totals.
    let se = session(&home(false), "cp-1");
    let u = usages(&se);
    assert_eq!((u.len(), u[0].input, u[0].output), (1, 99999, 9999));
}

#[test]
fn unknown_events_and_invalid_lines_become_warnings() {
    let se = session(&home(false), "cp-1");
    let messages: Vec<_> = se.warnings.iter().map(|w| w.message.as_str()).collect();
    assert!(
        messages.iter().any(|m| m.contains("future.event (2)")),
        "{messages:?}"
    );
    assert!(
        messages.iter().any(|m| m.contains("invalid JSON")),
        "{messages:?}"
    );
    assert!(
        !messages.iter().any(|m| m.contains("subagent.started")),
        "{messages:?}"
    );
}

#[test]
fn store_with_an_unknown_schema_falls_back_with_a_warning() {
    let h = home(false);
    let path = h.path().join(".copilot/session-store.db");
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch("CREATE TABLE assistant_usage_events (session_id TEXT, model TEXT);")
        .unwrap();
    drop(conn);
    let se = session(&h, "cp-1");
    assert_eq!(usages(&se).len(), 1);
    assert!(
        se.warnings
            .iter()
            .any(|w| w.message.contains("unsupported session-store schema"))
    );
}
