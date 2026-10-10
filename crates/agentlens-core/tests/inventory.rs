mod support;

use agentlens_core::inventory::{LocationKind, locations, scan};
use agentlens_core::model::Source;
use agentlens_core::tokens::CharsPerToken;
use support::TempHome;

fn skill(name: &str, description: &str, body: &str) -> String {
    format!("---\nname: {name}\ndescription: {description}\n---\n{body}")
}

#[test]
fn finds_skills_per_client_and_flags_duplicates() {
    let home = TempHome::new();
    home.write(
        ".claude/skills/alpha/SKILL.md",
        skill("alpha", "does alpha things", &"a".repeat(400)),
    );
    home.write(
        ".agents/skills/beta/SKILL.md",
        skill("beta", "does beta things", "short"),
    );
    // Same name, different content, different place.
    home.write(
        ".agents/skills/alpha/SKILL.md",
        skill("alpha", "another alpha", "different"),
    );
    let project = home.path().join("work/demo");
    home.write(
        "work/demo/.github/skills/gamma/SKILL.md",
        skill("gamma", "project skill", "g"),
    );
    // Not a skill: no SKILL.md.
    home.write(".claude/skills/not-a-skill/readme.txt", "x");

    let inv = scan(&locations(&home.env(), &[project]), &CharsPerToken);

    let names: Vec<_> = inv.skills.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["alpha", "alpha", "beta", "gamma"]);

    let beta = &inv.find("beta")[0];
    assert_eq!(
        beta.clients,
        [Source::OpenCode, Source::CopilotCli, Source::VsCodeCopilot]
    );
    assert!(
        inv.find("gamma")[0]
            .locations
            .iter()
            .any(|l| l.kind == LocationKind::ProjectGithub)
    );
    assert!(!inv.find("gamma")[0].clients.contains(&Source::ClaudeCode));
    assert!(
        inv.find("alpha")
            .iter()
            .any(|s| s.clients.contains(&Source::ClaudeCode))
    );

    assert_eq!(inv.duplicates.len(), 1);
    assert_eq!(inv.duplicates[0].name, "alpha");
    assert!(!inv.duplicates[0].identical);
    assert!(inv.warnings.is_empty());
}

#[test]
fn estimates_baseline_and_body_and_marks_them_estimated() {
    let home = TempHome::new();
    home.write(
        ".claude/skills/alpha/SKILL.md",
        skill("alpha", "does alpha things", &"a".repeat(400)),
    );
    let inv = scan(&locations(&home.env(), &[]), &CharsPerToken);
    let s = &inv.skills[0];
    // "- alpha: does alpha things" is 26 characters -> 7 tokens.
    assert_eq!(s.baseline_tokens.count, 7);
    assert_eq!(s.body_bytes, 400);
    assert_eq!(s.body_tokens.count, 100);
    assert!(!s.baseline_tokens.measured && !s.body_tokens.measured);
}

#[cfg(unix)]
#[test]
fn symlinked_copies_are_one_skill_with_two_locations() {
    let home = TempHome::new();
    home.write(".agents/skills/alpha/SKILL.md", skill("alpha", "d", "body"));
    std::fs::create_dir_all(home.path().join(".claude/skills")).unwrap();
    std::os::unix::fs::symlink(
        home.path().join(".agents/skills/alpha"),
        home.path().join(".claude/skills/alpha"),
    )
    .unwrap();

    let inv = scan(&locations(&home.env(), &[]), &CharsPerToken);
    assert_eq!(inv.skills.len(), 1);
    assert_eq!(inv.skills[0].locations.len(), 2);
    assert!(inv.skills[0].clients.contains(&Source::ClaudeCode));
    assert!(inv.duplicates.is_empty());
}

#[test]
fn identical_copies_are_reported_as_identical_duplicates() {
    let home = TempHome::new();
    let text = skill("alpha", "d", "body");
    home.write(".claude/skills/alpha/SKILL.md", &text);
    home.write(".agents/skills/alpha/SKILL.md", &text);
    let inv = scan(&locations(&home.env(), &[]), &CharsPerToken);
    assert_eq!(inv.duplicates.len(), 1);
    assert!(inv.duplicates[0].identical);
}

#[test]
fn plugin_skills_come_from_installed_plugins_json() {
    let home = TempHome::new();
    let install = home.path().join(".claude/plugins/cache/mp/demo/1");
    home.write(
        ".claude/plugins/cache/mp/demo/1/skills/plug/SKILL.md",
        skill("plug", "from a plugin", "b"),
    );
    home.write(
        ".claude/plugins/installed_plugins.json",
        format!(
            r#"{{"version":2,"plugins":{{"demo@mp":[{{"installPath":"{}"}}]}}}}"#,
            install.display().to_string().replace('\\', "\\\\")
        ),
    );
    let inv = scan(&locations(&home.env(), &[]), &CharsPerToken);
    assert_eq!(inv.skills.len(), 1);
    assert_eq!(inv.skills[0].clients, [Source::ClaudeCode]);
    assert_eq!(inv.skills[0].locations[0].kind, LocationKind::Plugin);
}

#[test]
fn skills_without_frontmatter_are_kept_with_a_warning() {
    let home = TempHome::new();
    home.write(".claude/skills/bare/SKILL.md", "# just text");
    let inv = scan(&locations(&home.env(), &[]), &CharsPerToken);
    assert_eq!(inv.skills[0].name, "bare");
    assert_eq!(inv.warnings.len(), 1);
}
