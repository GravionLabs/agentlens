mod support;

use agentlens_core::adapter::{Adapter, SessionFilter};
use agentlens_core::adapters::claude_code::ClaudeCodeAdapter;
use agentlens_core::analyzers::skills::analyze;
use agentlens_core::inventory::{locations, scan};
use agentlens_core::tokens::CharsPerToken;
use support::TempHome;

/// Inventory + Claude Code fixtures -> skill report. Review the snapshot when
/// the report format changes (`INSTA_UPDATE=always cargo test`).
#[test]
fn skill_report_for_claude_code_fixtures() {
    let home = TempHome::new();
    home.copy_fixture(
        "claude_code/skills.jsonl",
        ".claude/projects/-work-demo/skills.jsonl",
    );
    home.copy_fixture(
        "claude_code/basic.jsonl",
        ".claude/projects/-work-demo/basic.jsonl",
    );
    for (name, body) in [
        ("demo", "d".repeat(400)),
        ("orch", "o".repeat(800)),
        ("unused", "u".repeat(40)),
    ] {
        home.write(
            &format!(".claude/skills/{name}/SKILL.md"),
            format!("---\nname: {name}\ndescription: Does {name} things\n---\n{body}"),
        );
    }
    let env = home.env();
    let inventory = scan(&locations(&env, &[]), &CharsPerToken);
    let sessions: Vec<_> = ClaudeCodeAdapter
        .sessions(&env, &SessionFilter::default())
        .map(|r| r.unwrap())
        .collect();

    let report = analyze(&inventory, &sessions, &CharsPerToken);
    insta::assert_json_snapshot!(report);
}
