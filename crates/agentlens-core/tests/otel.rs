mod support;

use agentlens_core::adapter::{Adapter, SessionFilter};
use agentlens_core::adapters::claude_code::ClaudeCodeAdapter;
use agentlens_core::adapters::otel::OtelAdapter;
use agentlens_core::adapters::registry_with_otel;
use agentlens_core::model::{EventKind, SessionEvents, SkillTrigger, Source, ToolStatus, Usage};
use support::TempHome;

fn fixture_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/otel/claude.jsonl")
}

fn read() -> Vec<SessionEvents> {
    let home = TempHome::new();
    let adapter = OtelAdapter::new(vec![fixture_path()]);
    adapter
        .sessions(&home.env(), &SessionFilter::default())
        .map(|r| r.unwrap())
        .collect()
}

fn session(id: &str) -> SessionEvents {
    read().into_iter().find(|s| s.session.id == id).unwrap()
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
fn detects_only_existing_files() {
    let home = TempHome::new();
    assert!(!OtelAdapter::new(vec![]).detect(&home.env()).found);
    assert!(
        !OtelAdapter::new(vec!["/no/such/file".into()])
            .detect(&home.env())
            .found
    );
    assert!(
        OtelAdapter::new(vec![fixture_path()])
            .detect(&home.env())
            .found
    );
}

#[test]
fn api_requests_become_measured_usage_with_cost_and_skill_attribution() {
    let se = session("otel-session-1");
    assert_eq!(se.session.source, Source::ClaudeCode);
    assert_eq!(se.session.model.as_deref(), Some("claude-test-1"));
    let u = usages(&se);
    assert_eq!(u.len(), 2);
    assert!(u.iter().all(|u| u.measured));
    assert_eq!(
        (u[0].input, u[0].output, u[0].cache_read, u[0].cache_write),
        (100, 20, 1000, 300)
    );
    assert_eq!(u[0].cost_usd, Some(0.25));
    assert_eq!(u[0].skill.as_deref(), Some("demo"));
    assert!(!u[0].subagent);
    assert!(u[1].subagent, "query_source Explore is a subagent");
    assert_eq!(u[1].skill, None);
}

#[test]
fn skills_prompts_and_tools_are_mapped_without_their_content() {
    let se = session("otel-session-1");
    let skills: Vec<_> = se
        .events
        .iter()
        .filter_map(|e| match &e.kind {
            EventKind::SkillInvoked(s) => Some(s),
            _ => None,
        })
        .collect();
    assert_eq!(skills.len(), 1);
    assert_eq!(
        (skills[0].name.as_str(), skills[0].trigger),
        ("demo", SkillTrigger::User)
    );
    assert_eq!(
        se.events
            .iter()
            .filter(|e| matches!(e.kind, EventKind::UserTurn))
            .count(),
        1
    );
    let calls: Vec<_> = se
        .events
        .iter()
        .filter_map(|e| match &e.kind {
            EventKind::ToolCall(c) => Some(c),
            _ => None,
        })
        .collect();
    assert_eq!(calls.len(), 2);
    assert_eq!(
        (
            calls[0].name.as_str(),
            calls[0].status,
            calls[0].duration_ms
        ),
        ("Bash", ToolStatus::Ok, Some(120))
    );
    assert_eq!(calls[1].status, ToolStatus::Error);
    let json = serde_json::to_string(&se).unwrap();
    assert!(!json.contains("SECRET-COMMAND") && !json.contains("REDACTED"));
}

#[test]
fn events_are_ordered_by_time_and_the_nano_timestamp_is_understood() {
    let se = session("otel-session-1");
    let times: Vec<_> = se.events.iter().map(|e| e.at).collect();
    let mut sorted = times.clone();
    sorted.sort();
    assert_eq!(times, sorted);
    assert_eq!(
        se.session.started_at.unwrap().to_string(),
        "2026-10-05T10:00:00Z"
    );
    let two = session("otel-session-2");
    assert_eq!(
        two.events.len(),
        1,
        "eventName and timeUnixNano are accepted"
    );
    assert_eq!(two.events[0].at.unwrap().as_second(), 1_790_000_000);
}

#[test]
fn other_signals_and_unknown_records_are_counted_in_one_warning() {
    let all = read();
    let warnings: Vec<_> = all.iter().flat_map(|s| &s.warnings).collect();
    assert_eq!(warnings.len(), 1);
    let m = &warnings[0].message;
    for needle in [
        "resourceMetrics (1)",
        "resourceSpans (1)",
        "invalid JSON (1)",
        "records without session.id (1)",
        "other log records (1)",
        "event claude_code.tool_decision (1)",
    ] {
        assert!(m.contains(needle), "{needle} missing in {m}");
    }
}

#[test]
fn a_session_present_locally_is_counted_once_and_the_local_one_wins() {
    let home = TempHome::new();
    home.copy_fixture(
        "claude_code/basic.jsonl",
        ".claude/projects/-work-demo/basic.jsonl",
    );
    let env = home.env();
    let local = ClaudeCodeAdapter
        .sessions(&env, &SessionFilter::default())
        .map(|r| r.unwrap())
        .next()
        .unwrap();
    // An export of the same session.
    let line = format!(
        r#"{{"resourceLogs":[{{"resource":{{"attributes":[{{"key":"session.id","value":{{"stringValue":"{}"}}}}]}},"scopeLogs":[{{"logRecords":[{{"eventName":"claude_code.user_prompt","attributes":[]}}]}}]}}]}}"#,
        local.session.id
    );
    home.write("export.jsonl", line + "\n");
    let registry = registry_with_otel(vec![home.path().join("export.jsonl")]);
    let all: Vec<_> = registry
        .sessions(&env, &SessionFilter::default(), &[Source::ClaudeCode])
        .map(|r| r.unwrap())
        .filter(|s| s.session.id == local.session.id)
        .collect();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].events, local.events, "the local session wins");
    // `--source otel` selects the import.
    let only_otel: Vec<_> = registry
        .sessions(&env, &SessionFilter::default(), &[Source::Otel])
        .collect();
    assert_eq!(only_otel.len(), 1);
}
