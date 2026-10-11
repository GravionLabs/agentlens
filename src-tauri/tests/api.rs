use std::fs;
use std::path::Path;

use agentlens_app_lib::api::{self, Context, ExportOptions, Filters};
use agentlens_core::adapters::default_registry;
use agentlens_core::env::Env;
use serde_json::Value;
use tempfile::TempDir;

fn fixtures() -> &'static Path {
    Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../crates/agentlens-core/tests/fixtures"
    ))
}

/// A home with a Claude Code session, a Copilot CLI session and one installed skill.
fn setup() -> (TempDir, Context) {
    let dir = tempfile::tempdir().unwrap();
    let projects = dir.path().join(".claude/projects/-work-demo");
    fs::create_dir_all(&projects).unwrap();
    fs::copy(
        fixtures().join("claude_code/context.jsonl"),
        projects.join("context.jsonl"),
    )
    .unwrap();
    let copilot = dir.path().join(".copilot/session-state/cp-1");
    fs::create_dir_all(&copilot).unwrap();
    fs::copy(
        fixtures().join("copilot_cli/with_store.jsonl"),
        copilot.join("events.jsonl"),
    )
    .unwrap();
    let skill = dir.path().join(".claude/skills/demo");
    fs::create_dir_all(&skill).unwrap();
    fs::write(
        skill.join("SKILL.md"),
        "---\nname: demo\ndescription: Does demo things\n---\nbody",
    )
    .unwrap();
    let ctx = Context {
        env: Env::for_home(dir.path()),
        registry: default_registry(),
    };
    (dir, ctx)
}

fn json<T: serde::Serialize>(v: &T) -> Value {
    serde_json::to_value(v).unwrap()
}

fn keys(v: &Value) -> Vec<&str> {
    let mut k: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
    k.sort();
    k
}

fn claude() -> Filters {
    Filters {
        sources: vec!["claude-code".into()],
        ..Filters::default()
    }
}

#[test]
fn overview_lists_every_source_with_its_session_count() {
    let (_dir, ctx) = setup();
    let a = api::overview(&ctx, &Filters::default()).unwrap();
    let find = |id: &str| a.data.iter().find(|s| json(&s.source) == id).unwrap();
    assert!(find("claude-code").found && find("claude-code").sessions == 1);
    assert!(find("copilot-cli").found && find("copilot-cli").sessions == 1);
    assert!(!find("opencode").found);
    assert_eq!(keys(&json(&a)), ["data", "errors", "warnings"]);
}

#[test]
fn filters_restrict_sources_and_report_bad_input_readably() {
    let (_dir, ctx) = setup();
    let only = api::overview(&ctx, &claude()).unwrap();
    assert_eq!(only.data.len(), 1);
    let err = api::overview(
        &ctx,
        &Filters {
            since: Some("soon".into()),
            ..Filters::default()
        },
    )
    .unwrap_err();
    assert!(err.contains("cannot parse"), "{err}");
    let err = api::overview(
        &ctx,
        &Filters {
            sources: vec!["nope".into()],
            ..Filters::default()
        },
    )
    .unwrap_err();
    assert!(err.contains("unknown source"), "{err}");
    // A period that excludes the fixtures leaves nothing.
    let none = api::overview(
        &ctx,
        &Filters {
            since: Some("1h".into()),
            ..Filters::default()
        },
    )
    .unwrap();
    assert!(none.data.iter().all(|s| s.sessions == 0));
}

#[test]
fn skills_context_and_flow_return_the_report_types_of_the_core() {
    let (_dir, ctx) = setup();
    let skills = api::skills(&ctx, &claude()).unwrap();
    assert_eq!(skills.data.installed_skills, 1);
    assert_eq!(skills.data.report.sessions, 1);
    let v = json(&skills);
    assert_eq!(
        keys(&v["data"]),
        ["duplicates", "installed_skills", "report"]
    );
    assert!(keys(&v["data"]["report"]).contains(&"skills"));

    let context = api::context(&ctx, &claude()).unwrap();
    assert!(context.data.rows.iter().any(|r| r.owner == "srv"));
    assert_eq!(
        keys(&json(&context)["data"]),
        ["first_request", "rows", "sessions", "unavailable"]
    );

    let flow = api::flow(&ctx, &claude()).unwrap();
    assert_eq!(keys(&json(&flow)["data"]), ["errors", "growth", "hooks"]);
    assert_eq!(flow.data.growth.sessions[0].peak_tokens, 1060);
}

#[test]
fn cost_efficiency_and_compare_group_as_asked() {
    let (_dir, ctx) = setup();
    let cost = api::cost(&ctx, &Filters::default(), "project", None).unwrap();
    assert!(cost.data.report.rows.iter().any(|r| r.key == "/work/demo"));
    assert!(cost.data.what_if.is_none());
    let err = api::cost(&ctx, &Filters::default(), "model", Some("no-such-model")).unwrap_err();
    assert!(err.contains("no price for the what-if model"));
    let err = api::cost(&ctx, &Filters::default(), "weekday", None).unwrap_err();
    assert!(err.contains("unknown grouping"));
    let what_if = api::cost(&ctx, &Filters::default(), "model", Some("claude-haiku-4-5")).unwrap();
    assert_eq!(what_if.data.what_if.unwrap().target, "claude-haiku-4-5");

    let eff = api::efficiency(&ctx, &claude(), "session").unwrap();
    assert_eq!(eff.data.cache[0].key, "context");
    assert!(api::efficiency(&ctx, &claude(), "weekday").is_err());

    let cmp = api::compare(&ctx, &Filters::default()).unwrap();
    assert_eq!(cmp.data.sources.len(), 2);
}

#[test]
fn unused_defaults_to_thirty_days_and_lists_installed_items() {
    let (_dir, ctx) = setup();
    let a = api::unused(&ctx, &claude()).unwrap();
    assert!(a.data.items.iter().any(|i| i.name == "demo"));
}

#[test]
fn sessions_and_timeline_follow_each_other() {
    let (_dir, ctx) = setup();
    let list = api::sessions(&ctx, &claude()).unwrap();
    assert_eq!(list.data.len(), 1);
    let s = &list.data[0];
    assert_eq!(
        (s.id.as_str(), s.requests, s.peak_tokens),
        ("context", 3, 1060)
    );
    let t = api::timeline(&ctx, &claude(), "claude-code", "context").unwrap();
    assert_eq!(t.data.points.len(), 3);
    assert_eq!(t.data.markers.len(), 1, "one compaction");
    assert_eq!(
        keys(&json(&t)["data"]),
        ["markers", "points", "session", "thinned_from"]
    );
    assert!(
        api::timeline(&ctx, &claude(), "claude-code", "missing")
            .unwrap_err()
            .contains("not found")
    );
    assert!(
        api::timeline(&ctx, &claude(), "nope", "x")
            .unwrap_err()
            .contains("unknown source")
    );
}

/// The JSON the frontend receives is a contract: the TypeScript types mirror it
/// and its unit tests read these samples. Run with `UPDATE_SAMPLES=1` to rewrite them.
#[test]
fn json_samples_for_the_frontend_are_up_to_date() {
    let (dir, ctx) = setup();
    let all = Filters::default();
    let samples: Vec<(&str, String)> = vec![
        (
            "overview",
            serde_json::to_string_pretty(&api::overview(&ctx, &all).unwrap()).unwrap(),
        ),
        (
            "skills",
            serde_json::to_string_pretty(&api::skills(&ctx, &all).unwrap()).unwrap(),
        ),
        (
            "context",
            serde_json::to_string_pretty(&api::context(&ctx, &all).unwrap()).unwrap(),
        ),
        (
            "unused",
            serde_json::to_string_pretty(&api::unused(&ctx, &all).unwrap()).unwrap(),
        ),
        (
            "flow",
            serde_json::to_string_pretty(&api::flow(&ctx, &all).unwrap()).unwrap(),
        ),
        (
            "cost",
            serde_json::to_string_pretty(
                &api::cost(&ctx, &all, "model", Some("claude-haiku-4-5")).unwrap(),
            )
            .unwrap(),
        ),
        (
            "efficiency",
            serde_json::to_string_pretty(&api::efficiency(&ctx, &all, "source").unwrap()).unwrap(),
        ),
        (
            "compare",
            serde_json::to_string_pretty(&api::compare(&ctx, &all).unwrap()).unwrap(),
        ),
        (
            "sessions",
            serde_json::to_string_pretty(&api::sessions(&ctx, &all).unwrap()).unwrap(),
        ),
        (
            "timeline",
            serde_json::to_string_pretty(
                &api::timeline(&ctx, &claude(), "claude-code", "context").unwrap(),
            )
            .unwrap(),
        ),
    ];
    let home = dir.path().display().to_string().replace('\\', "\\\\");
    let target = Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/app/core/samples");
    for (name, text) in samples {
        let text = text.replace(&home, "/home/user") + "\n";
        let path = target.join(format!("{name}.json"));
        if std::env::var_os("UPDATE_SAMPLES").is_some() {
            fs::create_dir_all(&target).unwrap();
            fs::write(&path, &text).unwrap();
        } else {
            let have = fs::read_to_string(&path).unwrap_or_else(|_| {
                panic!("{} is missing; run with UPDATE_SAMPLES=1", path.display())
            });
            assert_eq!(
                have, text,
                "{name}.json is outdated; run with UPDATE_SAMPLES=1"
            );
        }
    }
}

#[test]
fn exports_are_the_json_the_cli_prints_and_the_report_file() {
    let (_dir, ctx) = setup();
    let none = ExportOptions::default();

    // The skills JSON is the report of `agentlens skills --format json`.
    let file = api::export(&ctx, "skills", "json", &claude(), &none).unwrap();
    let shown = api::skills(&ctx, &claude()).unwrap();
    let parsed: Value = serde_json::from_str(&file.content).unwrap();
    assert_eq!(parsed, json(&shown.data.report));
    assert!(file.suggested_name.starts_with("agentlens-skills-"));
    assert!(file.suggested_name.ends_with(".json"));

    // The HTML is the report file: the data is embedded and the placeholder is gone.
    let html = api::export(&ctx, "skills", "html", &claude(), &none).unwrap();
    assert!(html.content.starts_with("<!doctype html>") || html.content.contains("<html"));
    assert!(!html.content.contains("__REPORT_DATA__"));
    assert!(html.suggested_name.starts_with("agentlens-report-"));
    assert_eq!(
        api::export_file_name("skills", "html")[..17],
        html.suggested_name[..17]
    );

    // Every other view exports what it shows.
    for view in [
        "overview",
        "context",
        "unused",
        "flow",
        "compare",
        "cost",
        "efficiency",
    ] {
        let file = api::export(&ctx, view, "json", &claude(), &none).unwrap();
        serde_json::from_str::<Value>(&file.content).unwrap();
    }
    let by_project = ExportOptions {
        by: Some("project".into()),
        ..ExportOptions::default()
    };
    let cost = api::export(&ctx, "cost", "json", &claude(), &by_project).unwrap();
    let shown = api::cost(&ctx, &claude(), "project", None).unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&cost.content).unwrap(),
        json(&shown.data)
    );
}

#[test]
fn an_export_that_cannot_be_made_says_why() {
    let (_dir, ctx) = setup();
    let none = ExportOptions::default();
    let err = api::export(&ctx, "context", "html", &claude(), &none).unwrap_err();
    assert!(err.contains("only the skills view"), "{err}");
    let err = api::export(&ctx, "context", "xml", &claude(), &none).unwrap_err();
    assert!(err.contains("unknown format"), "{err}");
    let err = api::export(&ctx, "nothing", "json", &claude(), &none).unwrap_err();
    assert!(err.contains("cannot be exported"), "{err}");
}

#[test]
fn csv_exports_have_the_columns_of_the_cli() {
    let (_dir, ctx) = setup();
    let none = ExportOptions::default();
    let header = |view: &str, options: &ExportOptions| {
        let file = api::export(&ctx, view, "csv", &claude(), options).unwrap();
        assert!(file.suggested_name.ends_with(".csv"), "{view}");
        file.content.lines().next().unwrap().to_owned()
    };
    assert!(header("skills", &none).starts_with("skill,in_inventory,uses,sessions,load_tokens"));
    assert!(header("context", &none).starts_with("source,project,kind,owner,items"));
    assert!(header("unused", &none).starts_with("kind,name,project,clients,uses"));
    assert!(header("compare", &none).starts_with("source,sessions,requests,input_tokens"));
    assert!(header("efficiency", &none).starts_with("key,source,requests,input_tokens"));
    assert!(header("cost", &none).starts_with("model,requests,input_tokens"));

    let by_project = ExportOptions {
        by: Some("project".into()),
        ..ExportOptions::default()
    };
    assert!(header("cost", &by_project).starts_with("project,requests"));

    // The flow view exports the tab it shows.
    for (section, first) in [
        ("growth", "source,session,project,model"),
        ("errors", "source,tool,calls,errors"),
        ("hooks", "source,hook,runs,failures"),
    ] {
        let options = ExportOptions {
            section: Some(section.into()),
            ..ExportOptions::default()
        };
        assert!(header("flow", &options).starts_with(first), "{section}");
    }
}

#[test]
fn unused_csv_follows_the_all_toggle_and_overview_has_no_csv() {
    let (_dir, ctx) = setup();
    let rows = |all: bool| {
        let options = ExportOptions {
            all: Some(all),
            ..ExportOptions::default()
        };
        let file = api::export(&ctx, "unused", "csv", &claude(), &options).unwrap();
        file.content.lines().count()
    };
    let shown = api::unused(&ctx, &claude()).unwrap();
    let unused_only = shown.data.items.iter().filter(|i| i.uses == 0).count();
    assert_eq!(rows(false), 1 + unused_only);
    assert_eq!(rows(true), 1 + shown.data.items.len());

    let err = api::export(
        &ctx,
        "overview",
        "csv",
        &claude(),
        &ExportOptions::default(),
    )
    .unwrap_err();
    assert!(err.contains("cannot be exported as CSV"), "{err}");
    let bad = ExportOptions {
        section: Some("everything".into()),
        ..ExportOptions::default()
    };
    let err = api::export(&ctx, "flow", "csv", &claude(), &bad).unwrap_err();
    assert!(err.contains("unknown section"), "{err}");
}
