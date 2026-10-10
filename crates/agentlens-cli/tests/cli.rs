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
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
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
    assert_eq!(srv["tokens"], 8);
    assert_eq!(srv["requests"], 6, "summed over both items");
    assert_eq!(srv["overhead_tokens"], 24);
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
        "source,project,kind,owner,items,bytes,tokens,requests,overhead_tokens"
    );
    assert_eq!(
        lines.next().unwrap(),
        "claude-code,/work/demo,instruction-file,/work/demo/CLAUDE.md,1,400,100,3,300"
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
    assert_eq!(agent["overhead_tokens"], 15);
    let server = find("mcp-server", "srv");
    assert_eq!(server["uses"], 0);
    // Listings 2 x 4 tokens, definition 38 and instructions 75 tokens, three requests each.
    assert_eq!(server["overhead_tokens"], 363);
    assert!(!v.to_string().contains("never-read"));
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
