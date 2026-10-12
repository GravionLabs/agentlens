//! Findings that say what to change to save tokens, made from the skill report.
//!
//! No model is involved: every finding has a fixed kind, a fixed explanation and the numbers it rests on,
//! each with the flag that says whether it was measured. The CLI, the application and the MCP server show
//! the same findings, so an agent that gets them only has to explain and act, not to recompute.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::analyzers::context::ContextReport;
use crate::analyzers::skills::{LARGE_CONTEXT_TOKENS, SkillReport, SkillStats};
use crate::analyzers::unused::{calls_server, norm};
use crate::model::{ContextKind, EventKind, SessionEvents, Source};
use crate::references::{Reference, References};
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
    /// Installed and read through its files, but never loaded: the listing is paid, the model never picks it.
    ListedButLoadedByHand,
    /// The descriptions that cost the most in every request.
    CostlyDescriptions,
    /// Loaded in the period but no longer installed.
    UsedButNotInstalled,
    /// Loaded skill bodies that stay in the context for many requests.
    LargeBodies,
    /// The skills after which the work ran on the largest context.
    LargeContext,
    /// MCP servers whose static context is large next to how often their tools were called.
    CostlyMcpServers,
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
            FindingKind::ListedButLoadedByHand => {
                "Installed skills that were never loaded but whose files were read or run, for example a stack skill that another skill opens on instruction. Their name and description are in the context of every request, although the model never picks them from the listing."
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
            FindingKind::CostlyMcpServers => {
                "MCP servers put their instructions, tool listing and tool definitions into every request, whether or not a tool is called. Ranked by the static context per tool call; a server that is called a lot is listed as such, the point is the ratio."
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
            FindingKind::ListedButLoadedByHand => {
                "Put such a skill below the skill that opens it (as a file in its folder) instead of listing it on its own, or mark it as not automatically invoked in its front matter if the client supports it."
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
            FindingKind::CostlyMcpServers => {
                "Enable the server only in the projects that need it, or switch it off when it is not in use."
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
    /// For an unused skill: the installed agents, commands and skills whose files name it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub referenced_by: Vec<Reference>,
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
    analyze_with(report, &References::new(), &[])
}

/// What one MCP server costs in static context and how often its tools were called.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpServerUse {
    pub server: String,
    /// Instructions, tool listing and tool definitions, per request times requests. Measured only if
    /// the client reported every size.
    pub tokens: Tokens,
    /// The main-agent requests that carried the server's context.
    pub requests: u64,
    /// Calls of the server's tools (`mcp__<server>__…`) in the period.
    pub calls: u64,
}

/// The MCP servers whose context the clients reported, with the calls of their tools. Servers that the
/// clients do not report (nothing in the context rows) are not listed.
pub fn mcp_servers(context: &ContextReport, sessions: &[SessionEvents]) -> Vec<McpServerUse> {
    #[derive(Default)]
    struct Found {
        /// Tool rows name the server the way its tools do; their owner is preferred, then the smaller one.
        name: Option<(bool, String)>,
        tokens: Option<Tokens>,
        /// The most requests of any row, per client and project: those are disjoint sets of requests.
        requests: BTreeMap<(Source, Option<PathBuf>), u64>,
    }
    let mut found: BTreeMap<String, Found> = BTreeMap::new();
    for r in context.rows.iter().filter(|r| {
        matches!(
            r.kind,
            ContextKind::ToolListing | ContextKind::ToolDefinition | ContextKind::McpInstructions
        )
    }) {
        // The context rows of client tools and of all tools together belong to no server.
        if matches!(r.owner.as_str(), "built-in" | "all tools") {
            continue;
        }
        let entry = found.entry(norm(&r.owner)).or_default();
        let candidate = (r.kind == ContextKind::McpInstructions, r.owner.clone());
        if entry.name.as_ref().is_none_or(|n| candidate < *n) {
            entry.name = Some(candidate);
        }
        let tokens = entry.tokens.get_or_insert(Tokens::measured(0));
        tokens.count += r.overhead_tokens.count;
        tokens.measured &= r.overhead_tokens.measured;
        let requests = entry
            .requests
            .entry((r.source, r.project.clone()))
            .or_default();
        *requests = (*requests).max(r.requests);
    }
    found
        .into_iter()
        .map(|(key, f)| {
            // Only calls in the sessions whose context was reported: the ratio compares like with like.
            let calls = sessions
                .iter()
                .filter(|se| {
                    f.requests.keys().any(|(source, project)| {
                        *source == se.session.source
                            && (project.is_none() || *project == se.session.project_dir)
                    })
                })
                .flat_map(|se| &se.events)
                .filter(
                    |e| matches!(&e.kind, EventKind::ToolCall(c) if calls_server(&c.name, &key)),
                )
                .count() as u64;
            McpServerUse {
                server: f.name.map(|n| n.1).unwrap_or_default(),
                tokens: f.tokens.unwrap_or(Tokens::measured(0)),
                requests: f.requests.values().sum(),
                calls,
            }
        })
        .collect()
}

/// Like [`analyze`], and marks the unused skills that an agent, a command or a skill names.
pub fn analyze_with(
    report: &SkillReport,
    references: &References,
    mcp: &[McpServerUse],
) -> AdviceReport {
    let installed: Vec<&SkillStats> = report.skills.iter().filter(|s| s.in_inventory).collect();
    let baseline_total = Tokens {
        count: installed.iter().map(|s| s.baseline.tokens.count).sum(),
        measured: !installed.is_empty() && installed.iter().all(|s| s.baseline.tokens.measured),
    };
    let total_tokens = Tokens {
        count: report.totals.tokens,
        measured: report.totals.measured,
    };
    // The client's own listing is the whole truth about what it sent; the baseline covers only the skills
    // AgentLens has a file for. Say so where the numbers appear.
    let listing_note = {
        let t = &report.totals;
        let per_session = t.listing_tokens.count.checked_div(t.listing_sessions);
        let per_request = baseline_total.count.checked_div(t.requests);
        match (per_session, per_request) {
            (Some(listed), Some(computed)) if t.listing_sessions > 0 && computed > 0 => {
                Some(format!(
                    "The client reported skill listings of {}{} tokens per session on average ({} sessions); the {} installed skills AgentLens found cost {} tokens per request. The listing also holds skills AgentLens has no file for (built into the client, supplied by plugins or the account); the files may also have changed since the session. So the two do not add up. Use the context view for what the client sent.",
                    if t.listing_tokens.measured { "" } else { "~" },
                    n(listed),
                    n(t.listing_sessions),
                    installed.len(),
                    n(computed),
                ))
            }
            _ => None,
        }
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
    let referenced = |s: &SkillStats| references.get(&s.name).is_some_and(|r| !r.is_empty());
    unused.sort_by(|a, b| {
        referenced(a)
            .cmp(&referenced(b))
            .then_with(|| b.baseline.tokens.count.cmp(&a.baseline.tokens.count))
            .then_with(|| a.name.cmp(&b.name))
    });
    if !unused.is_empty() {
        let items: Vec<Item> = unused
            .iter()
            .map(|s| Item {
                name: s.name.clone(),
                tokens: s.baseline.tokens,
                requests: s.baseline.requests,
                detail: {
                    let mut detail = format!(
                        "{} tokens in {} requests",
                        n(s.baseline.tokens.count),
                        n(s.baseline.requests)
                    );
                    if let Some(refs) = references.get(&s.name).filter(|r| !r.is_empty()) {
                        let labels: Vec<String> = refs.iter().map(Reference::label).collect();
                        detail.push_str(&format!("; referenced by {}", labels.join(", ")));
                    }
                    detail
                },
                referenced_by: references.get(&s.name).cloned().unwrap_or_default(),
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
        f.notes.extend(listing_note.clone());
        if unused.iter().any(|s| referenced(s)) {
            f.notes.push(
                "Skills that an installed agent, command or skill names are listed last. A mention in a file does not prove that the skill is used, only that something is meant to use it.".to_owned(),
            );
        }
        findings.push(f);
    }

    // Installed skills that are read through their files but never loaded.
    let mut by_hand: Vec<&SkillStats> = installed
        .iter()
        .copied()
        .filter(|s| s.invocations == 0 && s.file_uses > 0)
        .collect();
    by_hand.sort_by(|a, b| {
        b.baseline
            .tokens
            .count
            .cmp(&a.baseline.tokens.count)
            .then_with(|| a.name.cmp(&b.name))
    });
    if !by_hand.is_empty() {
        let items: Vec<Item> = by_hand
            .iter()
            .take(TOP)
            .map(|s| Item {
                name: s.name.clone(),
                tokens: s.baseline.tokens,
                requests: s.baseline.requests,
                detail: format!(
                    "{} tokens per request, {} in total; files used {}",
                    n(per_request(s)),
                    n(s.baseline.tokens.count),
                    times(s.file_uses)
                ),
                referenced_by: Vec::new(),
            })
            .collect();
        let total = Tokens {
            count: by_hand.iter().map(|s| s.baseline.tokens.count).sum(),
            measured: by_hand.iter().all(|s| s.baseline.tokens.measured),
        };
        let mut f = finding(
            FindingKind::ListedButLoadedByHand,
            by_hand.len(),
            items,
            Some(total),
        );
        f.share_of_baseline = share(total.count, baseline_total.count);
        f.share_of_total_tokens = share(total.count, total_tokens.count);
        if !total.measured {
            f.notes.push(estimate_note());
        }
        f.notes.push(
            "That the files were read on instruction of another skill is inferred: a file use does not prove who asked for it, and a skill that is also useful on its own can stay listed.".to_owned(),
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
                referenced_by: Vec::new(),
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
        f.notes.extend(listing_note.clone());
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
                referenced_by: Vec::new(),
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
                referenced_by: Vec::new(),
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
                referenced_by: Vec::new(),
            })
            .collect();
        let mut f = finding(FindingKind::LargeContext, heavy.len(), items, None);
        f.notes.push(format!(
            "Only skills with at least {MIN_FOLLOW_ON_REQUESTS} requests after them are listed. The sizes are what the client reported."
        ));
        findings.push(f);
    }

    // MCP servers: static context against calls.
    let per_call = |m: &McpServerUse| m.tokens.count / m.calls.max(1);
    let mut servers: Vec<&McpServerUse> = mcp.iter().filter(|m| m.tokens.count > 0).collect();
    servers.sort_by(|a, b| {
        per_call(b)
            .cmp(&per_call(a))
            .then_with(|| a.server.cmp(&b.server))
    });
    if !servers.is_empty() {
        let items: Vec<Item> = servers
            .iter()
            .take(TOP)
            .map(|m| Item {
                name: m.server.clone(),
                tokens: m.tokens,
                requests: m.requests,
                detail: format!(
                    "{} tokens per request, {} in total; {} tool {}, {} tokens per call",
                    n(m.tokens.count.checked_div(m.requests).unwrap_or(0)),
                    n(m.tokens.count),
                    n(m.calls),
                    if m.calls == 1 { "call" } else { "calls" },
                    n(per_call(m))
                ),
                referenced_by: Vec::new(),
            })
            .collect();
        let total = Tokens {
            count: servers.iter().map(|m| m.tokens.count).sum(),
            measured: servers.iter().all(|m| m.tokens.measured),
        };
        let mut f = finding(
            FindingKind::CostlyMcpServers,
            servers.len(),
            items,
            Some(total),
        );
        f.share_of_total_tokens = share(total.count, total_tokens.count);
        if !total.measured {
            f.notes
                .push("The sizes are estimates where the client did not report them.".to_owned());
        }
        f.notes.push(
            "Only servers whose context the client reports are listed (Claude Code today). A server that is called a lot can be worth its context; the tokens per call show the ratio.".to_owned(),
        );
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
                tokens: Tokens {
                    count: c.average_context,
                    measured: totals.measured,
                },
                requests: c.requests,
                detail: format!(
                    "{} tokens per request on average over {} requests",
                    n(c.average_context),
                    n(c.requests)
                ),
                referenced_by: Vec::new(),
            })
            .collect();
        let mut f = finding(
            FindingKind::ContextSize,
            report.sessions as usize,
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
            ..PeriodTotals::default()
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
        assert_eq!(c.count, 3, "the sessions analyzed, not the ones listed");
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

    #[test]
    fn estimated_requests_are_marked_and_the_carry_gets_a_share() {
        use crate::analyzers::skills::{PeriodTotals, SessionContext};
        let mut big = skill("big", true, 1, 0, (10, 1_000));
        big.carry.tokens = Tokens::estimated(5_000);
        big.carry.requests = 40;
        let mut r = report(vec![big]);
        r.totals = PeriodTotals {
            requests: 10,
            tokens: 100_000,
            context_tokens: 90_000,
            measured: false,
            largest_sessions: vec![SessionContext {
                id: "s1".into(),
                project: None,
                requests: 10,
                average_context: 9_000,
            }],
            ..PeriodTotals::default()
        };
        let a = analyze(&r);
        let bodies = find(&a, FindingKind::LargeBodies);
        assert_eq!(bodies.total, Some(Tokens::estimated(5_000)));
        assert_eq!(bodies.share_of_total_tokens, Some(0.05));
        let costly = find(&a, FindingKind::CostlyDescriptions);
        assert!(costly.share_of_total_tokens.is_some());
        let c = find(&a, FindingKind::ContextSize);
        assert!(!c.total.unwrap().measured);
        assert!(!c.items[0].tokens.measured);
        assert!(c.notes.iter().any(|n| n.contains("estimated")));
        assert_eq!(a.findings.last().unwrap().kind, FindingKind::ContextSize);
    }

    #[test]
    fn zero_tokens_give_no_share() {
        use crate::analyzers::skills::PeriodTotals;
        let mut r = report(vec![skill("idle", true, 0, 0, (10, 100))]);
        r.totals = PeriodTotals {
            requests: 1,
            ..PeriodTotals::default()
        };
        let a = analyze(&r);
        assert_eq!(
            find(&a, FindingKind::UnusedSkills).share_of_total_tokens,
            None
        );
        assert_eq!(
            find(&a, FindingKind::ContextSize).share_of_total_tokens,
            None
        );
    }

    #[test]
    fn skills_that_are_only_used_through_their_files_have_their_own_finding() {
        let by_hand = {
            let mut s = skill("stack", true, 0, 3, (10, 2_000));
            s.baseline.tokens = Tokens::estimated(2_000);
            s
        };
        let a = analyze(&report(vec![
            by_hand,
            skill("idle", true, 0, 0, (10, 500)),
            skill("loaded", true, 2, 4, (10, 800)),
            skill("gone", false, 0, 2, (10, 900)),
        ]));
        let f = find(&a, FindingKind::ListedButLoadedByHand);
        assert_eq!(f.count, 1);
        assert_eq!(f.items[0].name, "stack");
        assert_eq!(f.items[0].requests, 10);
        assert!(f.items[0].detail.contains("files used 3 times"));
        assert_eq!(f.total, Some(Tokens::estimated(2_000)));
        assert!(f.notes.iter().any(|n| n.contains("inferred")));
        // `unused` keeps only skills with neither loads nor file uses.
        let unused = find(&a, FindingKind::UnusedSkills);
        assert_eq!(unused.count, 1);
        assert_eq!(unused.items[0].name, "idle");
    }

    #[test]
    fn no_file_only_skill_no_finding() {
        let a = analyze(&report(vec![skill("loaded", true, 2, 4, (10, 800))]));
        assert!(
            a.findings
                .iter()
                .all(|f| f.kind != FindingKind::ListedButLoadedByHand)
        );
    }

    #[test]
    fn file_only_skills_are_sorted_cut_and_measured_only_if_all_are() {
        let mut skills: Vec<SkillStats> = (0..12)
            .map(|i| {
                let mut s = skill(&format!("s{i:02}"), true, 0, 1, (10, 100 + i));
                s.baseline.tokens = Tokens::measured(100 + i);
                s
            })
            .collect();
        skills.push(skill("tie-b", true, 0, 1, (10, 111)));
        skills.push(skill("tie-a", true, 0, 1, (10, 111)));
        let mut r = report(skills.clone());
        r.totals.tokens = 1_000_000;
        let f = find(&analyze(&r), FindingKind::ListedButLoadedByHand).clone();
        assert_eq!(f.count, 14);
        assert_eq!(f.items.len(), TOP);
        let names: Vec<&str> = f.items.iter().take(4).map(|i| i.name.as_str()).collect();
        assert_eq!(
            names,
            ["s11", "tie-a", "tie-b", "s10"],
            "most tokens first, equal tokens by name"
        );
        assert!(!f.total.unwrap().measured, "the tie skills are estimated");
        assert_eq!(f.total.unwrap().count, (100..112).sum::<u64>() + 222);
        assert!(f.share_of_total_tokens.is_some());

        skills.truncate(12);
        let f = find(
            &analyze(&report(skills)),
            FindingKind::ListedButLoadedByHand,
        )
        .clone();
        assert!(f.total.unwrap().measured);
    }

    #[test]
    fn unused_skills_that_something_names_are_marked_and_ranked_last() {
        use crate::references::ReferenceKind;
        let mut refs = References::new();
        refs.insert(
            "big".into(),
            vec![Reference {
                kind: ReferenceKind::Agent,
                name: "flow".into(),
            }],
        );
        let r = report(vec![
            skill("big", true, 0, 0, (10, 9_000)),
            skill("small", true, 0, 0, (10, 100)),
            skill("mid", true, 0, 0, (10, 500)),
        ]);
        let a = analyze_with(&r, &refs, &[]);
        let f = find(&a, FindingKind::UnusedSkills);
        let names: Vec<&str> = f.items.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(
            names,
            ["mid", "small", "big"],
            "referenced skills come last"
        );
        assert_eq!(f.items[2].referenced_by.len(), 1);
        assert!(f.items[2].detail.contains("referenced by agent flow"));
        assert!(f.items[0].referenced_by.is_empty());
        assert!(!f.items[0].detail.contains("referenced"));
        assert!(f.notes.iter().any(|n| n.contains("does not prove")));
        // Without references nothing changes.
        let plain = analyze(&r);
        let f = find(&plain, FindingKind::UnusedSkills);
        assert_eq!(f.items[0].name, "big");
        assert!(f.notes.iter().all(|n| !n.contains("does not prove")));
    }

    fn mcp_row(
        owner: &str,
        kind: ContextKind,
        requests: u64,
        overhead: u64,
        measured: bool,
    ) -> crate::analyzers::context::ContextRow {
        crate::analyzers::context::ContextRow {
            source: crate::model::Source::ClaudeCode,
            project: None,
            kind,
            owner: owner.into(),
            items: 1,
            bytes: 0,
            tokens: Tokens::estimated(0),
            requests,
            overhead_tokens: Tokens {
                count: overhead,
                measured,
            },
        }
    }

    fn calls(tool: &str, n: usize) -> SessionEvents {
        use crate::model::{Event, Session, Source, ToolCall, ToolStatus};
        SessionEvents {
            session: Session::new("s", Source::ClaudeCode),
            events: (0..n)
                .map(|_| Event {
                    at: None,
                    kind: EventKind::ToolCall(ToolCall {
                        name: tool.into(),
                        input_summary: None,
                        status: ToolStatus::Ok,
                        duration_ms: None,
                    }),
                })
                .collect(),
            warnings: Vec::new(),
        }
    }

    #[test]
    fn mcp_servers_are_ranked_by_static_context_per_call() {
        use crate::analyzers::context::ContextReport;
        let context = ContextReport {
            rows: vec![
                mcp_row(
                    "claude_ai_Claude_Docs",
                    ContextKind::ToolListing,
                    100,
                    1_000,
                    true,
                ),
                mcp_row(
                    "claude_ai_Claude_Docs",
                    ContextKind::ToolDefinition,
                    100,
                    3_000,
                    true,
                ),
                mcp_row(
                    "claude.ai Claude Docs",
                    ContextKind::McpInstructions,
                    100,
                    500,
                    false,
                ),
                mcp_row("busy", ContextKind::ToolListing, 100, 6_000, true),
                mcp_row("built-in", ContextKind::ToolListing, 100, 99_999, true),
                mcp_row("all tools", ContextKind::ToolDefinition, 100, 99_999, true),
            ],
            ..ContextReport::default()
        };
        let sessions = [
            calls("mcp__claude_ai_Claude_Docs__read", 2),
            calls("mcp__busy__run", 60),
            calls("Read", 5),
        ];
        let servers = mcp_servers(&context, &sessions);
        assert_eq!(servers.len(), 2, "built-in and all tools are no servers");
        let docs = servers
            .iter()
            .find(|m| m.server == "claude_ai_Claude_Docs")
            .unwrap();
        assert_eq!(
            (docs.tokens.count, docs.calls, docs.requests),
            (4_500, 2, 100)
        );
        assert!(!docs.tokens.measured, "one row was estimated");

        let mut r = report(vec![]);
        r.totals.tokens = 100_000;
        let a = analyze_with(&r, &References::new(), &servers);
        let f = find(&a, FindingKind::CostlyMcpServers);
        assert_eq!(f.count, 2);
        // 4,500 / 2 = 2,250 per call; 6,000 / 60 = 100 per call.
        assert_eq!(f.items[0].name, "claude_ai_Claude_Docs");
        assert!(
            f.items[0]
                .detail
                .contains("2 tool calls, 2,250 tokens per call"),
            "{}",
            f.items[0].detail
        );
        assert_eq!(
            f.items[1].name, "busy",
            "a busy server is listed, with its ratio"
        );
        assert_eq!(f.total, Some(Tokens::estimated(10_500)));
        assert_eq!(f.share_of_total_tokens, Some(0.105));
        assert!(f.action.contains("only in the projects that need it"));
    }

    #[test]
    fn a_server_without_calls_costs_all_of_its_context_per_call() {
        let unused = McpServerUse {
            server: "idle".into(),
            tokens: Tokens::measured(700),
            requests: 7,
            calls: 0,
        };
        let busy = McpServerUse {
            server: "busy".into(),
            tokens: Tokens::measured(800),
            requests: 8,
            calls: 4,
        };
        let a = analyze_with(&report(vec![]), &References::new(), &[busy, unused]);
        let f = find(&a, FindingKind::CostlyMcpServers);
        assert_eq!(f.items[0].name, "idle");
        assert!(
            f.items[0]
                .detail
                .contains("0 tool calls, 700 tokens per call")
        );
        assert!(f.items[1].detail.contains("4 tool calls"));
        assert!(f.total.unwrap().measured);
        assert!(find_none(
            &analyze(&report(vec![])),
            FindingKind::CostlyMcpServers
        ));
    }

    #[test]
    fn requests_add_up_over_clients_and_projects_and_calls_stay_in_those_sessions() {
        use crate::analyzers::context::ContextReport;
        use crate::model::Session;
        let mut a = mcp_row("srv", ContextKind::ToolListing, 40, 400, true);
        a.project = Some("/work/a".into());
        let mut b = mcp_row("srv", ContextKind::ToolListing, 60, 600, true);
        b.project = Some("/work/b".into());
        let mut b_def = mcp_row("srv", ContextKind::ToolDefinition, 50, 500, true);
        b_def.project = Some("/work/b".into());
        let context = ContextReport {
            rows: vec![a, b, b_def],
            ..ContextReport::default()
        };
        let call = |source: Source, project: &str| {
            let mut c = calls("mcp__srv__run", 1);
            c.session = Session::new("x", source);
            c.session.project_dir = Some(project.into());
            c
        };
        let sessions = [
            call(Source::ClaudeCode, "/work/a"),
            call(Source::ClaudeCode, "/work/b"),
            call(Source::ClaudeCode, "/work/other"),
            call(Source::OpenCode, "/work/a"),
        ];
        let servers = mcp_servers(&context, &sessions);
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].tokens.count, 1_500);
        assert_eq!(servers[0].requests, 100, "40 in a, the larger 60 in b");
        assert_eq!(
            servers[0].calls, 2,
            "other projects and clients do not count"
        );
    }

    #[test]
    fn the_name_of_a_server_does_not_depend_on_the_row_order() {
        use crate::analyzers::context::ContextReport;
        let rows = vec![
            mcp_row("claude.ai Docs", ContextKind::McpInstructions, 1, 10, true),
            mcp_row("claude_ai_Docs", ContextKind::ToolListing, 1, 10, true),
        ];
        let forward = mcp_servers(
            &ContextReport {
                rows: rows.clone(),
                ..ContextReport::default()
            },
            &[],
        );
        let backward = mcp_servers(
            &ContextReport {
                rows: rows.into_iter().rev().collect(),
                ..ContextReport::default()
            },
            &[],
        );
        assert_eq!(forward[0].server, "claude_ai_Docs");
        assert_eq!(forward, backward);
    }

    #[test]
    fn servers_with_the_same_cost_per_call_are_ordered_by_name() {
        let server = |name: &str| McpServerUse {
            server: name.into(),
            tokens: Tokens::measured(100),
            requests: 1,
            calls: 1,
        };
        let a = analyze_with(
            &report(vec![]),
            &References::new(),
            &[server("zeta"), server("alpha")],
        );
        let names: Vec<&str> = find(&a, FindingKind::CostlyMcpServers)
            .items
            .iter()
            .map(|i| i.name.as_str())
            .collect();
        assert_eq!(names, ["alpha", "zeta"]);
    }

    fn find_none(r: &AdviceReport, kind: FindingKind) -> bool {
        r.findings.iter().all(|f| f.kind != kind)
    }

    #[test]
    fn the_client_listing_is_compared_with_the_computed_baseline() {
        use crate::analyzers::skills::PeriodTotals;
        let mut r = report(vec![skill("idle", true, 0, 0, (100, 20_000))]);
        r.totals = PeriodTotals {
            requests: 100,
            listing_sessions: 2,
            listing_tokens: Tokens::estimated(12_000),
            ..PeriodTotals::default()
        };
        let a = analyze(&r);
        for kind in [FindingKind::UnusedSkills, FindingKind::CostlyDescriptions] {
            let note = find(&a, kind)
                .notes
                .iter()
                .find(|n| n.contains("skill listings"))
                .unwrap_or_else(|| panic!("{kind:?} has no listing note"));
            assert!(note.contains("~6,000 tokens per session"), "{note}");
            assert!(note.contains("(2 sessions)"), "{note}");
            assert!(
                note.contains("1 installed skills AgentLens found cost 200 tokens per request"),
                "{note}"
            );
        }
        // Reported by the client: no tilde; no listing: no note.
        r.totals.listing_tokens.measured = true;
        let a = analyze(&r);
        let notes = &find(&a, FindingKind::UnusedSkills).notes;
        assert!(notes.iter().any(|n| n.contains("listings of 6,000 tokens")));
        r.totals.listing_sessions = 0;
        assert!(
            find(&analyze(&r), FindingKind::UnusedSkills)
                .notes
                .iter()
                .all(|n| !n.contains("skill listings"))
        );
    }
}
