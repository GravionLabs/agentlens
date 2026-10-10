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
