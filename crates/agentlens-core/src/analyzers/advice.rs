//! Findings that say what to change to save tokens, made from the skill report.
//!
//! No model is involved: every finding has a fixed kind, a fixed explanation and the numbers it rests on,
//! each with the flag that says whether it was measured. The CLI, the application and the MCP server show
//! the same findings, so an agent that gets them only has to explain and act, not to recompute.

use serde::{Deserialize, Serialize};

use crate::analyzers::skills::{LARGE_CONTEXT_TOKENS, SkillReport, SkillStats};
use crate::tokens::Tokens;

/// A description of about this many tokens is enough to tell when a skill applies.
pub const DESCRIPTION_TARGET_TOKENS: u64 = 40;
/// Ranked findings list this many items; `count` says how many there were.
pub const TOP: usize = 10;
/// A skill needs this many requests after it for its average context size to mean something.
pub const MIN_FOLLOW_ON_REQUESTS: u64 = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FindingKind {
    /// Installed, but neither loaded nor used through its files in the period.
    UnusedSkills,
    /// The descriptions that cost the most in every request.
    CostlyDescriptions,
    /// Loaded in the period but no longer installed.
    UsedButNotInstalled,
    /// Loaded skill bodies that stay in the context for many requests.
    LargeBodies,
    /// The skills after which the work ran on the largest context.
    LargeContext,
    /// How large the context of the average request is, and how much of the work ran on large contexts.
    ContextSize,
}

impl FindingKind {
    /// What the finding means, in a sentence that does not depend on the numbers.
    pub fn explanation(self) -> &'static str {
        match self {
            FindingKind::UnusedSkills => {
                "Installed skills that were neither loaded nor used through their files in the period. Their name and description are in the context of every request anyway."
            }
            FindingKind::CostlyDescriptions => {
                "The skill descriptions that take the most tokens in every request. A short description is enough to tell when a skill applies."
            }
            FindingKind::UsedButNotInstalled => {
                "Skills that were loaded in the period but are not installed now: renamed, removed, or built into the client or a plugin. References to them can be cleaned up."
            }
            FindingKind::LargeBodies => {
                "Loaded skill bodies that stay in the context and are sent again with every later request until a compaction. A large body that a script or a short note could replace costs this on every use."
            }
            FindingKind::LargeContext => {
                "The skills after which the work ran on the largest context, per request. Follow-on is the activity after a skill, not cost the skill caused: a skill used late in a long session inherits that session's context."
            }
            FindingKind::ContextSize => {
                "Every request sends the whole context again, so its size drives the cost far more than the skills do. This is how large the context of a request was on average, how many requests ran on a large context, and which sessions had the largest."
            }
        }
    }

    /// What can be done about it, in general terms.
    pub fn action(self) -> &'static str {
        match self {
            FindingKind::UnusedSkills => {
                "Remove the skill from the global folder or install it only in the projects that use it. If the client supports it, mark a skill that should only run on request as not automatically invoked in its front matter."
            }
            FindingKind::CostlyDescriptions => {
                "Shorten the description to the trigger conditions; leave how the skill works to its body."
            }
            FindingKind::UsedButNotInstalled => {
                "Check instruction files and prompts that still refer to the skill."
            }
            FindingKind::LargeBodies => {
                "Move deterministic steps into a script that the skill runs, and keep the body to what the model has to decide."
            }
            FindingKind::LargeContext => {
                "Start work such as a release or a pull request in a fresh session instead of at the end of a long one, and clear the context between tasks."
            }
            FindingKind::ContextSize => {
                "Start new work in a fresh session, clear the context between tasks, and delegate exploration and review to subagents, whose context stays separate."
            }
        }
    }
}

/// One skill in a finding.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Item {
    pub name: String,
    /// The number the finding is about for this skill (see the kind).
    pub tokens: Tokens,
    /// The requests behind it.
    pub requests: u64,
    /// The numbers in words, for a table.
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Finding {
    pub kind: FindingKind,
    pub explanation: String,
    pub action: String,
    /// How many skills the finding is about; `items` may list fewer (the top ones).
    pub count: u32,
    /// What the listed skills cost together, or could save, where that is a sum.
    pub total: Option<Tokens>,
    /// `total` as a share of the baseline cost of all installed skills.
    pub share_of_baseline: Option<f64>,
    /// `total` as a share of all tokens of the period (input, output and cache).
    #[serde(default)]
    pub share_of_total_tokens: Option<f64>,
    pub items: Vec<Item>,
    /// What the finding cannot know.
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdviceReport {
    pub sessions: u32,
    pub installed_skills: u32,
    /// Baseline cost (name and description in every request) of all installed skills.
    pub baseline_total: Tokens,
    /// All tokens of the period (input, output and cache) that the shares are taken of.
    #[serde(default)]
    pub total_tokens: Tokens,
    pub findings: Vec<Finding>,
}

/// `1234567` -> `1,234,567`.
fn n(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn times(count: u32) -> String {
    if count == 1 {
        "once".into()
    } else {
        format!("{count} times")
    }
}

fn sum(items: &[Item]) -> Tokens {
    Tokens {
        count: items.iter().map(|i| i.tokens.count).sum(),
        measured: !items.is_empty() && items.iter().all(|i| i.tokens.measured),
    }
}

fn share(part: u64, whole: u64) -> Option<f64> {
    (whole > 0).then(|| part as f64 / whole as f64)
}

fn finding(kind: FindingKind, count: usize, items: Vec<Item>, total: Option<Tokens>) -> Finding {
    Finding {
        kind,
        explanation: kind.explanation().to_owned(),
        action: kind.action().to_owned(),
        count: count as u32,
        total,
        share_of_baseline: None,
        share_of_total_tokens: None,
        items,
        notes: Vec::new(),
    }
}

fn per_request(s: &SkillStats) -> u64 {
    s.baseline
        .tokens
        .count
        .checked_div(s.baseline.requests)
        .unwrap_or(0)
}

pub fn analyze(report: &SkillReport) -> AdviceReport {
    let installed: Vec<&SkillStats> = report.skills.iter().filter(|s| s.in_inventory).collect();
    let baseline_total = Tokens {
        count: installed.iter().map(|s| s.baseline.tokens.count).sum(),
        measured: !installed.is_empty() && installed.iter().all(|s| s.baseline.tokens.measured),
    };
    let total_tokens = Tokens {
        count: report.totals.tokens,
        measured: report.totals.measured,
    };
    let estimate_note = || {
        "Description sizes are estimates (characters / 4); `--exact` counts them with the Anthropic API."
            .to_owned()
    };
    let mut findings = Vec::new();

    // Installed skills nobody used, neither by loading nor through their files.
    let mut unused: Vec<&SkillStats> = installed
        .iter()
        .copied()
        .filter(|s| s.invocations == 0 && s.file_uses == 0)
        .collect();
    unused.sort_by(|a, b| {
        b.baseline
            .tokens
            .count
            .cmp(&a.baseline.tokens.count)
            .then_with(|| a.name.cmp(&b.name))
    });
    if !unused.is_empty() {
        let items: Vec<Item> = unused
            .iter()
            .map(|s| Item {
                name: s.name.clone(),
                tokens: s.baseline.tokens,
                requests: s.baseline.requests,
                detail: format!(
                    "{} tokens in {} requests",
                    n(s.baseline.tokens.count),
                    n(s.baseline.requests)
                ),
            })
            .collect();
        let total = sum(&items);
        let mut f = finding(FindingKind::UnusedSkills, items.len(), items, Some(total));
        f.share_of_baseline = share(total.count, baseline_total.count);
        f.share_of_total_tokens = share(total.count, total_tokens.count);
        if !total.measured {
            f.notes.push(estimate_note());
        }
        f.notes.push(
            "A skill that is only needed now and then, or by a tool outside the sessions that were read, can still be worth keeping.".to_owned(),
        );
        findings.push(f);
    }

    // The descriptions that cost the most in every request.
    let mut costly: Vec<&SkillStats> = installed
        .iter()
        .copied()
        .filter(|s| per_request(s) > DESCRIPTION_TARGET_TOKENS)
        .collect();
    costly.sort_by(|a, b| {
        per_request(b)
            .cmp(&per_request(a))
            .then_with(|| a.name.cmp(&b.name))
    });
    if !costly.is_empty() {
        let saving =
            |s: &SkillStats| (per_request(s) - DESCRIPTION_TARGET_TOKENS) * s.baseline.requests;
        let total = Tokens::estimated(costly.iter().map(|s| saving(s)).sum());
        let items = costly
            .iter()
            .take(TOP)
            .map(|s| Item {
                name: s.name.clone(),
                tokens: s.baseline.tokens,
                requests: s.baseline.requests,
                detail: format!(
                    "{} tokens per request; about {} tokens saved at {DESCRIPTION_TARGET_TOKENS}",
                    n(per_request(s)),
                    n(saving(s))
                ),
            })
            .collect();
        let mut f = finding(
            FindingKind::CostlyDescriptions,
            costly.len(),
            items,
            Some(total),
        );
        f.share_of_baseline = share(total.count, baseline_total.count);
        f.share_of_total_tokens = share(total.count, total_tokens.count);
        f.notes.push(format!(
            "`total` is what shortening every listed description to {DESCRIPTION_TARGET_TOKENS} tokens would have saved in the period; it is an estimate."
        ));
        findings.push(f);
    }

    // Loaded but no longer installed.
    let mut gone: Vec<&SkillStats> = report
        .skills
        .iter()
        .filter(|s| !s.in_inventory && s.invocations > 0)
        .collect();
    gone.sort_by(|a, b| {
        b.invocations
            .cmp(&a.invocations)
            .then_with(|| a.name.cmp(&b.name))
    });
    if !gone.is_empty() {
        let items: Vec<Item> = gone
            .iter()
            .map(|s| Item {
                name: s.name.clone(),
                tokens: s.load,
                requests: u64::from(s.invocations),
                detail: format!("loaded {}", times(s.invocations)),
            })
            .collect();
        let mut f = finding(FindingKind::UsedButNotInstalled, items.len(), items, None);
        f.notes.push(
            "Skills built into the client or supplied by a plugin are not in the folders AgentLens inventories and show up here too.".to_owned(),
        );
        findings.push(f);
    }

    // Loaded bodies that stay in the context.
    let mut bodies: Vec<&SkillStats> = report
        .skills
        .iter()
        .filter(|s| s.carry.tokens.count > 0)
        .collect();
    bodies.sort_by(|a, b| {
        b.carry
            .tokens
            .count
            .cmp(&a.carry.tokens.count)
            .then_with(|| a.name.cmp(&b.name))
    });
    if !bodies.is_empty() {
        let items = bodies
            .iter()
            .take(TOP)
            .map(|s| Item {
                name: s.name.clone(),
                tokens: s.carry.tokens,
                requests: s.carry.requests,
                detail: format!(
                    "{} tokens loaded {}, sent again in {} requests",
                    n(s.load.count),
                    times(s.invocations),
                    n(s.carry.requests)
                ),
            })
            .collect();
        let carry_total = Tokens::estimated(bodies.iter().map(|s| s.carry.tokens.count).sum());
        let mut f = finding(
            FindingKind::LargeBodies,
            bodies.len(),
            items,
            Some(carry_total),
        );
        f.share_of_total_tokens = share(carry_total.count, total_tokens.count);
        f.notes.push(
            "Carry is an estimate: the body size times the requests after it; most of it is served from the cache, which is cheap.".to_owned(),
        );
        findings.push(f);
    }

    // The work that ran on the largest context.
    let average = |s: &SkillStats| {
        (s.follow_on.input + s.follow_on.cache_read + s.follow_on.cache_write)
            .checked_div(s.follow_on.requests)
            .unwrap_or(0)
    };
    let mut heavy: Vec<&SkillStats> = report
        .skills
        .iter()
        .filter(|s| s.follow_on.requests >= MIN_FOLLOW_ON_REQUESTS)
        .collect();
    heavy.sort_by(|a, b| {
        average(b)
            .cmp(&average(a))
            .then_with(|| a.name.cmp(&b.name))
    });
    if !heavy.is_empty() {
        let items = heavy
            .iter()
            .take(TOP)
            .map(|s| Item {
                name: s.name.clone(),
                tokens: Tokens::measured(average(s)),
                requests: s.follow_on.requests,
                detail: format!(
                    "{} tokens per request on average over {} requests",
                    n(average(s)),
                    n(s.follow_on.requests)
                ),
            })
            .collect();
        let mut f = finding(FindingKind::LargeContext, heavy.len(), items, None);
        f.notes.push(format!(
            "Only skills with at least {MIN_FOLLOW_ON_REQUESTS} requests after them are listed. The sizes are what the client reported."
        ));
        findings.push(f);
    }

    // The scale of it all: the context of every request, not the skills.
    let totals = &report.totals;
    if totals.requests > 0 {
        let average = totals.context_tokens / totals.requests;
        let items: Vec<Item> = totals
            .largest_sessions
            .iter()
            .map(|c| Item {
                name: c.id.clone(),
                tokens: Tokens::measured(c.average_context),
                requests: c.requests,
                detail: format!(
                    "{} tokens per request on average over {} requests",
                    n(c.average_context),
                    n(c.requests)
                ),
            })
            .collect();
        let mut f = finding(
            FindingKind::ContextSize,
            items.len(),
            items,
            Some(Tokens {
                count: totals.context_tokens,
                measured: totals.measured,
            }),
        );
        f.share_of_total_tokens = share(totals.context_tokens, totals.tokens);
        let pct = |part: u64| share(part, totals.requests).unwrap_or(0.0) * 100.0;
        f.notes.push(format!(
            "The average request carried {} tokens of context. {:.1}% of the {} requests had at least {} tokens; subagents made {:.1}% of the requests.",
            n(average),
            pct(totals.large_context_requests),
            n(totals.requests),
            n(LARGE_CONTEXT_TOKENS),
            pct(totals.subagent_requests),
        ));
        f.notes.push(
            "`total` is the input size of all requests added up (most of it served from the cache); the listed sessions are those with the largest average context."
                .to_owned(),
        );
        if !totals.measured {
            f.notes
                .push("Some requests are estimated, not reported by the client.".to_owned());
        }
        findings.push(f);
    }

    AdviceReport {
        sessions: report.sessions,
        installed_skills: installed.len() as u32,
        baseline_total,
        total_tokens,
        findings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzers::skills::{BaselineCost, CarryCost, TriggerCounts, UsageSum};

    fn skill(
        name: &str,
        installed: bool,
        loads: u32,
        files: u32,
        listed: (u64, u64),
    ) -> SkillStats {
        SkillStats {
            name: name.into(),
            in_inventory: installed,
            clients: vec![],
            invocations: loads,
            file_uses: files,
            sessions: 0,
            projects: vec![],
            first_used: None,
            last_used: None,
            triggers: TriggerCounts::default(),
            by_source: Default::default(),
            uses_by_day: Default::default(),
            load: Tokens::measured(0),
            unknown_loads: 0,
            carry: CarryCost::default(),
            follow_on: UsageSum::default(),
            attributed: UsageSum::default(),
            baseline: BaselineCost {
                requests: listed.0,
                tokens: Tokens::estimated(listed.1),
            },
        }
    }

    fn report(skills: Vec<SkillStats>) -> SkillReport {
        SkillReport {
            sessions: 3,
            skills,
            ..SkillReport::default()
        }
    }

    fn find(r: &AdviceReport, kind: FindingKind) -> &Finding {
        r.findings
            .iter()
            .find(|f| f.kind == kind)
            .unwrap_or_else(|| panic!("{kind:?} missing"))
    }

    #[test]
    fn unused_skills_are_those_neither_loaded_nor_used_through_files() {
        let r = analyze(&report(vec![
            skill("loaded", true, 2, 0, (100, 1000)),
            skill("by-files", true, 0, 3, (100, 1000)),
            skill("idle-big", true, 0, 0, (100, 3000)),
            skill("idle-small", true, 0, 0, (100, 1000)),
        ]));
        let f = find(&r, FindingKind::UnusedSkills);
        let names: Vec<&str> = f.items.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["idle-big", "idle-small"], "largest cost first");
        assert_eq!(f.count, 2);
        assert_eq!(f.total, Some(Tokens::estimated(4000)));
        // The baseline of all four skills is 6000 tokens; the two idle ones are two thirds of it.
        assert_eq!(r.baseline_total, Tokens::estimated(6000));
        assert!((f.share_of_baseline.unwrap() - 4000.0 / 6000.0).abs() < 1e-9);
        assert!(
            f.notes.iter().any(|n| n.contains("estimates")),
            "an estimate says so"
        );
    }

    #[test]
    fn a_sum_of_measured_listings_is_measured() {
        let mut a = skill("a", true, 0, 0, (10, 100));
        a.baseline.tokens = Tokens::measured(100);
        let r = analyze(&report(vec![a]));
        let f = find(&r, FindingKind::UnusedSkills);
        assert_eq!(f.total, Some(Tokens::measured(100)));
        assert!(!f.notes.iter().any(|n| n.contains("estimates")));
        assert!(r.baseline_total.measured);
    }

    #[test]
    fn costly_descriptions_are_ranked_by_tokens_per_request_and_estimate_the_saving() {
        let r = analyze(&report(vec![
            skill("long", true, 1, 0, (100, 13_200)), // 132 per request
            skill("medium", true, 1, 0, (50, 4_650)), // 93 per request
            skill("short", true, 1, 0, (100, 3_000)), // 30 per request: nothing to shorten
        ]));
        let f = find(&r, FindingKind::CostlyDescriptions);
        let names: Vec<&str> = f.items.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["long", "medium"]);
        // (132 - 40) * 100 + (93 - 40) * 50
        assert_eq!(f.total, Some(Tokens::estimated(9200 + 2650)));
        assert!(f.items[0].detail.contains("132 tokens per request"));
        assert!(
            f.items[0].detail.contains("9,200 tokens saved"),
            "{}",
            f.items[0].detail
        );
    }

    #[test]
    fn skills_used_but_not_installed_are_listed_by_how_often_they_were_loaded() {
        let r = analyze(&report(vec![
            skill("gone", false, 3, 0, (0, 0)),
            skill("also-gone", false, 1, 0, (0, 0)),
            skill("never-seen", false, 0, 0, (0, 0)),
            skill("here", true, 1, 0, (10, 100)),
        ]));
        let f = find(&r, FindingKind::UsedButNotInstalled);
        let names: Vec<&str> = f.items.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["gone", "also-gone"]);
        assert_eq!(f.items[0].requests, 3);
    }

    #[test]
    fn large_bodies_and_large_contexts_are_ranked_and_the_context_is_measured() {
        let mut big = skill("big", true, 2, 0, (10, 100));
        big.carry.tokens = Tokens::estimated(50_000);
        big.carry.requests = 40;
        let mut small = skill("small", true, 1, 0, (10, 100));
        small.carry.tokens = Tokens::estimated(500);
        let mut late = skill("late", true, 1, 0, (10, 100));
        late.follow_on = UsageSum {
            requests: 50,
            input: 100,
            cache_read: 24_999_900,
            cache_write: 0,
            output: 0,
        };
        let mut few = skill("few", true, 1, 0, (10, 100));
        few.follow_on = UsageSum {
            requests: 5,
            cache_read: 9_000_000,
            ..UsageSum::default()
        };
        let r = analyze(&report(vec![big, small, late, few]));
        let bodies = find(&r, FindingKind::LargeBodies);
        assert_eq!(bodies.items[0].name, "big");
        assert!(!bodies.items[0].tokens.measured, "carry is derived");
        let heavy = find(&r, FindingKind::LargeContext);
        assert_eq!(
            heavy.items.len(),
            1,
            "five requests are too few to say anything"
        );
        assert_eq!(heavy.items[0].name, "late");
        assert_eq!(heavy.items[0].tokens, Tokens::measured(500_000));
    }

    #[test]
    fn nothing_to_say_gives_no_findings() {
        let r = analyze(&report(vec![skill("busy", true, 5, 0, (10, 300))]));
        assert!(r.findings.is_empty());
        assert_eq!(r.installed_skills, 1);
        assert_eq!(analyze(&SkillReport::default()).findings, []);
    }

    #[test]
    fn context_size_puts_the_work_in_proportion() {
        use crate::analyzers::skills::{PeriodTotals, SessionContext};
        let mut unused = skill("idle", true, 0, 0, (100, 10_000));
        unused.baseline.tokens = Tokens::measured(10_000);
        let mut r = report(vec![unused]);
        r.totals = PeriodTotals {
            requests: 100,
            subagent_requests: 10,
            tokens: 1_000_000,
            context_tokens: 900_000,
            large_context_requests: 25,
            measured: true,
            largest_sessions: vec![SessionContext {
                id: "s1".into(),
                project: None,
                requests: 60,
                average_context: 250_000,
            }],
        };
        let a = analyze(&r);
        assert_eq!(a.total_tokens, Tokens::measured(1_000_000));
        let idle = find(&a, FindingKind::UnusedSkills);
        assert_eq!(idle.share_of_total_tokens, Some(0.01));
        let c = find(&a, FindingKind::ContextSize);
        assert_eq!(c.total, Some(Tokens::measured(900_000)));
        assert_eq!(c.share_of_total_tokens, Some(0.9));
        assert_eq!(c.items[0].name, "s1");
        assert!(c.items[0].tokens.measured);
        assert!(c.notes[0].contains("9,000 tokens"), "{:?}", c.notes);
        assert!(c.notes[0].contains("25.0%"), "{:?}", c.notes);
        assert!(c.notes[0].contains("10.0%"), "{:?}", c.notes);
        assert!(c.action.contains("fresh session"));
    }

    #[test]
    fn no_requests_no_context_finding() {
        let a = analyze(&report(vec![skill("idle", true, 0, 0, (10, 100))]));
        assert!(
            a.findings
                .iter()
                .all(|f| f.kind != FindingKind::ContextSize)
        );
        assert_eq!(a.findings[0].share_of_total_tokens, None);
    }
}
