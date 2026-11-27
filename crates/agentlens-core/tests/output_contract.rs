//! The shape of what AgentLens prints is a contract: the CLI's `--format json`, the results of the MCP
//! tools and the commands of the application all serialize these reports. A field that is renamed,
//! removed or retyped changes all three, so every analysis has a snapshot here.
//!
//! **When one of these snapshots changes, that is a decision, not a chore.** Review the diff and say in
//! the pull request whether it is compatible (a field was added) or breaking (a field was renamed,
//! removed or changed its type). A breaking change raises `report::SCHEMA_VERSION`, is named in the
//! changelog and updates `src/app/core/types.ts` and the samples of the application
//! (`UPDATE_SAMPLES=1 cargo test -p agentlens-app`). Accept a new snapshot with
//! `INSTA_UPDATE=always cargo test -p agentlens-core --test output_contract`.
//!
//! The snapshots hold unix paths, so the test runs on unix only.
#![cfg(unix)]

mod support;

use agentlens_core::adapters::default_registry;
use agentlens_core::api::{self, Context, Filters};
use agentlens_core::report::SCHEMA_VERSION;
use serde::Serialize;
use support::TempHome;

/// A home with the sessions and skills that make every analysis produce something.
fn setup() -> (TempHome, Context) {
    let home = TempHome::new();
    // A Claude Code session with context items, one in which skill folders are used, one with a typed
    // skill command, one with a retry loop, three that carried their context on (one opened by a /clear,
    // one compacted twice, one near the limit), one with a pause longer than the cache lifetime, and a Copilot CLI session with its store.
    home.copy_fixture(
        "claude_code/context.jsonl",
        ".claude/projects/-work-demo/context.jsonl",
    );
    home.copy_fixture(
        "claude_code/skill_files.jsonl",
        ".claude/projects/-work-demo/skill_files.jsonl",
    );
    home.copy_fixture(
        "claude_code/typed_skill.jsonl",
        ".claude/projects/-work-demo/typed_skill.jsonl",
    );
    home.copy_fixture(
        "claude_code/retry_loop.jsonl",
        ".claude/projects/-work-demo/retry_loop.jsonl",
    );
    // The typed-command session runs `/cliff-setup` a second time in the same context (a SkillQuality
    // signal: a repeat load). Appended here so that no other fixture or test changes.
    {
        use std::io::Write;
        let line = |ts: &str, content: &str| {
            serde_json::json!({"sessionId":"tc1","cwd":"/work/demo","gitBranch":"main","timestamp":ts,
                "type":"user","message":{"role":"user","content":content}})
            .to_string()
        };
        let meta = serde_json::json!({"sessionId":"tc1","cwd":"/work/demo","gitBranch":"main",
            "timestamp":"2026-10-02T11:03:00Z","type":"user","isMeta":true,
            "message":{"role":"user","content":[{"type":"text","text":
            "Base directory for this skill: /home/u/.claude/skills/cliff-setup\n\nbody body body"}]}})
        .to_string();
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(
                home.path()
                    .join(".claude/projects/-work-demo/typed_skill.jsonl"),
            )
            .unwrap();
        writeln!(
            file,
            "{}\n{meta}",
            line(
                "2026-10-02T11:03:00Z",
                "<command-message>cliff-setup</command-message>\n<command-name>/cliff-setup</command-name>\n<command-args>0.9</command-args>"
            )
        )
        .unwrap();
    }
    for fixture in [
        "clear_at_start",
        "two_compactions",
        "near_limit",
        "cache_expiry",
    ] {
        home.copy_fixture(
            &format!("claude_code/{fixture}.jsonl"),
            &format!(".claude/projects/-work-demo/{fixture}.jsonl"),
        );
    }
    home.copy_fixture(
        "copilot_cli/with_store.jsonl",
        ".copilot/session-state/cp-1/events.jsonl",
    );
    for name in ["demo", "idle", "cliff-setup"] {
        home.write(
            &format!(".claude/skills/{name}/SKILL.md"),
            format!(
                "---\nname: {name}\ndescription: Does {name} things\n---\n{}",
                "body ".repeat(60)
            ),
        );
    }
    home.write(
        ".claude/agents/reviewer.md",
        "---\nname: reviewer\ndescription: Reviews code\n---\nbody",
    );
    let ctx = Context {
        env: home.env(),
        registry: default_registry(),
    };
    (home, ctx)
}

/// Fixed period, so that nothing depends on today's date.
fn all() -> Filters {
    Filters {
        since: Some("2026-01-01".into()),
        ..Filters::default()
    }
}

fn claude() -> Filters {
    Filters {
        sources: vec!["claude-code".into()],
        ..all()
    }
}

/// Pretty JSON with the temporary home replaced, as the CLI would print it. Every snapshot carries the
/// schema version of its envelope, so raising `SCHEMA_VERSION` has to be accepted in all of them.
fn text<T: Serialize>(home: &TempHome, value: &T) -> String {
    let json = serde_json::to_string_pretty(value).unwrap();
    assert!(
        json.contains(&format!("\"schema_version\": {SCHEMA_VERSION},")),
        "the answer says which version of the layout it has"
    );
    // Paths may come back canonicalized (macOS resolves the temp dir through `/private`), so replace both spellings.
    let canonical = home
        .path()
        .canonicalize()
        .unwrap_or_else(|_| home.path().to_path_buf());
    json.replace(&canonical.display().to_string(), "/home/user")
        .replace(&home.path().display().to_string(), "/home/user")
}

#[test]
fn the_json_of_every_analysis_keeps_its_shape() {
    let (home, ctx) = setup();
    let snap = |name: &str, body: String| {
        insta::with_settings!({
            description => "A change here changes the CLI JSON, the MCP results and the application: review it as a decision about compatibility (see the top of tests/output_contract.rs).",
            omit_expression => true,
        }, {
            insta::assert_snapshot!(name, body);
        });
    };
    snap(
        "sources",
        text(&home, &api::overview(&ctx, &all()).unwrap()),
    );
    snap("skills", text(&home, &api::skills(&ctx, &all()).unwrap()));
    snap("advice", text(&home, &api::advice(&ctx, &all()).unwrap()));
    snap("tips", text(&home, &api::tips(&ctx, &all(), 5).unwrap()));
    snap(
        "tips_check",
        text(
            &home,
            // Fixed windows (2026-10-01..03 and 2026-10-03..05), so that nothing depends on today's date.
            &api::tips_check(
                &ctx,
                &Filters {
                    until: Some("2026-10-05".into()),
                    ..Filters::default()
                },
                "2026-10-03",
                5,
            )
            .unwrap(),
        ),
    );
    snap("context", text(&home, &api::context(&ctx, &all()).unwrap()));
    snap("unused", text(&home, &api::unused(&ctx, &all()).unwrap()));
    snap("flow", text(&home, &api::flow(&ctx, &all()).unwrap()));
    snap(
        "cost",
        text(&home, &api::cost(&ctx, &all(), "model", None, &[]).unwrap()),
    );
    snap(
        "cost_by_session",
        text(
            &home,
            &api::cost(&ctx, &all(), "session", None, &[]).unwrap(),
        ),
    );
    snap(
        "cost_what_if",
        text(
            &home,
            &api::cost(&ctx, &all(), "project", Some("claude-haiku-4-5"), &[]).unwrap(),
        ),
    );
    snap(
        "cost_without",
        text(
            &home,
            &api::cost(
                &ctx,
                &all(),
                "model",
                Some("claude-haiku-4-5"),
                &[
                    "skill:demo".to_owned(),
                    "agent:reviewer".to_owned(),
                    "mcp-server:claude.ai Claude Docs".to_owned(),
                    "skill:no-such-skill".to_owned(),
                ],
            )
            .unwrap(),
        ),
    );
    snap(
        "efficiency",
        text(&home, &api::efficiency(&ctx, &all(), "source").unwrap()),
    );
    snap("compare", text(&home, &api::compare(&ctx, &all()).unwrap()));
    snap(
        "trend",
        text(
            &home,
            // Fixed windows of different length (14 and 30 days), so that the snapshot shows
            // the per-day values and nothing depends on today's date.
            &api::trend(
                &ctx,
                &Filters {
                    since: Some("2026-10-01".into()),
                    until: Some("2026-10-15".into()),
                    ..Filters::default()
                },
                Some((
                    "2026-07-01T00:00:00Z".parse().unwrap(),
                    "2026-07-31T00:00:00Z".parse().unwrap(),
                )),
            )
            .unwrap(),
        ),
    );
    snap(
        "sessions",
        text(&home, &api::sessions(&ctx, &all()).unwrap()),
    );
    snap(
        "timeline",
        text(
            &home,
            &api::timeline(&ctx, &claude(), "claude-code", "context").unwrap(),
        ),
    );
}

/// The usage events of a workflow subagent carry `agent`. This is a compatible change (a field was
/// added): the field is left out when the client says nothing about the agent, as for the main session.
#[test]
fn a_workflow_subagent_shows_its_agent_in_the_usage_events() {
    use agentlens_core::adapter::{Adapter, SessionFilter};
    use agentlens_core::adapters::claude_code::ClaudeCodeAdapter;
    use agentlens_core::model::EventKind;

    let home = TempHome::new();
    let slug = ".claude/projects/-work-demo";
    home.copy_fixture(
        "claude_code/main_with_subagent.jsonl",
        &format!("{slug}/main.jsonl"),
    );
    home.copy_fixture(
        "claude_code/subagent.jsonl",
        &format!("{slug}/main/subagents/workflows/run-1/agent-w1.jsonl"),
    );
    home.copy_fixture(
        "claude_code/workflow_subagent.meta.json",
        &format!("{slug}/main/subagents/workflows/run-1/agent-w1.meta.json"),
    );
    let env = home.env();
    let se = ClaudeCodeAdapter
        .sessions(&env, &SessionFilter::default())
        .map(|r| r.unwrap())
        .next()
        .unwrap();
    let usages: Vec<_> = se
        .events
        .iter()
        .filter_map(|e| match &e.kind {
            EventKind::Usage(u) => Some(u),
            _ => None,
        })
        .map(|u| serde_json::json!({ "subagent": u.subagent, "agent": u.agent }))
        .collect();
    let body = serde_json::to_string_pretty(&usages).unwrap();
    insta::with_settings!({
        description => "The agent field of a workflow subagent's usage events (compatible: a field was added).",
        omit_expression => true,
    }, {
        insta::assert_snapshot!("usage_workflow_subagent", body);
    });
}
