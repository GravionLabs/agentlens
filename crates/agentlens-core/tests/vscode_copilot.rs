mod support;

use std::path::Path;

use agentlens_core::adapter::{Adapter, SessionFilter};
use agentlens_core::adapters::vscode_copilot::VsCodeCopilotAdapter;
use agentlens_core::model::{
    ContextKind, EventKind, SessionEvents, SkillTrigger, ToolStatus, Usage,
};
use support::TempHome;

const USER: &str = ".config/Code/User";

fn home_with(fixture: &str, name: &str) -> TempHome {
    let home = TempHome::new();
    home.write(
        &format!("{USER}/workspaceStorage/hash1/workspace.json"),
        r#"{"folder":"file:///work/demo"}"#,
    );
    home.copy_fixture(
        &format!("vscode_copilot/{fixture}"),
        &format!("{USER}/workspaceStorage/hash1/chatSessions/{name}"),
    );
    home
}

fn read(home: &TempHome) -> Vec<SessionEvents> {
    let env = home.env();
    VsCodeCopilotAdapter
        .sessions(&env, &SessionFilter::default())
        .map(|r| r.unwrap())
        .collect()
}

fn first(fixture: &str) -> SessionEvents {
    let name = if fixture.ends_with("jsonl") {
        "a.jsonl"
    } else {
        "a.json"
    };
    read(&home_with(fixture, name)).remove(0)
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
fn detects_user_directories_of_both_editions() {
    let home = TempHome::new();
    assert!(!VsCodeCopilotAdapter.detect(&home.env()).found);
    home.write(".config/Code/User/workspaceStorage/x/.keep", "");
    home.write(
        ".config/Code - Insiders/User/globalStorage/emptyWindowChatSessions/.keep",
        "",
    );
    let d = VsCodeCopilotAdapter.detect(&home.env());
    assert!(d.found);
    assert_eq!(d.roots.len(), 2);
    // macOS and Windows locations.
    let mac = TempHome::new();
    mac.write(
        "Library/Application Support/Code/User/workspaceStorage/x/.keep",
        "",
    );
    assert!(VsCodeCopilotAdapter.detect(&mac.env()).found);
    let win = TempHome::new();
    win.write("AppData/Roaming/Code/User/workspaceStorage/x/.keep", "");
    let env = win.env().with_var(
        "APPDATA",
        win.path().join("AppData/Roaming").display().to_string(),
    );
    assert!(VsCodeCopilotAdapter.detect(&env).found);
}

#[test]
fn json_and_replayed_jsonl_give_the_same_session() {
    let a = first("session.json");
    let b = first("session.jsonl");
    assert_eq!(a.session, b.session);
    assert_eq!(a.events, b.events);
    assert!(a.warnings.is_empty() && b.warnings.is_empty());
}

#[test]
fn session_metadata_comes_from_the_log_and_the_workspace() {
    let se = first("session.jsonl");
    assert_eq!(se.session.id, "vs-1");
    assert_eq!(
        se.session.project_dir.as_deref(),
        Some(Path::new("/work/demo"))
    );
    assert_eq!(
        se.session.model.as_deref(),
        Some("claude-test"),
        "the copilot/ prefix is removed"
    );
    assert_eq!(
        se.session.started_at.unwrap().as_millisecond(),
        1_789_999_999_000
    );
}

#[test]
fn skill_reads_become_skill_invocations() {
    let se = first("session.json");
    let skills: Vec<_> = se
        .events
        .iter()
        .filter_map(|e| match &e.kind {
            EventKind::SkillInvoked(s) => Some(s),
            _ => None,
        })
        .collect();
    assert_eq!(skills.len(), 1);
    assert_eq!(skills[0].name, "demo skill");
    assert_eq!(skills[0].trigger, SkillTrigger::Model);
    assert_eq!(skills[0].content_bytes, None, "the size is not recorded");
    assert_eq!(
        skills[0].path.as_deref(),
        Some(Path::new("/work/demo/.claude/skills/demo skill/SKILL.md"))
    );
}

#[test]
fn tool_calls_keep_their_completion_state_and_cancelled_requests_stay_unknown() {
    let se = first("session.json");
    let calls: Vec<_> = se
        .events
        .iter()
        .filter_map(|e| match &e.kind {
            EventKind::ToolCall(c) => Some(c),
            _ => None,
        })
        .collect();
    let status: Vec<_> = calls.iter().map(|c| (c.name.as_str(), c.status)).collect();
    assert_eq!(
        status,
        [
            ("copilot_readFile", ToolStatus::Ok),
            ("run_in_terminal", ToolStatus::Ok),
            ("copilot_readFile", ToolStatus::Unknown),
            ("run_in_terminal", ToolStatus::Unknown),
        ]
    );
    assert_eq!(
        se.events
            .iter()
            .filter(|e| matches!(e.kind, EventKind::UserTurn))
            .count(),
        2
    );
}

#[test]
fn usage_is_measured_with_the_premium_multiplier() {
    let se = first("session.json");
    let u = usages(&se);
    assert_eq!(u.len(), 1, "the cancelled request has no usage");
    assert_eq!((u[0].input, u[0].output), (10_000, 200));
    assert!(u[0].measured);
    assert_eq!(u[0].model.as_deref(), Some("claude-test"));
    assert_eq!(u[0].premium_requests, Some(1.0));
    // Recorded when the request finished.
    assert_eq!(
        u[0].model.as_ref().map(|_| se
            .events
            .iter()
            .find_map(|e| match &e.kind {
                EventKind::Usage(_) => e.at,
                _ => None,
            })
            .unwrap()
            .as_millisecond()),
        Some(1_790_000_005_000)
    );
}

#[test]
fn prompt_breakdown_becomes_context_items_with_reported_tokens() {
    let se = first("session.json");
    let items: Vec<_> = se
        .events
        .iter()
        .filter_map(|e| match &e.kind {
            EventKind::ContextItem(c) => Some((c.kind, c.tokens)),
            _ => None,
        })
        .collect();
    assert_eq!(
        items,
        [
            (ContextKind::SystemPrompt, Some(2000)),
            (ContextKind::ToolDefinition, Some(5000)),
        ],
        "messages are conversation, not static context"
    );
}

#[test]
fn empty_window_sessions_have_no_project() {
    let home = TempHome::new();
    home.copy_fixture(
        "vscode_copilot/session.json",
        &format!("{USER}/globalStorage/emptyWindowChatSessions/e.json"),
    );
    let se = read(&home).remove(0);
    assert_eq!(se.session.project_dir, None);
}

#[test]
fn a_broken_log_is_a_warning_for_that_file_only() {
    let home = home_with("session.json", "good.json");
    home.write(
        &format!("{USER}/workspaceStorage/hash1/chatSessions/bad.jsonl"),
        "{\"kind\":1,\"k\":[\"x\"],\"v\":1}\n",
    );
    let all = read(&home);
    assert_eq!(all.len(), 2);
    let bad = all.iter().find(|s| !s.warnings.is_empty()).unwrap();
    assert!(bad.warnings[0].message.contains("cannot read chat session"));
    assert!(
        all.iter()
            .any(|s| s.session.id == "vs-1" && s.warnings.is_empty())
    );
}
