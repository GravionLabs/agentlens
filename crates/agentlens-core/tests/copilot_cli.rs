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
           request_multiplier REAL, initiator TEXT, created_at TEXT);
         INSERT INTO assistant_usage_events
           (session_id, turn_index, agent_id, parent_tool_call_id, model, input_tokens,
            output_tokens, cache_read_tokens, cache_write_tokens, reasoning_tokens,
            request_multiplier, initiator, created_at)
         VALUES
           ('cp-1', 0, NULL, NULL, 'model-a', 1000, 50, 600, 100, 5, 9.0, 'user', '2026-07-15T04:00:02.000Z'),
           ('cp-1', 0, NULL, NULL, 'model-a', 1200, 70, 900, 0, 0, 9.0, 'agent', '2026-07-15T04:00:05.500Z'),
           ('cp-1', 0, 'explore-1', 'c3', 'model-b', 400, 30, 0, 0, 0, 1.0, 'agent', '2026-07-15T04:00:07.500Z');",
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

#[test]
fn hooks_and_permission_prompts_are_paired_without_their_content() {
    use agentlens_core::model::{Hook, PermissionPrompt};
    let se = session(&home(false), "cp-1");
    let hooks: Vec<&Hook> = se
        .events
        .iter()
        .filter_map(|e| match &e.kind {
            EventKind::Hook(h) => Some(h),
            _ => None,
        })
        .collect();
    assert_eq!(hooks.len(), 3);
    assert_eq!(
        (
            hooks[0].kind.as_str(),
            hooks[0].success,
            hooks[0].duration_ms
        ),
        ("postToolUse", Some(true), Some(250))
    );
    assert_eq!(
        (hooks[1].success, hooks[1].duration_ms),
        (Some(false), Some(500))
    );
    assert_eq!(
        (hooks[2].success, hooks[2].duration_ms),
        (None, None),
        "unpaired start is kept"
    );
    let prompts: Vec<&PermissionPrompt> = se
        .events
        .iter()
        .filter_map(|e| match &e.kind {
            EventKind::PermissionPrompt(p) => Some(p),
            _ => None,
        })
        .collect();
    assert_eq!(prompts.len(), 2);
    assert_eq!(prompts[0].wait_ms, Some(1500));
    assert_eq!(prompts[1].wait_ms, None);
    assert!(!serde_json::to_string(&se).unwrap().contains("never kept"));
    // Hook and permission events are known types, so they raise no warning.
    assert!(
        !se.warnings
            .iter()
            .any(|w| w.message.contains("hook") || w.message.contains("permission"))
    );
}

#[test]
fn premium_requests_are_carried_separately_from_tokens() {
    // Only the request the user started is charged, agent follow-ups are not.
    let se = session(&home(true), "cp-1");
    let u = usages(&se);
    assert_eq!(
        u.iter().map(|u| u.premium_requests).collect::<Vec<_>>(),
        [Some(9.0), Some(0.0), Some(0.0)]
    );
    assert!(
        u.iter().all(|u| u.cost_usd.is_none()),
        "premium requests are not money"
    );
    // Shutdown totals carry `requests.cost` per model.
    let se = session(&home(true), "cp-2");
    let mut premium: Vec<_> = usages(&se)
        .iter()
        .map(|u| (u.model.clone().unwrap(), u.premium_requests))
        .collect();
    premium.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        premium,
        [
            ("model-a".to_owned(), Some(1.0)),
            ("model-b".to_owned(), Some(3.0))
        ]
    );
}

#[test]
fn the_event_types_of_newer_versions_are_known_and_do_not_warn() {
    let home = TempHome::new();
    home.copy_fixture(
        "copilot_cli/newer_cli.jsonl",
        &format!("{STATE}/cp-3/events.jsonl"),
    );
    let se = read_all(&home).remove(0);
    let messages: Vec<&str> = se.warnings.iter().map(|w| w.message.as_str()).collect();
    // The usage records, the checkpoints and the `model.*` events are known: a type that nobody knows
    // still warns, and is the only one named.
    assert_eq!(messages.len(), 1, "{messages:?}");
    assert!(messages[0].contains("something.made.up"), "{messages:?}");
    for known in [
        "usage_record",
        "usage_checkpoint",
        "model.response",
        "auto_mode_resolved",
    ] {
        assert!(
            !messages[0].contains(known),
            "{known} must not be reported as unknown"
        );
    }
}

#[test]
fn usage_records_fill_in_for_a_session_without_store_rows() {
    let home = TempHome::new();
    home.copy_fixture(
        "copilot_cli/newer_cli.jsonl",
        &format!("{STATE}/cp-3/events.jsonl"),
    );
    let se = read_all(&home).remove(0);
    let u = usages(&se);
    assert_eq!(u.len(), 2);
    assert!(u.iter().all(|u| u.measured));
    // `inputTokens` is the whole prompt: 1000 - 600 - 100.
    assert_eq!((u[0].input, u[0].output), (300, 50));
    assert_eq!(
        (u[0].cache_read, u[0].cache_write, u[0].reasoning),
        (600, 100, 7)
    );
    assert_eq!(u[0].model.as_deref(), Some("model-c"));
    assert_eq!(u[0].premium_requests, Some(2.0));
    assert!(!u[0].subagent);
    // An agent follow-up is not charged, as in the store.
    assert_eq!((u[1].input, u[1].cache_read), (300, 1200));
    assert_eq!(u[1].premium_requests, Some(0.0));
}

#[test]
fn the_store_wins_when_the_session_has_rows_and_records() {
    let home = TempHome::new();
    home.copy_fixture(
        "copilot_cli/newer_cli.jsonl",
        &format!("{STATE}/cp-3/events.jsonl"),
    );
    write_store(&home);
    let conn = Connection::open(home.path().join(".copilot/session-store.db")).unwrap();
    conn.execute_batch(
        "INSERT INTO assistant_usage_events
           (session_id, turn_index, model, input_tokens, output_tokens, cache_read_tokens,
            cache_write_tokens, reasoning_tokens, request_multiplier, initiator, created_at)
         VALUES ('cp-3', 0, 'model-c', 1000, 50, 600, 100, 0, 1.0, 'user', '2026-10-05T08:00:03.200Z');",
    )
    .unwrap();
    drop(conn);
    let se = session(&home, "cp-3");
    let u = usages(&se);
    assert_eq!(u.len(), 1, "the records are not counted again");
    assert_eq!(
        u[0].premium_requests,
        Some(1.0),
        "the row of the store, not the record"
    );
}

#[test]
fn a_record_without_tokens_and_a_session_with_records_and_shutdown_count_once() {
    let home = TempHome::new();
    let events = [
        r#"{"type":"session.start","data":{"sessionId":"cp-4","context":{"cwd":"/work/x"}},"id":"a","timestamp":"2026-10-05T08:00:00.000Z","parentId":null}"#,
        r#"{"type":"session.usage_record","data":{"usage":{"model":"m","inputTokens":0,"outputTokens":0}},"id":"b","timestamp":"2026-10-05T08:00:01.000Z","parentId":null}"#,
        r#"{"type":"session.usage_record","data":{"usage":{"model":"m","inputTokens":100,"outputTokens":10}},"id":"c","timestamp":"2026-10-05T08:00:02.000Z","parentId":null}"#,
        r#"{"type":"session.shutdown","data":{"currentModel":"m","modelMetrics":{"m":{"usage":{"inputTokens":100,"outputTokens":10,"cacheReadTokens":0,"cacheWriteTokens":0},"requests":{"cost":1}}}},"id":"d","timestamp":"2026-10-05T08:00:03.000Z","parentId":null}"#,
    ];
    let path = home.path().join(format!("{STATE}/cp-4/events.jsonl"));
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, events.join("\n")).unwrap();
    let se = read_all(&home).remove(0);
    let u = usages(&se);
    assert_eq!(
        u.len(),
        1,
        "the totals of the shutdown event are not added to the records"
    );
    assert_eq!(u[0].input, 100);
}

#[test]
fn malformed_usage_records_are_skipped_and_missing_fields_stay_unknown() {
    let home = TempHome::new();
    let events = [
        r#"{"type":"session.start","data":{"sessionId":"cp-5","context":{"cwd":"/work/x"}},"id":"a","timestamp":"2026-10-05T08:00:00.000Z","parentId":null}"#,
        // Later in the file but earlier in time.
        r#"{"type":"session.usage_record","data":{"usage":{"model":"late","inputTokens":50,"outputTokens":5}},"id":"b","timestamp":"2026-10-05T08:00:09.000Z","parentId":null}"#,
        r#"{"type":"session.usage_record","data":{"usage":{"model":"early","inputTokens":100,"outputTokens":10,"cacheReadTokens":100}},"id":"c","timestamp":"2026-10-05T08:00:02.000Z","parentId":null}"#,
        r#"{"type":"session.usage_record","data":{},"id":"d","timestamp":"2026-10-05T08:00:03.000Z","parentId":null}"#,
        r#"{"type":"session.usage_record","data":{"usage":{"model":"bad","inputTokens":"many","outputTokens":null}},"id":"e","timestamp":"2026-10-05T08:00:04.000Z","parentId":null}"#,
        r#"{"type":"session.usage_record","data":{"usage":"nope"},"id":"f","timestamp":"2026-10-05T08:00:05.000Z","parentId":null}"#,
    ];
    let path = home.path().join(format!("{STATE}/cp-5/events.jsonl"));
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, events.join("\n")).unwrap();
    let se = read_all(&home).remove(0);
    assert!(se.warnings.is_empty(), "{:?}", se.warnings);
    let u = usages(&se);
    let models: Vec<_> = u.iter().map(|u| u.model.as_deref().unwrap()).collect();
    assert_eq!(models, ["early", "late"], "in time order");
    assert_eq!(u[0].input, 0, "all of the prompt was cached");
    assert_eq!(
        u[0].premium_requests, None,
        "no cost recorded: unknown, not zero"
    );
}

#[test]
fn a_commit_message_keeps_only_its_issue_number() {
    let home = TempHome::new();
    home.write(
        &format!("{STATE}/cp-9/events.jsonl"),
        concat!(
            r#"{"type":"session.start","data":{"sessionId":"cp-9","version":1,"context":{"cwd":"/work/demo","branch":"release/next"}},"id":"g1","timestamp":"2026-07-16T04:00:00.000Z","parentId":null}"#,
            "\n",
            r#"{"type":"tool.execution_start","data":{"toolCallId":"c1","toolName":"bash","arguments":{"command":"git commit -m 'fix: y' -m 'Closes #41'"}},"id":"g2","timestamp":"2026-07-16T04:00:01.000Z","parentId":"g1"}"#,
            "\n",
        ),
    );
    let se = session(&home, "cp-9");
    let issues: Vec<u32> = se
        .events
        .iter()
        .filter_map(|e| match e.kind {
            EventKind::CommitIssue { issue } => Some(issue),
            _ => None,
        })
        .collect();
    assert_eq!(issues, [41]);
    let json = serde_json::to_string(&se.events).unwrap();
    assert!(!json.contains("fix: y"), "the message is not kept");
}

fn runs(se: &SessionEvents) -> Vec<&EventKind> {
    se.events
        .iter()
        .map(|e| &e.kind)
        .filter(|k| matches!(k, EventKind::AgentRun { .. }))
        .collect()
}

#[test]
fn a_started_subagent_is_a_run_of_the_main_session() {
    let se = session(&home(true), "cp-1");
    let r = runs(&se);
    assert_eq!(r.len(), 1);
    let EventKind::AgentRun {
        agent_id,
        parent_agent_id,
        tool_use_id,
        agent_type,
        description,
        depth,
        started_at,
        ended_at,
        ..
    } = r[0]
    else {
        unreachable!()
    };
    assert_eq!(agent_id, "c3");
    assert_eq!(tool_use_id.as_deref(), Some("c3"));
    assert_eq!(*parent_agent_id, None);
    assert_eq!(agent_type.as_deref(), Some("explore"));
    assert_eq!(description.as_deref(), Some("Explore"));
    assert_eq!(*depth, Some(1));
    assert_eq!(started_at.to_string(), "2026-07-15T04:00:07Z");
    assert_eq!(*ended_at, None, "the fixture never completes it");
}

#[test]
fn store_rows_of_a_subagent_carry_the_run_id() {
    let se = session(&home(true), "cp-1");
    let u = usages(&se);
    let sub: Vec<_> = u.iter().filter(|u| u.subagent).collect();
    assert_eq!(sub.len(), 1);
    assert_eq!(
        sub[0].agent.as_ref().and_then(|a| a.agent_id.as_deref()),
        Some("c3")
    );
    assert!(u.iter().filter(|u| !u.subagent).all(|u| u.agent.is_none()));
}

fn events_home(lines: &[&str]) -> TempHome {
    let home = TempHome::new();
    home.write(
        &format!("{STATE}/cp-9/events.jsonl"),
        lines.join("\n") + "\n",
    );
    home
}

#[test]
fn completed_failed_and_nested_runs_are_linked() {
    let home = events_home(&[
        r#"{"type":"subagent.started","data":{"toolCallId":"a","agentName":"general","agentDisplayName":"General"},"timestamp":"2026-07-15T04:00:00.000Z"}"#,
        r#"{"type":"subagent.started","data":{"toolCallId":"b","agentName":"explore","agentDisplayName":"Explore","parentToolCallId":"a"},"timestamp":"2026-07-15T04:00:01.000Z"}"#,
        r#"{"type":"subagent.started","data":{"toolCallId":"c","agentName":"x","parentToolCallId":"nobody"},"timestamp":"2026-07-15T04:00:01.500Z"}"#,
        r#"{"type":"subagent.failed","data":{"toolCallId":"b"},"timestamp":"2026-07-15T04:00:02.000Z"}"#,
        r#"{"type":"subagent.completed","data":{"toolCallId":"a"},"timestamp":"2026-07-15T04:00:03.000Z"}"#,
    ]);
    let se = session(&home, "cp-9");
    assert!(se.warnings.is_empty(), "{:?}", se.warnings);
    let mut got: Vec<_> = runs(&se)
        .into_iter()
        .map(|k| match k {
            EventKind::AgentRun {
                agent_id,
                parent_agent_id,
                depth,
                ended_at,
                ..
            } => (
                agent_id.clone(),
                parent_agent_id.clone(),
                *depth,
                ended_at.map(|t| t.to_string()),
            ),
            _ => unreachable!(),
        })
        .collect();
    got.sort();
    assert_eq!(
        got,
        [
            (
                "a".to_owned(),
                None,
                Some(1),
                Some("2026-07-15T04:00:03Z".to_owned())
            ),
            (
                "b".to_owned(),
                Some("a".to_owned()),
                Some(2),
                Some("2026-07-15T04:00:02Z".to_owned())
            ),
            ("c".to_owned(), None, None, None),
        ]
    );
}

#[test]
fn a_started_event_without_id_or_time_warns_and_gives_no_run() {
    let home = events_home(&[
        r#"{"type":"subagent.started","data":{"agentName":"x"},"timestamp":"2026-07-15T04:00:00.000Z"}"#,
        r#"{"type":"subagent.started","data":{"toolCallId":"z"}}"#,
    ]);
    let se = session(&home, "cp-9");
    assert!(runs(&se).is_empty());
    assert_eq!(se.warnings.len(), 2, "{:?}", se.warnings);
}
