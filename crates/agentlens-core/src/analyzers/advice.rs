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
use crate::model::{ContextKind, EventKind, SessionEvents, Source, Usage};
use crate::prices::PriceTable;
use crate::references::{Reference, References};
use crate::tokens::Tokens;

/// A description of about this many tokens is enough to tell when a skill applies.
pub const DESCRIPTION_TARGET_TOKENS: u64 = 40;
/// A skill body of about this many tokens holds what the model has to decide; what is above it is
/// what moving deterministic steps into a script could save (the saving of Large skill bodies).
pub const BODY_TARGET_TOKENS: u64 = 2_000;
/// Without any price the findings are ordered by their saving in tokens, each token class weighted by
/// what it costs next to uncached input on the usual price lists: input 1, cache write 1.25, cache
/// read 0.1.
pub const INPUT_WEIGHT: f64 = 1.0;
pub const CACHE_WRITE_WEIGHT: f64 = 1.25;
pub const CACHE_READ_WEIGHT: f64 = 0.1;
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

/// How sure the saving of a finding is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Confidence {
    /// Every number behind the saving was reported by the client (a finding without a saving too).
    #[default]
    Measured,
    /// Some numbers behind the saving were computed from file sizes or heuristics.
    Estimated,
    /// The saving rests on an assumption about how the work could have run instead.
    Inferred,
}

/// What the action of a finding could have saved in the period, not what the finding is about.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Impact {
    pub saving: Tokens,
    /// The saving in USD; always computed, never reported. `None` without a price, and for the kinds
    /// that have no saving.
    pub saving_usd: Option<f64>,
    pub confidence: Confidence,
}

impl Impact {
    /// The impact of the kinds whose action saves nothing that can be counted.
    fn none() -> Impact {
        Impact {
            saving: Tokens::measured(0),
            saving_usd: None,
            confidence: Confidence::Measured,
        }
    }
}

/// Saved tokens split by token class: what prices a saving, and what weighs it without a price.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Split {
    input: f64,
    cache_read: f64,
    cache_write: f64,
}

impl Split {
    fn cache_read(tokens: u64) -> Split {
        Split {
            cache_read: tokens as f64,
            ..Split::default()
        }
    }

    /// The fallback weight: tokens weighted by class.
    fn weight(&self) -> f64 {
        self.input * INPUT_WEIGHT
            + self.cache_write * CACHE_WRITE_WEIGHT
            + self.cache_read * CACHE_READ_WEIGHT
    }
}

/// The average price per token class over the period, in USD per million tokens: each class weighted
/// with that class's tokens over all priced requests (a class without tokens in the period by the
/// requests). It prices the savings of the findings that do not know their requests (skill listings,
/// bodies, MCP servers).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Pricing {
    pub input: f64,
    pub cache_read: f64,
    pub cache_write: f64,
    /// The requests the averages were taken over.
    pub priced_requests: u64,
    /// Requests whose model has no price; they are left out of the averages.
    pub unpriced_requests: u64,
}

impl Pricing {
    /// The averages over all requests of the sessions, main agent and subagents.
    pub fn from_sessions(sessions: &[SessionEvents], prices: &PriceTable) -> Pricing {
        // Per class: price × tokens, tokens and the plain sum of the prices.
        let mut sums = [(0.0f64, 0u64, 0.0f64); 3];
        let mut pricing = Pricing::default();
        for u in sessions
            .iter()
            .flat_map(|se| &se.events)
            .filter_map(|e| match &e.kind {
                EventKind::Usage(u) => Some(u),
                _ => None,
            })
        {
            let Some(r) = u
                .model
                .as_deref()
                .and_then(|m| prices.rates(m, u.total_input()))
            else {
                pricing.unpriced_requests += 1;
                continue;
            };
            pricing.priced_requests += 1;
            for (sum, (rate, tokens)) in sums.iter_mut().zip([
                (r.input, u.input),
                (r.cache_read, u.cache_read),
                (r.cache_write, u.cache_write),
            ]) {
                sum.0 += rate * tokens as f64;
                sum.1 += tokens;
                sum.2 += rate;
            }
        }
        let average = |(weighted, tokens, plain): (f64, u64, f64)| {
            if tokens > 0 {
                weighted / tokens as f64
            } else if pricing.priced_requests > 0 {
                plain / pricing.priced_requests as f64
            } else {
                0.0
            }
        };
        pricing.input = average(sums[0]);
        pricing.cache_read = average(sums[1]);
        pricing.cache_write = average(sums[2]);
        pricing
    }

    /// `true` if at least one request of the period had a price.
    pub fn is_priced(&self) -> bool {
        self.priced_requests > 0
    }

    fn usd(&self, split: &Split) -> Option<f64> {
        self.is_priced().then(|| {
            (split.input * self.input
                + split.cache_read * self.cache_read
                + split.cache_write * self.cache_write)
                / 1_000_000.0
        })
    }
}

/// What starting fresh sessions could have saved: on every main-agent request with at least
/// [`LARGE_CONTEXT_TOKENS`] of input, the input above the size of its session's first main-agent request.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ContextSaving {
    /// The saved tokens, split in each request's own input : cache read : cache write ratio.
    pub input: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    /// The main-agent requests with at least `LARGE_CONTEXT_TOKENS` of input.
    pub requests: u64,
    /// The saving of the priced requests, each priced with its own model.
    pub usd: f64,
    /// Requests with a saving, with and without a price for their model.
    pub priced_requests: u64,
    pub unpriced_requests: u64,
}

impl ContextSaving {
    pub fn tokens(&self) -> u64 {
        self.input + self.cache_read + self.cache_write
    }
}

/// The saving of the Context size finding, from the requests the skill report does not keep.
///
/// When ContextHygiene (#346) lands, it subtracts what that finding already counts, so that nothing is
/// counted twice.
pub fn context_saving(sessions: &[SessionEvents], prices: &PriceTable) -> ContextSaving {
    let mut out = ContextSaving::default();
    for se in sessions {
        let mut first: Option<u64> = None;
        for u in se.events.iter().filter_map(|e| match &e.kind {
            EventKind::Usage(u) if !u.subagent => Some(u),
            _ => None,
        }) {
            let total = u.total_input();
            let start = *first.get_or_insert(total);
            if total < LARGE_CONTEXT_TOKENS {
                continue;
            }
            out.requests += 1;
            let saved = total.saturating_sub(start);
            if saved == 0 {
                continue;
            }
            let part =
                |class: u64| (u128::from(saved) * u128::from(class) / u128::from(total)) as u64;
            let (input, cache_write) = (part(u.input), part(u.cache_write));
            let saved_usage = Usage {
                model: u.model.clone(),
                input,
                cache_write,
                cache_read: saved - input - cache_write,
                ..Usage::default()
            };
            out.input += saved_usage.input;
            out.cache_read += saved_usage.cache_read;
            out.cache_write += saved_usage.cache_write;
            match u
                .model
                .as_deref()
                .and_then(|m| prices.compute(m, &saved_usage))
            {
                Some(usd) => {
                    out.usd += usd;
                    out.priced_requests += 1;
                }
                None => out.unpriced_requests += 1,
            }
        }
    }
    out
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
    /// The place in the order of the findings, 1 for the largest saving.
    #[serde(default)]
    pub rank: u32,
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
    /// What the finding's action could have saved in the period.
    #[serde(default)]
    pub impact: Impact,
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
        rank: 0,
        kind,
        explanation: kind.explanation().to_owned(),
        action: kind.action().to_owned(),
        count: count as u32,
        total,
        share_of_baseline: None,
        share_of_total_tokens: None,
        items,
        notes: Vec::new(),
        impact: Impact::none(),
    }
}

fn per_request(s: &SkillStats) -> u64 {
    s.baseline
        .tokens
        .count
        .checked_div(s.baseline.requests)
        .unwrap_or(0)
}

/// The findings without references, MCP servers, requests or prices (for tests).
pub fn analyze(report: &SkillReport) -> AdviceReport {
    analyze_with(
        report,
        &References::new(),
        &[],
        &ContextSaving::default(),
        &Pricing::default(),
    )
}

/// What one MCP server costs in static context and how often its tools were called.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpServerUse {
    pub server: String,
    /// Instructions, tool listing and tool definitions, per request times requests. Measured only if
    /// the client reported every size.
    pub tokens: Tokens,
    /// The part of `tokens` in the (client, project) groups in which none of the server's tools was
    /// called: what enabling the server only where it is used would have saved.
    pub idle_tokens: Tokens,
    /// The main-agent requests that carried the server's context.
    pub requests: u64,
    /// Calls of the server's tools (`mcp__<server>__…`) in the period.
    pub calls: u64,
}

/// The MCP servers whose context the clients reported, with the calls of their tools. Servers that the
/// clients do not report (nothing in the context rows) are not listed.
pub fn mcp_servers(context: &ContextReport, sessions: &[SessionEvents]) -> Vec<McpServerUse> {
    #[derive(Default)]
    struct Group {
        /// The most requests of any row: the rows of a group cover the same requests.
        requests: u64,
        tokens: Option<Tokens>,
    }
    #[derive(Default)]
    struct Found {
        /// Tool rows name the server the way its tools do; their owner is preferred, then the smaller one.
        name: Option<(bool, String)>,
        /// Per client and project: those are disjoint sets of requests.
        groups: BTreeMap<(Source, Option<PathBuf>), Group>,
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
        let group = entry
            .groups
            .entry((r.source, r.project.clone()))
            .or_default();
        let tokens = group.tokens.get_or_insert(Tokens::measured(0));
        tokens.count += r.overhead_tokens.count;
        tokens.measured &= r.overhead_tokens.measured;
        group.requests = group.requests.max(r.requests);
    }
    let add = |a: Tokens, b: Tokens| Tokens {
        count: a.count + b.count,
        measured: a.measured && b.measured,
    };
    found
        .into_iter()
        .map(|(key, f)| {
            let in_group = |se: &SessionEvents, (source, project): &(Source, Option<PathBuf>)| {
                *source == se.session.source
                    && (project.is_none() || *project == se.session.project_dir)
            };
            let calls_in = |se: &SessionEvents| {
                se.events
                    .iter()
                    .filter(|e| {
                        matches!(&e.kind, EventKind::ToolCall(c) if calls_server(&c.name, &key))
                    })
                    .count() as u64
            };
            // Only calls in the sessions whose context was reported: the ratio compares like with like.
            let calls = sessions
                .iter()
                .filter(|se| f.groups.keys().any(|g| in_group(se, g)))
                .map(calls_in)
                .sum();
            let mut tokens = Tokens::measured(0);
            let mut idle_tokens = Tokens::measured(0);
            for (g, group) in &f.groups {
                let group_tokens = group.tokens.unwrap_or(Tokens::measured(0));
                tokens = add(tokens, group_tokens);
                let group_calls: u64 = sessions
                    .iter()
                    .filter(|se| in_group(se, g))
                    .map(calls_in)
                    .sum();
                if group_calls == 0 {
                    idle_tokens = add(idle_tokens, group_tokens);
                }
            }
            McpServerUse {
                server: f.name.map(|n| n.1).unwrap_or_default(),
                tokens,
                idle_tokens,
                requests: f.groups.values().map(|g| g.requests).sum(),
                calls,
            }
        })
        .collect()
}

/// Like [`analyze`], and marks the unused skills that an agent, a command or a skill names, adds the MCP
/// servers, and gives every finding its impact, priced with `pricing` (the Context size finding with
/// the prices of `context`). The findings come ranked by that impact.
pub fn analyze_with(
    report: &SkillReport,
    references: &References,
    mcp: &[McpServerUse],
    context: &ContextSaving,
    pricing: &Pricing,
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
    // Each finding with the weight of its saving for the order without prices.
    let mut findings: Vec<(Finding, f64)> = Vec::new();
    let unpriced_note = |f: &mut Finding| {
        if f.impact.saving.count > 0 && pricing.is_priced() && pricing.unpriced_requests > 0 {
            f.notes.push(format!(
                "The USD saving uses the average price per token class of the period; {} {} without a price {} left out of it.",
                n(pricing.unpriced_requests),
                if pricing.unpriced_requests == 1 { "request" } else { "requests" },
                if pricing.unpriced_requests == 1 { "is" } else { "are" },
            ));
        }
    };
    // A saving that is all listing tokens, priced as cache read.
    let listing_impact = |saving: Tokens, confidence: Confidence| {
        let split = Split::cache_read(saving.count);
        (
            Impact {
                saving,
                saving_usd: pricing.usd(&split),
                confidence,
            },
            split.weight(),
        )
    };
    let confidence_of = |t: Tokens| {
        if t.measured {
            Confidence::Measured
        } else {
            Confidence::Estimated
        }
    };

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
        let (impact, weight) = listing_impact(total, confidence_of(total));
        f.impact = impact;
        unpriced_note(&mut f);
        findings.push((f, weight));
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
        let (impact, weight) = listing_impact(total, confidence_of(total));
        f.impact = impact;
        unpriced_note(&mut f);
        findings.push((f, weight));
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
        let (impact, weight) = listing_impact(total, Confidence::Estimated);
        f.impact = impact;
        unpriced_note(&mut f);
        findings.push((f, weight));
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
        findings.push((f, 0.0));
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
        // The body above the target, times the requests that carried it, split like the skill's carry.
        let mut split = Split::default();
        let mut saved = 0;
        for s in &bodies {
            let per_load = s
                .load
                .count
                .checked_div(u64::from(s.invocations))
                .unwrap_or(0);
            let tokens = per_load.saturating_sub(BODY_TARGET_TOKENS) * s.carry.requests;
            saved += tokens;
            let carried = s.carry.cache_read_tokens + s.carry.uncached_tokens;
            let read_share = if carried > 0 {
                s.carry.cache_read_tokens as f64 / carried as f64
            } else {
                1.0
            };
            split.cache_read += tokens as f64 * read_share;
            split.input += tokens as f64 * (1.0 - read_share);
        }
        f.impact = Impact {
            saving: Tokens::estimated(saved),
            saving_usd: pricing.usd(&split),
            confidence: Confidence::Estimated,
        };
        f.notes.push(format!(
            "The saving is the part of each body above {} tokens, times the requests that carried it.",
            n(BODY_TARGET_TOKENS)
        ));
        unpriced_note(&mut f);
        findings.push((f, split.weight()));
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
        findings.push((f, 0.0));
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
        let idle = Tokens {
            count: servers.iter().map(|m| m.idle_tokens.count).sum(),
            measured: servers.iter().all(|m| m.idle_tokens.measured),
        };
        let (impact, weight) = listing_impact(idle, confidence_of(idle));
        f.impact = impact;
        f.notes.push(
            "The saving is the context of each server in the clients and projects where none of its tools was called.".to_owned(),
        );
        unpriced_note(&mut f);
        findings.push((f, weight));
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
        let saved = context.tokens();
        let split = Split {
            input: context.input as f64,
            cache_read: context.cache_read as f64,
            cache_write: context.cache_write as f64,
        };
        f.impact = Impact {
            saving: Tokens::estimated(saved),
            // Each request priced with its own model; nothing to price is a priced 0.
            saving_usd: (context.priced_requests > 0 || (saved == 0 && pricing.is_priced()))
                .then_some(context.usd),
            confidence: Confidence::Inferred,
        };
        f.notes.push(format!(
            "The saving assumes that the work on requests of at least {} tokens could have run in a fresh session, at the size of the session's first request; it is inferred.",
            n(LARGE_CONTEXT_TOKENS)
        ));
        if saved > 0 && context.unpriced_requests > 0 {
            f.notes.push(format!(
                "{} of the requests with a saving had no price for their model; their tokens are in the saving, not in the USD.",
                n(context.unpriced_requests)
            ));
        }
        findings.push((f, split.weight()));
    }

    // Ranked by the USD saving, then by the weighted tokens, then in the order above; findings without
    // a saving last, in the order above (the sort is stable).
    findings.sort_by(|(a, wa), (b, wb)| {
        let (sa, sb) = (a.impact.saving.count > 0, b.impact.saving.count > 0);
        sb.cmp(&sa).then_with(|| {
            if !(sa && sb) {
                return std::cmp::Ordering::Equal;
            }
            let usd = match (a.impact.saving_usd, b.impact.saving_usd) {
                (Some(x), Some(y)) => y.total_cmp(&x),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            };
            usd.then_with(|| wb.total_cmp(wa))
        })
    });
    let findings = findings
        .into_iter()
        .enumerate()
        .map(|(i, (mut f, _))| {
            f.rank = i as u32 + 1;
            f
        })
        .collect();

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

    /// `analyze_with` without requests and prices.
    fn with(r: &SkillReport, refs: &References, mcp: &[McpServerUse]) -> AdviceReport {
        analyze_with(r, refs, mcp, &ContextSaving::default(), &Pricing::default())
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
        let a = with(&r, &refs, &[]);
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
        let a = with(&r, &References::new(), &servers);
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
            idle_tokens: Tokens::measured(700),
            requests: 7,
            calls: 0,
        };
        let busy = McpServerUse {
            server: "busy".into(),
            tokens: Tokens::measured(800),
            idle_tokens: Tokens::measured(0),
            requests: 8,
            calls: 4,
        };
        let a = with(&report(vec![]), &References::new(), &[busy, unused]);
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
            idle_tokens: Tokens::measured(0),
            requests: 1,
            calls: 1,
        };
        let a = with(
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

    // ---- impact and rank ----

    /// Prices per million tokens: cache read 0.5, input 3, cache write 3.75.
    fn pricing() -> Pricing {
        Pricing {
            input: 3.0,
            cache_read: 0.5,
            cache_write: 3.75,
            priced_requests: 10,
            unpriced_requests: 0,
        }
    }

    fn close(a: Option<f64>, b: f64) -> bool {
        a.is_some_and(|a| (a - b).abs() < 1e-9)
    }

    /// A price table with one test model: $2 input, $10 output, $2.50 cache write, $0.10 cache read.
    fn prices() -> PriceTable {
        PriceTable::parse(
            r#"{"test-model": {"input": 2.0, "output": 10.0, "cache_write": 2.5, "cache_read": 0.1}}"#,
        )
        .unwrap()
    }

    /// One request: (model, input, cache read, cache write, subagent).
    type Req<'a> = (&'a str, u64, u64, u64, bool);

    fn requests(id: &str, reqs: &[Req]) -> SessionEvents {
        use crate::model::{Event, Session};
        SessionEvents {
            session: Session::new(id, Source::ClaudeCode),
            events: reqs
                .iter()
                .map(|&(model, input, cache_read, cache_write, subagent)| Event {
                    at: None,
                    kind: EventKind::Usage(Usage {
                        model: Some(model.into()),
                        input,
                        cache_read,
                        cache_write,
                        subagent,
                        ..Usage::default()
                    }),
                })
                .collect(),
            warnings: Vec::new(),
        }
    }

    fn priced(r: &SkillReport, mcp: &[McpServerUse], context: &ContextSaving) -> AdviceReport {
        analyze_with(r, &References::new(), mcp, context, &pricing())
    }

    #[test]
    fn listing_savings_are_their_totals_priced_as_cache_read() {
        let mut measured = skill("idle", true, 0, 0, (100, 4_000));
        measured.baseline.tokens = Tokens::measured(4_000);
        let a = priced(
            &report(vec![
                measured,
                skill("by-hand", true, 0, 2, (100, 2_000)),
                skill("long", true, 3, 0, (100, 13_200)), // 132 per request
            ]),
            &[],
            &ContextSaving::default(),
        );
        let unused = find(&a, FindingKind::UnusedSkills);
        assert_eq!(unused.impact.saving, Tokens::measured(4_000));
        assert_eq!(unused.impact.confidence, Confidence::Measured);
        // 4,000 × $0.50 / M
        assert!(close(unused.impact.saving_usd, 0.002));
        let by_hand = find(&a, FindingKind::ListedButLoadedByHand);
        assert_eq!(by_hand.impact.saving, Tokens::estimated(2_000));
        assert_eq!(by_hand.impact.confidence, Confidence::Estimated);
        assert!(close(by_hand.impact.saving_usd, 0.001));
        let costly = find(&a, FindingKind::CostlyDescriptions);
        // Per request: idle 40, by-hand 20, long 132. Only `long` is above the target:
        // (132 - 40) × 100 = 9,200.
        assert_eq!(costly.impact.saving, Tokens::estimated(9_200));
        assert_eq!(costly.impact.confidence, Confidence::Estimated);
        assert!(close(costly.impact.saving_usd, 0.0046));
    }

    #[test]
    fn large_bodies_save_the_body_above_the_target_times_its_requests() {
        // 10,000 tokens in 2 loads = 5,000 per load; (5,000 - 2,000) × 40 requests = 120,000.
        let mut big = skill("big", true, 2, 0, (10, 100));
        big.load = Tokens::measured(10_000);
        big.carry = CarryCost {
            requests: 40,
            tokens: Tokens::estimated(200_000),
            cache_read_tokens: 150_000,
            uncached_tokens: 50_000,
        };
        // 1,500 per load is below the target: nothing to save.
        let mut small = skill("small", true, 1, 0, (10, 100));
        small.load = Tokens::measured(1_500);
        small.carry = CarryCost {
            requests: 30,
            tokens: Tokens::estimated(45_000),
            cache_read_tokens: 45_000,
            uncached_tokens: 0,
        };
        let a = priced(&report(vec![big, small]), &[], &ContextSaving::default());
        let f = find(&a, FindingKind::LargeBodies);
        assert_eq!(f.impact.saving, Tokens::estimated(120_000));
        assert_eq!(f.impact.confidence, Confidence::Estimated);
        // 75% of the carry was cache read: 90,000 × $0.50 + 30,000 × $3 per M = $0.135.
        assert!(close(f.impact.saving_usd, 0.135), "{:?}", f.impact);
        assert!(f.notes.iter().any(|n| n.contains("above 2,000 tokens")));
    }

    #[test]
    fn mcp_servers_save_their_context_where_they_were_not_called() {
        use crate::analyzers::context::ContextReport;
        use crate::model::Session;
        let mut a = mcp_row("srv", ContextKind::ToolListing, 40, 400, true);
        a.project = Some("/work/a".into());
        let mut b = mcp_row("srv", ContextKind::ToolListing, 60, 600, true);
        b.project = Some("/work/b".into());
        let mut b_def = mcp_row("srv", ContextKind::ToolDefinition, 60, 900, false);
        b_def.project = Some("/work/b".into());
        let context = ContextReport {
            rows: vec![a, b, b_def],
            ..ContextReport::default()
        };
        let mut called = calls("mcp__srv__run", 3);
        called.session = Session::new("x", Source::ClaudeCode);
        called.session.project_dir = Some("/work/a".into());
        let servers = mcp_servers(&context, &[called]);
        assert_eq!(servers[0].tokens.count, 1_900);
        assert_eq!(servers[0].calls, 3);
        assert_eq!(
            servers[0].idle_tokens,
            Tokens::estimated(1_500),
            "called in a, idle in b: only b saves"
        );

        let f = priced(&report(vec![]), &servers, &ContextSaving::default());
        let f = find(&f, FindingKind::CostlyMcpServers);
        assert_eq!(f.impact.saving, Tokens::estimated(1_500));
        assert_eq!(f.impact.confidence, Confidence::Estimated);
        assert!(close(f.impact.saving_usd, 0.00075));
    }

    #[test]
    fn a_server_called_everywhere_saves_nothing() {
        let busy = McpServerUse {
            server: "busy".into(),
            tokens: Tokens::measured(800),
            idle_tokens: Tokens::measured(0),
            requests: 8,
            calls: 4,
        };
        let a = priced(&report(vec![]), &[busy], &ContextSaving::default());
        let f = find(&a, FindingKind::CostlyMcpServers);
        assert_eq!(f.impact.saving, Tokens::measured(0));
        assert_eq!(f.impact.confidence, Confidence::Measured);
    }

    #[test]
    fn context_saving_counts_the_input_above_the_first_request_of_the_session() {
        let sessions = [
            requests(
                "s1",
                &[
                    ("test-model", 50_000, 0, 0, false),
                    // Below the threshold: nothing.
                    ("test-model", 1_000, 150_000, 0, false),
                    // A subagent's large request is not the main context.
                    ("test-model", 0, 400_000, 0, true),
                    // 250,000 - 50,000 = 200,000, split 10k : 230k : 10k of 250k.
                    ("test-model", 10_000, 230_000, 10_000, false),
                    // No price: counted, not priced. 300,000 - 50,000 = 250,000, all cache read.
                    ("other-model", 0, 300_000, 0, false),
                ],
            ),
            // Starts large and stays below its start: nothing to save.
            requests(
                "s2",
                &[
                    ("test-model", 0, 300_000, 0, false),
                    ("test-model", 0, 250_000, 0, false),
                ],
            ),
        ];
        let c = context_saving(&sessions, &prices());
        assert_eq!(c.requests, 4, "main-agent requests of at least 200,000");
        assert_eq!(
            (c.input, c.cache_read, c.cache_write),
            (8_000, 184_000 + 250_000, 8_000)
        );
        assert_eq!(c.tokens(), 450_000);
        assert_eq!((c.priced_requests, c.unpriced_requests), (1, 1));
        // 8,000 × $2 + 184,000 × $0.10 + 8,000 × $2.50 per M
        assert!((c.usd - 0.0544).abs() < 1e-9, "{}", c.usd);

        let mut r = report(vec![]);
        r.totals.requests = 7;
        let a = priced(&r, &[], &c);
        let f = find(&a, FindingKind::ContextSize);
        assert_eq!(f.impact.saving, Tokens::estimated(450_000));
        assert_eq!(f.impact.confidence, Confidence::Inferred);
        assert!(close(f.impact.saving_usd, 0.0544));
        assert!(f.notes.iter().any(|n| n.contains("it is inferred")));
        assert!(
            f.notes
                .iter()
                .any(|n| n.starts_with("1 of the requests with a saving had no price"))
        );
    }

    #[test]
    fn a_session_whose_large_requests_stay_below_its_start_saves_nothing() {
        let c = context_saving(
            &[requests(
                "s",
                &[
                    ("test-model", 0, 320_000, 0, false),
                    ("test-model", 0, 210_000, 0, false),
                    ("test-model", 0, 320_000, 0, false),
                ],
            )],
            &prices(),
        );
        assert_eq!(c.requests, 3);
        assert_eq!(c.tokens(), 0);
        let mut r = report(vec![]);
        r.totals.requests = 3;
        let a = priced(&r, &[], &c);
        let f = find(&a, FindingKind::ContextSize);
        assert_eq!(f.impact.saving.count, 0);
        assert_eq!(
            f.impact.saving_usd,
            Some(0.0),
            "priced period, nothing to price"
        );
    }

    #[test]
    fn a_large_context_saving_outranks_unused_skills_and_findings_without_saving_come_last() {
        let mut r = report(vec![
            skill("idle", true, 0, 0, (100, 4_000)),
            skill("gone", false, 2, 0, (0, 0)),
        ]);
        r.totals.requests = 100;
        let context = ContextSaving {
            cache_read: 1_000_000,
            requests: 5,
            usd: 0.1,
            priced_requests: 5,
            ..ContextSaving::default()
        };
        let a = priced(&r, &[], &context);
        let kinds: Vec<FindingKind> = a.findings.iter().map(|f| f.kind).collect();
        assert_eq!(
            kinds,
            [
                FindingKind::ContextSize,
                FindingKind::UnusedSkills,
                FindingKind::UsedButNotInstalled
            ]
        );
        let ranks: Vec<u32> = a.findings.iter().map(|f| f.rank).collect();
        assert_eq!(ranks, [1, 2, 3]);
        let gone = find(&a, FindingKind::UsedButNotInstalled);
        assert_eq!(gone.impact.saving, Tokens::measured(0));
        assert_eq!(gone.impact.saving_usd, None);
    }

    #[test]
    fn without_prices_the_order_falls_back_to_weighted_tokens() {
        // Unused: 4,000 listing tokens × 0.1 = 400. Context: 1,000 input × 1 = 1,000. So context first,
        // although it has fewer tokens; and no finding has a USD saving.
        let mut r = report(vec![skill("idle", true, 0, 0, (100, 4_000))]);
        r.totals.requests = 10;
        let context = ContextSaving {
            input: 1_000,
            requests: 1,
            unpriced_requests: 1,
            ..ContextSaving::default()
        };
        let a = analyze_with(&r, &References::new(), &[], &context, &Pricing::default());
        assert_eq!(a.findings[0].kind, FindingKind::ContextSize);
        assert_eq!(a.findings[1].kind, FindingKind::UnusedSkills);
        assert!(a.findings.iter().all(|f| f.impact.saving_usd.is_none()));
        // Cache write weighs 1.25: 900 cache write (1,125) beats 1,000 input.
        let write = ContextSaving {
            cache_write: 900,
            ..context
        };
        let mut unused = skill("idle", true, 0, 0, (100, 10_500)); // 1,050 weighted
        unused.baseline.tokens = Tokens::estimated(10_500);
        let mut r = report(vec![unused]);
        r.totals.requests = 10;
        let a = analyze_with(&r, &References::new(), &[], &write, &Pricing::default());
        assert_eq!(a.findings[0].kind, FindingKind::ContextSize);
        let a = analyze_with(
            &r,
            &References::new(),
            &[],
            &ContextSaving {
                input: 1_000,
                ..ContextSaving::default()
            },
            &Pricing::default(),
        );
        assert_eq!(
            a.findings[0].kind,
            FindingKind::UnusedSkills,
            "1,050 beats 1,000"
        );
    }

    #[test]
    fn pricing_averages_each_token_class_over_the_priced_requests() {
        let table = PriceTable::parse(
            r#"{
                "cheap": {"input": 1.0, "output": 5.0, "cache_write": 1.25, "cache_read": 0.1},
                "dear": {"input": 3.0, "output": 15.0, "cache_write": 3.75, "cache_read": 0.3}
            }"#,
        )
        .unwrap();
        let p = Pricing::from_sessions(
            &[requests(
                "s",
                &[
                    ("cheap", 1_000, 3_000, 0, false),
                    ("dear", 3_000, 1_000, 0, true),
                    ("unknown", 9_000, 9_000, 9_000, false),
                ],
            )],
            &table,
        );
        assert_eq!((p.priced_requests, p.unpriced_requests), (2, 1));
        // Input: (1,000 × 1 + 3,000 × 3) / 4,000 = 2.5; cache read: (3,000 × 0.1 + 1,000 × 0.3) / 4,000.
        assert!((p.input - 2.5).abs() < 1e-9);
        assert!((p.cache_read - 0.15).abs() < 1e-9);
        // No cache write tokens: the plain average of the two prices.
        assert!((p.cache_write - 2.5).abs() < 1e-9);
        assert!(p.is_priced());

        let none = Pricing::from_sessions(&[requests("s", &[("unknown", 1, 1, 1, false)])], &table);
        assert!(!none.is_priced());
        assert_eq!(none.unpriced_requests, 1);
    }

    #[test]
    fn unpriced_requests_are_named_in_a_note() {
        let p = Pricing {
            unpriced_requests: 3,
            ..pricing()
        };
        let a = analyze_with(
            &report(vec![skill("idle", true, 0, 0, (10, 100))]),
            &References::new(),
            &[],
            &ContextSaving::default(),
            &p,
        );
        let f = find(&a, FindingKind::UnusedSkills);
        assert!(
            f.notes
                .iter()
                .any(|n| n.contains("3 requests without a price are left out")),
            "{:?}",
            f.notes
        );
        let quiet = priced(
            &report(vec![skill("idle", true, 0, 0, (10, 100))]),
            &[],
            &ContextSaving::default(),
        );
        assert!(
            find(&quiet, FindingKind::UnusedSkills)
                .notes
                .iter()
                .all(|n| !n.contains("without a price"))
        );
    }
}
