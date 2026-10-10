use std::fs;
use std::path::Path;

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;
use tempfile::TempDir;

fn home() -> TempDir {
    let fixtures = Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../agentlens-core/tests/fixtures/claude_code"
    ));
    let dir = tempfile::tempdir().unwrap();
    let projects = dir.path().join(".claude/projects/-work-demo");
    fs::create_dir_all(&projects).unwrap();
    fs::copy(fixtures.join("skills.jsonl"), projects.join("skills.jsonl")).unwrap();
    let skill = dir.path().join(".claude/skills/demo");
    fs::create_dir_all(&skill).unwrap();
    fs::write(
        skill.join("SKILL.md"),
        "---\nname: demo\ndescription: Does demo things\n---\nbody",
    )
    .unwrap();
    dir
}

fn command(home: &TempDir) -> Command {
    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("agentlens");
    cmd.env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .env_remove("CLAUDE_CONFIG_DIR")
        .env_remove("COPILOT_HOME")
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("XDG_DATA_HOME");
    cmd
}

/// The JSON the page embeds in its data script element.
fn embedded(page: &str) -> Value {
    let start = page
        .find("<script id=\"data\" type=\"application/json\">")
        .unwrap();
    let rest = &page[start..];
    let json = &rest[rest.find('>').unwrap() + 1..rest.find("</script>").unwrap()];
    serde_json::from_str(json).unwrap()
}

#[test]
fn html_report_embeds_the_data_and_needs_no_network() {
    let home = home();
    let out = home.path().join("report.html");
    command(&home)
        .args(["report", "--html"])
        .arg(&out)
        .assert()
        .success()
        .stdout(predicate::str::contains("wrote"));
    let page = fs::read_to_string(&out).unwrap();
    assert!(page.starts_with("<!doctype html>"));
    assert!(!page.contains("__REPORT_DATA__"));
    assert!(!page.contains("src=\"http"));
    let data = embedded(&page);
    assert_eq!(data["meta"]["schema_version"], 1);
    assert!(data["skills"]["sessions"].as_u64().unwrap() >= 1);
    let skills = data["skills"]["skills"].as_array().unwrap();
    assert!(skills.iter().any(|s| s["name"] == "demo"));
    assert!(
        skills
            .iter()
            .any(|s| s["uses_by_day"].as_object().is_some_and(|d| !d.is_empty()))
    );
}

#[test]
fn report_without_html_flag_is_a_usage_error() {
    let home = home();
    command(&home).arg("report").assert().failure().code(2);
}

#[test]
fn unwritable_target_fails_with_a_message() {
    let home = home();
    command(&home)
        .args(["report", "--html"])
        .arg(home.path().join("missing-dir/report.html"))
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot write"));
}
