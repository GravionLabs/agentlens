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
    // skill command, and a Copilot CLI session with its store.
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
    json.replace(&home.path().display().to_string(), "/home/user")
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
    snap("context", text(&home, &api::context(&ctx, &all()).unwrap()));
    snap("unused", text(&home, &api::unused(&ctx, &all()).unwrap()));
    snap("flow", text(&home, &api::flow(&ctx, &all()).unwrap()));
    snap(
        "cost",
        text(&home, &api::cost(&ctx, &all(), "model", None).unwrap()),
    );
    snap(
        "cost_what_if",
        text(
            &home,
            &api::cost(&ctx, &all(), "project", Some("claude-haiku-4-5")).unwrap(),
        ),
    );
    snap(
        "efficiency",
        text(&home, &api::efficiency(&ctx, &all(), "source").unwrap()),
    );
    snap("compare", text(&home, &api::compare(&ctx, &all()).unwrap()));
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
