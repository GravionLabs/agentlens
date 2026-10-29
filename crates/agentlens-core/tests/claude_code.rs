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
fn session_filter_reads_only_the_named_session_with_its_subagents() {
    let home = TempHome::new();
    home.copy_fixture("claude_code/basic.jsonl", &format!("{SLUG}/basic.jsonl"));
    home.copy_fixture(
        "claude_code/main_with_subagent.jsonl",
        &format!("{SLUG}/main.jsonl"),
    );
    home.copy_fixture(
        "claude_code/subagent.jsonl",
        &format!("{SLUG}/main/subagents/agent-1.jsonl"),
    );
    let env = home.env();
    let read = |ids: &[&str]| -> Vec<SessionEvents> {
        let filter = SessionFilter {
            session: ids.iter().map(|s| (*s).to_owned()).collect(),
            ..SessionFilter::default()
        };
        ClaudeCodeAdapter
            .sessions(&env, &filter)
            .map(|r| r.unwrap())
            .collect()
    };
    let one = read(&["main"]);
    assert_eq!(one.len(), 1);
    assert_eq!(one[0].session.id, "main");
    assert_eq!(usages(&one[0]).iter().filter(|u| u.subagent).count(), 1);
    assert_eq!(read(&["main", "basic"]).len(), 2);
    assert!(read(&["unknown"]).is_empty());
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
        ..SessionFilter::default()
    };
    assert_eq!(ClaudeCodeAdapter.sessions(&env, &future).count(), 0);
}

fn context_items(se: &SessionEvents) -> Vec<&agentlens_core::model::ContextItem> {
    se.events
        .iter()
        .filter_map(|e| match &e.kind {
            EventKind::ContextItem(c) => Some(c),
            _ => None,
        })
        .collect()
}

#[test]
fn attachments_become_context_items_with_sizes_only() {
    use agentlens_core::model::ContextKind::*;
    let home = TempHome::new();
    home.copy_fixture(
        "claude_code/context.jsonl",
        &format!("{SLUG}/context.jsonl"),
    );
    let se = read_all(&home).remove(0);
    let items: Vec<(_, Option<&str>, u64)> = context_items(&se)
        .into_iter()
        .map(|c| (c.kind, c.name.as_deref(), c.bytes))
        .collect();
    assert_eq!(
        items,
        [
            (InstructionFile, Some("/work/demo/CLAUDE.md"), 400),
            (SkillListing, None, 800),
            (ToolListing, Some("mcp__srv__alpha"), 15),
            (ToolListing, Some("mcp__srv__beta"), 14),
            (ToolListing, Some("WebFetch"), 8),
            (ToolDefinition, Some("mcp__srv__alpha"), 150),
            (McpInstructions, Some("srv"), 300),
            (AgentListing, Some("explore"), 100),
        ]
    );
    // One compaction and three requests are still found.
    assert_eq!(
        se.events
            .iter()
            .filter(|e| matches!(e.kind, EventKind::Compaction))
            .count(),
        1
    );
    assert_eq!(usages(&se).len(), 3);
}

#[test]
fn the_same_session_under_two_project_directories_counts_once() {
    let home = TempHome::new();
    let full = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/claude_code/basic.jsonl"),
    )
    .unwrap();
    // The older copy is shorter; the longer one wins wherever it is.
    let short: String = full.lines().take(3).collect::<Vec<_>>().join("\n") + "\n";
    home.write(".claude/projects/-work-a/same.jsonl", short);
    home.write(".claude/projects/-work-b/same.jsonl", full.clone());
    home.write(".claude/projects/-work-b/other.jsonl", full);
    let all = read_all(&home);
    let ids: Vec<_> = all.iter().map(|s| s.session.id.as_str()).collect();
    assert_eq!(ids.iter().filter(|i| **i == "same").count(), 1, "{ids:?}");
    assert_eq!(all.len(), 2);
    let same = all.iter().find(|s| s.session.id == "same").unwrap();
    let other = all.iter().find(|s| s.session.id == "other").unwrap();
    assert_eq!(
        same.events.len(),
        other.events.len(),
        "the complete copy was kept"
    );
}

#[test]
fn tool_calls_in_a_skill_folder_become_file_uses_that_keep_only_the_skill_name() {
    let home = TempHome::new();
    home.copy_fixture(
        "claude_code/skill_files.jsonl",
        &format!("{SLUG}/skill_files.jsonl"),
    );
    let se = read_all(&home).remove(0);
    let names: Vec<&str> = se
        .events
        .iter()
        .filter_map(|e| match &e.kind {
            EventKind::SkillFileUse(n) => Some(n.as_str()),
            _ => None,
        })
        .collect();
    // Read and a run script in the folder of `demo`, and the project folder `features/skills/skills.ts`,
    // which yields a name the analysis will not know. Listing and printing the files of `idle` is not
    // a use, and the Edit of `other` is working on it.
    assert_eq!(names, ["demo", "demo", "skills.ts"]);
    let edited: Vec<&str> = se
        .events
        .iter()
        .filter_map(|e| match &e.kind {
            EventKind::SkillFileEdit(n) => Some(n.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(edited, ["other"]);
    // Nothing of the calls is kept: no argument, no path.
    let dump = format!("{:?}", se.events);
    assert!(
        !dump.contains("secret-argument"),
        "arguments are never stored"
    );
    assert!(!dump.contains("run.sh"));
}

#[test]
fn a_typed_skill_command_is_a_user_invocation_with_the_content_the_client_injected() {
    let home = TempHome::new();
    home.copy_fixture(
        "claude_code/typed_skill.jsonl",
        &format!("{SLUG}/typed_skill.jsonl"),
    );
    let se = read_all(&home).remove(0);
    let skills: Vec<_> = se
        .events
        .iter()
        .filter_map(|e| match &e.kind {
            EventKind::SkillInvoked(s) => Some(s),
            _ => None,
        })
        .collect();
    // `/cliff-setup` is a skill the user typed: the message starts with the message tag, and the
    // skill text follows as a meta message, without a call of the skill tool. `/statusline` and
    // `/clear` are built in and load nothing.
    assert_eq!(skills.len(), 1, "{skills:?}");
    assert_eq!(skills[0].name, "cliff-setup");
    assert_eq!(skills[0].trigger, SkillTrigger::User);
    assert!(
        skills[0].content_bytes.unwrap() > 200,
        "the injected text is its load"
    );
    assert_eq!(
        skills[0].path.as_deref(),
        Some(Path::new("/home/u/.claude/skills/cliff-setup"))
    );
    // Each typed command is a turn of the user.
    let turns = se
        .events
        .iter()
        .filter(|e| matches!(e.kind, EventKind::UserTurn))
        .count();
    assert_eq!(turns, 3);
}

#[test]
fn workflow_subagents_are_counted_with_their_meta() {
    let home = TempHome::new();
    home.copy_fixture(
        "claude_code/main_with_subagent.jsonl",
        &format!("{SLUG}/main.jsonl"),
    );
    home.copy_fixture(
        "claude_code/subagent.jsonl",
        &format!("{SLUG}/main/subagents/workflows/run-1/agent-w1.jsonl"),
    );
    home.copy_fixture(
        "claude_code/workflow_subagent.meta.json",
        &format!("{SLUG}/main/subagents/workflows/run-1/agent-w1.meta.json"),
    );
    let se = read_all(&home).remove(0);
    let u = usages(&se);
    assert_eq!(u.len(), 3);
    let sub: Vec<_> = u.iter().filter(|u| u.subagent).collect();
    assert_eq!(sub.len(), 1);
    let agent = sub[0].agent.as_ref().unwrap();
    assert_eq!(agent.agent_type.as_deref(), Some("workflow-subagent"));
    assert_eq!(agent.description.as_deref(), Some("implement #12"));
    assert_eq!(agent.workflow_phase.as_deref(), Some("Implement"));
    assert!(u.iter().filter(|u| !u.subagent).all(|u| u.agent.is_none()));
}

#[test]
fn malformed_meta_is_ignored() {
    let home = TempHome::new();
    home.copy_fixture(
        "claude_code/main_with_subagent.jsonl",
        &format!("{SLUG}/main.jsonl"),
    );
    home.copy_fixture(
        "claude_code/subagent.jsonl",
        &format!("{SLUG}/main/subagents/agent-1.jsonl"),
    );
    home.copy_fixture(
        "claude_code/basic.jsonl",
        &format!("{SLUG}/main/subagents/agent-1.meta.json"),
    );
    let se = read_all(&home).remove(0);
    let u = usages(&se);
    assert_eq!(u.iter().filter(|u| u.subagent).count(), 1);
    assert!(u.iter().all(|u| u.agent.is_none()));
}
