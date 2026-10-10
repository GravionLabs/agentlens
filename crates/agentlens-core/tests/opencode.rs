mod support;

use std::path::Path;

use agentlens_core::adapter::{Adapter, SessionFilter};
use agentlens_core::adapters::opencode::OpenCodeAdapter;
use agentlens_core::model::{EventKind, SessionEvents, SkillTrigger, ToolStatus, Usage};
use rusqlite::Connection;
use support::TempHome;

fn fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/opencode")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// A home directory whose OpenCode database is built from the SQL fixtures.
fn home_with_db(extra_sql: &str) -> (TempHome, Connection) {
    let home = TempHome::new();
    let path = home.path().join(".local/share/opencode/opencode.db");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch(&fixture("schema.sql")).unwrap();
    conn.execute_batch(&fixture("session.sql")).unwrap();
    conn.execute_batch(extra_sql).unwrap();
    (home, conn)
}

fn read(home: &TempHome, filter: &SessionFilter) -> Vec<SessionEvents> {
    let env = home.env();
    OpenCodeAdapter
        .sessions(&env, filter)
        .map(|r| r.unwrap())
        .collect()
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

fn root() -> SessionEvents {
    let (home, _conn) = home_with_db("");
    read(&home, &SessionFilter::default())
        .into_iter()
        .find(|s| s.session.id == "ses_root")
        .unwrap()
}

#[test]
fn detects_the_database_under_the_data_home() {
    let home = TempHome::new();
    assert!(!OpenCodeAdapter.detect(&home.env()).found);
    let (home, _conn) = home_with_db("");
    let d = OpenCodeAdapter.detect(&home.env());
    assert!(d.found);
    assert_eq!(
        d.roots,
        [home.path().join(".local/share/opencode/opencode.db")]
    );
    let custom = home.env().with_var("XDG_DATA_HOME", "/nonexistent");
    assert!(!OpenCodeAdapter.detect(&custom).found);
}

#[test]
fn session_metadata_comes_from_the_session_row() {
    let s = root().session;
    assert_eq!(s.project_dir.as_deref(), Some(Path::new("/work/demo")));
    assert_eq!(s.model.as_deref(), Some("model-a"));
    assert_eq!(s.started_at.unwrap().as_millisecond(), 1780000000000);
}

#[test]
fn usage_is_measured_per_step_and_subagents_are_merged() {
    let se = root();
    let u = usages(&se);
    // Two steps of m2, the message-level fallback of m4, one subagent step.
    assert_eq!(u.len(), 4);
    assert!(u.iter().all(|u| u.measured));
    let main: Vec<_> = u.iter().filter(|u| !u.subagent).collect();
    assert_eq!(main.len(), 3);
    assert_eq!(
        (
            main[0].input,
            main[0].output,
            main[0].cache_read,
            main[0].cache_write
        ),
        (100, 20, 500, 300)
    );
    assert_eq!(main[0].cost_usd, Some(0.01));
    assert_eq!(main[0].model.as_deref(), Some("model-a"));
    assert_eq!(main[2].reasoning, 50);
    let sub: Vec<_> = u.iter().filter(|u| u.subagent).collect();
    assert_eq!(sub.len(), 1);
    assert_eq!(sub[0].model.as_deref(), Some("model-b"));
    assert_eq!(sub[0].cache_read, 180);
}

#[test]
fn events_are_ordered_by_time() {
    let se = root();
    let times: Vec<_> = se.events.iter().map(|e| e.at).collect();
    let mut sorted = times.clone();
    sorted.sort();
    assert_eq!(times, sorted);
    assert_eq!(
        se.events
            .iter()
            .filter(|e| matches!(e.kind, EventKind::UserTurn))
            .count(),
        2
    );
    assert_eq!(
        se.events
            .iter()
            .filter(|e| matches!(e.kind, EventKind::Compaction))
            .count(),
        1
    );
}

#[test]
fn skill_call_has_size_and_path() {
    let se = root();
    let skills: Vec<_> = se
        .events
        .iter()
        .filter_map(|e| match &e.kind {
            EventKind::SkillInvoked(s) => Some(s),
            _ => None,
        })
        .collect();
    assert_eq!(skills.len(), 1);
    assert_eq!(skills[0].name, "demo-skill");
    assert_eq!(skills[0].trigger, SkillTrigger::Model);
    assert_eq!(skills[0].content_bytes, Some(40));
    assert_eq!(
        skills[0].path.as_deref(),
        Some(Path::new("/home/u/.claude/skills/demo-skill/SKILL.md"))
    );
}

#[test]
fn failed_tool_call_is_marked_and_shell_arguments_are_dropped() {
    let se = root();
    let calls: Vec<_> = se
        .events
        .iter()
        .filter_map(|e| match &e.kind {
            EventKind::ToolCall(c) => Some(c),
            _ => None,
        })
        .collect();
    let read = calls.iter().find(|c| c.name == "read").unwrap();
    assert_eq!(read.status, ToolStatus::Error);
    assert_eq!(read.duration_ms, Some(100));
    let bash = calls.iter().find(|c| c.name == "bash").unwrap();
    assert_eq!(bash.status, ToolStatus::Ok);
    assert_eq!(bash.input_summary.as_deref(), Some("curl"));
}

#[test]
fn invalid_part_is_skipped_with_a_warning() {
    let se = root();
    assert_eq!(se.warnings.len(), 1);
    assert!(se.warnings[0].message.contains("invalid JSON"));
}

#[test]
fn since_filter_drops_old_sessions() {
    let (home, _conn) = home_with_db("");
    let all = read(&home, &SessionFilter::default());
    assert_eq!(all.len(), 2, "subagent session is merged into its parent");
    let f = SessionFilter {
        since: Some("2026-01-01T00:00:00Z".parse().unwrap()),
        project: None,
    };
    let recent = read(&home, &f);
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].session.id, "ses_root");
}

#[test]
fn database_can_be_read_while_a_writer_holds_the_wal() {
    let (home, conn) = home_with_db("");
    conn.pragma_update(None, "journal_mode", "WAL").unwrap();
    conn.execute_batch(
        "BEGIN; INSERT INTO session VALUES \
         ('ses_live','p1',NULL,'/work/live','Live',NULL,1780000200000,1780000200000);",
    )
    .unwrap();
    // The writer has an open transaction; a read-only reader still works.
    let sessions = read(&home, &SessionFilter::default());
    assert!(sessions.iter().any(|s| s.session.id == "ses_root"));
    assert!(!sessions.iter().any(|s| s.session.id == "ses_live"));
    conn.execute_batch("COMMIT;").unwrap();
    assert!(
        read(&home, &SessionFilter::default())
            .iter()
            .any(|s| s.session.id == "ses_live")
    );
}

#[test]
fn unknown_schema_is_reported_clearly() {
    let home = TempHome::new();
    let path = home.path().join(".local/share/opencode/opencode.db");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch("CREATE TABLE session (id text); CREATE TABLE message (id text); CREATE TABLE part (id text);")
        .unwrap();
    drop(conn);
    let env = home.env();
    let results: Vec<_> = OpenCodeAdapter
        .sessions(&env, &SessionFilter::default())
        .collect();
    assert_eq!(results.len(), 1);
    let err = results[0].as_ref().unwrap_err().to_string();
    assert!(err.contains("unsupported OpenCode schema"), "{err}");
}
