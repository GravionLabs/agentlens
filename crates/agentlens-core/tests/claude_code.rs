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
    // Only the run id is known; the type, description and phase are not.
    assert!(u.iter().filter(|u| !u.subagent).all(|u| u.agent.is_none()));
}

struct Run {
    id: String,
    parent: Option<String>,
    tool_use: Option<String>,
    agent_type: Option<String>,
    name: Option<String>,
    model: Option<String>,
    depth: Option<u32>,
    started: String,
    ended: Option<String>,
}

fn runs(se: &SessionEvents) -> Vec<Run> {
    let mut v: Vec<Run> = se
        .events
        .iter()
        .filter_map(|e| match &e.kind {
            EventKind::AgentRun {
                agent_id,
                parent_agent_id,
                tool_use_id,
                agent_type,
                name,
                model,
                depth,
                started_at,
                ended_at,
                ..
            } => Some(Run {
                id: agent_id.clone(),
                parent: parent_agent_id.clone(),
                tool_use: tool_use_id.clone(),
                agent_type: agent_type.clone(),
                name: name.clone(),
                model: model.clone(),
                depth: *depth,
                started: started_at.to_string(),
                ended: ended_at.map(|t| t.to_string()),
            }),
            _ => None,
        })
        .collect();
    v.sort_by(|a, b| a.id.cmp(&b.id));
    v
}

fn session_with(subs: &[(&str, Option<&str>)]) -> TempHome {
    let home = TempHome::new();
    add_session(&home, "main", subs);
    home
}

/// Adds a session `name` with the given subagents (path stem, optional meta fixture).
fn add_session(home: &TempHome, name: &str, subs: &[(&str, Option<&str>)]) {
    home.copy_fixture(
        "claude_code/main_with_subagent.jsonl",
        &format!("{SLUG}/{name}.jsonl"),
    );
    for (path, meta) in subs {
        home.copy_fixture(
            "claude_code/subagent.jsonl",
            &format!("{SLUG}/{name}/subagents/{path}.jsonl"),
        );
        if let Some(meta) = meta {
            home.copy_fixture(meta, &format!("{SLUG}/{name}/subagents/{path}.meta.json"));
        }
    }
}

#[test]
fn depth_one_run_carries_its_caller_and_times() {
    let home = session_with(&[("agent-a1", Some("claude_code/agent_depth1.meta.json"))]);
    let se = read_all(&home).remove(0);
    let r = runs(&se);
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].id, "a1");
    assert_eq!(r[0].parent, None);
    assert_eq!(r[0].depth, Some(1));
    assert_eq!(r[0].tool_use.as_deref(), Some("toolu_01"));
    assert_eq!(r[0].agent_type.as_deref(), Some("general-purpose"));
    assert_eq!(r[0].model.as_deref(), Some("claude-sonnet-4-5"));
    assert_eq!(r[0].started, "2026-10-01T12:00:05Z");
    assert_eq!(r[0].ended.as_deref(), Some("2026-10-01T12:00:07Z"));
    // Requests of the run are charged to it; main-session requests are not.
    let u = usages(&se);
    let sub: Vec<_> = u.iter().filter(|u| u.subagent).collect();
    assert_eq!(sub.len(), 1);
    assert_eq!(
        sub[0].agent.as_ref().and_then(|a| a.agent_id.as_deref()),
        Some("a1")
    );
    assert!(u.iter().filter(|u| !u.subagent).all(|u| u.agent.is_none()));
    assert!(se.warnings.is_empty());
}

#[test]
fn depth_two_run_names_its_parent() {
    let home = session_with(&[("agent-a2", Some("claude_code/agent_depth2.meta.json"))]);
    let r = runs(&read_all(&home).remove(0));
    assert_eq!(r[0].parent.as_deref(), Some("parent-1"));
    assert_eq!(r[0].depth, Some(2));
    assert_eq!(r[0].tool_use.as_deref(), Some("toolu_02"));
}

#[test]
fn forked_skill_run_keeps_its_name() {
    let home = session_with(&[("agent-a3", Some("claude_code/agent_forked_skill.meta.json"))]);
    let r = runs(&read_all(&home).remove(0));
    assert_eq!(r[0].name.as_deref(), Some("review"));
    assert_eq!(r[0].depth, Some(1));
    assert_eq!(r[0].parent, None);
}

#[test]
fn old_meta_directly_in_subagents_is_depth_one() {
    let home = session_with(&[("agent-a4", Some("claude_code/agent_old.meta.json"))]);
    let r = runs(&read_all(&home).remove(0));
    assert_eq!(r[0].depth, Some(1));
    assert_eq!(r[0].parent, None);
    assert_eq!(r[0].agent_type.as_deref(), Some("Explore"));
}

#[test]
fn workflow_run_is_a_node_above_its_subagents() {
    let home = TempHome::new();
    home.copy_fixture(
        "claude_code/main_with_subagent.jsonl",
        &format!("{SLUG}/main.jsonl"),
    );
    for id in ["w1", "w2"] {
        home.copy_fixture(
            "claude_code/subagent.jsonl",
            &format!("{SLUG}/main/subagents/workflows/run-1/agent-{id}.jsonl"),
        );
        home.copy_fixture(
            "claude_code/workflow_subagent.meta.json",
            &format!("{SLUG}/main/subagents/workflows/run-1/agent-{id}.meta.json"),
        );
    }
    let r = runs(&read_all(&home).remove(0));
    assert_eq!(
        r.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
        ["run-1", "w1", "w2"]
    );
    assert_eq!(r[0].agent_type.as_deref(), Some("workflow"));
    assert_eq!((r[0].depth, r[0].parent.clone()), (Some(1), None));
    assert_eq!(r[0].started, "2026-10-01T12:00:05Z");
    assert_eq!(r[0].ended.as_deref(), Some("2026-10-01T12:00:07Z"));
    for sub in &r[1..] {
        assert_eq!(sub.depth, Some(2));
        assert_eq!(sub.parent.as_deref(), Some("run-1"));
        assert_eq!(sub.agent_type.as_deref(), Some("workflow-subagent"));
    }
}

#[test]
fn missing_meta_leaves_the_caller_unknown_and_summarizes_once() {
    let home = session_with(&[("agent-a5", None)]);
    let se = read_all(&home).remove(0);
    let r = runs(&se);
    assert_eq!((r[0].depth, r[0].parent.clone()), (None, None));
    assert_eq!(se.warnings.len(), 1);
    assert_eq!(
        se.warnings[0].message,
        "1 subagent transcripts have no meta file (older Claude Code); their runs have no caller or depth"
    );
    assert_eq!(usages(&se).iter().filter(|u| u.subagent).count(), 1);
}

#[test]
fn three_missing_metas_give_one_summary_warning_naming_three() {
    let home = session_with(&[("agent-b1", None), ("agent-b2", None), ("agent-b3", None)]);
    let se = read_all(&home).remove(0);
    assert_eq!(se.warnings.len(), 1);
    assert!(
        se.warnings[0]
            .message
            .starts_with("3 subagent transcripts have no meta file")
    );
}

#[test]
fn missing_metas_across_sessions_give_one_summary_for_the_whole_scan() {
    let home = TempHome::new();
    add_session(&home, "first", &[("agent-d1", None), ("agent-d2", None)]);
    add_session(
        &home,
        "second",
        &[
            ("agent-d3", None),
            ("agent-d4", Some("claude_code/agent_depth1.meta.json")),
        ],
    );
    add_session(&home, "third", &[("agent-d5", None)]);
    let sessions = read_all(&home);
    assert_eq!(sessions.len(), 3);
    let warnings: Vec<&str> = sessions
        .iter()
        .flat_map(|se| se.warnings.iter().map(|w| w.message.as_str()))
        .collect();
    assert_eq!(
        warnings,
        [
            "4 subagent transcripts have no meta file (older Claude Code); their runs have no caller or depth"
        ]
    );
}

#[test]
fn complete_metas_give_no_meta_warning() {
    let home = session_with(&[("agent-c1", Some("claude_code/agent_depth1.meta.json"))]);
    assert!(read_all(&home).remove(0).warnings.is_empty());
}

#[test]
fn unreadable_meta_warns_per_file_and_the_scan_goes_on() {
    let home = session_with(&[("agent-a6", Some("claude_code/basic.jsonl"))]);
    let se = read_all(&home).remove(0);
    let r = runs(&se);
    assert_eq!((r[0].depth, r[0].parent.clone()), (None, None));
    assert_eq!(se.warnings.len(), 1);
    assert_eq!(
        se.warnings[0].location.as_deref(),
        Some("agent-a6.meta.json")
    );
    assert_eq!(usages(&se).len(), 3);
}

#[test]
fn efficiency_roles_cover_skill_phase_type_and_main() {
    use agentlens_core::analyzers::efficiency::{CacheBy, RoleKind, analyze};
    use agentlens_core::prices::PriceTable;

    let home = session_with(&[
        ("agent-a1", Some("claude_code/agent_depth1.meta.json")),
        ("agent-a3", Some("claude_code/agent_forked_skill.meta.json")),
        (
            "workflows/run-1/agent-w1",
            Some("claude_code/workflow_subagent.meta.json"),
        ),
    ]);
    let report = analyze(&read_all(&home), &PriceTable::bundled(), CacheBy::Source);
    let roles: Vec<(&str, RoleKind, u64)> = report
        .roles
        .iter()
        .map(|r| (r.role.as_str(), r.kind, r.runs))
        .collect();
    for want in [
        ("main", RoleKind::Main, 1),
        ("review", RoleKind::Skill, 1),
        ("Implement", RoleKind::WorkflowPhase, 1),
        ("general-purpose", RoleKind::AgentType, 1),
    ] {
        assert!(roles.contains(&want), "{want:?} missing in {roles:?}");
    }
    // The workflow container made no request but is a run of its own.
    assert_eq!(roles.len(), 5, "{roles:?}");
    let container = report.roles.iter().find(|r| r.role == "workflow").unwrap();
    assert_eq!((container.runs, container.requests), (1, 0));
    assert_eq!(container.usd, None);
    let skill = report.roles.iter().find(|r| r.role == "review").unwrap();
    // The subagent fixture request: 10 input + 100 cache read + 200 cache write.
    assert_eq!(skill.requests, 1);
    assert_eq!(skill.first_request_context.count, 310);
    assert_eq!(skill.uncached_after_first.count, 0);
    assert!(skill.first_request_context.measured);
}

#[test]
fn a_clear_opens_the_new_transcript_and_is_recorded_at_its_start() {
    let home = TempHome::new();
    home.copy_fixture(
        "claude_code/clear_at_start.jsonl",
        &format!("{SLUG}/clear_at_start.jsonl"),
    );
    let se = read_all(&home).remove(0);
    let clears: Vec<usize> = se
        .events
        .iter()
        .enumerate()
        .filter(|(_, e)| matches!(e.kind, EventKind::Clear))
        .map(|(i, _)| i)
        .collect();
    assert_eq!(clears.len(), 1, "one /clear, one event");
    let first_request = se
        .events
        .iter()
        .position(|e| matches!(e.kind, EventKind::Usage(_)))
        .unwrap();
    assert!(
        clears[0] < first_request,
        "the clear comes before the first request of the session"
    );
    assert!(
        se.events[..clears[0]]
            .iter()
            .all(|e| matches!(e.kind, EventKind::UserTurn)),
        "only the command's own turn precedes it: {:?}",
        &se.events[..clears[0]]
    );
    assert_eq!(
        se.events[clears[0]].at,
        Some("2026-10-03T10:00:00Z".parse().unwrap())
    );
    // `/clear` stays a built-in command: no skill.
    assert!(
        se.events
            .iter()
            .all(|e| !matches!(e.kind, EventKind::SkillInvoked(_)))
    );
}

#[test]
fn other_builtin_commands_record_no_clear() {
    let home = TempHome::new();
    home.copy_fixture("claude_code/skills.jsonl", &format!("{SLUG}/skills.jsonl"));
    let se = read_all(&home).remove(0);
    assert!(
        se.events
            .iter()
            .all(|e| !matches!(e.kind, EventKind::Clear)),
        "/model is no clear"
    );
}

#[test]
fn a_recorded_clear_is_a_measured_fresh_start() {
    use agentlens_core::analyzers::advice::hygiene_saving;
    use agentlens_core::prices::PriceTable;

    let home = TempHome::new();
    home.copy_fixture(
        "claude_code/clear_at_start.jsonl",
        &format!("{SLUG}/clear_at_start.jsonl"),
    );
    // Same client, project and branch, but no /clear: Claude Code records it, so no proxy.
    home.copy_fixture(
        "claude_code/two_compactions.jsonl",
        &format!("{SLUG}/two_compactions.jsonl"),
    );
    let sessions = read_all(&home);
    let h = hygiene_saving(&sessions, &PriceTable::default());
    assert_eq!((h.fresh_measured, h.fresh_estimated), (1, 0));
    let clears: Vec<(&str, u64, bool)> = h
        .sessions
        .iter()
        .map(|s| (s.session.as_str(), s.clears, s.clears_measured))
        .collect();
    assert_eq!(
        clears,
        [("clear_at_start", 1, true), ("two_compactions", 0, true)],
        "both have two compactions"
    );
}

#[test]
fn delegation_is_missing_where_subagents_ran_and_present_where_none_did() {
    use agentlens_core::analyzers::advice::delegation_use;

    let home = TempHome::new();
    home.copy_fixture(
        "claude_code/main_with_subagent.jsonl",
        &format!("{SLUG}/main_with_subagent.jsonl"),
    );
    let d = delegation_use(&read_all(&home));
    assert!(d.sources.iter().all(|s| s.large_requests == 0));
    assert!(d.sessions.is_empty());

    let home = TempHome::new();
    home.copy_fixture(
        "claude_code/large_context_no_subagents.jsonl",
        &format!("{SLUG}/large_context_no_subagents.jsonl"),
    );
    let d = delegation_use(&read_all(&home));
    assert_eq!((d.requests, d.large_requests), (20, 20));
    let [s] = d.sources.as_slice() else {
        panic!("one source")
    };
    assert_eq!(
        (
            s.share.requests,
            s.share.subagent_requests,
            s.large_requests
        ),
        (20, 0, 20)
    );
    assert_eq!(d.sessions[0].id, "large_context_no_subagents");
}

#[test]
fn skill_loads_are_flagged_when_they_happen_in_a_subagent_transcript() {
    let home = TempHome::new();
    home.copy_fixture("claude_code/skills.jsonl", &format!("{SLUG}/main.jsonl"));
    home.copy_fixture(
        "claude_code/subagent_skill.jsonl",
        &format!("{SLUG}/main/subagents/agent-1.jsonl"),
    );
    let se = read_all(&home).remove(0);
    let loads: Vec<_> = se
        .events
        .iter()
        .filter_map(|e| match &e.kind {
            EventKind::SkillInvoked(s) => Some((s.name.as_str(), s.trigger, s.subagent)),
            _ => None,
        })
        .collect();
    assert!(loads.contains(&("demo", SkillTrigger::Model, true)));
    assert!(loads.contains(&("demo", SkillTrigger::Model, false)));
    assert!(loads.contains(&("orch", SkillTrigger::User, false)));
}
