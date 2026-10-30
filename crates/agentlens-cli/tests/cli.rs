use std::fs;
use std::path::Path;

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;
use tempfile::TempDir;

fn fixtures() -> &'static Path {
    Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../agentlens-core/tests/fixtures/claude_code"
    ))
}

/// A home directory with two Claude Code sessions and three installed skills.
fn home() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    let projects = dir.path().join(".claude/projects/-work-demo");
    fs::create_dir_all(&projects).unwrap();
    for f in ["skills.jsonl", "basic.jsonl"] {
        fs::copy(fixtures().join(f), projects.join(f)).unwrap();
    }
    for name in ["demo", "orch", "unused"] {
        let d = dir.path().join(format!(".claude/skills/{name}"));
        fs::create_dir_all(&d).unwrap();
        fs::write(
            d.join("SKILL.md"),
            format!(
                "---\nname: {name}\ndescription: Does {name} things\n---\n{}",
                "x".repeat(400)
            ),
        )
        .unwrap();
    }
    dir
}

fn agentlens(home: &TempDir) -> Command {
    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("agentlens");
    cmd.env("HOME", home.path()).env("USERPROFILE", home.path());
    for var in [
        "CLAUDE_CONFIG_DIR",
        "COPILOT_HOME",
        "XDG_CACHE_HOME",
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
        "ANTHROPIC_API_KEY",
        "ANTHROPIC_BASE_URL",
    ] {
        cmd.env_remove(var);
    }
    cmd
}

fn json(cmd: &mut Command) -> Value {
    let out = cmd.assert().success().get_output().stdout.clone();
    serde_json::from_slice(&out).expect("valid JSON on stdout")
}

#[test]
fn help_lists_both_commands_and_the_global_flags() {
    let home = home();
    agentlens(&home).arg("--help").assert().success().stdout(
        predicate::str::contains("scan")
            .and(predicate::str::contains("skills"))
            .and(predicate::str::contains("--since"))
            .and(predicate::str::contains("--source"))
            .and(predicate::str::contains("--project"))
            .and(predicate::str::contains("--format")),
    );
}

#[test]
fn scan_reports_the_found_source_in_a_table() {
    let home = home();
    agentlens(&home)
        .arg("scan")
        .assert()
        .success()
        .stdout(predicate::str::contains("claude-code").and(predicate::str::contains("yes")));
}

#[test]
fn scan_json_has_counts_per_source() {
    let home = home();
    let v = json(agentlens(&home).args(["scan", "--format", "json"]));
    let claude = &v.as_array().unwrap()[0];
    assert_eq!(claude["source"], "claude-code");
    assert_eq!(claude["found"], true);
    assert_eq!(claude["sessions"], 2);
    assert_eq!(claude["warnings"], 1, "the bad line in basic.jsonl");
}

#[test]
fn scan_without_any_client_data_says_so() {
    let empty = tempfile::tempdir().unwrap();
    agentlens(&empty)
        .arg("scan")
        .assert()
        .success()
        .stdout(predicate::str::contains("no").and(predicate::str::contains("does not exist")));
}

#[test]
fn skills_table_marks_estimates_and_lists_unused_skills() {
    let home = home();
    agentlens(&home).arg("skills").assert().success().stdout(
        predicate::str::contains("demo")
            .and(predicate::str::contains("orch"))
            .and(predicate::str::contains("unused"))
            .and(predicate::str::contains("~"))
            .and(predicate::str::contains("estimated")),
    );
}

#[test]
fn skills_json_is_the_skill_report() {
    let home = home();
    let v = json(agentlens(&home).args(["skills", "--format", "json"]));
    assert_eq!(v["sessions"], 2);
    let names: Vec<_> = v["skills"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["demo", "orch", "unused"]);
    assert_eq!(v["skills"][0]["invocations"], 1);
}

#[test]
fn skills_csv_has_one_row_per_skill() {
    let home = home();
    let out = agentlens(&home)
        .args(["skills", "--format", "csv"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(out).unwrap();
    let mut lines = text.lines();
    assert!(
        lines
            .next()
            .unwrap()
            .starts_with("skill,in_inventory,uses,sessions,load_tokens,load_measured")
    );
    assert_eq!(lines.count(), 3);
}

#[test]
fn since_in_the_future_selects_no_sessions() {
    let home = home();
    let v = json(agentlens(&home).args(["skills", "--format", "json", "--since", "2999-01-01"]));
    assert_eq!(v["sessions"], 0);
}

#[test]
fn until_closes_the_period_on_the_fixture_sessions() {
    let home = home();
    let sessions = |args: &[&str]| {
        let mut cmd = agentlens(&home);
        cmd.args(["skills", "--format", "json"]).args(args);
        json(&mut cmd)["sessions"].clone()
    };
    // The sessions ran on 2026-10-01 (10:00-10:02 and 11:00-11:06).
    assert_eq!(sessions(&["--until", "2026-10-02"]), 2);
    assert_eq!(sessions(&["--until", "2026-10-01"]), 0);
    assert_eq!(
        sessions(&["--since", "2026-10-01", "--until", "2026-10-02"]),
        2
    );
    assert_eq!(
        sessions(&[
            "--since",
            "2026-10-01T10:30:00Z",
            "--until",
            "2026-10-01T12:00:00Z"
        ]),
        1
    );
    assert_eq!(
        sessions(&["--since", "2026-10-02", "--until", "2026-10-03"]),
        0
    );
}

#[test]
fn a_relative_until_counts_back_from_now() {
    let home = home();
    let sessions = |args: &[&str]| {
        let mut cmd = agentlens(&home);
        cmd.args(["skills", "--format", "json"]).args(args);
        json(&mut cmd)["sessions"].clone()
    };
    // The sessions are from October 2026: an hour ago is after them, 3000 days ago long before.
    assert_eq!(sessions(&["--since", "3000d", "--until", "1h"]), 2);
    assert_eq!(sessions(&["--until", "3000d"]), 0);
}

#[test]
fn until_not_after_since_fails_naming_both() {
    let home = home();
    agentlens(&home)
        .args(["skills", "--since", "2026-10-02", "--until", "2026-10-01"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--until").and(predicate::str::contains("--since")));
    agentlens(&home)
        .args(["skills", "--since", "2026-10-02", "--until", "2026-10-02"])
        .assert()
        .failure();
}

#[test]
fn invalid_until_fails_with_a_hint() {
    let home = home();
    agentlens(&home)
        .args(["skills", "--until", "soon"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--until").and(predicate::str::contains("30d")));
}

#[test]
fn project_filter_keeps_only_sessions_below_the_directory() {
    let home = home();
    let v = json(agentlens(&home).args(["skills", "--format", "json", "--project", "/work/demo"]));
    assert_eq!(v["sessions"], 2);
    let v = json(agentlens(&home).args(["skills", "--format", "json", "--project", "/elsewhere"]));
    assert_eq!(v["sessions"], 0);
}

#[test]
fn source_filter_rejects_unknown_ids_and_accepts_known_ones() {
    let home = home();
    agentlens(&home)
        .args(["skills", "--source", "nope"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown source"));
    let v = json(agentlens(&home).args(["skills", "--format", "json", "--source", "copilot-cli"]));
    assert_eq!(v["sessions"], 0);
}

#[test]
fn invalid_since_fails_with_a_hint() {
    let home = home();
    agentlens(&home)
        .args(["skills", "--since", "soon"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("30d"));
    agentlens(&home)
        .args(["skills", "--since", "soon"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--since: cannot parse"));
}

fn home_with_context() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    let projects = dir.path().join(".claude/projects/-work-demo");
    fs::create_dir_all(&projects).unwrap();
    fs::copy(
        fixtures().join("context.jsonl"),
        projects.join("context.jsonl"),
    )
    .unwrap();
    dir
}

#[test]
fn context_json_groups_by_kind_and_owner() {
    let home = home_with_context();
    let v = json(agentlens(&home).args(["context", "--kind", "tools", "--format", "json"]));
    let rows = v["rows"].as_array().unwrap();
    assert!(rows.iter().all(|r| r["kind"] != "instruction-file"));
    let srv = rows
        .iter()
        .find(|r| r["kind"] == "tool-listing" && r["owner"] == "srv")
        .unwrap();
    // Two listed tools of 15 and 14 bytes -> 4 + 4 tokens, three requests follow each.
    assert_eq!(srv["tokens"]["count"], 8);
    assert_eq!(
        srv["tokens"]["measured"], false,
        "taken from the size in bytes"
    );
    assert_eq!(srv["requests"], 6, "summed over both items");
    assert_eq!(srv["overhead_tokens"]["count"], 24);
    assert_eq!(srv["overhead_tokens"]["measured"], false);
    assert_eq!(v["first_request"]["claude-code"]["max_tokens"], 1010);
}

#[test]
fn context_instructions_view_and_csv() {
    let home = home_with_context();
    let out = agentlens(&home)
        .args(["context", "--kind", "instructions", "--format", "csv"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(out).unwrap();
    let mut lines = text.lines();
    assert_eq!(
        lines.next().unwrap(),
        "source,project,kind,owner,items,bytes,tokens,requests,overhead_tokens,tokens_measured"
    );
    assert_eq!(
        lines.next().unwrap(),
        "claude-code,/work/demo,instruction-file,/work/demo/CLAUDE.md,1,400,100,3,300,false"
    );
    assert!(lines.next().is_none());
}

#[test]
fn context_table_marks_estimates_and_unavailable_sources() {
    let home = home_with_context();
    agentlens(&home)
        .arg("context")
        .assert()
        .success()
        .stdout(predicate::str::contains("~100"))
        .stdout(predicate::str::contains("first request"));
}

#[test]
fn unused_lists_idle_agents_and_mcp_servers_with_their_cost() {
    let home = home_with_context();
    let agents = home.path().join(".claude/agents");
    fs::create_dir_all(&agents).unwrap();
    fs::write(
        agents.join("reviewer.md"),
        "---\nname: reviewer\ndescription: Reviews code\n---\nbody",
    )
    .unwrap();
    fs::write(
        home.path().join(".claude.json"),
        r#"{"mcpServers":{"srv":{"command":"never-read"}}}"#,
    )
    .unwrap();
    let v = json(agentlens(&home).args(["unused", "--since", "2026-01-01", "--format", "json"]));
    let items = v["items"].as_array().unwrap();
    let find = |kind: &str, name: &str| {
        items
            .iter()
            .find(|i| i["kind"] == kind && i["name"] == name)
            .unwrap_or_else(|| panic!("{kind} {name} missing"))
    };
    let agent = find("agent", "reviewer");
    assert_eq!(agent["uses"], 0);
    // "reviewerReviews code" = 20 characters = 5 tokens, three requests.
    assert_eq!(agent["overhead_tokens"]["count"], 15);
    assert_eq!(agent["overhead_tokens"]["measured"], false);
    let server = find("mcp-server", "srv");
    assert_eq!(server["uses"], 0);
    // Listings 2 x 4 tokens, definition 38 and instructions 75 tokens, three requests each.
    assert_eq!(server["overhead_tokens"]["count"], 363);
    assert!(!v.to_string().contains("never-read"));
}

#[test]
fn unused_and_advise_say_which_agents_and_commands_name_an_unused_skill() {
    let home = home();
    let agents = home.path().join(".claude/agents");
    fs::create_dir_all(&agents).unwrap();
    fs::write(
        agents.join("flow.md"),
        "---\nname: flow\ndescription: Flow\n---\nRuns the unused skill and my-orch-helper.",
    )
    .unwrap();
    let commands = home.path().join(".claude/commands");
    fs::create_dir_all(&commands).unwrap();
    fs::write(commands.join("ship.md"), "Run unused first.").unwrap();

    let v = json(agentlens(&home).args(["unused", "--since", "2026-01-01", "--format", "json"]));
    let idle = v["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["kind"] == "skill" && i["name"] == "unused")
        .unwrap();
    assert_eq!(
        idle["referenced_by"],
        serde_json::json!([
            {"kind": "agent", "name": "flow"},
            {"kind": "command", "name": "ship"}
        ])
    );
    // Items that nothing names have no `referenced_by` at all.
    assert!(
        v["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|i| i["name"] != "unused")
            .all(|i| i.get("referenced_by").is_none())
    );
    agentlens(&home)
        .args(["unused", "--since", "2026-01-01"])
        .assert()
        .success()
        .stdout(predicate::str::contains("agent flow, command ship"));

    let a = json(agentlens(&home).args(["advise", "--since", "2026-01-01", "--format", "json"]));
    let finding = a["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["kind"] == "unused-skills")
        .unwrap();
    assert!(
        finding["items"][0]["detail"]
            .as_str()
            .unwrap()
            .contains("referenced by agent flow, command ship")
    );
}

#[test]
fn advise_lists_mcp_servers_by_their_static_context() {
    let home = home_with_context();
    let a = json(agentlens(&home).args(["advise", "--since", "2026-01-01", "--format", "json"]));
    let f = a["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["kind"] == "costly-mcp-servers")
        .expect("the finding");
    assert_eq!(f["items"][0]["name"], "srv");
    // The same 363 estimated tokens as `unused` shows for this server.
    assert_eq!(f["items"][0]["tokens"]["count"], 363);
    assert_eq!(f["items"][0]["tokens"]["measured"], false);
    assert!(
        f["items"][0]["detail"]
            .as_str()
            .unwrap()
            .contains("0 tool calls")
    );
    agentlens(&home)
        .args(["advise", "--since", "2026-01-01"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Costly MCP servers (1)"));
}

#[test]
fn unused_hides_used_items_unless_all_is_given() {
    let home = home();
    let csv = |args: &[&str]| {
        let out = agentlens(&home)
            .args(args)
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        String::from_utf8(out).unwrap()
    };
    let idle = csv(&["unused", "--since", "2026-01-01", "--format", "csv"]);
    let all = csv(&[
        "unused",
        "--since",
        "2026-01-01",
        "--all",
        "--format",
        "csv",
    ]);
    assert!(idle.contains("skill,unused,"));
    assert!(!idle.contains("skill,demo,"), "{idle}");
    assert!(all.contains("skill,demo,"), "{all}");
}

#[test]
fn unused_window_is_set_with_days_and_overridden_by_since() {
    let home = home();
    // The fixture sessions are from October 2026, so a one-day window is empty.
    let v = json(agentlens(&home).args(["unused", "--days", "1", "--format", "json"]));
    assert_eq!(v["sessions"], 0);
    let v = json(agentlens(&home).args([
        "unused",
        "--days",
        "1",
        "--since",
        "2026-01-01",
        "--format",
        "json",
    ]));
    assert!(v["sessions"].as_u64().unwrap() > 0);
}

#[test]
fn unused_days_are_counted_back_from_until_and_ignored_with_since() {
    let home = home();
    let sessions = |args: &[&str]| {
        let mut cmd = agentlens(&home);
        cmd.arg("unused").args(args).args(["--format", "json"]);
        json(&mut cmd)["sessions"].clone()
    };
    // One day before 2026-10-02 holds the sessions of October 1; before 2026-10-01 nothing.
    assert_eq!(sessions(&["--days", "1", "--until", "2026-10-02"]), 2);
    assert_eq!(sessions(&["--days", "1", "--until", "2026-10-01"]), 0);
    // The default of 30 days is counted from --until too, not from now: counted from now it would
    // still reach the sessions of October 1 for the first and be empty for the second.
    assert_eq!(sessions(&["--until", "2026-12-31"]), 0);
    assert_eq!(sessions(&["--until", "2026-10-25"]), 2);
    assert_eq!(sessions(&["--days", "100", "--until", "2027-06-01"]), 0);
    // --since wins over --days.
    assert_eq!(
        sessions(&[
            "--days",
            "1",
            "--since",
            "2026-09-01",
            "--until",
            "2026-10-02"
        ]),
        2
    );
}

#[test]
fn flow_growth_reports_measured_context_sizes_and_compactions() {
    let home = home_with_context();
    let v = json(agentlens(&home).args(["flow", "--section", "growth", "--format", "json"]));
    let g = &v["growth"]["sessions"][0];
    assert_eq!(g["requests"], 3);
    assert_eq!(g["first_tokens"], 1010);
    assert_eq!(g["peak_tokens"], 1060);
    assert_eq!(g["last_tokens"], 820);
    assert_eq!(g["compactions"], serde_json::json!([1060]));
    assert_eq!(g["window"], 200_000);
    assert_eq!(g["window_basis"], "documented");
    assert_eq!(g["near_limit"], false);
}

#[test]
fn flow_csv_and_table_have_one_row_per_session() {
    let home = home_with_context();
    let out = agentlens(&home)
        .args(["flow", "--section", "growth", "--format", "csv"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(out).unwrap();
    assert_eq!(text.lines().count(), 2);
    assert!(
        text.lines()
            .nth(1)
            .unwrap()
            .contains(",3,1010,820,1060,1,1060,false")
    );
    agentlens(&home)
        .arg("flow")
        .assert()
        .success()
        .stdout(predicate::str::contains("Context growth"))
        .stdout(predicate::str::contains("1,060"));
}

#[test]
fn flow_errors_section_counts_failures_and_loops() {
    let home = tempfile::tempdir().unwrap();
    let projects = home.path().join(".claude/projects/-work-demo");
    fs::create_dir_all(&projects).unwrap();
    let line = |ts: &str, id: &str, status_error: bool| {
        // An assistant tool call followed by its result.
        format!(
            "{{\"sessionId\":\"e1\",\"cwd\":\"/work/demo\",\"timestamp\":\"{ts}\",\"type\":\"assistant\",\"isSidechain\":false,\"requestId\":\"{id}\",\"message\":{{\"id\":\"m{id}\",\"role\":\"assistant\",\"model\":\"claude-test-1\",\"content\":[{{\"type\":\"tool_use\",\"id\":\"t{id}\",\"name\":\"Bash\",\"input\":{{\"command\":\"make all\"}}}}],\"usage\":{{\"input_tokens\":1,\"output_tokens\":1}}}}}}\n{{\"sessionId\":\"e1\",\"cwd\":\"/work/demo\",\"timestamp\":\"{ts}\",\"type\":\"user\",\"message\":{{\"role\":\"user\",\"content\":[{{\"type\":\"tool_result\",\"tool_use_id\":\"t{id}\",\"is_error\":{status_error},\"content\":\"x\"}}]}}}}\n"
        )
    };
    let body = [
        line("2026-10-03T10:00:01Z", "1", true),
        line("2026-10-03T10:00:02Z", "2", true),
        line("2026-10-03T10:00:03Z", "3", false),
    ]
    .concat();
    fs::write(projects.join("errors.jsonl"), body).unwrap();
    let v = json(agentlens(&home).args(["flow", "--section", "errors", "--format", "json"]));
    let bash = v["errors"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["tool"] == "Bash")
        .unwrap();
    assert_eq!(
        (bash["calls"].as_u64(), bash["errors"].as_u64()),
        (Some(3), Some(2))
    );
    assert_eq!(bash["loops"], 1);
    assert_eq!(v["errors"]["loops"][0]["input"], "make");
    assert!(v.get("growth").is_none());
    agentlens(&home)
        .args(["flow", "--section", "errors"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Retry loops"))
        .stdout(predicate::str::contains("66.7%"));
}

#[test]
fn flow_csv_needs_a_section() {
    let home = home_with_context();
    agentlens(&home)
        .args(["flow", "--format", "csv"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--section"));
}

#[test]
fn flow_hooks_section_sums_runtime_and_lists_sources_without_data() {
    let home = home();
    let dir = home.path().join(".copilot/session-state/cp-1");
    fs::create_dir_all(&dir).unwrap();
    fs::copy(
        fixtures().join("../copilot_cli/with_store.jsonl"),
        dir.join("events.jsonl"),
    )
    .unwrap();
    let v = json(agentlens(&home).args(["flow", "--section", "hooks", "--format", "json"]));
    let hooks = v["hooks"]["hooks"].as_array().unwrap();
    let post = hooks.iter().find(|h| h["hook"] == "postToolUse").unwrap();
    assert_eq!(
        (
            post["runs"].as_u64(),
            post["unknown"].as_u64(),
            post["total_ms"].as_u64()
        ),
        (Some(2), Some(1), Some(250))
    );
    let pre = hooks.iter().find(|h| h["hook"] == "preToolUse").unwrap();
    assert_eq!(
        (pre["failures"].as_u64(), pre["total_ms"].as_u64()),
        (Some(1), Some(500))
    );
    let perms = &v["hooks"]["permissions"][0];
    assert_eq!(
        (
            perms["prompts"].as_u64(),
            perms["answered"].as_u64(),
            perms["total_wait_ms"].as_u64()
        ),
        (Some(2), Some(1), Some(1500))
    );
    assert_eq!(v["hooks"]["unavailable"]["claude-code"], 2);
    agentlens(&home)
        .args(["flow", "--section", "hooks"])
        .assert()
        .success()
        .stdout(predicate::str::contains("claude-code: not available"));
    let csv = agentlens(&home)
        .args(["flow", "--section", "hooks", "--format", "csv"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert!(
        String::from_utf8(csv)
            .unwrap()
            .contains("copilot-cli,preToolUse,1,1,0,500")
    );
}

fn price_file(home: &TempDir) -> std::path::PathBuf {
    let path = home.path().join("prices.json");
    fs::write(
        &path,
        r#"{"claude-test":{"input":1,"output":2,"cache_read":0.5,"cache_write":1.5}}"#,
    )
    .unwrap();
    path
}

#[test]
fn cost_uses_the_price_file_and_marks_computed_values() {
    let home = home_with_context();
    let file = price_file(&home);
    let v = json(
        agentlens(&home)
            .args(["cost", "--format", "json", "--prices"])
            .arg(&file),
    );
    let row = &v["rows"][0];
    assert_eq!(row["key"], "claude-test-1");
    assert_eq!(row["requests"], 3);
    // (40 x 1 + 15 x 2 + 1000 x 0.5 + 1850 x 1.5) / 1e6
    assert!((row["estimated_usd"].as_f64().unwrap() - 0.003345).abs() < 1e-9);
    assert_eq!(row["measured_usd"], 0.0);
    assert_eq!(v["unpriced_models"].as_object().unwrap().len(), 0);
    agentlens(&home)
        .args(["cost", "--prices"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("~$0.00"))
        .stdout(predicate::str::contains("computed from the price table"));
}

#[test]
fn session_filter_limits_cost_to_one_session_and_unknown_ids_are_empty() {
    let home = home();
    let all = json(agentlens(&home).args(["cost", "--format", "json"]));
    let one = json(agentlens(&home).args(["cost", "--format", "json", "--session", "basic"]));
    assert!(one["total"]["requests"].as_u64() < all["total"]["requests"].as_u64());
    assert!(one["total"]["requests"].as_u64() > Some(0));
    let none = json(agentlens(&home).args(["cost", "--format", "json", "--session", "nope"]));
    assert_eq!(none["total"]["requests"], 0);
}

#[test]
fn cost_lists_models_without_a_price_instead_of_guessing() {
    let home = home_with_context();
    let v = json(agentlens(&home).args(["cost", "--format", "json"]));
    assert_eq!(v["unpriced_models"]["claude-test-1"], 3);
    assert_eq!(v["total"]["unpriced_requests"], 3);
    agentlens(&home)
        .arg("cost")
        .assert()
        .success()
        .stdout(predicate::str::contains("unpriced"))
        .stdout(predicate::str::contains("No price for: claude-test-1 (3)"));
}

#[test]
fn cost_csv_and_a_broken_price_file() {
    let home = home_with_context();
    let file = price_file(&home);
    let out = agentlens(&home)
        .args(["cost", "--format", "csv", "--prices"])
        .arg(&file)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(out).unwrap();
    assert!(
        text.lines()
            .next()
            .unwrap()
            .starts_with("model,requests,input_tokens")
    );
    assert!(
        text.lines()
            .nth(1)
            .unwrap()
            .starts_with("claude-test-1,3,40,15,1000,1850,0.000000,0.003345,0,")
    );
    let bad = home.path().join("bad.json");
    fs::write(&bad, "{ nope").unwrap();
    agentlens(&home)
        .args(["cost", "--prices"])
        .arg(&bad)
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot load prices"));
}

#[test]
fn cost_can_be_grouped_by_project_branch_and_issue() {
    let home = tempfile::tempdir().unwrap();
    let projects = home.path().join(".claude/projects/-work-demo");
    fs::create_dir_all(&projects).unwrap();
    let text = fs::read_to_string(fixtures().join("context.jsonl")).unwrap();
    fs::write(
        projects.join("a.jsonl"),
        text.replace("\"main\"", "\"feat/42-login\""),
    )
    .unwrap();
    fs::write(projects.join("b.jsonl"), text.replace("ctx1", "ctx2")).unwrap();
    let file = price_file(&home);
    let keys = |by: &str| -> Vec<String> {
        let v = json(
            agentlens(&home)
                .args(["cost", "--format", "json", "--by", by, "--prices"])
                .arg(&file),
        );
        v["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["key"].as_str().unwrap().to_owned())
            .collect()
    };
    assert_eq!(keys("project"), ["/work/demo"]);
    let mut branches = keys("branch");
    branches.sort();
    assert_eq!(branches, ["feat/42-login", "main"]);
    let mut issues = keys("issue");
    issues.sort();
    assert_eq!(issues, ["#42", "(unassigned)"]);
    let v = json(
        agentlens(&home)
            .args(["cost", "--format", "json", "--by", "issue", "--prices"])
            .arg(&file),
    );
    assert_eq!(v["total"]["requests"], 6);
    agentlens(&home)
        .args(["cost", "--by", "issue", "--prices"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("issue"))
        .stdout(predicate::str::contains("#42"));
    let csv = agentlens(&home)
        .args(["cost", "--format", "csv", "--by", "branch", "--prices"])
        .arg(&file)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert!(
        String::from_utf8(csv)
            .unwrap()
            .starts_with("branch,requests")
    );
}

#[test]
fn cost_by_issue_falls_back_to_the_commit_message_when_the_branch_has_none() {
    let home = tempfile::tempdir().unwrap();
    let projects = home.path().join(".claude/projects/-work-demo");
    fs::create_dir_all(&projects).unwrap();
    let text = fs::read_to_string(fixtures().join("context.jsonl")).unwrap();
    let commit = |id: &str, branch: &str| {
        format!(
            r#"{{"sessionId":"{id}","cwd":"/work/demo","gitBranch":"{branch}","timestamp":"2026-10-02T09:00:40Z","type":"assistant","isSidechain":false,"requestId":"r9","message":{{"id":"m9","role":"assistant","model":"claude-test-1","content":[{{"type":"tool_use","id":"t9","name":"Bash","input":{{"command":"git add a.rs && git commit -m \"feat: x (#502)\""}}}}]}}}}"#
        )
    };
    let on = |id: &str, branch: &str| {
        let body = text
            .replace("ctx1", id)
            .replace("\"main\"", &format!("\"{branch}\""));
        format!("{body}{}\n", commit(id, branch))
    };
    fs::write(projects.join("a.jsonl"), on("ctx1", "release/next")).unwrap();
    fs::write(projects.join("b.jsonl"), on("ctx2", "feat/42-login")).unwrap();
    let file = price_file(&home);
    let by = |by: &str| {
        json(
            agentlens(&home)
                .args(["cost", "--format", "json", "--by", by, "--prices"])
                .arg(&file),
        )
    };
    let issues = by("issue");
    let mut rows: Vec<(String, String)> = issues["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            (
                r["key"].as_str().unwrap().into(),
                r["issue_source"].as_str().unwrap().into(),
            )
        })
        .collect();
    rows.sort();
    assert_eq!(
        rows,
        [
            ("#42".into(), "branch".into()),
            ("#502".into(), "commit".into())
        ],
        "a branch match wins over a commit match"
    );
    let sessions = by("session");
    let mut sessions: Vec<(String, String, String)> = sessions["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            (
                s["session"].as_str().unwrap().into(),
                s["issue"].as_str().unwrap().into(),
                s["issue_source"].as_str().unwrap().into(),
            )
        })
        .collect();
    sessions.sort();
    assert_eq!(
        sessions,
        [
            ("a".into(), "#502".into(), "commit".into()),
            ("b".into(), "#42".into(), "branch".into()),
        ]
    );
    let models = by("model");
    assert!(models["rows"][0].get("issue_source").is_none());
}

#[test]
fn efficiency_reports_cache_hit_rate_and_subagent_share() {
    let home = home_with_context();
    let file = price_file(&home);
    let v = json(
        agentlens(&home)
            .args([
                "efficiency",
                "--format",
                "json",
                "--by",
                "session",
                "--prices",
            ])
            .arg(&file),
    );
    let row = &v["cache"][0];
    assert_eq!(row["key"], "context");
    assert_eq!(
        (
            row["input"].as_u64(),
            row["cache_read"].as_u64(),
            row["cache_write"].as_u64()
        ),
        (Some(40), Some(1000), Some(1850))
    );
    assert!((row["hit_rate"].as_f64().unwrap() - 1000.0 / 2890.0).abs() < 1e-9);
    let share = &v["subagents"][0];
    assert_eq!(
        (
            share["requests"].as_u64(),
            share["subagent_requests"].as_u64()
        ),
        (Some(3), Some(0))
    );
    assert!(
        share["usd"].as_f64().unwrap() > 0.0,
        "the price file prices the fixture model"
    );
    agentlens(&home)
        .args(["efficiency", "--prices"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("34.6%"))
        .stdout(predicate::str::contains("Subagent share"));
}

#[test]
fn efficiency_prints_the_roles_table_and_its_csv() {
    let home = home_with_context();
    let file = price_file(&home);
    agentlens(&home)
        .args(["efficiency", "--prices"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("Roles"))
        .stdout(predicate::str::contains("first request context"))
        .stdout(predicate::str::contains("main"));
    let out = agentlens(&home)
        .args(["efficiency", "--format", "csv", "--csv-table", "roles"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(out).unwrap();
    assert!(text.starts_with("role,kind,runs,requests,requests_per_run,"));
    assert!(
        text.lines()
            .nth(1)
            .unwrap()
            .starts_with("main,main,1,3,3.00,")
    );
}

#[test]
fn efficiency_csv_and_grouping_by_project() {
    let home = home_with_context();
    let out = agentlens(&home)
        .args(["efficiency", "--by", "project", "--format", "csv"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(out).unwrap();
    assert!(text.starts_with(
        "key,source,requests,input_tokens,cache_read_tokens,cache_write_tokens,hit_rate"
    ));
    assert!(
        text.lines()
            .nth(1)
            .unwrap()
            .starts_with("/work/demo,claude-code,3,40,1000,1850,0.346021")
    );
}

#[test]
fn cost_what_if_reprices_the_same_tokens() {
    let home = home_with_context();
    let file = price_file(&home);
    let v = json(
        agentlens(&home)
            .args([
                "cost",
                "--format",
                "json",
                "--what-if",
                "claude-sonnet-5-5",
                "--prices",
            ])
            .arg(&file),
    );
    let w = &v["what_if"];
    assert_eq!(w["target"], "claude-sonnet-5-5");
    // (40 x 2 + 15 x 10 + 1000 x 0.1 + 1850 x 2.5) / 1e6
    assert!((w["total"]["what_if_usd"].as_f64().unwrap() - 0.004955).abs() < 1e-9);
    assert!((w["total"]["actual_usd"].as_f64().unwrap() - 0.003345).abs() < 1e-9);
    assert_eq!(w["total"]["excluded_requests"], 0);
    agentlens(&home)
        .args(["cost", "--what-if", "claude-sonnet-5-5", "--prices"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "What if everything ran on claude-sonnet-5-5?",
        ))
        .stdout(predicate::str::contains("+48%"));
    let csv = agentlens(&home)
        .args([
            "cost",
            "--format",
            "csv",
            "--what-if",
            "claude-sonnet-5-5",
            "--prices",
        ])
        .arg(&file)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert!(
        String::from_utf8(csv)
            .unwrap()
            .starts_with("model,requests,actual_usd,what_if_usd,excluded_requests")
    );
}

#[test]
fn cost_what_if_needs_a_priced_model() {
    let home = home_with_context();
    agentlens(&home)
        .args(["cost", "--what-if", "no-such-model"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("no price for the what-if model"));
}

fn home_with_two_clients() -> TempDir {
    let home = home_with_context();
    let dir = home.path().join(".copilot/session-state/cp-1");
    fs::create_dir_all(&dir).unwrap();
    fs::copy(
        fixtures().join("../copilot_cli/with_store.jsonl"),
        dir.join("events.jsonl"),
    )
    .unwrap();
    home
}

#[test]
fn compare_joins_the_clients_over_the_same_sessions() {
    let home = home_with_two_clients();
    let v = json(agentlens(&home).args(["compare", "--format", "json"]));
    let sources = v["sources"].as_array().unwrap();
    assert_eq!(sources.len(), 2);
    let cc = sources
        .iter()
        .find(|s| s["source"] == "claude-code")
        .unwrap();
    assert_eq!(
        (
            cc["requests"].as_u64(),
            cc["input_tokens"].as_u64(),
            cc["output_tokens"].as_u64()
        ),
        (Some(3), Some(2890), Some(15))
    );
    assert!((cc["cache_hit_rate"].as_f64().unwrap() - 1000.0 / 2890.0).abs() < 1e-9);
    assert!(
        cc["tool_error_rate"].is_null(),
        "no tool call has an outcome in this fixture"
    );
    let cp = sources
        .iter()
        .find(|s| s["source"] == "copilot-cli")
        .unwrap();
    assert_eq!(
        (cp["tool_calls"].as_u64(), cp["tool_errors"].as_u64()),
        (Some(2), Some(1))
    );
    assert!((cp["tool_error_rate"].as_f64().unwrap() - 0.5).abs() < 1e-9);
    assert_eq!(cp["skill_invocations"], 2);
    assert!(
        cp["cache_hit_rate"].is_null(),
        "shutdown totals carry no cache tokens"
    );
}

#[test]
fn compare_table_and_csv_show_missing_values_as_not_available() {
    let home = home_with_two_clients();
    agentlens(&home)
        .arg("compare")
        .assert()
        .success()
        .stdout(predicate::str::contains("claude-code"))
        .stdout(predicate::str::contains("copilot-cli"))
        .stdout(predicate::str::contains("n/a"))
        .stdout(predicate::str::contains("skill overhead (est.)"));
    let out = agentlens(&home)
        .args(["compare", "--format", "csv", "--source", "copilot-cli"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(out).unwrap();
    assert_eq!(text.lines().count(), 2, "only the selected source");
    assert!(
        text.lines()
            .next()
            .unwrap()
            .starts_with("source,sessions,requests")
    );
    assert!(text.lines().nth(1).unwrap().starts_with("copilot-cli,1,1,"));
}

fn otel_fixture() -> std::path::PathBuf {
    fixtures().join("../otel/claude.jsonl")
}

#[test]
fn import_otel_reports_what_the_export_holds() {
    let home = home();
    let v = json(
        agentlens(&home)
            .args(["import-otel", "--format", "json"])
            .arg(otel_fixture()),
    );
    assert_eq!(v["sessions"], 2);
    assert_eq!(v["requests"], 2);
    assert_eq!(v["skill_invocations"], 1);
    assert_eq!(v["tool_calls"], 2);
    assert_eq!(v["also_in_local_data"], 0);
    assert!(
        v["warnings"][0]
            .as_str()
            .unwrap()
            .contains("resourceMetrics")
    );
    agentlens(&home)
        .arg("import-otel")
        .arg(otel_fixture())
        .assert()
        .success()
        .stdout(predicate::str::contains("also in local data"));
    agentlens(&home)
        .args(["import-otel", "/no/such/file"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("is not a file"));
}

#[test]
fn otel_flag_adds_measured_skill_attribution_to_the_skills_report() {
    let home = home();
    let v = json(
        agentlens(&home)
            .args([
                "skills",
                "--since",
                "2026-01-01",
                "--format",
                "json",
                "--otel",
            ])
            .arg(otel_fixture()),
    );
    let demo = v["skills"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "demo")
        .unwrap();
    assert_eq!(demo["attributed"]["requests"], 1);
    assert_eq!(demo["attributed"]["input"], 100);
    assert_eq!(demo["attributed"]["cache_read"], 1000);
    agentlens(&home)
        .args(["skills", "--since", "2026-01-01", "--otel"])
        .arg(otel_fixture())
        .assert()
        .success()
        .stdout(predicate::str::contains("attributed"));
    // Without the flag nothing is attributed and the column is absent.
    agentlens(&home)
        .args(["skills", "--since", "2026-01-01"])
        .assert()
        .success()
        .stdout(predicate::str::contains("attributed").not());
}

#[test]
fn otel_cost_comes_from_the_export_as_measured_cost() {
    let home = tempfile::tempdir().unwrap();
    let v = json(
        agentlens(&home)
            .args([
                "cost",
                "--since",
                "2026-01-01",
                "--format",
                "json",
                "--source",
                "otel",
                "--otel",
            ])
            .arg(otel_fixture()),
    );
    assert!((v["total"]["measured_usd"].as_f64().unwrap() - 0.30).abs() < 1e-9);
    assert_eq!(v["total"]["unpriced_requests"], 0);
}

/// Answers every request with `{"input_tokens": 11}`; returns its URL and a request counter.
fn count_tokens_mock() -> (String, std::sync::Arc<std::sync::atomic::AtomicU64>) {
    use std::io::{Read, Write};
    use std::sync::atomic::{AtomicU64, Ordering};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let hits = std::sync::Arc::new(AtomicU64::new(0));
    let counter = hits.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut s) = stream else { break };
            let mut seen = Vec::new();
            let mut buf = [0u8; 8192];
            loop {
                let n = s.read(&mut buf).unwrap_or(0);
                if n == 0 {
                    break;
                }
                seen.extend_from_slice(&buf[..n]);
                let text = String::from_utf8_lossy(&seen).to_string();
                if let Some(h) = text.find("\r\n\r\n") {
                    let len = text[..h]
                        .lines()
                        .find_map(|l| {
                            l.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .and_then(|v| v.trim().parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    if seen.len() >= h + 4 + len {
                        break;
                    }
                }
            }
            counter.fetch_add(1, Ordering::SeqCst);
            let body = r#"{"input_tokens": 11}"#;
            let _ = write!(
                s,
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    (url, hits)
}

#[test]
fn exact_without_a_key_stops_with_a_clear_message() {
    let home = home();
    agentlens(&home)
        .args(["--exact", "skills", "--format", "json"])
        .assert()
        .failure()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("ANTHROPIC_API_KEY"));
}

#[test]
fn exact_model_needs_exact() {
    let home = home();
    agentlens(&home)
        .args(["--exact-model", "m", "skills"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--exact"));
}

#[test]
fn exact_counts_skills_with_the_api_and_caches_them() {
    let home = home();
    let (url, hits) = count_tokens_mock();
    let run = || {
        json(
            agentlens(&home)
                .env("ANTHROPIC_API_KEY", "test-key")
                .env("ANTHROPIC_BASE_URL", &url)
                .args(["--exact", "--format", "json", "skills"]),
        )
    };
    let first = run();
    let skills = first["skills"].as_array().unwrap();
    let demo = skills.iter().find(|s| s["name"] == "demo").unwrap();
    assert_eq!(demo["baseline"]["tokens"]["measured"], true);
    // The mock counts 11 tokens for the listing; the baseline is that times the requests.
    let requests = demo["baseline"]["requests"].as_u64().unwrap();
    assert!(requests > 0);
    assert_eq!(
        demo["baseline"]["tokens"]["count"].as_u64().unwrap(),
        11 * requests
    );
    let calls = hits.load(std::sync::atomic::Ordering::SeqCst);
    assert!(calls > 0);

    run();
    assert_eq!(
        hits.load(std::sync::atomic::Ordering::SeqCst),
        calls,
        "the second run is answered from the cache"
    );
}

#[test]
fn without_exact_the_counts_are_estimates() {
    let home = home();
    let v = json(agentlens(&home).args(["--format", "json", "skills"]));
    let demo = v["skills"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "demo")
        .unwrap();
    assert_eq!(demo["baseline"]["tokens"]["measured"], false);
}

// ---- mcp-setup ----

/// A file standing in for `agentlens-mcp`.
fn fake_mcp_binary(dir: &TempDir) -> std::path::PathBuf {
    let path = dir.path().join("agentlens-mcp");
    std::fs::write(&path, "").unwrap();
    path
}

#[test]
fn mcp_setup_shows_what_to_add_and_changes_nothing() {
    let home = tempfile::tempdir().unwrap();
    let bin = fake_mcp_binary(&home);
    let before: Vec<_> = std::fs::read_dir(home.path()).unwrap().collect();
    agentlens(&home)
        .args(["mcp-setup", "--binary"])
        .arg(&bin)
        .assert()
        .success()
        .stdout(predicates::str::contains("Claude Code"))
        .stdout(predicates::str::contains(
            "claude mcp add --scope user agentlens --",
        ))
        .stdout(predicates::str::contains("\"servers\""))
        .stdout(predicates::str::contains("\"mcpServers\""))
        .stdout(predicates::str::contains("\"mcp\""))
        .stdout(predicates::str::contains("Nothing was changed"));
    assert_eq!(
        std::fs::read_dir(home.path()).unwrap().count(),
        before.len()
    );
}

#[test]
fn mcp_setup_registers_is_idempotent_and_removes() {
    let home = tempfile::tempdir().unwrap();
    let bin = fake_mcp_binary(&home);
    let file = home.path().join(".copilot/mcp-config.json");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(
        &file,
        r#"{"mcpServers":{"other":{"type":"http","url":"u","tools":["*"]}}}"#,
    )
    .unwrap();

    let setup = |extra: &[&str]| {
        let mut cmd = agentlens(&home);
        cmd.args(["mcp-setup", "--client", "copilot-cli", "--yes", "--binary"])
            .arg(&bin)
            .args(extra);
        cmd
    };
    setup(&["--write"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Copilot CLI: added"))
        .stdout(predicates::str::contains("backup:"));
    let doc: Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(doc["mcpServers"]["agentlens"]["type"], "local");
    assert_eq!(doc["mcpServers"]["other"]["url"], "u");

    setup(&["--write"])
        .assert()
        .success()
        .stdout(predicates::str::contains("nothing changed"));

    setup(&["--remove"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Copilot CLI: removed"));
    let doc: Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    assert!(doc["mcpServers"].get("agentlens").is_none());
    assert_eq!(doc["mcpServers"]["other"]["url"], "u");
}

#[test]
fn mcp_setup_leaves_a_file_it_cannot_read_alone_and_says_so() {
    let home = tempfile::tempdir().unwrap();
    let bin = fake_mcp_binary(&home);
    let file = home.path().join(".config/Code/User/mcp.json");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    let text = "{\n  // my servers\n  \"servers\": {}\n}\n";
    std::fs::write(&file, text).unwrap();
    agentlens(&home)
        .args(["mcp-setup", "--write", "--yes", "--binary"])
        .arg(&bin)
        .assert()
        .failure()
        .stderr(predicates::str::contains("left untouched"))
        .stderr(predicates::str::contains("could not be changed"));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), text);
}

#[test]
fn mcp_setup_asks_before_writing_and_only_touches_found_clients() {
    let home = tempfile::tempdir().unwrap();
    let bin = fake_mcp_binary(&home);
    // Nothing installed: nothing to do.
    agentlens(&home)
        .args(["mcp-setup", "--write", "--yes", "--binary"])
        .arg(&bin)
        .assert()
        .success()
        .stdout(predicates::str::contains("No supported client was found"));
    // A client is there, but there is no terminal to ask in and no --yes.
    std::fs::create_dir_all(home.path().join(".copilot")).unwrap();
    agentlens(&home)
        .args(["mcp-setup", "--write", "--binary"])
        .arg(&bin)
        .assert()
        .failure()
        .stderr(predicates::str::contains("--yes"));
    assert!(!home.path().join(".copilot/mcp-config.json").exists());
}

#[cfg(unix)]
#[test]
fn mcp_setup_registers_claude_code_with_its_own_command() {
    use std::os::unix::fs::PermissionsExt;
    let home = tempfile::tempdir().unwrap();
    let bin = fake_mcp_binary(&home);
    // A `claude` that only records how it was called.
    let tools = tempfile::tempdir().unwrap();
    let log = tools.path().join("calls.log");
    let claude = tools.path().join("claude");
    std::fs::write(
        &claude,
        format!("#!/bin/sh\necho \"$@\" >> {}\n", log.display()),
    )
    .unwrap();
    std::fs::set_permissions(&claude, std::fs::Permissions::from_mode(0o755)).unwrap();
    let path = format!(
        "{}:{}",
        tools.path().display(),
        std::env::var("PATH").unwrap()
    );

    agentlens(&home)
        .env("PATH", &path)
        .args([
            "mcp-setup",
            "--client",
            "claude-code",
            "--write",
            "--yes",
            "--binary",
        ])
        .arg(&bin)
        .assert()
        .success()
        .stdout(predicates::str::contains("Claude Code: registered"));
    agentlens(&home)
        .env("PATH", &path)
        .args(["mcp-setup", "--client", "claude-code", "--remove", "--yes"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Claude Code: removed"));
    let calls = std::fs::read_to_string(&log).unwrap();
    let lines: Vec<&str> = calls.lines().collect();
    assert_eq!(
        lines[0],
        format!("mcp add --scope user agentlens -- {}", bin.display())
    );
    assert_eq!(lines[1], "mcp remove --scope user agentlens");
}

// ---- skills used through their files ----

fn home_with_skill_files() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    let projects = dir.path().join(".claude/projects/-work-demo");
    fs::create_dir_all(&projects).unwrap();
    fs::copy(
        fixtures().join("skill_files.jsonl"),
        projects.join("skill_files.jsonl"),
    )
    .unwrap();
    for name in ["demo", "idle"] {
        let d = dir.path().join(format!(".claude/skills/{name}"));
        fs::create_dir_all(&d).unwrap();
        fs::write(
            d.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: Does {name} things\n---\nbody"),
        )
        .unwrap();
    }
    dir
}

#[test]
fn a_skill_used_only_through_its_files_is_not_unused() {
    let home = home_with_skill_files();
    // The skills view shows the file uses: two tool calls on the files of `demo`, none on `idle`.
    let v = json(agentlens(&home).args(["skills", "--format", "json"]));
    let skill = |name: &str| {
        v["skills"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["name"] == name)
            .unwrap_or_else(|| panic!("{name} missing"))
            .clone()
    };
    assert_eq!(skill("demo")["file_uses"], 2);
    assert_eq!(skill("demo")["invocations"], 0);
    assert_eq!(skill("idle")["file_uses"], 0);
    assert!(
        v["skills"]
            .as_array()
            .unwrap()
            .iter()
            .all(|s| s["name"] != "skills.ts")
    );
    agentlens(&home)
        .arg("skills")
        .assert()
        .success()
        .stdout(predicate::str::contains("file uses"));

    // `unused` lists the idle skill and not the one whose files are in use.
    let u = json(agentlens(&home).args(["unused", "--format", "json"]));
    let names: Vec<&str> = u["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|i| i["kind"] == "skill")
        .map(|i| i["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["idle"]);
    // The CSV keeps its columns and adds the count at the end.
    let out = agentlens(&home)
        .args(["skills", "--format", "csv"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let header = String::from_utf8(out).unwrap();
    assert!(
        header
            .lines()
            .next()
            .unwrap()
            .ends_with(",overhead_measured,file_uses")
    );
}

// ---- advise ----

#[test]
fn advise_lists_what_to_change_in_json_csv_and_a_table() {
    let home = home();
    let v = json(agentlens(&home).args(["advise", "--format", "json"]));
    assert_eq!(v["installed_skills"], 3);
    assert_eq!(
        v["baseline_total"]["measured"], false,
        "descriptions are estimated"
    );
    let findings = v["findings"].as_array().unwrap();
    let unused = findings
        .iter()
        .find(|f| f["kind"] == "unused-skills")
        .expect("an installed skill was never used");
    let names: Vec<&str> = unused["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"unused"), "{names:?}");
    assert!(!names.contains(&"demo"), "demo was loaded");
    assert!(unused["explanation"].as_str().unwrap().len() > 20);
    assert!(unused["items"][0]["tokens"]["measured"].is_boolean());

    let out = agentlens(&home)
        .args(["advise", "--format", "csv"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(out).unwrap();
    assert_eq!(
        text.lines().next().unwrap(),
        "finding,skill,tokens,tokens_measured,requests,detail,rank,saving_tokens,saving_usd,confidence"
    );
    assert!(text.contains("unused-skills,unused,"));
    let unused_row = text
        .lines()
        .find(|l| l.starts_with("unused-skills,unused,"))
        .unwrap();
    assert!(
        unused_row.ends_with(",estimated"),
        "estimated listings give an estimated saving: {unused_row}"
    );

    agentlens(&home)
        .arg("advise")
        .assert()
        .success()
        .stdout(predicate::str::contains("Unused skills"))
        .stdout(predicate::str::contains("What to do:"))
        .stdout(predicate::str::contains("Could save ~"))
        .stdout(predicate::str::contains("ranked by what they could save"))
        .stdout(predicate::str::contains("~ estimated"));
    let ranks: Vec<u64> = findings
        .iter()
        .map(|f| f["rank"].as_u64().unwrap())
        .collect();
    assert_eq!(ranks, (1..=findings.len() as u64).collect::<Vec<_>>());
    assert!(unused["impact"]["saving"]["count"].as_u64().unwrap() > 0);
}

#[test]
fn advise_falls_back_to_the_bundled_prices_when_the_price_file_is_broken() {
    let home = home();
    let file = home.path().join(".config/agentlens/prices.json");
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(&file, "{ nope").unwrap();
    let out = agentlens(&home)
        .args(["advise", "--format", "json"])
        .assert()
        .success()
        .stderr(predicate::str::contains("warning: cannot load prices"))
        .stderr(predicate::str::contains("prices.json"))
        .stderr(predicate::str::contains("the bundled prices are used"))
        .get_output()
        .stdout
        .clone();
    let v: Value = serde_json::from_slice(&out).expect("valid JSON on stdout");
    assert!(!v["findings"].as_array().unwrap().is_empty());
}

#[test]
fn advise_has_nothing_to_say_without_sessions() {
    let home = tempfile::tempdir().unwrap();
    agentlens(&home)
        .arg("advise")
        .assert()
        .success()
        .stdout(predicate::str::contains("Nothing to suggest"));
}

// ---- typed skill commands ----

#[test]
fn a_skill_the_user_typed_counts_as_a_user_trigger() {
    let dir = tempfile::tempdir().unwrap();
    let projects = dir.path().join(".claude/projects/-work-demo");
    fs::create_dir_all(&projects).unwrap();
    fs::copy(
        fixtures().join("typed_skill.jsonl"),
        projects.join("typed_skill.jsonl"),
    )
    .unwrap();
    let skill = dir.path().join(".claude/skills/cliff-setup");
    fs::create_dir_all(&skill).unwrap();
    fs::write(
        skill.join("SKILL.md"),
        "---\nname: cliff-setup\ndescription: Sets up a changelog\n---\nbody",
    )
    .unwrap();
    let v = json(agentlens(&dir).args(["skills", "--format", "json"]));
    let cliff = v["skills"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["name"] == "cliff-setup")
        .expect("cliff-setup");
    assert_eq!(cliff["invocations"], 1);
    assert_eq!(cliff["triggers"]["user"], 1);
    assert_eq!(cliff["triggers"]["model"], 0);
    // Built-in commands such as /statusline and /clear are neither skills nor ignored commands.
    assert!(v["ignored_commands"].as_array().unwrap().is_empty());
}

#[test]
fn rounded_shows_token_counts_in_text_tables_and_keeps_json_exact() {
    let home = home_with_context();
    let file = price_file(&home);
    agentlens(&home)
        .args(["cost", "--rounded", "--prices"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("2.89 k").and(predicate::str::contains("2,890").not()));
    agentlens(&home)
        .args(["cost", "--prices"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("2,890").and(predicate::str::contains("2.89 k").not()));
    let v = json(
        agentlens(&home)
            .args(["cost", "--rounded", "--format", "json", "--prices"])
            .arg(&file),
    );
    assert_eq!(v["rows"][0]["cache_write"], 1850);
    agentlens(&home)
        .args(["cost", "--rounded", "--format", "csv", "--prices"])
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("1850").and(predicate::str::contains(" k").not()));
}

/// A home with the context, skills, basic and subagent fixtures: counts above 1,000 in several analyses.
fn home_with_busy_sessions() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    let projects = dir.path().join(".claude/projects/-work-demo");
    fs::create_dir_all(&projects).unwrap();
    for f in [
        "context.jsonl",
        "skills.jsonl",
        "basic.jsonl",
        "main_with_subagent.jsonl",
        "subagent.jsonl",
    ] {
        fs::copy(fixtures().join(f), projects.join(f)).unwrap();
    }
    dir
}

#[test]
fn rounded_applies_to_compare_flow_advise_and_context_text_only() {
    let home = home_with_busy_sessions();
    // (command, exact text, rounded text)
    let cases = [
        ("compare", "4,441", "4.44 k"),
        ("flow", "1,010", "1.01 k"),
        ("advise", "4,504", "4.50 k"),
        ("context", "1,010", "1.01 k"),
    ];
    for (command, exact, rounded) in cases {
        agentlens(&home)
            .arg(command)
            .assert()
            .success()
            .stdout(predicate::str::contains(exact));
        agentlens(&home)
            .args([command, "--rounded"])
            .assert()
            .success()
            .stdout(predicate::str::contains(rounded).and(predicate::str::contains(exact).not()));
        let plain = json(agentlens(&home).args([command, "--format", "json"]));
        let with_flag = json(agentlens(&home).args([command, "--rounded", "--format", "json"]));
        assert_eq!(plain, with_flag, "{command}: JSON must not change");
    }
}

#[test]
fn the_binary_carries_the_marker_of_the_command_line_tools() {
    // The desktop application recognizes an old copy in ~/.local/bin by it (without running it).
    let bin = std::path::Path::new(env!("CARGO_BIN_EXE_agentlens"));
    assert!(agentlens_core::cli_tools::has_marker(bin));
}

#[test]
fn cost_by_session_has_a_row_per_session_and_top_limits_only_the_table() {
    let home = home();
    let file = price_file(&home);
    let v = json(
        agentlens(&home)
            .args(["cost", "--format", "json", "--by", "session", "--prices"])
            .arg(&file),
    );
    let sessions = v["sessions"].as_array().unwrap();
    assert_eq!(sessions.len(), 2);
    for s in sessions {
        assert_eq!(s["project"], "/work/demo");
        assert!(s["peak_context"].as_u64().unwrap() > 0);
    }
    let csv = agentlens(&home)
        .args(["cost", "--format", "csv", "--by", "session", "--prices"])
        .arg(&file)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert_eq!(String::from_utf8(csv).unwrap().lines().count(), 3);
    let table = agentlens(&home)
        .args(["cost", "--by", "session", "--top", "1", "--prices"])
        .arg(&file)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let table = String::from_utf8(table).unwrap();
    assert!(table.contains("subagent share"), "{table}");
    assert!(table.contains("1 more rows"), "{table}");
}
