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
        ..SessionFilter::default()
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

// ---- legacy JSON storage (OpenCode before 1.2.0) ----

/// Writes `value` to `<data home>/opencode/storage/<rel>`.
fn put(home: &TempHome, rel: &str, value: &str) {
    let path = home.path().join(".local/share/opencode/storage").join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, value).unwrap();
}

/// One synthetic legacy session `ses_legacy` with a subagent `ses_legacy_sub`.
fn legacy_home() -> TempHome {
    let home = TempHome::new();
    put(
        &home,
        "session/proj1/ses_legacy.json",
        r#"{"id":"ses_legacy","projectID":"proj1","directory":"/work/legacy","title":"Legacy","time":{"created":1780000000000,"updated":1780000100000}}"#,
    );
    put(
        &home,
        "session/proj1/ses_legacy_sub.json",
        r#"{"id":"ses_legacy_sub","parentID":"ses_legacy","projectID":"proj1","directory":"/work/legacy","time":{"created":1780000030000,"updated":1780000040000}}"#,
    );
    put(
        &home,
        "message/ses_legacy/msg_1.json",
        r#"{"id":"msg_1","sessionID":"ses_legacy","role":"user","time":{"created":1780000000000}}"#,
    );
    put(
        &home,
        "message/ses_legacy/msg_2.json",
        r#"{"id":"msg_2","sessionID":"ses_legacy","role":"assistant","modelID":"model-a","providerID":"prov","cost":0.03,"tokens":{"input":300,"output":100,"reasoning":0,"cache":{"read":1000,"write":300}},"time":{"created":1780000001000,"completed":1780000020000}}"#,
    );
    put(
        &home,
        "part/msg_2/prt_1.json",
        r#"{"id":"prt_1","messageID":"msg_2","sessionID":"ses_legacy","type":"tool","tool":"skill","state":{"status":"completed","input":{"name":"demo-skill"},"output":"0123456789","metadata":{"dir":"/home/u/.claude/skills/demo-skill"},"time":{"start":1780000002000,"end":1780000002500}}}"#,
    );
    put(
        &home,
        "part/msg_2/prt_2.json",
        r#"{"id":"prt_2","messageID":"msg_2","sessionID":"ses_legacy","type":"step-finish","tokens":{"input":100,"output":20,"reasoning":0,"cache":{"read":500,"write":300}},"cost":0.01}"#,
    );
    put(
        &home,
        "part/msg_2/prt_3.json",
        r#"{"id":"prt_3","messageID":"msg_2","sessionID":"ses_legacy","type":"step-finish","tokens":{"input":200,"output":80,"reasoning":0,"cache":{"read":500,"write":0}},"cost":0.02}"#,
    );
    put(
        &home,
        "message/ses_legacy_sub/msg_s.json",
        r#"{"id":"msg_s","sessionID":"ses_legacy_sub","role":"assistant","modelID":"model-b","cost":0.005,"tokens":{"input":50,"output":20,"reasoning":0,"cache":{"read":180,"write":0}},"time":{"created":1780000031000,"completed":1780000040000}}"#,
    );
    home
}

#[test]
fn detects_the_legacy_storage_directory() {
    let home = legacy_home();
    let d = OpenCodeAdapter.detect(&home.env());
    assert!(d.found);
    assert_eq!(d.roots, [home.path().join(".local/share/opencode/storage")]);
}

#[test]
fn legacy_sessions_are_read_with_the_same_mapping() {
    let home = legacy_home();
    let all = read(&home, &SessionFilter::default());
    assert_eq!(all.len(), 1, "the subagent is merged into its root");
    let se = &all[0];
    assert_eq!(se.session.id, "ses_legacy");
    assert_eq!(
        se.session.project_dir.as_deref(),
        Some(Path::new("/work/legacy"))
    );
    assert_eq!(se.session.model.as_deref(), Some("model-a"));
    assert_eq!(
        se.session.started_at.unwrap().as_millisecond(),
        1780000000000
    );

    let u = usages(se);
    assert_eq!(u.len(), 3, "two steps of msg_2 and one subagent request");
    assert!(u.iter().all(|u| u.measured));
    let main: Vec<_> = u.iter().filter(|u| !u.subagent).collect();
    assert_eq!(main.len(), 2);
    assert_eq!(
        (
            main[0].input,
            main[0].output,
            main[0].cache_read,
            main[0].cache_write
        ),
        (100, 20, 500, 300)
    );
    assert_eq!(main[0].model.as_deref(), Some("model-a"));
    let sub: Vec<_> = u.iter().filter(|u| u.subagent).collect();
    assert_eq!(sub.len(), 1, "a message without steps counts by its totals");
    assert_eq!(sub[0].cache_read, 180);

    let skill = se.events.iter().find_map(|e| match &e.kind {
        EventKind::SkillInvoked(s) => Some(s),
        _ => None,
    });
    let skill = skill.expect("the skill call");
    assert_eq!(skill.name, "demo-skill");
    assert_eq!(skill.content_bytes, Some(10));
    assert_eq!(
        se.events
            .iter()
            .filter(|e| matches!(e.kind, EventKind::UserTurn))
            .count(),
        1
    );
    assert!(se.warnings.is_empty(), "{:?}", se.warnings);
}

#[test]
fn a_session_in_both_places_is_counted_once_from_the_database() {
    let (home, _conn) = home_with_db("");
    // The same id as in the database fixture, with different numbers.
    put(
        &home,
        "session/p/ses_root.json",
        r#"{"id":"ses_root","directory":"/work/legacy","time":{"created":1780000000000,"updated":1780000100000}}"#,
    );
    put(
        &home,
        "message/ses_root/m.json",
        r#"{"id":"m","role":"assistant","modelID":"x","tokens":{"input":9999,"output":1,"cache":{"read":0,"write":0}},"time":{"created":1780000001000}}"#,
    );
    let all = read(&home, &SessionFilter::default());
    let roots: Vec<_> = all.iter().filter(|s| s.session.id == "ses_root").collect();
    assert_eq!(roots.len(), 1);
    assert_eq!(
        roots[0].session.project_dir.as_deref(),
        Some(Path::new("/work/demo"))
    );
    assert!(usages(roots[0]).iter().all(|u| u.input != 9999));
}

#[test]
fn legacy_only_sessions_are_listed_next_to_database_sessions() {
    let (home, _conn) = home_with_db("");
    put(
        &home,
        "session/p/ses_only_legacy.json",
        r#"{"id":"ses_only_legacy","directory":"/work/legacy","time":{"created":1700000000000,"updated":1700000001000}}"#,
    );
    let ids: Vec<_> = read(&home, &SessionFilter::default())
        .into_iter()
        .map(|s| s.session.id)
        .collect();
    assert!(ids.contains(&"ses_root".to_string()));
    assert!(ids.contains(&"ses_only_legacy".to_string()));
}

#[test]
fn broken_legacy_files_are_skipped_with_a_warning_for_parts() {
    let home = legacy_home();
    put(&home, "session/proj1/broken.json", "not json");
    put(&home, "message/ses_legacy/msg_bad.json", "{");
    put(&home, "part/msg_2/prt_bad.json", "oops");
    let all = read(&home, &SessionFilter::default());
    assert_eq!(all.len(), 1);
    assert_eq!(usages(&all[0]).len(), 3, "the valid records still count");
    assert_eq!(all[0].warnings.len(), 1);
}

#[test]
fn the_since_filter_applies_to_legacy_sessions() {
    let home = legacy_home();
    let filter = SessionFilter {
        since: Some(jiff::Timestamp::from_millisecond(1790000000000).unwrap()),
        ..SessionFilter::default()
    };
    assert!(read(&home, &filter).is_empty());
}

#[test]
fn a_commit_message_keeps_only_its_issue_number() {
    let (home, _conn) = home_with_db(
        r#"INSERT INTO part VALUES ('p99', 'm2', 'ses_root', 1780000008000, 1780000008000,
        '{"type":"tool","tool":"bash","callID":"c9","state":{"status":"completed","input":{"command":"git commit -m \"feat: x (#502)\""},"output":"ok","time":{"start":1780000008000,"end":1780000008100}}}');"#,
    );
    let se = read(&home, &SessionFilter::default())
        .into_iter()
        .find(|s| s.session.id == "ses_root")
        .unwrap();
    let issues: Vec<u32> = se
        .events
        .iter()
        .filter_map(|e| match e.kind {
            EventKind::CommitIssue { issue } => Some(issue),
            _ => None,
        })
        .collect();
    assert_eq!(issues, [502]);
}

fn runs(se: &SessionEvents) -> Vec<&EventKind> {
    se.events
        .iter()
        .map(|e| &e.kind)
        .filter(|k| matches!(k, EventKind::AgentRun { .. }))
        .collect()
}

#[test]
fn a_session_with_a_parent_is_a_run_called_by_the_main_session() {
    let se = root();
    let r = runs(&se);
    assert_eq!(r.len(), 1, "the root is not a run");
    let EventKind::AgentRun {
        agent_id,
        parent_agent_id,
        agent_type,
        model,
        depth,
        started_at,
        ended_at,
        ..
    } = r[0]
    else {
        unreachable!()
    };
    assert_eq!(agent_id, "ses_sub");
    assert_eq!(*parent_agent_id, None);
    assert_eq!(*depth, Some(1));
    assert_eq!(*agent_type, None, "no user message names an agent");
    assert_eq!(model.as_deref(), Some("model-b"));
    assert_eq!(started_at.as_millisecond(), 1780000030000);
    assert_eq!(ended_at.unwrap().as_millisecond(), 1780000040000);
}

#[test]
fn the_requests_of_a_subagent_session_carry_the_run_id() {
    let se = root();
    let u = usages(&se);
    let sub: Vec<_> = u.iter().filter(|u| u.subagent).collect();
    assert_eq!(
        sub[0].agent.as_ref().and_then(|a| a.agent_id.as_deref()),
        Some("ses_sub")
    );
    assert!(u.iter().filter(|u| !u.subagent).all(|u| u.agent.is_none()));
}

#[test]
fn nested_sessions_follow_the_parent_chain() {
    let (home, _conn) = home_with_db(
        "INSERT INTO session VALUES
           ('ses_deep', 'p1', 'ses_sub', '/work/demo', 'Deep', NULL, 1780000032000, 1780000036000);
         INSERT INTO message VALUES
           ('d0', 'ses_deep', 1780000032000, 1780000032000, '{\"role\":\"user\",\"agent\":\"explore\",\"time\":{\"created\":1780000032000}}'),
           ('d1', 'ses_deep', 1780000033000, 1780000036000, '{\"role\":\"assistant\",\"modelID\":\"model-b\",\"cost\":0.001,\"tokens\":{\"input\":10,\"output\":5,\"reasoning\":0,\"cache\":{\"read\":0,\"write\":0}}}');",
    );
    let se = read(&home, &SessionFilter::default())
        .into_iter()
        .find(|s| s.session.id == "ses_root")
        .unwrap();
    let deep = runs(&se)
        .into_iter()
        .find_map(|k| match k {
            EventKind::AgentRun {
                agent_id,
                parent_agent_id,
                agent_type,
                depth,
                ..
            } if agent_id == "ses_deep" => {
                Some((parent_agent_id.clone(), agent_type.clone(), *depth))
            }
            _ => None,
        })
        .expect("the nested run");
    assert_eq!(
        deep,
        (
            Some("ses_sub".to_owned()),
            Some("explore".to_owned()),
            Some(2)
        )
    );
    let sub = runs(&se)
        .into_iter()
        .filter(|k| matches!(k, EventKind::AgentRun { depth: Some(1), .. }))
        .count();
    assert_eq!(sub, 1);
    let deep_usage = usages(&se)
        .into_iter()
        .find(|u| u.input == 10)
        .expect("the request of the nested session");
    assert_eq!(
        deep_usage
            .agent
            .as_ref()
            .and_then(|a| a.agent_id.as_deref()),
        Some("ses_deep")
    );
}

#[test]
fn a_parent_cycle_gives_no_run_and_no_guess() {
    let (home, _conn) = home_with_db(
        "INSERT INTO session VALUES
           ('cyc_a', 'p1', 'cyc_b', '/work/c', 'A', NULL, 1780000050000, 1780000051000),
           ('cyc_b', 'p1', 'cyc_a', '/work/c', 'B', NULL, 1780000050000, 1780000051000);",
    );
    // The cycle has no root: its sessions are read as roots, never as runs with an invented depth.
    let all = read(&home, &SessionFilter::default());
    for id in ["cyc_a", "cyc_b"] {
        let se = all.iter().find(|s| s.session.id == id).unwrap();
        assert!(runs(se).is_empty());
    }
}

#[test]
fn skill_loads_in_a_child_session_are_flagged_as_subagent() {
    let home = legacy_home();
    put(
        &home,
        "part/msg_s/prt_sk.json",
        r#"{"id":"prt_sk","messageID":"msg_s","sessionID":"ses_legacy_sub","type":"tool","tool":"skill","state":{"status":"completed","input":{"name":"child-skill"},"output":"0123","time":{"start":1780000032000,"end":1780000033000}}}"#,
    );
    let all = read(&home, &SessionFilter::default());
    let flags: Vec<_> = all[0]
        .events
        .iter()
        .filter_map(|e| match &e.kind {
            EventKind::SkillInvoked(s) => Some((s.name.as_str(), s.subagent)),
            _ => None,
        })
        .collect();
    assert!(flags.contains(&("demo-skill", false)));
    assert!(flags.contains(&("child-skill", true)));
}
