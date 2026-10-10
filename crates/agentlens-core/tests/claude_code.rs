mod support;

use std::path::Path;

use agentlens_core::adapter::{Adapter, SessionFilter};
use agentlens_core::adapters::claude_code::ClaudeCodeAdapter;
use agentlens_core::model::{EventKind, SessionEvents, SkillTrigger, ToolStatus, Usage};
use support::TempHome;

const SLUG: &str = ".claude/projects/-work-demo";

fn read_all(home: &TempHome) -> Vec<SessionEvents> {
    let env = home.env();
    let filter = SessionFilter::default();
    ClaudeCodeAdapter
        .sessions(&env, &filter)
        .map(|r| r.unwrap())
        .collect()
}

fn usages(se: &SessionEvents) -> Vec<&Usage> {
    se.events
        .iter()
        .filter_map(|e| {
            if let EventKind::Usage(u) = &e.kind {
                Some(u)
            } else {
                None
            }
        })
        .collect()
}

fn basic() -> SessionEvents {
    let home = TempHome::new();
    home.copy_fixture("claude_code/basic.jsonl", &format!("{SLUG}/basic.jsonl"));
    read_all(&home).remove(0)
}

#[test]
fn detects_projects_directory() {
    let home = TempHome::new();
    assert!(!ClaudeCodeAdapter.detect(&home.env()).found);
    home.write(".claude/projects/-work-demo/.keep", "");
    let d = ClaudeCodeAdapter.detect(&home.env());
    assert!(d.found);
    assert_eq!(d.roots, [home.path().join(".claude/projects")]);
}

#[test]
fn claude_config_dir_overrides_the_default_location() {
    let home = TempHome::new();
    home.copy_fixture(
        "claude_code/basic.jsonl",
        "custom/projects/-work-demo/basic.jsonl",
    );
    let env = home.env().with_var(
        "CLAUDE_CONFIG_DIR",
        home.path().join("custom").to_string_lossy(),
    );
    let n = ClaudeCodeAdapter
        .sessions(&env, &SessionFilter::default())
        .count();
    assert_eq!(n, 1);
}

#[test]
fn session_metadata_comes_from_the_transcript() {
    let se = basic();
    assert_eq!(se.session.id, "basic");
    assert_eq!(
        se.session.project_dir.as_deref(),
        Some(Path::new("/work/demo"))
    );
    assert_eq!(se.session.branch.as_deref(), Some("feat/demo"));
    assert_eq!(se.session.model.as_deref(), Some("claude-test-1"));
    assert_eq!(
        se.session.started_at,
        Some("2026-10-01T10:00:00Z".parse().unwrap())
    );
    assert_eq!(
        se.session.ended_at,
        Some("2026-10-01T10:02:10Z".parse().unwrap())
    );
}

#[test]
fn tool_calls_get_status_and_duration_and_safe_summaries() {
    let se = basic();
    let calls: Vec<_> = se
        .events
        .iter()
        .filter_map(|e| {
            if let EventKind::ToolCall(c) = &e.kind {
                Some(c)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(calls.len(), 2);
    assert_eq!(
        (
            calls[0].name.as_str(),
            calls[0].status,
            calls[0].duration_ms
        ),
        ("Read", ToolStatus::Ok, Some(500))
    );
    assert_eq!(calls[0].input_summary.as_deref(), Some("src/main.rs"));
    assert_eq!(
        (calls[1].name.as_str(), calls[1].status),
        ("Bash", ToolStatus::Error)
    );
    assert_eq!(calls[1].input_summary.as_deref(), Some("cargo"));
}

#[test]
fn a_response_split_over_three_lines_yields_one_usage() {
    let se = basic();
    let u = usages(&se);
    // m1 (three lines), the synthetic message is dropped, m3 follows.
    assert_eq!(u.len(), 2);
    assert_eq!(
        (u[0].input, u[0].output, u[0].cache_read, u[0].cache_write),
        (10, 20, 100, 200)
    );
    assert_eq!(u[0].reasoning, 3);
    assert!(u[0].measured && !u[0].subagent);
    assert_eq!(u[1].output, 2);
}

#[test]
fn turns_compactions_and_bad_lines() {
    let se = basic();
    let turns = se
        .events
        .iter()
        .filter(|e| matches!(e.kind, EventKind::UserTurn))
        .count();
    assert_eq!(
        turns, 1,
        "tool results, local command output and compact summaries are no turns"
    );
    assert_eq!(
        se.events
            .iter()
            .filter(|e| matches!(e.kind, EventKind::Compaction))
            .count(),
        1
    );
    assert_eq!(se.warnings.len(), 1);
    assert_eq!(se.warnings[0].location.as_deref(), Some("basic.jsonl:9"));
}

#[test]
fn skills_by_tool_call_slash_command_and_builtin() {
    let home = TempHome::new();
    home.copy_fixture("claude_code/skills.jsonl", &format!("{SLUG}/skills.jsonl"));
    let se = read_all(&home).remove(0);
    let skills: Vec<_> = se
        .events
        .iter()
        .filter_map(|e| {
            if let EventKind::SkillInvoked(s) = &e.kind {
                Some(s)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(skills.len(), 2, "/model is a built-in command, not a skill");

    assert_eq!(
        (skills[0].name.as_str(), skills[0].trigger),
        ("demo", SkillTrigger::Model)
    );
    assert_eq!(
        skills[0].path.as_deref(),
        Some(Path::new("/home/x/.claude/skills/demo"))
    );
    assert!(skills[0].content_bytes.unwrap() > 100);

    assert_eq!(
        (skills[1].name.as_str(), skills[1].trigger),
        ("orch", SkillTrigger::User)
    );
    assert_eq!(
        skills[1].path.as_deref(),
        Some(Path::new("/home/x/.claude/skills/orch"))
    );
    assert!(skills[1].content_bytes.is_some());

    // The model call is also a tool call; the user commands are turns.
    assert_eq!(
        se.events
            .iter()
            .filter(|e| matches!(e.kind, EventKind::UserTurn))
            .count(),
        3
    );
}

#[test]
fn subagent_transcripts_are_merged_and_flagged() {
    let home = TempHome::new();
    home.copy_fixture(
        "claude_code/main_with_subagent.jsonl",
        &format!("{SLUG}/main.jsonl"),
    );
    home.copy_fixture(
        "claude_code/subagent.jsonl",
        &format!("{SLUG}/main/subagents/agent-1.jsonl"),
    );
    let se = read_all(&home).remove(0);
    let u = usages(&se);
    assert_eq!(u.len(), 3);
    assert_eq!(u.iter().filter(|u| u.subagent).count(), 1);
    // Ordered by time: main, main, subagent... the subagent request happened between them.
    assert_eq!(
        u.iter().map(|u| u.subagent).collect::<Vec<_>>(),
        [false, true, false]
    );
    assert_eq!(
        se.events
            .iter()
            .filter(|e| matches!(e.kind, EventKind::UserTurn))
            .count(),
        1
    );
}

#[test]
fn since_filter_skips_files_not_touched_since() {
    let home = TempHome::new();
    home.copy_fixture("claude_code/basic.jsonl", &format!("{SLUG}/basic.jsonl"));
    let env = home.env();
    let future = SessionFilter {
        since: Some("2999-01-01T00:00:00Z".parse().unwrap()),
        project: None,
    };
    assert_eq!(ClaudeCodeAdapter.sessions(&env, &future).count(), 0);
}
