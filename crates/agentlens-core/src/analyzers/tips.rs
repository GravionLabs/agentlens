//! Tips: the few advice findings that pay most, said in one line each, with the concrete targets and,
//! where the client documents the syntax, a ready step.
//!
//! No model is involved: the same sessions give the same tips. A tip is a view over a finding of the
//! advice report; it adds no analysis of its own.

use serde::{Deserialize, Serialize};

use crate::analyzers::advice::{AdviceReport, Confidence, Finding, FindingKind, n};
use crate::inventory::{LocationKind, SkillEntry};
use crate::model::Source;
use crate::tokens::Tokens;

/// A finding becomes a tip only if its saving is at least this share of the period's cost (0.005 is
/// 0.5 %); of the class-weighted tokens of the period when the period has no price at all.
pub const MIN_TIP_SHARE: f64 = 0.005;
/// A tip lists this many items of its finding as targets; `more` counts the rest.
pub const TIP_TARGETS: usize = 3;
/// The default number of tips.
pub const DEFAULT_LIMIT: usize = 5;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TipsReport {
    /// The cost of all priced requests of the period, estimated. `None` without any price.
    pub period_cost_usd: Option<f64>,
    /// [`MIN_TIP_SHARE`], echoed for the reader.
    pub threshold: f64,
    /// Ranked, at most the limit.
    pub tips: Vec<Tip>,
    /// Findings with a saving whose share is below the threshold.
    pub below_threshold: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tip {
    /// The rank of the finding in the advice report.
    pub rank: u32,
    pub kind: FindingKind,
    /// A fixed sentence per kind, filled with the numbers; estimates carry a tilde.
    pub headline: String,
    pub saving: Tokens,
    /// Always computed from the price table: an estimate.
    pub saving_usd: Option<f64>,
    /// `saving_usd` over the period's cost, or `saving` over the weighted tokens of the period without prices.
    pub share: Option<f64>,
    pub confidence: Confidence,
    /// The general action of the finding.
    pub action: String,
    /// The first [`TIP_TARGETS`] items of the finding.
    pub targets: Vec<Target>,
    /// Items of the finding that are not listed.
    pub more: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Target {
    pub name: String,
    pub detail: String,
    /// Ready steps; empty where no client documents the syntax.
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Step {
    pub client: Source,
    /// The file to change, where there is one.
    pub file: Option<String>,
    pub change: String,
}

fn percent(share: f64) -> String {
    format!("{:.1} %", share * 100.0)
}

/// The saving in words: `~$0.40`, or its tokens where no price is known.
fn amount(saving: Tokens, usd: Option<f64>) -> String {
    match usd {
        Some(usd) => format!("~${usd:.2}"),
        None => {
            let tilde = if saving.measured { "" } else { "~" };
            format!("{tilde}{} tokens", n(saving.count))
        }
    }
}

fn plural(count: u32, one: &str, many: &str) -> String {
    if count == 1 {
        format!("1 {one}")
    } else {
        format!("{} {many}", n(u64::from(count)))
    }
}

/// The fixed sentence of a kind, filled with the count of the finding and its saving.
fn headline(f: &Finding, share: Option<f64>) -> String {
    let c = f.count;
    let s = amount(f.impact.saving, f.impact.saving_usd);
    let tail = share
        .map(|p| format!(" ({})", percent(p)))
        .unwrap_or_default();
    match f.kind {
        FindingKind::UnusedSkills => format!(
            "{} cost {s} in the period{tail}",
            plural(c, "unused skill", "unused skills")
        ),
        FindingKind::ListedButLoadedByHand => format!(
            "{} listed in every request but only loaded by hand: {s}{tail}",
            plural(c, "skill is", "skills are")
        ),
        FindingKind::CostlyDescriptions => format!(
            "{} longer than needed: {s}{tail}",
            plural(c, "skill description is", "skill descriptions are")
        ),
        FindingKind::UsedButNotInstalled => format!(
            "{} used but not installed: {s}{tail}",
            plural(c, "skill", "skills")
        ),
        FindingKind::LargeBodies => format!(
            "{} larger than needed, carried in later requests: {s}{tail}",
            plural(c, "skill body is", "skill bodies are")
        ),
        FindingKind::LargeContext => format!(
            "Work after {} ran on a large context: {s}{tail}",
            plural(c, "skill", "skills")
        ),
        FindingKind::CostlyMcpServers => format!(
            "{} cost {s} in the period{tail}",
            plural(c, "MCP server", "MCP servers")
        ),
        FindingKind::ContextSize => {
            format!("Context above the session start on large requests: {s}{tail}")
        }
        FindingKind::RetryLoops => format!("Repeated failing tool calls: {s}{tail}"),
        FindingKind::ModelMix => format!(
            "Small requests on the costliest model of {}: {s}{tail}",
            plural(c, "client", "clients")
        ),
        FindingKind::ContextHygiene => {
            format!("Sessions that carried their context on after the task: {s}{tail}")
        }
        FindingKind::Delegation => format!("Little delegation on large contexts: {s}{tail}"),
        FindingKind::CacheExpiry => {
            format!("Context written to the cache again after a pause: {s}{tail}")
        }
    }
}

/// The Claude Code step for a skill that should leave the listing, or `None`.
///
/// Documented by Claude Code: `disable-model-invocation: true` in the front matter of a skill keeps the
/// skill out of the model's context while the user can still run it as `/name`.
/// https://code.claude.com/docs/en/skills
fn skill_step(name: &str, skills: &[SkillEntry]) -> Option<Step> {
    // A skill that a plugin supplies is changed in the plugin, not in its cache.
    let entry = skills.iter().find(|s| {
        s.name == name
            && s.clients.contains(&Source::ClaudeCode)
            && s.locations.iter().any(|l| l.kind != LocationKind::Plugin)
    })?;
    Some(Step {
        client: Source::ClaudeCode,
        file: Some(entry.file.display().to_string()),
        change: format!(
            "Add `disable-model-invocation: true` to the front matter. The skill leaves the listing and stays usable as `/{name}`."
        ),
    })
}

/// The Claude Code step for an MCP server that is loaded where it is not used.
///
/// Documented by Claude Code: a server in the user scope is stored in `~/.claude.json` and loads in all
/// projects; a project-scoped server is stored in `.mcp.json` in the project root and loads in that
/// project only. https://code.claude.com/docs/en/mcp
fn mcp_step(server: &str) -> Step {
    Step {
        client: Source::ClaudeCode,
        file: Some(".mcp.json".into()),
        change: format!(
            "In Claude Code, remove `{server}` from the user scope (`~/.claude.json`) and add it to `.mcp.json` in the root of each project that calls it."
        ),
    }
}

fn steps(kind: FindingKind, name: &str, skills: &[SkillEntry]) -> Vec<Step> {
    match kind {
        FindingKind::UnusedSkills | FindingKind::ListedButLoadedByHand => {
            skill_step(name, skills).into_iter().collect()
        }
        FindingKind::CostlyMcpServers => vec![mcp_step(name)],
        _ => Vec::new(),
    }
}

/// The tips of an advice report: one per finding with a saving of at least [`MIN_TIP_SHARE`] of the
/// period's cost, in the order of the findings, at most `limit`.
///
/// `weighted_tokens` is the class-weighted token count of the period; it stands in for the cost when
/// `period_cost_usd` is `None` or zero. `skills` is the inventory, for the steps.
pub fn tips(
    advice: &AdviceReport,
    skills: &[SkillEntry],
    period_cost_usd: Option<f64>,
    weighted_tokens: u64,
    limit: usize,
) -> TipsReport {
    let cost = period_cost_usd.filter(|c| *c > 0.0);
    let mut out = TipsReport {
        period_cost_usd,
        threshold: MIN_TIP_SHARE,
        tips: Vec::new(),
        below_threshold: 0,
    };
    for f in advice.findings.iter().filter(|f| f.impact.saving.count > 0) {
        // A saving never exceeds the period it is taken from, however it was priced.
        let share = match (f.impact.saving_usd, cost) {
            (Some(usd), Some(cost)) => Some(usd / cost),
            _ if weighted_tokens > 0 => Some(f.impact.saving.count as f64 / weighted_tokens as f64),
            _ => None,
        }
        .map(|s| s.min(1.0));
        if !share.is_some_and(|s| s >= MIN_TIP_SHARE) {
            out.below_threshold += 1;
            continue;
        }
        if out.tips.len() >= limit {
            continue;
        }
        out.tips.push(Tip {
            rank: f.rank,
            kind: f.kind,
            headline: headline(f, share),
            saving: f.impact.saving,
            saving_usd: f.impact.saving_usd,
            share,
            confidence: f.impact.confidence,
            action: f.action.clone(),
            targets: f
                .items
                .iter()
                .take(TIP_TARGETS)
                .map(|i| Target {
                    name: i.name.clone(),
                    detail: i.detail.clone(),
                    steps: steps(f.kind, &i.name, skills),
                })
                .collect(),
            more: f.count.saturating_sub(TIP_TARGETS as u32),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzers::advice::{Impact, Item};
    use crate::inventory::Location;
    use std::path::PathBuf;

    fn item(name: &str) -> Item {
        Item {
            name: name.into(),
            tokens: Tokens::estimated(10),
            requests: 1,
            detail: format!("detail of {name}"),
            referenced_by: Vec::new(),
            session_id: None,
        }
    }

    fn finding(
        rank: u32,
        kind: FindingKind,
        saving: u64,
        usd: Option<f64>,
        names: &[&str],
    ) -> Finding {
        Finding {
            rank,
            kind,
            explanation: String::new(),
            action: kind.action().into(),
            count: names.len() as u32,
            total: None,
            share_of_baseline: None,
            share_of_total_tokens: None,
            items: names.iter().map(|n| item(n)).collect(),
            notes: Vec::new(),
            impact: Impact {
                saving: Tokens::estimated(saving),
                saving_usd: usd,
                confidence: Confidence::Estimated,
            },
        }
    }

    fn advice(findings: Vec<Finding>) -> AdviceReport {
        AdviceReport {
            sessions: 1,
            installed_skills: 0,
            baseline_total: Tokens::default(),
            total_tokens: Tokens::default(),
            findings,
        }
    }

    fn entry(name: &str, kind: LocationKind, clients: Vec<Source>) -> SkillEntry {
        let dir = PathBuf::from(format!("/home/user/skills/{name}"));
        SkillEntry {
            name: name.into(),
            description: None,
            file: dir.join("SKILL.md"),
            locations: vec![Location {
                kind,
                dir: dir.clone(),
                project: None,
            }],
            clients,
            baseline_tokens: Tokens::estimated(0),
            body_tokens: Tokens::estimated(0),
            body_bytes: 0,
        }
    }

    #[test]
    fn the_limit_cuts_the_ranked_tips() {
        let findings = (1..=4)
            .map(|r| finding(r, FindingKind::RetryLoops, 100, Some(1.0), &["a"]))
            .collect();
        let t = tips(&advice(findings), &[], Some(10.0), 0, 2);
        assert_eq!(t.tips.iter().map(|t| t.rank).collect::<Vec<_>>(), [1, 2]);
        assert_eq!(t.below_threshold, 0);
    }

    #[test]
    fn a_finding_without_a_saving_is_no_tip() {
        let a = advice(vec![
            finding(1, FindingKind::Delegation, 0, None, &["x"]),
            finding(2, FindingKind::RetryLoops, 100, Some(1.0), &["a"]),
        ]);
        let t = tips(&a, &[], Some(10.0), 0, 5);
        assert_eq!(t.tips.len(), 1);
        assert_eq!(t.tips[0].kind, FindingKind::RetryLoops);
        assert_eq!(t.below_threshold, 0, "no saving is not below the threshold");
    }

    #[test]
    fn the_share_never_exceeds_one() {
        let a = advice(vec![finding(
            1,
            FindingKind::RetryLoops,
            100,
            Some(3.0),
            &["a"],
        )]);
        let t = tips(&a, &[], Some(2.0), 0, 5);
        assert_eq!(t.tips[0].share, Some(1.0));
        assert_eq!(
            t.tips[0].headline,
            "Repeated failing tool calls: ~$3.00 (100.0 %)"
        );
    }

    #[test]
    fn a_finding_below_half_a_percent_is_counted_not_listed() {
        let a = advice(vec![
            finding(1, FindingKind::RetryLoops, 100, Some(1.0), &["a"]),
            finding(2, FindingKind::ModelMix, 100, Some(0.04), &["b"]),
        ]);
        let t = tips(&a, &[], Some(10.0), 0, 5);
        assert_eq!(t.tips.len(), 1);
        assert_eq!(t.below_threshold, 1);
        assert_eq!(t.threshold, 0.005);
        assert_eq!(t.tips[0].share, Some(0.1));
    }

    #[test]
    fn without_prices_the_weighted_tokens_stand_in_for_the_cost() {
        let a = advice(vec![
            finding(1, FindingKind::RetryLoops, 5_000, None, &["a"]),
            finding(2, FindingKind::ModelMix, 100, None, &["b"]),
        ]);
        let t = tips(&a, &[], None, 100_000, 5);
        assert_eq!(t.tips.len(), 1);
        assert_eq!(t.tips[0].share, Some(0.05));
        assert_eq!(t.below_threshold, 1);
        assert_eq!(
            t.tips[0].headline,
            "Repeated failing tool calls: ~5,000 tokens (5.0 %)"
        );
    }

    #[test]
    fn targets_are_cut_at_three_and_more_counts_the_rest() {
        let a = advice(vec![finding(
            1,
            FindingKind::RetryLoops,
            100,
            Some(1.0),
            &["a", "b", "c", "d", "e"],
        )]);
        let t = &tips(&a, &[], Some(10.0), 0, 5).tips[0];
        assert_eq!(
            t.targets
                .iter()
                .map(|x| x.name.as_str())
                .collect::<Vec<_>>(),
            ["a", "b", "c"]
        );
        assert_eq!(t.more, 2);
        let few = advice(vec![finding(
            1,
            FindingKind::RetryLoops,
            100,
            Some(1.0),
            &["a"],
        )]);
        assert_eq!(tips(&few, &[], Some(10.0), 0, 5).tips[0].more, 0);
    }

    #[test]
    fn a_claude_code_skill_gets_the_front_matter_step_an_opencode_skill_none() {
        let skills = vec![
            entry("cc", LocationKind::UserClaude, vec![Source::ClaudeCode]),
            entry("oc", LocationKind::UserOpenCode, vec![Source::OpenCode]),
            entry("pl", LocationKind::Plugin, vec![Source::ClaudeCode]),
        ];
        let a = advice(vec![finding(
            1,
            FindingKind::UnusedSkills,
            100,
            Some(1.0),
            &["cc", "oc", "pl"],
        )]);
        let t = &tips(&a, &skills, Some(10.0), 0, 5).tips[0];
        let cc = &t.targets[0].steps;
        assert_eq!(cc.len(), 1);
        assert_eq!(cc[0].client, Source::ClaudeCode);
        assert_eq!(cc[0].file.as_deref(), Some("/home/user/skills/cc/SKILL.md"));
        assert!(cc[0].change.contains("disable-model-invocation: true"));
        assert!(t.targets[1].steps.is_empty(), "OpenCode only");
        assert!(t.targets[2].steps.is_empty(), "plugin skill");
    }

    #[test]
    fn a_costly_mcp_server_gets_the_scope_step() {
        let a = advice(vec![finding(
            1,
            FindingKind::CostlyMcpServers,
            100,
            Some(1.0),
            &["srv"],
        )]);
        let s = &tips(&a, &[], Some(10.0), 0, 5).tips[0].targets[0].steps;
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].file.as_deref(), Some(".mcp.json"));
        assert!(s[0].change.contains("`srv`") && s[0].change.contains("~/.claude.json"));
    }

    #[test]
    fn other_findings_have_the_action_but_no_step() {
        let a = advice(vec![finding(
            1,
            FindingKind::CacheExpiry,
            100,
            Some(1.0),
            &["s"],
        )]);
        let t = &tips(&a, &[], Some(10.0), 0, 5).tips[0];
        assert_eq!(t.action, FindingKind::CacheExpiry.action());
        assert!(t.targets[0].steps.is_empty());
    }

    #[test]
    fn the_headline_names_count_saving_and_share() {
        let names: Vec<String> = (0..14).map(|i| format!("s{i}")).collect();
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        let a = advice(vec![finding(
            1,
            FindingKind::UnusedSkills,
            100,
            Some(0.4),
            &names,
        )]);
        let t = tips(&a, &[], Some(50.0), 0, 5);
        assert_eq!(
            t.tips[0].headline,
            "14 unused skills cost ~$0.40 in the period (0.8 %)"
        );
    }

    #[test]
    fn an_empty_period_gives_no_tips() {
        let t = tips(&advice(Vec::new()), &[], None, 0, 5);
        assert!(t.tips.is_empty());
        assert_eq!(t.below_threshold, 0);
        assert_eq!(t.period_cost_usd, None);
    }
}
