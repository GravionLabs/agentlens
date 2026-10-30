//! The `roles` filter of `api::Filters`: sessions that started a run of a role, with all their requests.

mod support;

use agentlens_core::adapters::default_registry;
use agentlens_core::api::{self, Context, Filters};
use support::TempHome;

const SLUG: &str = ".claude/projects/-work-demo";

/// One session with a workflow run and its subagent (roles `workflow` and `Implement`) and one
/// session without any run.
fn setup() -> (TempHome, Context) {
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
    home.copy_fixture("claude_code/basic.jsonl", &format!("{SLUG}/basic.jsonl"));
    let ctx = Context {
        env: home.env(),
        registry: default_registry(),
    };
    (home, ctx)
}

fn roles(r: &[&str]) -> Filters {
    Filters {
        roles: r.iter().map(|s| (*s).into()).collect(),
        ..Filters::default()
    }
}

fn cost_requests(ctx: &Context, f: &Filters) -> u64 {
    let a = api::cost(ctx, f, "session", None).unwrap();
    a.data.report.sessions.iter().map(|s| s.requests).sum()
}

#[test]
fn no_role_keeps_every_session() {
    let (_h, ctx) = setup();
    let all = api::cost(&ctx, &Filters::default(), "session", None).unwrap();
    assert_eq!(all.data.report.sessions.len(), 2);
}

#[test]
fn a_role_keeps_the_session_that_started_it_with_all_its_requests() {
    let (_h, ctx) = setup();
    let all = cost_requests(&ctx, &Filters::default());
    let with_runs = api::cost(&ctx, &roles(&["Implement"]), "session", None).unwrap();
    assert_eq!(with_runs.data.report.sessions.len(), 1);
    let kept: u64 = with_runs
        .data
        .report
        .sessions
        .iter()
        .map(|s| s.requests)
        .sum();
    // The main requests stay with the subagent's, so the kept session is more than the subagent alone.
    assert!(kept > 1 && kept < all);
}

#[test]
fn several_roles_match_any_of_them() {
    let (_h, ctx) = setup();
    let one = cost_requests(&ctx, &roles(&["workflow"]));
    let both = cost_requests(&ctx, &roles(&["nope", "workflow", "Implement"]));
    assert_eq!(one, both);
    assert!(one > 0);
}

#[test]
fn an_unknown_role_keeps_no_session() {
    let (_h, ctx) = setup();
    assert_eq!(cost_requests(&ctx, &roles(&["no-such-role"])), 0);
    let e = api::efficiency(&ctx, &roles(&["no-such-role"]), "session").unwrap();
    assert!(e.data.cache.is_empty());
}

#[test]
fn the_main_agent_is_not_a_role_to_filter_by() {
    let (_h, ctx) = setup();
    assert_eq!(cost_requests(&ctx, &roles(&["main"])), 0);
}

#[test]
fn flow_and_compare_take_the_filter_too() {
    let (_h, ctx) = setup();
    let f = api::flow(&ctx, &roles(&["no-such-role"])).unwrap();
    assert!(f.data.growth.sessions.is_empty());
    let c = api::compare(&ctx, &roles(&["no-such-role"])).unwrap();
    assert!(c.data.sources.is_empty());
    let c = api::compare(&ctx, &roles(&["Implement"])).unwrap();
    assert_eq!(c.data.sources.len(), 1);
}
