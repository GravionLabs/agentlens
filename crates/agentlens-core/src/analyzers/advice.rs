//! Findings that say what to change to save tokens, made from the skill report.
//!
//! No model is involved: every finding has a fixed kind, a fixed explanation and the numbers it rests on,
//! each with the flag that says whether it was measured. The CLI, the application and the MCP server show
//! the same findings, so an agent that gets them only has to explain and act, not to recompute.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::analyzers::context::ContextReport;
use crate::analyzers::efficiency::{self, CacheBy, SubagentShare};
use crate::analyzers::flow;
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
/// A request is small for the ModelMix finding when it wrote fewer output tokens than this ...
pub const SMALL_OUTPUT_TOKENS: u64 = 500;
/// ... and sent fewer input tokens (input, cache read and cache write) than this. Together they keep it
/// below the requests of the Context size finding.
pub const SMALL_CONTEXT_TOKENS: u64 = 50_000;
/// The ModelMix finding needs at least this many small requests on the costliest model of a client.
pub const MIN_MODEL_MIX_REQUESTS: u64 = 20;
/// The ModelMix finding needs a saving of at least this share of the period's cost (0.05 is 5 %).
pub const MIN_MODEL_MIX_SHARE: f64 = 0.05;
/// A session compacted at least this many times carried its context on through several tasks
/// (ContextHygiene).
pub const MANY_COMPACTIONS: usize = 2;
/// The ContextHygiene finding needs at least this many flagged sessions ...
pub const MIN_HYGIENE_SESSIONS: usize = 3;
/// ... and a saving of at least this share of the period's cost (0.05 is 5 %); of the period's tokens
/// when the period has no price at all.
pub const MIN_HYGIENE_SHARE: f64 = 0.05;
/// A session of a client that records no `/clear` counts as a fresh start (an estimate) when it began at
/// most this many seconds after another session of the same client, project and branch ended.
pub const CLEAR_PROXY_SECONDS: i64 = 60;
/// The clients whose adapters record subagent requests. Only they can show "almost no delegation": for
/// the others a share of zero means the client does not say, not that the user never delegates. A fixed
/// list on purpose: a data-driven rule would leave out exactly the user who never delegates.
pub const DELEGATING_SOURCES: [Source; 3] =
    [Source::ClaudeCode, Source::OpenCode, Source::CopilotCli];
/// The Delegation finding needs a subagent share below this share of a client's requests (0.05 is 5 %) ...
pub const DELEGATION_MAX_SHARE: f64 = 0.05;
/// ... and at least this many main-agent requests with [`LARGE_CONTEXT_TOKENS`] of input in that client.
pub const MIN_DELEGATION_REQUESTS: u64 = 20;

/// SkillQuality: a skill loaded twice into the same context (a repeat load) is listed from this many.
pub const MIN_REPEAT_LOADS: u32 = 1;
/// SkillQuality: misfires (a model load with no work after it) need at least this many ...
pub const MIN_MISFIRES: u32 = 2;
/// ... and this share of the skill's model loads (0.2 is 20 %).
pub const MIN_MISFIRE_SHARE: f64 = 0.2;
/// SkillQuality: a skill the user starts as `/name` at least this many times and the model never loads
/// is listed although the model could pick it (it sits in the listing for nothing).
pub const MIN_USER_ONLY_STARTS: u32 = 3;
/// SkillQuality: failing calls after a skill need at least this many ...
pub const MIN_FOLLOW_ON_ERRORS: u64 = 5;
/// ... at this many times the failure rate of the rest of the skill's sessions.
pub const MIN_ERROR_RATE_FACTOR: u64 = 2;
/// SkillQuality: retry loops that begin after a skill are listed from this many.
pub const MIN_FOLLOW_ON_RETRY_LOOPS: u32 = 1;
/// SkillQuality: compactions soon after a load need at least this many ...
pub const MIN_COMPACTIONS_SOON: u32 = 2;
/// ... and this share of the skill's loads (0.25 is 25 %).
pub const MIN_COMPACTION_SHARE: f64 = 0.25;

/// How the SkillQuality detail of an item names the signal of a skill only the user starts; `tips` finds
/// it by this text.
pub const USER_ONLY_SIGNAL: &str = "only started by /name";

/// How long Anthropic keeps a prompt cache entry after its last use, by default: 5 minutes. A request after
/// a longer pause writes its context to the cache again instead of reading it (CacheExpiry).
pub const CACHE_TTL_5M: std::time::Duration = std::time::Duration::from_secs(5 * 60);
/// The longer lifetime of 1 hour, which a client opts into. A session has it when any of its requests
/// wrote tokens with it (`Usage::cache_write_1h`).
pub const CACHE_TTL_1H: std::time::Duration = std::time::Duration::from_secs(60 * 60);

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
    /// The same failing tool call repeated: every retry sends the whole context again.
    RetryLoops,
    /// Small requests on the costliest model that the next cheaper model, already in use, could have answered.
    ModelMix,
    /// Sessions that carried their context on after the task was done: near the limit without a
    /// compaction, or compacted again and again.
    ContextHygiene,
    /// Work on a large context with almost no subagent requests: reading and research stay in the main context.
    Delegation,
    /// Requests after a pause longer than the cache lifetime: the context was written again, not read.
    CacheExpiry,
    /// Skills whose order of events hints at a problem: loaded twice, loaded without work after, never
    /// picked by the model, or followed by failing work.
    SkillQuality,
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
            FindingKind::RetryLoops => {
                "Three or more identical tool calls in a row whose first two failed. Each retry sends the whole context again, and the call fails for the same reason until the cause is fixed."
            }
            FindingKind::ModelMix => {
                "Short answers on a large model cost the same input as long ones. These requests were small (little output, a small context) and ran on the costliest model of the client, although a cheaper model was in use as well."
            }
            FindingKind::ContextHygiene => {
                "Context that is carried on after the task is done is paid on every request. These sessions came near the limit of the context window without a compaction, or were compacted again and again instead of starting fresh."
            }
            FindingKind::CacheExpiry => {
                "After a pause longer than the cache lifetime the provider's prompt cache expired, and the whole context was written to the cache again at the write price instead of being read at the read price."
            }
            FindingKind::SkillQuality => {
                "Skills with signs of a problem: loaded twice into the same context, loaded by the model without any work following, never picked by the model although listed, or followed by failing work. These are signals from the order of events, not proof."
            }
            FindingKind::Delegation => {
                "Research and long reads that happen in the main context are carried on every later request; a subagent reads them in its own context and returns a summary. These clients ran many requests on a large context and almost none by subagents. No saving is estimated: what a subagent would have kept out of the main context cannot be counted from the requests."
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
            FindingKind::RetryLoops => {
                "Read the tool error and fix the cause, such as a missing file, a wrong path, a permission or a hook. When a hook failed inside the loop, check the hook configuration first."
            }
            FindingKind::ModelMix => {
                "Pick the model per task (`/model` in Claude Code, the model picker of the other clients) and give subagents a cheaper model."
            }
            FindingKind::ContextHygiene => {
                "Clear the context between tasks (`/clear` in Claude Code, a new session in the other clients), compact before the limit (`/compact`), and keep sessions short."
            }
            FindingKind::CacheExpiry => {
                "Before a long pause, finish the task and start the next one in a fresh session, or compact (`/compact`) so that less has to be written again. Long-idle sessions with a large context are the expensive ones."
            }
            FindingKind::SkillQuality => {
                "Repeat loads: say in the body that the skill is loaded once per task. Misfires: narrow the description to the situations it is for. Only started by /name: mark it as not automatically invoked (Claude Code: `disable-model-invocation: true`), so it leaves the listing. Errors or retry loops after it: check the commands, paths and scripts the skill gives. Compaction soon after: shorten the body or move steps into a script."
            }
            FindingKind::Delegation => {
                "Delegate file searches, log reading and reviews to subagents, and keep the main context for decisions."
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
        for (se, u) in sessions.iter().flat_map(|se| {
            se.events.iter().filter_map(move |e| match &e.kind {
                EventKind::Usage(u) => Some((se, u)),
                _ => None,
            })
        }) {
            // Like `period_totals`: a request without a model takes the session's.
            let Some(r) = u
                .model
                .as_deref()
                .or(se.session.model.as_deref())
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

/// The cost of all priced requests of the sessions (`None` if no request has a price) and their tokens
/// weighted by class ([`INPUT_WEIGHT`] and the others; output is not weighted, no finding saves it).
pub fn period_totals(sessions: &[SessionEvents], prices: &PriceTable) -> (Option<f64>, u64) {
    let mut usd = None;
    let mut weighted = 0.0;
    for se in sessions {
        for e in &se.events {
            let EventKind::Usage(u) = &e.kind else {
                continue;
            };
            let mut usage = u.clone();
            if usage.model.is_none() {
                usage.model = se.session.model.clone();
            }
            if let Some(c) = prices.cost(&usage) {
                *usd.get_or_insert(0.0) += c.usd;
            }
            weighted += Split {
                input: u.input as f64,
                cache_read: u.cache_read as f64,
                cache_write: u.cache_write as f64,
            }
            .weight();
        }
    }
    (usd, weighted.round() as u64)
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
    /// The part of the saved tokens on requests without a price for their model, split like the rest;
    /// [`analyze_with`] prices it with the period's average [`Pricing`].
    pub unpriced_input: u64,
    pub unpriced_cache_read: u64,
    pub unpriced_cache_write: u64,
}

impl ContextSaving {
    pub fn tokens(&self) -> u64 {
        self.input + self.cache_read + self.cache_write
    }

    /// What is left after taking away `other`, a part of the same requests.
    fn without(&self, other: &ContextSaving) -> ContextSaving {
        ContextSaving {
            input: self.input.saturating_sub(other.input),
            cache_read: self.cache_read.saturating_sub(other.cache_read),
            cache_write: self.cache_write.saturating_sub(other.cache_write),
            requests: self.requests.saturating_sub(other.requests),
            usd: (self.usd - other.usd).max(0.0),
            priced_requests: self.priced_requests.saturating_sub(other.priced_requests),
            unpriced_requests: self
                .unpriced_requests
                .saturating_sub(other.unpriced_requests),
            unpriced_input: self.unpriced_input.saturating_sub(other.unpriced_input),
            unpriced_cache_read: self
                .unpriced_cache_read
                .saturating_sub(other.unpriced_cache_read),
            unpriced_cache_write: self
                .unpriced_cache_write
                .saturating_sub(other.unpriced_cache_write),
        }
    }
}

/// The saving of the Context size finding, from the requests the skill report does not keep.
///
/// [`analyze_with`] takes away the requests that the ContextHygiene finding counts when it shows that
/// finding ([`ContextHygieneUse::overlap`]), so that nothing is counted twice.
pub fn context_saving(sessions: &[SessionEvents], prices: &PriceTable) -> ContextSaving {
    let mut out = ContextSaving::default();
    for se in sessions {
        add_growth_saving(&mut out, se, prices, LARGE_CONTEXT_TOKENS);
    }
    out
}

/// Adds to `out` the saving of one session: on every main-agent request with at least `min` tokens of
/// input, the input above the size of the session's first main-agent request, split in the request's own
/// input : cache read : cache write ratio and priced with its own model. Returns the saved tokens and
/// whether all requests behind them were measured.
fn add_growth_saving(
    out: &mut ContextSaving,
    se: &SessionEvents,
    prices: &PriceTable,
    min: u64,
) -> (u64, bool) {
    let mut tokens = 0;
    let mut measured = true;
    let mut first: Option<u64> = None;
    for u in se.events.iter().filter_map(|e| match &e.kind {
        EventKind::Usage(u) if !u.subagent => Some(u),
        _ => None,
    }) {
        let total = u.total_input();
        let start = *first.get_or_insert(total);
        if total < min {
            continue;
        }
        out.requests += 1;
        let saved = total.saturating_sub(start);
        if saved == 0 {
            continue;
        }
        tokens += saved;
        measured &= u.measured;
        let part = |class: u64| (u128::from(saved) * u128::from(class) / u128::from(total)) as u64;
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
            None => {
                out.unpriced_requests += 1;
                out.unpriced_input += saved_usage.input;
                out.unpriced_cache_read += saved_usage.cache_read;
                out.unpriced_cache_write += saved_usage.cache_write;
            }
        }
    }
    (tokens, measured)
}

/// One session the ContextHygiene finding flags, with its evidence.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HygieneSession {
    pub source: Option<Source>,
    pub session: String,
    pub project: Option<PathBuf>,
    /// Main-agent requests.
    pub requests: u64,
    /// The context of the first and of the largest main-agent request.
    pub first_tokens: u64,
    pub peak_tokens: u64,
    pub compactions: u64,
    /// The average context right before a compaction; `None` without one.
    pub before_compaction: Option<u64>,
    /// The limit the session was compared with (see [`flow::SessionGrowth`]), if one is known.
    pub window: Option<u64>,
    pub near_limit: bool,
    /// Fresh starts: recorded `/clear` events (measured), or 1 if the proxy saw one (estimated).
    pub clears: u64,
    pub clears_measured: bool,
    /// The saving of this session (see [`hygiene_saving`]) and whether its requests were measured.
    pub saved: u64,
    pub measured: bool,
}

/// The input of the ContextHygiene finding.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ContextHygieneUse {
    /// The flagged sessions, largest peak first.
    pub sessions: Vec<HygieneSession>,
    /// The saving over all main-agent requests of the flagged sessions (`requests` counts them all).
    pub saving: ContextSaving,
    /// The part of [`context_saving`] that lies in the flagged sessions: what the Context size finding
    /// leaves out when this finding is shown.
    pub overlap: ContextSaving,
    /// What all requests of the period cost, for the share of the saving: the reported cost, else the
    /// computed one; requests without a price are not in it.
    pub period_usd: f64,
    /// Sessions of the period that started fresh: after a recorded `/clear`, and by the proxy.
    pub fresh_measured: u64,
    pub fresh_estimated: u64,
}

/// Clients that record a `/clear` as [`EventKind::Clear`]; the others count with the proxy.
fn records_clear(source: Source) -> bool {
    source == Source::ClaudeCode
}

/// The fresh starts of every session, `(count, measured)`: the recorded [`EventKind::Clear`] events of a
/// client that records them, else 1 when the session began at most [`CLEAR_PROXY_SECONDS`] after another
/// session of the same client, project and branch ended (an estimate). The same branch on purpose: an
/// unattended queue starts a fresh session per task on another branch, and that is no `/clear`.
fn fresh_starts(sessions: &[SessionEvents]) -> Vec<(u64, bool)> {
    type Key<'a> = (Source, Option<&'a std::path::Path>, Option<&'a str>);
    fn key(se: &SessionEvents) -> Key<'_> {
        (
            se.session.source,
            se.session.project_dir.as_deref(),
            se.session.branch.as_deref(),
        )
    }
    // Per key of the proxy: the end of each session (in milliseconds) and its index, sorted.
    let mut ends: BTreeMap<Key, Vec<(i64, usize)>> = BTreeMap::new();
    for (i, se) in sessions.iter().enumerate() {
        if records_clear(se.session.source) {
            continue;
        }
        if let Some(end) = se.session.ended_at {
            ends.entry(key(se))
                .or_default()
                .push((end.as_millisecond(), i));
        }
    }
    for v in ends.values_mut() {
        v.sort_unstable();
    }
    sessions
        .iter()
        .enumerate()
        .map(|(i, se)| {
            if records_clear(se.session.source) {
                let clears = se
                    .events
                    .iter()
                    .filter(|e| matches!(e.kind, EventKind::Clear))
                    .count() as u64;
                return (clears, true);
            }
            let Some(start) = se.session.started_at.map(|t| t.as_millisecond()) else {
                return (0, false);
            };
            let window = CLEAR_PROXY_SECONDS * 1_000;
            let after = ends.get(&key(se)).is_some_and(|v| {
                let from = v.partition_point(|&(end, _)| end < start - window);
                v[from..]
                    .iter()
                    .take_while(|&&(end, _)| end <= start)
                    .any(|&(_, j)| j != i)
            });
            (u64::from(after), false)
        })
        .collect()
}

/// The input of the ContextHygiene finding.
///
/// A session is flagged when it reached the near-limit flag of [`flow::SessionGrowth`] without a
/// compaction, or has at least [`MANY_COMPACTIONS`] compactions. The saving is, on every main-agent
/// request of a flagged session, the input above the size of the session's first main-agent request (the
/// quantity of [`context_saving`] without its threshold), priced per request with its own model.
pub fn hygiene_saving(sessions: &[SessionEvents], prices: &PriceTable) -> ContextHygieneUse {
    let mut out = ContextHygieneUse::default();
    let fresh = fresh_starts(sessions);
    for &(count, measured) in &fresh {
        if count > 0 {
            if measured {
                out.fresh_measured += 1;
            } else {
                out.fresh_estimated += 1;
            }
        }
    }
    for se in sessions {
        for e in &se.events {
            let EventKind::Usage(u) = &e.kind else {
                continue;
            };
            let mut usage = u.clone();
            if usage.model.is_none() {
                usage.model = se.session.model.clone();
            }
            if let Some(c) = prices.cost(&usage) {
                out.period_usd += c.usd;
            }
        }
    }
    let index: std::collections::HashMap<(Source, &str), usize> = sessions
        .iter()
        .enumerate()
        .map(|(i, se)| ((se.session.source, se.session.id.as_str()), i))
        .collect();
    for g in flow::growth(sessions).sessions {
        let compactions = g.compactions.len();
        let flagged = (g.near_limit && compactions == 0) || compactions >= MANY_COMPACTIONS;
        let Some(&i) = index.get(&(g.source, g.session.as_str())) else {
            continue;
        };
        if !flagged {
            continue;
        }
        let se = &sessions[i];
        let (saved, measured) = add_growth_saving(&mut out.saving, se, prices, 0);
        add_growth_saving(&mut out.overlap, se, prices, LARGE_CONTEXT_TOKENS);
        let (clears, clears_measured) = fresh[i];
        out.sessions.push(HygieneSession {
            source: Some(g.source),
            session: g.session,
            project: g.project,
            requests: g.requests,
            first_tokens: g.first_tokens,
            peak_tokens: g.peak_tokens,
            compactions: compactions as u64,
            before_compaction: g
                .compactions
                .iter()
                .sum::<u64>()
                .checked_div(compactions as u64),
            window: g.window,
            near_limit: g.near_limit,
            clears,
            clears_measured,
            saved,
            measured,
        });
    }
    out
}

/// The retry loops of one tool at one client, and what their retries cost.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RetryToolUse {
    pub source: Option<Source>,
    pub tool: String,
    pub loops: u64,
    pub calls: u64,
    /// Distinct sessions with a loop of this tool.
    pub sessions: u64,
    /// The input summary most loops repeated (ties: the first seen).
    pub input: Option<String>,
    /// Loops with at least one failed hook run between their first and last call.
    pub hook_failures: u64,
    /// The hook type that failed most inside the loops (ties: the first seen).
    pub hook: Option<String>,
    /// The requests that issued the retries (the first attempt is not counted).
    pub requests: u64,
    /// The input of those requests split by token class.
    pub input_tokens: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    /// `false` if one of the requests was estimated, not reported.
    pub measured: bool,
    /// The retry tokens of the priced requests in USD, each priced with its own model.
    pub usd: f64,
    pub priced_requests: u64,
    pub unpriced_requests: u64,
    /// The tokens of the requests without a price for their model, split by class;
    /// [`analyze_with`] prices them with the period's average [`Pricing`].
    pub unpriced_input: u64,
    pub unpriced_cache_read: u64,
    pub unpriced_cache_write: u64,
}

impl RetryToolUse {
    pub fn tokens(&self) -> u64 {
        self.input_tokens + self.cache_read + self.cache_write
    }
}

/// The retry loops of the period, one row per `(client, tool)`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RetryLoopUse {
    pub tools: Vec<RetryToolUse>,
}

/// Counts `key` in `counts`, which keeps the order of first sight.
fn tally(counts: &mut Vec<(String, u64)>, key: &str) {
    match counts.iter_mut().find(|(k, _)| k == key) {
        Some((_, c)) => *c += 1,
        None => counts.push((key.to_owned(), 1)),
    }
}

/// The key with the largest count; the first of equals.
fn most(counts: &[(String, u64)]) -> Option<String> {
    let mut best: Option<&(String, u64)> = None;
    for c in counts {
        if best.is_none_or(|b| c.1 > b.1) {
            best = Some(c);
        }
    }
    best.map(|(k, _)| k.clone())
}

/// The input of the RetryLoops finding, from the retry loops of [`flow::retry_loops`] and the requests
/// that lie inside them.
///
/// A retry request is a `Usage` event strictly between the first and the last call of a loop: the first
/// attempt was issued before it, so it is not counted. A request of a subagent that falls inside the span
/// is counted too; spans are short, so this is rare and small.
pub fn retry_saving(sessions: &[SessionEvents], prices: &PriceTable) -> RetryLoopUse {
    // Per (client, tool): the row, the most seen inputs and hooks, the sessions seen.
    type Acc = (
        RetryToolUse,
        Vec<(String, u64)>,
        Vec<(String, u64)>,
        std::collections::BTreeSet<String>,
    );
    let mut acc: BTreeMap<(Source, String), Acc> = BTreeMap::new();
    for se in sessions {
        let source = se.session.source;
        for span in flow::retry_loops(&se.events) {
            let (row, inputs, hooks, seen) =
                acc.entry((source, span.tool.clone())).or_insert_with(|| {
                    (
                        RetryToolUse {
                            source: Some(source),
                            tool: span.tool.clone(),
                            measured: true,
                            ..RetryToolUse::default()
                        },
                        Vec::new(),
                        Vec::new(),
                        Default::default(),
                    )
                });
            row.loops += 1;
            row.calls += span.calls;
            if seen.insert(se.session.id.clone()) {
                row.sessions += 1;
            }
            if let Some(input) = &span.input {
                tally(inputs, input);
            }
            let inside = &se.events[span.first..=span.last];
            let mut failed = false;
            for e in inside {
                if let EventKind::Hook(h) = &e.kind
                    && h.success == Some(false)
                {
                    failed = true;
                    tally(hooks, &h.kind);
                }
            }
            row.hook_failures += u64::from(failed);
            for e in &se.events[span.first + 1..span.last] {
                let EventKind::Usage(u) = &e.kind else {
                    continue;
                };
                row.requests += 1;
                row.measured &= u.measured;
                row.input_tokens += u.input;
                row.cache_read += u.cache_read;
                row.cache_write += u.cache_write;
                let counted = Usage {
                    output: 0,
                    reasoning: 0,
                    cost_usd: None,
                    ..u.clone()
                };
                match u.model.as_deref().and_then(|m| prices.compute(m, &counted)) {
                    Some(usd) => {
                        row.usd += usd;
                        row.priced_requests += 1;
                    }
                    None => {
                        row.unpriced_requests += 1;
                        row.unpriced_input += u.input;
                        row.unpriced_cache_read += u.cache_read;
                        row.unpriced_cache_write += u.cache_write;
                    }
                }
            }
        }
    }
    RetryLoopUse {
        tools: acc
            .into_values()
            .map(|(mut row, inputs, hooks, _)| {
                row.input = most(&inputs);
                row.hook = most(&hooks);
                row
            })
            .collect(),
    }
}

/// One session in the input of the CacheExpiry finding.
#[derive(Debug, Clone, PartialEq)]
pub struct CacheExpirySession {
    pub id: String,
    pub project: Option<PathBuf>,
    /// The session's cache lifetime: [`CACHE_TTL_1H`] if any request wrote 1-hour tokens, else [`CACHE_TTL_5M`].
    pub ttl: std::time::Duration,
    /// Requests that came after a pause longer than `ttl` and wrote more than they read.
    pub requests: u64,
    /// The cache-write tokens of those requests: what was written again.
    pub tokens: u64,
    /// The shortest and the longest pause before such a request, in seconds.
    pub min_gap_secs: u64,
    pub max_gap_secs: u64,
    /// `false` if one of those requests was estimated, not reported.
    pub measured: bool,
    /// What writing again cost over reading, per request with the prices of its own model, in USD.
    pub usd: f64,
    /// The requests among them with a price for their model; `usd` covers only these.
    pub priced_requests: u64,
}

/// The input of the CacheExpiry finding.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CacheExpiryUse {
    /// The sessions with at least one expired request, the largest saving first.
    pub sessions: Vec<CacheExpirySession>,
    /// Requests left out because their model is not an Anthropic model ...
    pub skipped_models: u64,
    /// ... and because they or the request before them have no timestamp.
    pub skipped_untimed: u64,
}

/// The input of the CacheExpiry finding: per session the main-agent requests that came after a pause longer
/// than the session's cache lifetime and wrote more tokens than they read. The first request of a session
/// never counts, and neither do subagent requests, which have their own context.
pub fn cache_expiry_use(sessions: &[SessionEvents], prices: &PriceTable) -> CacheExpiryUse {
    let mut out = CacheExpiryUse::default();
    for se in sessions {
        let one_hour = se
            .events
            .iter()
            .any(|e| matches!(&e.kind, EventKind::Usage(u) if u.cache_write_1h > 0));
        let ttl = if one_hour { CACHE_TTL_1H } else { CACHE_TTL_5M };
        let mut row = CacheExpirySession {
            id: se.session.id.clone(),
            project: se.session.project_dir.clone(),
            ttl,
            requests: 0,
            tokens: 0,
            min_gap_secs: u64::MAX,
            max_gap_secs: 0,
            measured: true,
            usd: 0.0,
            priced_requests: 0,
        };
        let mut previous: Option<Option<jiff::Timestamp>> = None;
        for e in &se.events {
            let EventKind::Usage(u) = &e.kind else {
                continue;
            };
            if u.subagent {
                continue;
            }
            let Some(before) = previous.replace(e.at) else {
                continue;
            };
            let model = u.model.as_deref();
            if !model.is_some_and(|m| crate::prices::normalize(m).starts_with("claude")) {
                out.skipped_models += 1;
                continue;
            }
            let (Some(from), Some(to)) = (before, e.at) else {
                out.skipped_untimed += 1;
                continue;
            };
            let gap_ms = to.as_millisecond() - from.as_millisecond();
            if gap_ms <= ttl.as_millis() as i64 || u.cache_write <= u.cache_read {
                continue;
            }
            let gap = (gap_ms / 1000) as u64;
            row.requests += 1;
            row.tokens += u.cache_write;
            row.min_gap_secs = row.min_gap_secs.min(gap);
            row.max_gap_secs = row.max_gap_secs.max(gap);
            row.measured &= u.measured;
            if let Some(r) = model.and_then(|m| prices.rates(m, u.total_input())) {
                row.usd += u.cache_write as f64 * (r.cache_write - r.cache_read) / 1_000_000.0;
                row.priced_requests += 1;
            }
        }
        if row.requests > 0 {
            out.sessions.push(row);
        }
    }
    out.sessions.sort_by(|a, b| {
        b.usd
            .total_cmp(&a.usd)
            .then_with(|| b.tokens.cmp(&a.tokens))
            .then_with(|| a.id.cmp(&b.id))
    });
    out
}

/// One client in the input of the Delegation finding.
#[derive(Debug, Clone, PartialEq)]
pub struct DelegationSource {
    /// The client's requests and subagent requests, as the efficiency analysis counts them.
    pub share: SubagentShare,
    /// Main-agent requests with at least [`LARGE_CONTEXT_TOKENS`] of input.
    pub large_requests: u64,
    /// Whether every request of the client was measured.
    pub measured: bool,
}

impl DelegationSource {
    /// Subagent requests over all requests, 0 without requests.
    pub fn subagent_share(&self) -> f64 {
        share(self.share.subagent_requests, self.share.requests).unwrap_or(0.0)
    }

    fn qualifies(&self) -> bool {
        DELEGATING_SOURCES.contains(&self.share.source)
            && self.large_requests >= MIN_DELEGATION_REQUESTS
            && self.subagent_share() < DELEGATION_MAX_SHARE
    }
}

/// One session in the input of the Delegation finding.
#[derive(Debug, Clone, PartialEq)]
pub struct DelegationSession {
    pub id: String,
    pub source: Source,
    pub project: Option<PathBuf>,
    pub large_requests: u64,
    pub subagent_requests: u64,
    pub measured: bool,
}

/// The input of the Delegation finding.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DelegationUse {
    /// Per client, ordered by client.
    pub sources: Vec<DelegationSource>,
    /// Sessions with at least one large-context main-agent request, most of them first.
    pub sessions: Vec<DelegationSession>,
    /// All requests of the period and those that were main-agent requests on a large context.
    pub requests: u64,
    pub large_requests: u64,
}

/// The input of the Delegation finding: per client the subagent share of [`efficiency::analyze`] and the
/// main-agent requests with at least [`LARGE_CONTEXT_TOKENS`] of input, and the same per session.
pub fn delegation_use(sessions: &[SessionEvents]) -> DelegationUse {
    let shares = efficiency::analyze(sessions, &PriceTable::default(), CacheBy::Source).subagents;
    let mut out = DelegationUse::default();
    let mut large: BTreeMap<Source, (u64, bool)> = BTreeMap::new();
    for se in sessions {
        let mut row = DelegationSession {
            id: se.session.id.clone(),
            source: se.session.source,
            project: se.session.project_dir.clone(),
            large_requests: 0,
            subagent_requests: 0,
            measured: true,
        };
        for e in &se.events {
            let EventKind::Usage(u) = &e.kind else {
                continue;
            };
            out.requests += 1;
            let entry = large.entry(row.source).or_insert((0, true));
            entry.1 &= u.measured;
            row.measured &= u.measured;
            if u.subagent {
                row.subagent_requests += 1;
            } else if u.total_input() >= LARGE_CONTEXT_TOKENS {
                row.large_requests += 1;
                entry.0 += 1;
                out.large_requests += 1;
            }
        }
        if row.large_requests > 0 {
            out.sessions.push(row);
        }
    }
    out.sources = shares
        .into_iter()
        .map(|share| {
            let (large_requests, measured) = large.get(&share.source).copied().unwrap_or((0, true));
            DelegationSource {
                share,
                large_requests,
                measured,
            }
        })
        .collect();
    out.sessions.sort_by(|a, b| {
        b.large_requests
            .cmp(&a.large_requests)
            .then_with(|| a.source.id().cmp(b.source.id()))
            .then_with(|| a.id.cmp(&b.id))
    });
    out
}

/// The small requests of the costliest model of one client, and what the next cheaper model would have cost.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ModelMixRow {
    pub source: Option<Source>,
    /// The model with the highest list input price that ran at this client.
    pub model: String,
    /// The next lower list price among the models that ran at this client.
    pub cheaper: String,
    /// All requests of `model`, main agent and subagents.
    pub requests: u64,
    /// Those with `output < SMALL_OUTPUT_TOKENS` and `total_input() < SMALL_CONTEXT_TOKENS`.
    pub small_requests: u64,
    /// Of the small requests, those a subagent made.
    pub subagent_requests: u64,
    /// The input of the small requests split by token class.
    pub input_tokens: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    /// `false` if one of the small requests was estimated, not reported.
    pub measured: bool,
    /// The saving in USD: the cost on `model` minus the cost on `cheaper`, summed over the small requests.
    pub usd: f64,
}

impl ModelMixRow {
    pub fn tokens(&self) -> u64 {
        self.input_tokens + self.cache_read + self.cache_write
    }
}

/// The input of the ModelMix finding: one row per client with a pair of models.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ModelMixUse {
    pub rows: Vec<ModelMixRow>,
    /// What all requests of the period cost, for the share of the saving: the reported cost, else the
    /// computed one; requests without a price are not in it.
    pub period_usd: f64,
    /// Small requests of models without a price (or without any model): they are on no pair, so they
    /// are left out of the saving; counted here so that a note can say so.
    pub unpriced_requests: u64,
}

/// The costliest entry (ties: the first by name) of `models`, optionally only below a price.
fn priciest(models: &BTreeMap<String, f64>, below: Option<f64>) -> Option<(&String, f64)> {
    let mut best: Option<(&String, f64)> = None;
    for (m, &p) in models {
        if below.is_some_and(|b| p >= b) {
            continue;
        }
        if best.is_none_or(|(_, bp)| p > bp) {
            best = Some((m, p));
        }
    }
    best
}

/// The input of the ModelMix finding.
///
/// Per client, the costliest model is the priced model with the highest list input price among those that
/// ran in the period, the cheaper one the next lower list price among the priced models that ran there; a
/// client with one priced model has no pair. A request's model is `Usage.model`, else the session's. The
/// saving of a small request is `PriceTable::compute(costliest) - PriceTable::compute(cheaper)` on its own
/// tokens.
pub fn model_mix_saving(sessions: &[SessionEvents], prices: &PriceTable) -> ModelMixUse {
    let model_of =
        |se: &SessionEvents, u: &Usage| u.model.clone().or_else(|| se.session.model.clone());
    let list_price = |m: &str| prices.rates(m, 0).map(|r| r.input);
    let mut out = ModelMixUse::default();
    // Per client: the priced models that ran, with their list input price.
    let mut ran: BTreeMap<Source, BTreeMap<String, f64>> = BTreeMap::new();
    for se in sessions {
        for e in &se.events {
            let EventKind::Usage(u) = &e.kind else {
                continue;
            };
            let mut usage = u.clone();
            usage.model = model_of(se, u);
            if let Some(m) = &usage.model
                && let Some(p) = list_price(m)
            {
                ran.entry(se.session.source)
                    .or_default()
                    .insert(m.clone(), p);
            }
            if let Some(c) = prices.cost(&usage) {
                out.period_usd += c.usd;
            }
        }
    }
    let mut pairs: BTreeMap<Source, (String, String)> = BTreeMap::new();
    for (source, models) in &ran {
        if let Some((top, price)) = priciest(models, None)
            && let Some((cheaper, _)) = priciest(models, Some(price))
        {
            pairs.insert(*source, (top.clone(), cheaper.clone()));
        }
    }
    let mut rows: BTreeMap<Source, ModelMixRow> = BTreeMap::new();
    for se in sessions {
        let source = se.session.source;
        for e in &se.events {
            let EventKind::Usage(u) = &e.kind else {
                continue;
            };
            let small = u.output < SMALL_OUTPUT_TOKENS && u.total_input() < SMALL_CONTEXT_TOKENS;
            let model = model_of(se, u);
            if model.as_deref().and_then(list_price).is_none() {
                out.unpriced_requests += u64::from(small);
                continue;
            }
            let Some((top, cheaper)) = pairs.get(&source) else {
                continue;
            };
            if model.as_deref() != Some(top.as_str()) {
                continue;
            }
            let row = rows.entry(source).or_insert_with(|| ModelMixRow {
                source: Some(source),
                model: top.clone(),
                cheaper: cheaper.clone(),
                measured: true,
                ..ModelMixRow::default()
            });
            row.requests += 1;
            if !small {
                continue;
            }
            row.small_requests += 1;
            row.subagent_requests += u64::from(u.subagent);
            row.measured &= u.measured;
            row.input_tokens += u.input;
            row.cache_read += u.cache_read;
            row.cache_write += u.cache_write;
            if let (Some(now), Some(then)) = (prices.compute(top, u), prices.compute(cheaper, u)) {
                row.usd += now - then;
            }
        }
    }
    out.rows = rows.into_values().collect();
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
    /// For a session (context size): the full session id, which `--session` takes. `name` is the
    /// recognizable label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

/// A recognizable name for a session: the project folder and the start of the id.
fn session_label(project: Option<&std::path::Path>, id: &str) -> String {
    let short: String = id.chars().take(8).collect();
    let folder = project
        .map(|p| p.to_string_lossy().into_owned())
        .and_then(|p| {
            p.trim_end_matches(['/', '\\'])
                .rsplit(['/', '\\'])
                .next()
                .filter(|f| !f.is_empty())
                .map(str::to_owned)
        });
    match folder {
        Some(f) => format!("{f} · {short}"),
        None => format!("(no project) · {short}"),
    }
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
pub(crate) fn n(value: u64) -> String {
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

/// `1234567` -> `1.2M`, `12345` -> `12.3k`, small numbers as they are.
fn short_tokens(value: u64) -> String {
    if value >= 1_000_000 {
        format!("{:.1}M", value as f64 / 1_000_000.0)
    } else if value >= 1_000 {
        format!("{:.1}k", value as f64 / 1_000.0)
    } else {
        value.to_string()
    }
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

/// The quality signals of a skill that pass their minimums, in the order of the table in the analyses
/// guide. The first three have a saving, the rest have none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum QualitySignal {
    RepeatLoads,
    Misfires,
    UserOnly,
    Errors,
    RetryLoops,
    Compactions,
}

impl QualitySignal {
    fn has_saving(self) -> bool {
        matches!(
            self,
            QualitySignal::RepeatLoads | QualitySignal::Misfires | QualitySignal::UserOnly
        )
    }
}

fn quality_signals(s: &SkillStats) -> Vec<QualitySignal> {
    let q = &s.quality;
    let mut out = Vec::new();
    if q.repeat_loads >= MIN_REPEAT_LOADS {
        out.push(QualitySignal::RepeatLoads);
    }
    if q.misfires >= MIN_MISFIRES
        && f64::from(q.misfires) >= MIN_MISFIRE_SHARE * f64::from(s.triggers.model)
    {
        out.push(QualitySignal::Misfires);
    }
    if s.in_inventory && s.triggers.user >= MIN_USER_ONLY_STARTS && s.triggers.model == 0 {
        out.push(QualitySignal::UserOnly);
    }
    // Rates compared by cross-multiplying; a baseline without errors always passes.
    if q.follow_on_errors >= MIN_FOLLOW_ON_ERRORS
        && u128::from(q.follow_on_errors) * u128::from(q.baseline_calls)
            >= u128::from(MIN_ERROR_RATE_FACTOR)
                * u128::from(q.baseline_errors)
                * u128::from(q.follow_on_calls)
    {
        out.push(QualitySignal::Errors);
    }
    if q.follow_on_retry_loops >= MIN_FOLLOW_ON_RETRY_LOOPS {
        out.push(QualitySignal::RetryLoops);
    }
    if q.compactions_soon >= MIN_COMPACTIONS_SOON
        && f64::from(q.compactions_soon) >= MIN_COMPACTION_SHARE * f64::from(s.invocations)
    {
        out.push(QualitySignal::Compactions);
    }
    out
}

fn rate(errors: u64, calls: u64) -> String {
    format!("{:.0} %", share(errors, calls).unwrap_or(0.0) * 100.0)
}

/// The passing signals in words.
fn quality_detail(s: &SkillStats, signals: &[QualitySignal]) -> String {
    let q = &s.quality;
    signals
        .iter()
        .map(|sig| match sig {
            QualitySignal::RepeatLoads => {
                format!(
                    "loaded again into the same context ({})",
                    times(q.repeat_loads)
                )
            }
            QualitySignal::Misfires => format!(
                "{} of {} model loads without any work after",
                q.misfires, s.triggers.model
            ),
            QualitySignal::UserOnly => {
                format!("{USER_ONLY_SIGNAL} ({})", times(s.triggers.user))
            }
            QualitySignal::Errors => format!(
                "{} failed calls after it vs {} in the rest of its sessions",
                rate(q.follow_on_errors, q.follow_on_calls),
                rate(q.baseline_errors, q.baseline_calls)
            ),
            QualitySignal::RetryLoops => format!(
                "{} after it",
                if q.follow_on_retry_loops == 1 {
                    "1 retry loop".to_owned()
                } else {
                    format!("{} retry loops", q.follow_on_retry_loops)
                }
            ),
            QualitySignal::Compactions => format!(
                "compaction soon after {} of {} loads",
                q.compactions_soon, s.invocations
            ),
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// The saving of one skill listed for SkillQuality: the loads that did no good, each priced as load
/// (cache write) and carry (the skill's own cache read : uncached ratio), and the listing of a skill
/// only the user starts (cache read).
fn quality_split(s: &SkillStats, signals: &[QualitySignal]) -> Split {
    let mut split = Split::default();
    let wasted = u64::from(
        if signals.contains(&QualitySignal::RepeatLoads) {
            s.quality.repeat_loads
        } else {
            0
        } + if signals.contains(&QualitySignal::Misfires) {
            s.quality.misfires
        } else {
            0
        },
    );
    if wasted > 0 && s.invocations > 0 {
        let uses = f64::from(s.invocations);
        let load = s.load.count as f64 / uses * wasted as f64;
        let carry = s.carry.tokens.count as f64 / uses * wasted as f64;
        let carried = s.carry.cache_read_tokens + s.carry.uncached_tokens;
        let read_share = if carried > 0 {
            s.carry.cache_read_tokens as f64 / carried as f64
        } else {
            1.0
        };
        split.cache_write += load;
        split.cache_read += carry * read_share;
        split.input += carry * (1.0 - read_share);
    }
    if signals.contains(&QualitySignal::UserOnly) {
        split.cache_read += s.baseline.tokens.count as f64;
    }
    split
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
        &RetryLoopUse::default(),
        &ModelMixUse::default(),
        &ContextHygieneUse::default(),
        &DelegationUse::default(),
        &CacheExpiryUse::default(),
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
#[allow(clippy::too_many_arguments)]
pub fn analyze_with(
    report: &SkillReport,
    references: &References,
    mcp: &[McpServerUse],
    context: &ContextSaving,
    retries: &RetryLoopUse,
    model_mix: &ModelMixUse,
    hygiene: &ContextHygieneUse,
    delegation: &DelegationUse,
    cache_expiry: &CacheExpiryUse,
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
                session_id: None,
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
                session_id: None,
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
                session_id: None,
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
                session_id: None,
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
                session_id: None,
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
                session_id: None,
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
                session_id: None,
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

    // Whether the ContextHygiene finding is shown: it decides whether Context size leaves out the requests
    // of its sessions.
    let hygiene_saved = hygiene.saving.tokens();
    let hygiene_unpriced = Split {
        input: hygiene.saving.unpriced_input as f64,
        cache_read: hygiene.saving.unpriced_cache_read as f64,
        cache_write: hygiene.saving.unpriced_cache_write as f64,
    };
    let hygiene_usd = (hygiene.saving.priced_requests > 0 || pricing.is_priced())
        .then(|| hygiene.saving.usd + pricing.usd(&hygiene_unpriced).unwrap_or(0.0));
    let hygiene_share = match hygiene_usd {
        Some(usd) if hygiene.period_usd > 0.0 => usd / hygiene.period_usd,
        _ => share(hygiene_saved, total_tokens.count).unwrap_or(0.0),
    };
    let show_hygiene = hygiene.sessions.len() >= MIN_HYGIENE_SESSIONS
        && hygiene_saved > 0
        && hygiene_share >= MIN_HYGIENE_SHARE;
    let context = if show_hygiene {
        context.without(&hygiene.overlap)
    } else {
        *context
    };

    // The scale of it all: the context of every request, not the skills.
    let totals = &report.totals;
    if totals.requests > 0 {
        let average = totals.context_tokens / totals.requests;
        let items: Vec<Item> = totals
            .largest_sessions
            .iter()
            .map(|c| Item {
                name: session_label(c.project.as_deref(), &c.id),
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
                session_id: Some(c.id.clone()),
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
        // A request with a price for its model is priced with it; one without at the period's average
        // per token class, split in its own ratio. No USD only when the period has no price at all.
        let unpriced = Split {
            input: context.unpriced_input as f64,
            cache_read: context.unpriced_cache_read as f64,
            cache_write: context.unpriced_cache_write as f64,
        };
        f.impact = Impact {
            saving: Tokens::estimated(saved),
            saving_usd: (context.priced_requests > 0 || pricing.is_priced())
                .then(|| context.usd + pricing.usd(&unpriced).unwrap_or(0.0)),
            confidence: Confidence::Inferred,
        };
        f.notes.push(format!(
            "The saving assumes that the work on requests of at least {} tokens could have run in a fresh session, at the size of the session's first request; it is inferred.",
            n(LARGE_CONTEXT_TOKENS)
        ));
        if show_hygiene && hygiene.overlap.requests > 0 {
            f.notes.push(format!(
                "{} of those requests lie in the sessions of the Context hygiene finding; they are left out here, that finding counts them.",
                n(hygiene.overlap.requests)
            ));
        }
        if saved > 0 && context.unpriced_requests > 0 {
            f.notes.push(if pricing.is_priced() {
                format!(
                    "{} of the requests with a saving had no price for their model; they are priced at the period's average price per token class.",
                    n(context.unpriced_requests)
                )
            } else {
                format!(
                    "{} of the requests with a saving had no price for their model; their tokens are in the saving, not in the USD.",
                    n(context.unpriced_requests)
                )
            });
        }
        findings.push((f, split.weight()));
    }

    // The same failing call, again and again: the requests that issued the retries.
    if !retries.tools.is_empty() {
        let mut rows: Vec<&RetryToolUse> = retries.tools.iter().collect();
        rows.sort_by(|a, b| {
            b.tokens()
                .cmp(&a.tokens())
                .then_with(|| b.loops.cmp(&a.loops))
                .then_with(|| a.tool.cmp(&b.tool))
        });
        let items: Vec<Item> = rows
            .iter()
            .take(TOP)
            .map(|r| {
                let mut detail = format!(
                    "{} {} in {} {}, {} calls",
                    n(r.loops),
                    if r.loops == 1 { "loop" } else { "loops" },
                    n(r.sessions),
                    if r.sessions == 1 {
                        "session"
                    } else {
                        "sessions"
                    },
                    n(r.calls)
                );
                if let Some(input) = &r.input {
                    detail.push_str(&format!("; most often on `{input}`"));
                }
                if r.hook_failures > 0 {
                    detail.push_str(&format!(
                        "; {}hook failed in {} of the {} loops",
                        r.hook.as_ref().map_or(String::new(), |h| format!("a {h} ")),
                        n(r.hook_failures),
                        n(r.loops)
                    ));
                }
                Item {
                    name: r.tool.clone(),
                    tokens: Tokens {
                        count: r.tokens(),
                        measured: r.measured,
                    },
                    requests: r.requests,
                    detail,
                    referenced_by: Vec::new(),
                    session_id: None,
                }
            })
            .collect();
        let sum_of = |f: fn(&RetryToolUse) -> u64| rows.iter().map(|r| f(r)).sum::<u64>();
        let split = Split {
            input: sum_of(|r| r.input_tokens) as f64,
            cache_read: sum_of(|r| r.cache_read) as f64,
            cache_write: sum_of(|r| r.cache_write) as f64,
        };
        let unpriced = Split {
            input: sum_of(|r| r.unpriced_input) as f64,
            cache_read: sum_of(|r| r.unpriced_cache_read) as f64,
            cache_write: sum_of(|r| r.unpriced_cache_write) as f64,
        };
        let saved = sum_of(RetryToolUse::tokens);
        let measured = rows.iter().all(|r| r.measured);
        let (priced, unpriced_requests) = (
            sum_of(|r| r.priced_requests),
            sum_of(|r| r.unpriced_requests),
        );
        let total = Tokens {
            count: saved,
            measured,
        };
        let mut f = finding(FindingKind::RetryLoops, rows.len(), items, Some(total));
        f.share_of_total_tokens = share(saved, total_tokens.count);
        f.impact = Impact {
            saving: Tokens::estimated(saved),
            saving_usd: (priced > 0 || pricing.is_priced()).then(|| {
                rows.iter().map(|r| r.usd).sum::<f64>() + pricing.usd(&unpriced).unwrap_or(0.0)
            }),
            confidence: Confidence::Inferred,
        };
        if !measured {
            f.notes
                .push("Some requests are estimated, not reported by the client.".to_owned());
        }
        f.notes.push(
            "The saving is the input of the requests that issued the retries (not the first attempt); it assumes that fixing the cause avoids them, so it is inferred.".to_owned(),
        );
        f.notes.push(
            "The requests of a loop can also be in the Context size saving; the savings are not additive. Calls without an input summary cannot be compared and never form a loop.".to_owned(),
        );
        if saved > 0 && unpriced_requests > 0 {
            f.notes.push(if pricing.is_priced() {
                format!(
                    "{} of the retry requests had no price for their model; they are priced at the period's average price per token class.",
                    n(unpriced_requests)
                )
            } else {
                format!(
                    "{} of the retry requests had no price for their model; their tokens are in the saving, not in the USD.",
                    n(unpriced_requests)
                )
            });
        }
        findings.push((f, split.weight()));
    }

    // Small requests on the costliest model of a client, next to the cheaper model it also ran.
    let mut mix: Vec<&ModelMixRow> = model_mix
        .rows
        .iter()
        .filter(|r| r.small_requests >= MIN_MODEL_MIX_REQUESTS)
        .collect();
    let mix_usd: f64 = mix.iter().map(|r| r.usd).sum();
    if !mix.is_empty() && mix_usd > 0.0 && mix_usd >= MIN_MODEL_MIX_SHARE * model_mix.period_usd {
        mix.sort_by(|a, b| b.usd.total_cmp(&a.usd).then_with(|| a.model.cmp(&b.model)));
        let pct = |part: u64, whole: u64| share(part, whole).unwrap_or(0.0) * 100.0;
        let items: Vec<Item> = mix
            .iter()
            .take(TOP)
            .map(|r| Item {
                name: r.model.clone(),
                tokens: Tokens {
                    count: r.tokens(),
                    measured: r.measured,
                },
                requests: r.small_requests,
                detail: format!(
                    "{} of {} requests small ({:.1}%), {:.1}% of them by subagents; ~${:.2} saved on {}",
                    n(r.small_requests),
                    n(r.requests),
                    pct(r.small_requests, r.requests),
                    pct(r.subagent_requests, r.small_requests),
                    r.usd,
                    r.cheaper
                ),
                referenced_by: Vec::new(),
                session_id: None,
            })
            .collect();
        let tokens: u64 = mix.iter().map(|r| r.tokens()).sum();
        let measured = mix.iter().all(|r| r.measured);
        let mut f = finding(
            FindingKind::ModelMix,
            mix.len(),
            items,
            Some(Tokens {
                count: tokens,
                measured,
            }),
        );
        f.share_of_total_tokens = share(tokens, total_tokens.count);
        f.impact = Impact {
            saving: Tokens::estimated(0),
            saving_usd: Some(mix_usd),
            confidence: Confidence::Inferred,
        };
        f.notes.push(format!(
            "A request is small with fewer than {} output tokens and fewer than {} tokens of input. The saving is what the same tokens would have cost on the next cheaper model that ran in the same client, {:.1}% of the period's cost (~${:.2}); it assumes that model could have answered them, so it is inferred. A model switch saves cost, not tokens.",
            n(SMALL_OUTPUT_TOKENS),
            n(SMALL_CONTEXT_TOKENS),
            mix_usd / model_mix.period_usd * 100.0,
            model_mix.period_usd
        ));
        if !measured {
            f.notes
                .push("Some requests are estimated, not reported by the client.".to_owned());
        }
        if model_mix.unpriced_requests > 0 {
            f.notes.push(format!(
                "{} small requests had no price for their model; they are not in the saving.",
                n(model_mix.unpriced_requests)
            ));
        }
        findings.push((f, 0.0));
    }

    // Sessions that carried their context on: the input above their start on every request.
    if show_hygiene {
        let mut rows: Vec<&HygieneSession> = hygiene.sessions.iter().collect();
        rows.sort_by(|a, b| {
            b.saved
                .cmp(&a.saved)
                .then_with(|| a.session.cmp(&b.session))
        });
        let count = |value: u64, one: &str, many: &str| {
            format!("{} {}", n(value), if value == 1 { one } else { many })
        };
        let items: Vec<Item> = rows
            .iter()
            .take(TOP)
            .map(|h| {
                let limit = if h.near_limit {
                    "near the limit"
                } else if h.window.is_some() {
                    "not near the limit"
                } else {
                    "limit unknown"
                };
                let start = match h.before_compaction {
                    Some(before) => format!(
                        "{} tokens before a compaction against {} at the start",
                        n(before),
                        n(h.first_tokens)
                    ),
                    None => format!("{} tokens at the start", n(h.first_tokens)),
                };
                Item {
                    name: session_label(h.project.as_deref(), &h.session),
                    tokens: Tokens {
                        count: h.saved,
                        measured: h.measured,
                    },
                    requests: h.requests,
                    detail: format!(
                        "{}, peak {} tokens, {}, {}{}, {limit}; {start}",
                        count(h.requests, "request", "requests"),
                        n(h.peak_tokens),
                        count(h.compactions, "compaction", "compactions"),
                        if h.clears_measured { "" } else { "~" },
                        count(h.clears, "clear", "clears"),
                    ),
                    referenced_by: Vec::new(),
                    session_id: Some(h.session.clone()),
                }
            })
            .collect();
        let measured = rows.iter().all(|h| h.measured);
        let total = Tokens {
            count: hygiene_saved,
            measured,
        };
        let mut f = finding(FindingKind::ContextHygiene, rows.len(), items, Some(total));
        f.share_of_total_tokens = share(hygiene_saved, total_tokens.count);
        f.impact = Impact {
            saving: Tokens::estimated(hygiene_saved),
            saving_usd: hygiene_usd,
            confidence: Confidence::Inferred,
        };
        f.notes.push(format!(
            "A session is listed when it came near the limit of its context window without a compaction, or was compacted at least {MANY_COMPACTIONS} times."
        ));
        f.notes.push(match hygiene_usd {
            Some(usd) if hygiene.period_usd > 0.0 => format!(
                "The saving is, on every main-agent request of these sessions, the input above the size of the session's first request: {:.1}% of the period's cost (~${:.2} of ~${:.2}). It assumes that the work could have started fresh at that size, so it is inferred.",
                hygiene_share * 100.0,
                usd,
                hygiene.period_usd
            ),
            _ => format!(
                "The saving is, on every main-agent request of these sessions, the input above the size of the session's first request: {:.1}% of the period's tokens (no price known). It assumes that the work could have started fresh at that size, so it is inferred.",
                hygiene_share * 100.0
            ),
        });
        f.notes.push(format!(
            "Fresh starts in the period: {} after a recorded /clear (Claude Code), ~{} by the proxy for the clients that record none (a session that began at most {CLEAR_PROXY_SECONDS} seconds after another of the same client, project and branch ended).",
            n(hygiene.fresh_measured),
            n(hygiene.fresh_estimated)
        ));
        f.notes.push(
            "The requests of these sessions are left out of the Context size saving, so the two savings add up."
                .to_owned(),
        );
        if !measured {
            f.notes
                .push("Some requests are estimated, not reported by the client.".to_owned());
        }
        if hygiene.saving.unpriced_requests > 0 {
            f.notes.push(if pricing.is_priced() {
                format!(
                    "{} of the requests with a saving had no price for their model; they are priced at the period's average price per token class.",
                    n(hygiene.saving.unpriced_requests)
                )
            } else {
                format!(
                    "{} of the requests with a saving had no price for their model; their tokens are in the saving, not in the USD.",
                    n(hygiene.saving.unpriced_requests)
                )
            });
        }
        let weight = Split {
            input: hygiene.saving.input as f64,
            cache_read: hygiene.saving.cache_read as f64,
            cache_write: hygiene.saving.cache_write as f64,
        }
        .weight();
        findings.push((f, weight));
    }

    // Requests after the cache expired: the context was written again at the write price.
    if !cache_expiry.sessions.is_empty() {
        let rows = &cache_expiry.sessions;
        let tokens: u64 = rows.iter().map(|r| r.tokens).sum();
        let measured = rows.iter().all(|r| r.measured);
        let priced = rows.iter().any(|r| r.priced_requests > 0);
        let usd: f64 = rows.iter().map(|r| r.usd).sum();
        let unpriced: u64 = rows.iter().map(|r| r.requests - r.priced_requests).sum();
        let minutes = |secs: u64| (secs as f64 / 60.0).round() as u64;
        let count = |value: u64, one: &str, many: &str| {
            format!("{} {}", n(value), if value == 1 { one } else { many })
        };
        let items: Vec<Item> = rows
            .iter()
            .take(TOP)
            .map(|r| {
                let (lo, hi) = (minutes(r.min_gap_secs), minutes(r.max_gap_secs));
                let pauses = if lo == hi {
                    format!("{lo} min")
                } else {
                    format!("{lo}\u{2013}{hi} min")
                };
                Item {
                    name: r.id.clone(),
                    tokens: Tokens {
                        count: r.tokens,
                        measured: r.measured,
                    },
                    requests: r.requests,
                    detail: format!(
                        "{} after {} of {pauses}, {} tokens written again, TTL {} min",
                        count(r.requests, "request", "requests"),
                        if r.requests == 1 { "a pause" } else { "pauses" },
                        short_tokens(r.tokens),
                        r.ttl.as_secs() / 60
                    ),
                    referenced_by: Vec::new(),
                    session_id: Some(r.id.clone()),
                }
            })
            .collect();
        let total = Tokens {
            count: tokens,
            measured,
        };
        let mut f = finding(FindingKind::CacheExpiry, rows.len(), items, Some(total));
        f.share_of_total_tokens = share(tokens, total_tokens.count);
        f.impact = Impact {
            saving: total,
            saving_usd: priced.then_some(usd),
            confidence: Confidence::Inferred,
        };
        f.notes.push(
            "A request counts when it came more than the session's cache lifetime after the previous main-agent request and wrote more tokens than it read. The saving is those written tokens, in USD the difference between the write and the read price of the request's own model. It is inferred: the action shrinks the rewrite, it does not prove that it would not have happened."
                .to_owned(),
        );
        f.notes.push(format!(
            "The lifetime is {} minutes, or {} minutes for a session in which a request wrote tokens with the 1-hour lifetime. It is read from the client's own report where the client gives one (Claude Code) and assumed to be {} minutes elsewhere.",
            CACHE_TTL_5M.as_secs() / 60,
            CACHE_TTL_1H.as_secs() / 60,
            CACHE_TTL_5M.as_secs() / 60
        ));
        if rows.iter().any(|r| r.ttl == CACHE_TTL_1H) {
            f.notes.push(
                "The price table has one cache-write price, that of the 5-minute lifetime. Writes with the 1-hour lifetime cost more, so the USD saving of such a session is a lower bound."
                    .to_owned(),
            );
        }
        if unpriced > 0 {
            f.notes.push(format!(
                "{} of the requests had no price for their model; their tokens are in the saving, not in the USD.",
                n(unpriced)
            ));
        }
        if cache_expiry.skipped_models > 0 {
            f.notes.push(format!(
                "Skipped, because the cache lifetime is not known: {} of models that are not Anthropic models.",
                count(cache_expiry.skipped_models, "request", "requests")
            ));
        }
        if cache_expiry.skipped_untimed > 0 {
            f.notes.push(format!(
                "Skipped, because the pause cannot be told: {} without a timestamp, or right after one without.",
                count(cache_expiry.skipped_untimed, "request", "requests")
            ));
        }
        f.notes.push(
            "The Context size saving counts the volume of a large context, this finding the price difference of writing it again. Both can name the same session; their savings are not additive."
                .to_owned(),
        );
        if !measured {
            f.notes
                .push("Some requests are estimated, not reported by the client.".to_owned());
        }
        let weight = tokens as f64 * (CACHE_WRITE_WEIGHT - CACHE_READ_WEIGHT);
        findings.push((f, weight));
    }

    // Work on a large context with almost no subagent requests. No saving: weight 0, Impact::none().
    let delegating: Vec<&DelegationSource> = delegation
        .sources
        .iter()
        .filter(|s| s.qualifies())
        .collect();
    if !delegating.is_empty() {
        let pct = |part: u64, whole: u64| share(part, whole).unwrap_or(0.0) * 100.0;
        let mut items: Vec<Item> = delegating
            .iter()
            .map(|s| Item {
                name: s.share.source.id().to_owned(),
                tokens: Tokens {
                    count: s.share.tokens,
                    measured: s.measured,
                },
                requests: s.share.requests,
                detail: format!(
                    "{} requests, {} on a large context, {} by subagents ({:.1}%)",
                    n(s.share.requests),
                    n(s.large_requests),
                    n(s.share.subagent_requests),
                    s.subagent_share() * 100.0
                ),
                referenced_by: Vec::new(),
                session_id: None,
            })
            .collect();
        let sources = items.len();
        items.extend(
            delegation
                .sessions
                .iter()
                .filter(|s| delegating.iter().any(|d| d.share.source == s.source))
                .take(TOP)
                .map(|s| Item {
                    name: session_label(s.project.as_deref(), &s.id),
                    tokens: Tokens {
                        count: 0,
                        measured: s.measured,
                    },
                    requests: s.large_requests,
                    detail: format!(
                        "{} on a large context, {} by subagents",
                        n(s.large_requests),
                        n(s.subagent_requests)
                    ),
                    referenced_by: Vec::new(),
                    session_id: Some(s.id.clone()),
                }),
        );
        let mut f = finding(FindingKind::Delegation, sources, items, None);
        f.notes.push(format!(
            "{:.1}% of the period's requests ({} of {}) were main-agent requests on a large context (at least {} tokens of input).",
            pct(delegation.large_requests, delegation.requests),
            n(delegation.large_requests),
            n(delegation.requests),
            n(LARGE_CONTEXT_TOKENS)
        ));
        f.notes.push(format!(
            "Shown for a client with a subagent share below {:.0}% and at least {} main-agent requests on a large context. The first rows are the clients, then the sessions with the most large-context requests. No saving is estimated.",
            DELEGATION_MAX_SHARE * 100.0,
            n(MIN_DELEGATION_REQUESTS)
        ));
        findings.push((f, 0.0));
    }

    // Skills with signs of a problem in the order of their events.
    {
        struct Row<'a> {
            skill: &'a SkillStats,
            signals: Vec<QualitySignal>,
            split: Split,
            usd: Option<f64>,
            saving: u64,
        }
        let mut rows: Vec<Row> = report
            .skills
            .iter()
            .filter_map(|s| {
                let signals = quality_signals(s);
                if signals.is_empty() {
                    return None;
                }
                let split = quality_split(s, &signals);
                Some(Row {
                    skill: s,
                    usd: pricing.usd(&split),
                    saving: (split.input + split.cache_read + split.cache_write).round() as u64,
                    signals,
                    split,
                })
            })
            .collect();
        if !rows.is_empty() {
            let has = |r: &Row| r.signals.iter().any(|sig| sig.has_saving());
            rows.sort_by(|a, b| {
                has(b)
                    .cmp(&has(a))
                    .then_with(|| match (a.usd, b.usd) {
                        (Some(x), Some(y)) => y.total_cmp(&x),
                        _ => b.split.weight().total_cmp(&a.split.weight()),
                    })
                    .then_with(|| b.signals.len().cmp(&a.signals.len()))
                    .then_with(|| a.skill.name.cmp(&b.skill.name))
            });
            let items: Vec<Item> = rows
                .iter()
                .take(TOP)
                .map(|r| Item {
                    name: r.skill.name.clone(),
                    tokens: Tokens::estimated(r.saving),
                    requests: u64::from(r.skill.invocations),
                    detail: quality_detail(r.skill, &r.signals),
                    referenced_by: Vec::new(),
                    session_id: None,
                })
                .collect();
            let saved: u64 = rows.iter().map(|r| r.saving).sum();
            let split = rows.iter().fold(Split::default(), |mut acc, r| {
                acc.input += r.split.input;
                acc.cache_read += r.split.cache_read;
                acc.cache_write += r.split.cache_write;
                acc
            });
            let total = Tokens::estimated(saved);
            let mut f = finding(FindingKind::SkillQuality, rows.len(), items, Some(total));
            f.share_of_total_tokens = share(saved, total_tokens.count);
            if saved > 0 {
                f.impact = Impact {
                    saving: total,
                    saving_usd: pricing.usd(&split),
                    confidence: Confidence::Inferred,
                };
            }
            f.notes.push(
                "Signals count loads by the main agent only; skills of subagents are not judged. They hint at a problem and prove nothing.".to_owned(),
            );
            f.notes.push(
                "A skill can also appear in Costly descriptions or Large skill bodies; the savings of the findings are not additive.".to_owned(),
            );
            f.notes.push(
                "The saving is the load and carry of repeat loads and misfires, and the listing of a skill only started by /name. Errors, retry loops and compactions after a skill have no saving: their cost cannot be attributed to the skill; a skill listed only for them ranks last.".to_owned(),
            );
            unpriced_note(&mut f);
            findings.push((f, split.weight()));
        }
    }

    // Ranked by the USD saving, then by the weighted tokens, then in the order above; findings without
    // a saving last, in the order above (the sort is stable).
    findings.sort_by(|(a, wa), (b, wb)| {
        let has = |f: &Finding| {
            f.impact.saving.count > 0 || f.impact.saving_usd.is_some_and(|usd| usd > 0.0)
        };
        let (sa, sb) = (has(a), has(b));
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
            quality: Default::default(),
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
        analyze_with(
            r,
            refs,
            mcp,
            &ContextSaving::default(),
            &RetryLoopUse::default(),
            &ModelMixUse::default(),
            &ContextHygieneUse::default(),
            &DelegationUse::default(),
            &CacheExpiryUse::default(),
            &Pricing::default(),
        )
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
        assert_eq!(c.items[0].name, "(no project) · s1");
        assert_eq!(c.items[0].session_id.as_deref(), Some("s1"));
        assert!(c.items[0].tokens.measured);
        assert_eq!(c.count, 3, "the sessions analyzed, not the ones listed");
        assert!(c.notes[0].contains("9,000 tokens"), "{:?}", c.notes);
        assert!(c.notes[0].contains("25.0%"), "{:?}", c.notes);
        assert!(c.notes[0].contains("10.0%"), "{:?}", c.notes);
        assert!(c.action.contains("fresh session"));
    }

    #[test]
    fn context_size_names_a_session_by_project_folder_and_id_start() {
        use crate::analyzers::skills::{PeriodTotals, SessionContext};
        let session = |id: &str, project: Option<&str>| SessionContext {
            id: id.into(),
            project: project.map(Into::into),
            requests: 10,
            average_context: 9_000,
        };
        let mut r = report(vec![skill("idle", true, 0, 0, (10, 100))]);
        r.totals = PeriodTotals {
            requests: 30,
            tokens: 100_000,
            context_tokens: 90_000,
            measured: true,
            largest_sessions: vec![
                session(
                    "cad3c7a2-f626-4332-b1af-29291ebd5c7b",
                    Some("/work/demo/shop/"),
                ),
                session("0123456789", Some("C:\\work\\api")),
                session("abcdef12-0000", None),
            ],
            ..PeriodTotals::default()
        };
        let a = analyze(&r);
        let c = find(&a, FindingKind::ContextSize);
        assert_eq!(c.items[0].name, "shop · cad3c7a2");
        assert_eq!(
            c.items[0].session_id.as_deref(),
            Some("cad3c7a2-f626-4332-b1af-29291ebd5c7b")
        );
        assert_eq!(c.items[1].name, "api · 01234567");
        assert_eq!(c.items[2].name, "(no project) · abcdef12");
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
        analyze_with(
            r,
            &References::new(),
            mcp,
            context,
            &RetryLoopUse::default(),
            &ModelMixUse::default(),
            &ContextHygieneUse::default(),
            &DelegationUse::default(),
            &CacheExpiryUse::default(),
            &pricing(),
        )
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
        assert_eq!(
            (
                c.unpriced_input,
                c.unpriced_cache_read,
                c.unpriced_cache_write
            ),
            (0, 250_000, 0)
        );
        // 8,000 × $2 + 184,000 × $0.10 + 8,000 × $2.50 per M
        assert!((c.usd - 0.0544).abs() < 1e-9, "{}", c.usd);

        let mut r = report(vec![]);
        r.totals.requests = 7;
        let a = priced(&r, &[], &c);
        let f = find(&a, FindingKind::ContextSize);
        assert_eq!(f.impact.saving, Tokens::estimated(450_000));
        assert_eq!(f.impact.confidence, Confidence::Inferred);
        // The unpriced 250,000 cache read at the period average of $0.50: 0.0544 + 0.125.
        assert!(
            close(f.impact.saving_usd, 0.1794),
            "{:?}",
            f.impact.saving_usd
        );
        assert!(f.notes.iter().any(|n| n.contains("it is inferred")));
        assert!(f.notes.iter().any(|n| n.starts_with(
            "1 of the requests with a saving had no price for their model; they are priced at the period's average"
        )));
    }

    #[test]
    fn a_context_saving_on_unpriced_models_is_priced_at_the_period_average_and_ranks_by_it() {
        // Only a model without a price: 260,000 - 20,000 = 240,000 saved, split in the request's own
        // 26k : 208k : 26k ratio.
        let c = context_saving(
            &[requests(
                "s",
                &[
                    ("other-model", 20_000, 0, 0, false),
                    ("other-model", 26_000, 208_000, 26_000, false),
                ],
            )],
            &prices(),
        );
        assert_eq!((c.priced_requests, c.unpriced_requests), (0, 1));
        assert_eq!(
            (
                c.unpriced_input,
                c.unpriced_cache_read,
                c.unpriced_cache_write
            ),
            (24_000, 192_000, 24_000)
        );
        // An unused skill of 400,000 listing tokens: more tokens, but 400,000 × $0.50 = $0.20.
        let mut r = report(vec![skill("idle", true, 0, 0, (20_000, 400_000))]);
        r.totals.requests = 2;
        let a = priced(&r, &[], &c);
        let f = find(&a, FindingKind::ContextSize);
        assert_eq!(f.impact.saving, Tokens::estimated(240_000));
        // 24,000 × $3 + 192,000 × $0.50 + 24,000 × $3.75 per M = 0.072 + 0.096 + 0.09.
        assert!(
            close(f.impact.saving_usd, 0.258),
            "{:?}",
            f.impact.saving_usd
        );
        let kinds: Vec<FindingKind> = a.findings.iter().map(|f| f.kind).collect();
        assert_eq!(kinds, [FindingKind::ContextSize, FindingKind::UnusedSkills]);
        assert!(
            f.notes
                .iter()
                .any(|n| n.contains("priced at the period's average"))
        );
        // Ranked by that USD: 600,000 listing tokens × $0.50 = $0.30 outranks it.
        let mut r = report(vec![skill("idle", true, 0, 0, (20_000, 600_000))]);
        r.totals.requests = 2;
        let kinds: Vec<FindingKind> = priced(&r, &[], &c)
            .findings
            .iter()
            .map(|f| f.kind)
            .collect();
        assert_eq!(kinds, [FindingKind::UnusedSkills, FindingKind::ContextSize]);

        // Without any price in the period: no USD, the tokens still counted.
        let a = analyze_with(
            &r,
            &References::new(),
            &[],
            &c,
            &RetryLoopUse::default(),
            &ModelMixUse::default(),
            &ContextHygieneUse::default(),
            &DelegationUse::default(),
            &CacheExpiryUse::default(),
            &Pricing::default(),
        );
        let f = find(&a, FindingKind::ContextSize);
        assert_eq!(f.impact.saving_usd, None);
        assert!(f.notes.iter().any(|n| n.contains("not in the USD")));
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
        let a = analyze_with(
            &r,
            &References::new(),
            &[],
            &context,
            &RetryLoopUse::default(),
            &ModelMixUse::default(),
            &ContextHygieneUse::default(),
            &DelegationUse::default(),
            &CacheExpiryUse::default(),
            &Pricing::default(),
        );
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
        let a = analyze_with(
            &r,
            &References::new(),
            &[],
            &write,
            &RetryLoopUse::default(),
            &ModelMixUse::default(),
            &ContextHygieneUse::default(),
            &DelegationUse::default(),
            &CacheExpiryUse::default(),
            &Pricing::default(),
        );
        assert_eq!(a.findings[0].kind, FindingKind::ContextSize);
        let a = analyze_with(
            &r,
            &References::new(),
            &[],
            &ContextSaving {
                input: 1_000,
                ..ContextSaving::default()
            },
            &RetryLoopUse::default(),
            &ModelMixUse::default(),
            &ContextHygieneUse::default(),
            &DelegationUse::default(),
            &CacheExpiryUse::default(),
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
            &RetryLoopUse::default(),
            &ModelMixUse::default(),
            &ContextHygieneUse::default(),
            &DelegationUse::default(),
            &CacheExpiryUse::default(),
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

    /// One event of a session built for the retry loop tests.
    enum Ev<'a> {
        /// A call of `Bash` with this input, failed or not.
        Call(&'a str, bool),
        /// A call of another tool.
        Tool(&'a str, &'a str, bool),
        /// A request: model, input, cache read, cache write.
        Req(&'a str, u64, u64, u64),
        /// A hook run: type, success.
        Hook(&'a str, bool),
    }

    fn trace(id: &str, events: &[Ev]) -> SessionEvents {
        use crate::model::{Event, Hook, Session, ToolCall, ToolStatus};
        let call = |name: &str, input: &str, failed: bool| {
            EventKind::ToolCall(ToolCall {
                name: name.into(),
                input_summary: Some(input.into()),
                status: if failed {
                    ToolStatus::Error
                } else {
                    ToolStatus::Ok
                },
                duration_ms: None,
            })
        };
        SessionEvents {
            session: Session::new(id, Source::ClaudeCode),
            events: events
                .iter()
                .map(|e| Event {
                    at: None,
                    kind: match *e {
                        Ev::Call(input, failed) => call("Bash", input, failed),
                        Ev::Tool(name, input, failed) => call(name, input, failed),
                        Ev::Req(model, input, cache_read, cache_write) => EventKind::Usage(Usage {
                            model: Some(model.into()),
                            input,
                            cache_read,
                            cache_write,
                            measured: true,
                            ..Usage::default()
                        }),
                        Ev::Hook(kind, ok) => EventKind::Hook(Hook {
                            kind: kind.into(),
                            success: Some(ok),
                            duration_ms: None,
                        }),
                    },
                })
                .collect(),
            warnings: Vec::new(),
        }
    }

    type R<'a> = (&'a str, u64, u64, u64);

    /// A loop of three calls: the requests before the second and the third are the retries.
    fn a_loop<'a>(first: R<'a>, second: R<'a>, third: R<'a>, after: R<'a>) -> Vec<Ev<'a>> {
        let req = |r: R<'a>| Ev::Req(r.0, r.1, r.2, r.3);
        vec![
            req(first),
            Ev::Call("make", true),
            req(second),
            Ev::Call("make", true),
            req(third),
            Ev::Call("make", false),
            req(after),
        ]
    }

    fn retry_advice(sessions: &[SessionEvents], pricing: &Pricing) -> AdviceReport {
        analyze_with(
            &report(vec![]),
            &References::new(),
            &[],
            &ContextSaving::default(),
            &retry_saving(sessions, &prices()),
            &ModelMixUse::default(),
            &ContextHygieneUse::default(),
            &DelegationUse::default(),
            &CacheExpiryUse::default(),
            pricing,
        )
    }

    #[test]
    fn no_loop_no_finding() {
        let s = trace(
            "s",
            &a_loop(
                ("test-model", 1, 1, 1),
                ("test-model", 1, 1, 1),
                ("test-model", 1, 1, 1),
                ("test-model", 1, 1, 1),
            )
            .into_iter()
            .map(|e| match e {
                // The second call succeeds: only one leading failure.
                Ev::Call(i, true) => Ev::Call(i, false),
                e => e,
            })
            .collect::<Vec<_>>(),
        );
        assert!(
            retry_saving(std::slice::from_ref(&s), &prices())
                .tools
                .is_empty()
        );
        assert!(find_none(
            &retry_advice(&[s], &pricing()),
            FindingKind::RetryLoops
        ));
    }

    #[test]
    fn two_failing_calls_are_no_loop() {
        let s = trace(
            "s",
            &[
                Ev::Call("make", true),
                Ev::Req("test-model", 10, 100, 0),
                Ev::Call("make", true),
                Ev::Req("test-model", 10, 100, 0),
            ],
        );
        assert!(retry_saving(&[s], &prices()).tools.is_empty());
    }

    #[test]
    fn the_tokens_are_those_of_the_retries_without_the_first_attempt() {
        // Retries: (100 + 1,000 + 200) + (50 + 2,000 + 0) = 3,350 in the split 150 : 3,000 : 200. The first
        // request (1,000 input) issued the first attempt and the last one came after the loop.
        let s = trace(
            "s",
            &a_loop(
                ("test-model", 1_000, 0, 0),
                ("test-model", 100, 1_000, 200),
                ("test-model", 50, 2_000, 0),
                ("test-model", 999, 0, 0),
            ),
        );
        let use_ = retry_saving(std::slice::from_ref(&s), &prices());
        let row = &use_.tools[0];
        assert_eq!(
            (
                row.requests,
                row.input_tokens,
                row.cache_read,
                row.cache_write
            ),
            (2, 150, 3_000, 200)
        );
        let a = retry_advice(&[s], &pricing());
        let f = find(&a, FindingKind::RetryLoops);
        assert_eq!(f.total, Some(Tokens::measured(3_350)));
        assert_eq!((f.items[0].requests, f.items[0].tokens.count), (2, 3_350));
        assert_eq!(f.items[0].name, "Bash");
        // 150 × $2 + 3,000 × $0.10 + 200 × $2.50 per M = 0.0003 + 0.0003 + 0.0005.
        assert_eq!(f.impact.saving, Tokens::estimated(3_350));
        assert!(
            close(f.impact.saving_usd, 0.0011),
            "{:?}",
            f.impact.saving_usd
        );
        assert_eq!(f.impact.confidence, Confidence::Inferred);
        assert_eq!(f.count, 1);
    }

    #[test]
    fn requests_are_priced_with_their_own_model_and_the_rest_at_the_average() {
        // The priced request: 100 input × $2 = 0.0002. The unpriced one: 100 × $3 + 1,000 × $0.50
        // (the period's average) = 0.0008.
        let s = trace(
            "s",
            &a_loop(
                ("test-model", 5, 0, 0),
                ("test-model", 100, 0, 0),
                ("other-model", 100, 1_000, 0),
                ("test-model", 5, 0, 0),
            ),
        );
        let row = retry_saving(std::slice::from_ref(&s), &prices())
            .tools
            .remove(0);
        assert_eq!((row.priced_requests, row.unpriced_requests), (1, 1));
        assert!(close(Some(row.usd), 0.0002));
        let a = retry_advice(std::slice::from_ref(&s), &pricing());
        let f = find(&a, FindingKind::RetryLoops);
        assert!(
            close(f.impact.saving_usd, 0.001),
            "{:?}",
            f.impact.saving_usd
        );
        assert!(f.notes.iter().any(|n| n.contains("average price")));
        // No price anywhere: tokens, no USD.
        let a = retry_advice(&[s], &Pricing::default());
        let f = find(&a, FindingKind::RetryLoops);
        assert!(close(f.impact.saving_usd, 0.0002));
        let only_unpriced = trace(
            "u",
            &a_loop(
                ("other-model", 5, 0, 0),
                ("other-model", 100, 0, 0),
                ("other-model", 100, 0, 0),
                ("other-model", 5, 0, 0),
            ),
        );
        let a = retry_advice(&[only_unpriced], &Pricing::default());
        let f = find(&a, FindingKind::RetryLoops);
        assert_eq!(f.impact.saving, Tokens::estimated(200));
        assert_eq!(f.impact.saving_usd, None);
        assert!(f.notes.iter().any(|n| n.contains("not in the USD")));
    }

    #[test]
    fn loops_are_grouped_per_tool_across_sessions() {
        let looped = |tool: &'static str, input: &'static str, tokens: u64| {
            vec![
                Ev::Tool(tool, input, true),
                Ev::Req("test-model", tokens, 0, 0),
                Ev::Tool(tool, input, true),
                Ev::Req("test-model", tokens, 0, 0),
                Ev::Tool(tool, input, false),
            ]
        };
        // Loops of the same call right after each other would be one run; the Edit loop separates them.
        let mut s1 = looped("Bash", "make", 100);
        s1.extend(looped("Edit", "a.rs", 1_000));
        s1.extend(looped("Bash", "make", 100));
        let mut s2 = looped("Bash", "cargo", 100);
        s2.extend(looped("Bash", "make", 100));
        let a = retry_advice(&[trace("s1", &s1), trace("s2", &s2)], &pricing());
        let f = find(&a, FindingKind::RetryLoops);
        assert_eq!(f.count, 2);
        // Edit: 2,000 tokens; Bash: 4 loops × 200 = 800.
        let names: Vec<&str> = f.items.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["Edit", "Bash"]);
        assert_eq!(
            f.items[1].detail,
            "4 loops in 2 sessions, 12 calls; most often on `make`"
        );
        assert_eq!(
            f.items[0].detail,
            "1 loop in 1 session, 3 calls; most often on `a.rs`"
        );
        assert_eq!(f.total, Some(Tokens::measured(2_800)));
    }

    #[test]
    fn a_failed_hook_inside_a_loop_is_named_and_one_outside_is_not() {
        let with_hook = trace(
            "h",
            &[
                Ev::Hook("preToolUse", false),
                Ev::Call("make", true),
                Ev::Req("test-model", 10, 0, 0),
                Ev::Hook("preToolUse", false),
                Ev::Call("make", true),
                Ev::Hook("postToolUse", true),
                Ev::Req("test-model", 10, 0, 0),
                Ev::Call("make", false),
                Ev::Hook("preToolUse", false),
            ],
        );
        let row = retry_saving(std::slice::from_ref(&with_hook), &prices())
            .tools
            .remove(0);
        assert_eq!(
            (row.hook_failures, row.hook.as_deref()),
            (1, Some("preToolUse"))
        );
        let a = retry_advice(&[with_hook], &pricing());
        assert_eq!(
            find(&a, FindingKind::RetryLoops).items[0].detail,
            "1 loop in 1 session, 3 calls; most often on `make`; a preToolUse hook failed in 1 of the 1 loops"
        );
        let without = trace(
            "n",
            &[
                Ev::Hook("preToolUse", false),
                Ev::Call("make", true),
                Ev::Call("make", true),
                Ev::Hook("preToolUse", true),
                Ev::Call("make", false),
                Ev::Hook("preToolUse", false),
            ],
        );
        let a = retry_advice(&[without], &pricing());
        assert!(
            !find(&a, FindingKind::RetryLoops).items[0]
                .detail
                .contains("hook")
        );
    }

    #[test]
    fn an_estimated_request_makes_the_total_estimated() {
        let mut s = trace(
            "s",
            &a_loop(
                ("test-model", 1, 0, 0),
                ("test-model", 100, 0, 0),
                ("test-model", 100, 0, 0),
                ("test-model", 1, 0, 0),
            ),
        );
        for e in &mut s.events {
            if let EventKind::Usage(u) = &mut e.kind {
                u.measured = false;
            }
        }
        let a = retry_advice(&[s], &pricing());
        assert_eq!(
            find(&a, FindingKind::RetryLoops).total,
            Some(Tokens::estimated(200))
        );
    }

    /// $5 input / $0.5 cache read for `big`, $1 / $0.1 for `small`, `mid` in between.
    fn mix_prices() -> PriceTable {
        PriceTable::parse(
            r#"{
                "big": {"input": 5.0, "output": 25.0, "cache_write": 6.0, "cache_read": 0.5},
                "mid": {"input": 3.0, "output": 15.0, "cache_write": 3.5, "cache_read": 0.3},
                "small": {"input": 1.0, "output": 5.0, "cache_write": 1.25, "cache_read": 0.1}
            }"#,
        )
        .unwrap()
    }

    fn mix_advice(sessions: &[SessionEvents]) -> AdviceReport {
        let table = mix_prices();
        analyze_with(
            &report(vec![]),
            &References::new(),
            &[],
            &ContextSaving::default(),
            &RetryLoopUse::default(),
            &model_mix_saving(sessions, &table),
            &ContextHygieneUse::default(),
            &DelegationUse::default(),
            &CacheExpiryUse::default(),
            &Pricing::from_sessions(sessions, &table),
        )
    }

    /// `count` small requests of 1,000 input tokens on `model`, the first `subagents` of them by subagents.
    fn small(model: &'static str, count: usize, subagents: usize) -> Vec<Req<'static>> {
        (0..count)
            .map(|i| (model, 1_000, 0, 0, i < subagents))
            .collect()
    }

    #[test]
    fn small_requests_on_the_costliest_model_are_priced_on_the_next_cheaper_one() {
        let mut reqs = small("big", 20, 5);
        reqs.push(("big", 60_000, 0, 0, false)); // large: not small
        reqs.push(("small", 1_000, 0, 0, false));
        reqs.push(("mid", 1_000, 0, 0, false)); // the next lower price is mid, not small
        let a = mix_advice(&[requests("s", &reqs)]);
        let f = find(&a, FindingKind::ModelMix);
        // 20 requests × 1,000 input × ($5 - $3) per million = $0.04.
        assert!(
            close(f.impact.saving_usd, 0.04),
            "{:?}",
            f.impact.saving_usd
        );
        assert_eq!(f.impact.confidence, Confidence::Inferred);
        assert_eq!(f.items[0].name, "big");
        assert_eq!((f.items[0].requests, f.items[0].tokens.count), (20, 20_000));
        let detail = &f.items[0].detail;
        assert!(detail.contains("20 of 21 requests small"), "{detail}");
        assert!(detail.contains("25.0% of them by subagents"), "{detail}");
        assert!(detail.ends_with("saved on mid"), "{detail}");
        assert_eq!(f.total, Some(Tokens::measured(20_000)));
        assert!(f.notes.iter().any(|n| n.contains("inferred")));
    }

    #[test]
    fn a_single_model_is_no_finding() {
        let a = mix_advice(&[requests("s", &small("big", 40, 0))]);
        assert!(find_none(&a, FindingKind::ModelMix));
    }

    #[test]
    fn fewer_than_the_minimum_of_small_requests_is_no_finding() {
        let mut reqs = small("big", 19, 0);
        reqs.push(("mid", 1_000, 0, 0, false));
        assert!(find_none(
            &mix_advice(&[requests("s", &reqs)]),
            FindingKind::ModelMix
        ));
        reqs.push(("big", 1_000, 0, 0, false));
        assert!(!find_none(
            &mix_advice(&[requests("s", &reqs)]),
            FindingKind::ModelMix
        ));
    }

    #[test]
    fn a_saving_below_five_percent_of_the_cost_is_no_finding() {
        // The saving is $0.04; one large request of 10M tokens costs $50.
        let mut reqs = small("big", 20, 0);
        reqs.push(("mid", 1_000, 0, 0, false));
        reqs.push(("big", 10_000_000, 0, 0, false));
        assert!(find_none(
            &mix_advice(&[requests("s", &reqs)]),
            FindingKind::ModelMix
        ));
    }

    #[test]
    fn requests_without_a_price_are_left_out_and_noted() {
        let mut reqs = small("big", 20, 0);
        reqs.push(("mid", 1_000, 0, 0, false));
        reqs.extend(small("mystery", 3, 0));
        let sessions = [requests("s", &reqs)];
        let m = model_mix_saving(&sessions, &mix_prices());
        assert_eq!((m.unpriced_requests, m.rows[0].small_requests), (3, 20));
        let a = mix_advice(&sessions);
        let f = find(&a, FindingKind::ModelMix);
        assert!(close(f.impact.saving_usd, 0.04));
        assert!(
            f.notes
                .iter()
                .any(|n| n.contains("3 small requests had no price"))
        );
    }

    #[test]
    fn the_pair_is_chosen_per_client() {
        use crate::model::Session;
        let mut other = requests("o", &small("small", 30, 0));
        other.session = Session::new("o", Source::CopilotCli);
        // Claude Code has big and mid, Copilot CLI only small: one finding row, for Claude Code.
        let mut reqs = small("big", 20, 0);
        reqs.push(("mid", 1_000, 0, 0, false));
        let m = model_mix_saving(&[requests("s", &reqs), other], &mix_prices());
        assert_eq!(m.rows.len(), 1);
        assert_eq!(m.rows[0].source, Some(Source::ClaudeCode));
    }

    // ---- context hygiene ----

    /// One event of a session built for the context hygiene tests.
    enum Hy {
        /// A main-agent request of this many tokens, all cache read.
        Req(u64),
        /// A subagent request of this many tokens.
        Sub(u64),
        Compact,
    }

    /// $2 input, $10 output, $2.50 cache write, $0.10 cache read for every `claude-h…` model, whose
    /// documented window is 200,000 tokens.
    fn hygiene_prices() -> PriceTable {
        PriceTable::parse(
            r#"{"claude-h": {"input": 2.0, "output": 10.0, "cache_write": 2.5, "cache_read": 0.1}}"#,
        )
        .unwrap()
    }

    fn hy(id: &str, source: Source, model: &str, events: &[Hy]) -> SessionEvents {
        use crate::model::{Event, Session};
        SessionEvents {
            session: Session::new(id, source),
            events: events
                .iter()
                .map(|e| Event {
                    at: None,
                    kind: match *e {
                        Hy::Req(tokens) | Hy::Sub(tokens) => EventKind::Usage(Usage {
                            model: Some(model.into()),
                            cache_read: tokens,
                            subagent: matches!(e, Hy::Sub(_)),
                            measured: true,
                            ..Usage::default()
                        }),
                        Hy::Compact => EventKind::Compaction,
                    },
                })
                .collect(),
            warnings: Vec::new(),
        }
    }

    /// A session with two compactions: 10,000, 50,000, C, 20,000, 60,000, C, 30,000. The saving is
    /// 40,000 + 10,000 + 50,000 + 20,000 = 120,000 tokens of cache read.
    fn compacted(id: &str) -> SessionEvents {
        use Hy::*;
        hy(
            id,
            Source::ClaudeCode,
            "claude-h1",
            &[
                Req(10_000),
                Req(50_000),
                Compact,
                Req(20_000),
                Req(60_000),
                Compact,
                Req(30_000),
            ],
        )
    }

    fn hygiene_advice(sessions: &[SessionEvents]) -> AdviceReport {
        let table = hygiene_prices();
        let mut r = report(vec![]);
        r.totals.tokens = 1_000_000;
        analyze_with(
            &r,
            &References::new(),
            &[],
            &context_saving(sessions, &table),
            &RetryLoopUse::default(),
            &ModelMixUse::default(),
            &hygiene_saving(sessions, &table),
            &DelegationUse::default(),
            &CacheExpiryUse::default(),
            &Pricing::from_sessions(sessions, &table),
        )
    }

    fn flagged(sessions: &[SessionEvents]) -> Vec<String> {
        hygiene_saving(sessions, &hygiene_prices())
            .sessions
            .into_iter()
            .map(|h| h.session)
            .collect()
    }

    #[test]
    fn two_compactions_flag_a_session_and_one_does_not() {
        use Hy::*;
        let once = hy(
            "once",
            Source::ClaudeCode,
            "claude-h1",
            &[Req(10_000), Req(90_000), Compact, Req(20_000)],
        );
        assert_eq!(flagged(&[compacted("twice"), once]), ["twice"]);
        let h = hygiene_saving(&[compacted("twice")], &hygiene_prices());
        let s = &h.sessions[0];
        assert_eq!(
            (s.requests, s.first_tokens, s.peak_tokens, s.compactions),
            (5, 10_000, 60_000, 2)
        );
        assert_eq!(s.before_compaction, Some(55_000), "(50,000 + 60,000) / 2");
        assert!(!s.near_limit);
        assert_eq!((s.saved, s.measured), (120_000, true));
        assert_eq!(h.saving.cache_read, 120_000);
        assert_eq!(h.saving.requests, 5, "every main-agent request");
        // 120,000 × $0.10 per M.
        assert!(close(Some(h.saving.usd), 0.012), "{}", h.saving.usd);
    }

    #[test]
    fn near_the_limit_without_a_compaction_is_flagged_and_with_one_it_is_not() {
        use Hy::*;
        let near = hy(
            "near",
            Source::ClaudeCode,
            "claude-h1",
            &[Req(20_000), Req(120_000), Req(185_000), Sub(150_000)],
        );
        let near_compacted = hy(
            "near-compacted",
            Source::ClaudeCode,
            "claude-h1",
            &[Req(20_000), Req(185_000), Compact, Req(30_000)],
        );
        let far = hy(
            "far",
            Source::ClaudeCode,
            "claude-h1",
            &[Req(20_000), Req(100_000)],
        );
        let h = hygiene_saving(&[near, near_compacted, far], &hygiene_prices());
        let ids: Vec<&str> = h.sessions.iter().map(|s| s.session.as_str()).collect();
        assert_eq!(ids, ["near"]);
        let s = &h.sessions[0];
        assert!(s.near_limit);
        assert_eq!((s.compactions, s.before_compaction), (0, None));
        assert_eq!(s.window, Some(200_000));
        // 100,000 + 165,000; the subagent's request is not the main context.
        assert_eq!(s.saved, 265_000);
        assert_eq!(s.requests, 3);
    }

    #[test]
    fn the_finding_lists_the_flagged_sessions_with_their_evidence() {
        use Hy::*;
        let mut cleared = compacted("cleared");
        cleared.events.insert(
            0,
            crate::model::Event {
                at: None,
                kind: EventKind::Clear,
            },
        );
        cleared.session.project_dir = Some("/work/demo".into());
        let near = hy(
            "near",
            Source::ClaudeCode,
            "claude-h1",
            &[Req(20_000), Req(185_000)],
        );
        let a = hygiene_advice(&[cleared, compacted("plain"), near]);
        let f = find(&a, FindingKind::ContextHygiene);
        assert_eq!(f.count, 3);
        let names: Vec<&str> = f.items.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "(no project) · near",
                "demo · cleared",
                "(no project) · plain"
            ],
            "largest saving first, then by id"
        );
        assert_eq!(
            f.items[0].detail,
            "2 requests, peak 185,000 tokens, 0 compactions, 0 clears, near the limit; 20,000 tokens at the start"
        );
        assert_eq!(
            f.items[1].detail,
            "5 requests, peak 60,000 tokens, 2 compactions, 1 clear, not near the limit; 55,000 tokens before a compaction against 10,000 at the start"
        );
        assert_eq!(f.items[1].session_id.as_deref(), Some("cleared"));
        assert_eq!(f.items[1].tokens, Tokens::measured(120_000));
        assert_eq!(f.total, Some(Tokens::measured(165_000 + 240_000)));
        assert_eq!(f.share_of_total_tokens, Some(0.405));
        assert_eq!(f.impact.saving, Tokens::estimated(405_000));
        assert_eq!(f.impact.confidence, Confidence::Inferred);
        // 405,000 × $0.10 per M.
        assert!(close(f.impact.saving_usd, 0.0405), "{:?}", f.impact);
        assert!(
            f.notes
                .iter()
                .any(|n| n.contains("1 after a recorded /clear (Claude Code), ~0 by the proxy")),
            "{:?}",
            f.notes
        );
        assert!(f.notes.iter().any(|n| n.contains("% of the period's cost")));
        assert!(f.action.contains("/clear"));
    }

    fn at(time: &str) -> Option<jiff::Timestamp> {
        Some(format!("2026-10-03T{time}Z").parse().unwrap())
    }

    /// An OpenCode session in `/work/demo` on `branch`, from `start` to `end`, with two compactions.
    fn proxied(id: &str, branch: &str, start: &str, end: &str) -> SessionEvents {
        let mut se = compacted(id);
        se.session.source = Source::OpenCode;
        se.session.project_dir = Some("/work/demo".into());
        se.session.branch = Some(branch.into());
        se.session.started_at = at(start);
        se.session.ended_at = at(end);
        se
    }

    fn clears_of(sessions: &[SessionEvents]) -> Vec<(String, u64, bool)> {
        hygiene_saving(sessions, &hygiene_prices())
            .sessions
            .into_iter()
            .map(|h| (h.session, h.clears, h.clears_measured))
            .collect::<Vec<_>>()
    }

    #[test]
    fn the_proxy_counts_a_session_that_starts_right_after_another_on_the_same_branch() {
        let clears = clears_of(&[
            proxied("first", "feat/a", "10:00:00", "10:20:00"),
            // 30 seconds after `first` ended, same branch: a fresh start, estimated.
            proxied("same-branch", "feat/a", "10:20:30", "10:40:00"),
            // A queue: right after, but on another branch.
            proxied("other-branch", "feat/b", "10:40:10", "11:00:00"),
            // 61 seconds after `other-branch` ended, on its branch.
            proxied("too-late", "feat/b", "11:01:01", "11:20:00"),
        ]);
        let get = |id: &str| {
            clears
                .iter()
                .find(|c| c.0 == id)
                .map(|c| (c.1, c.2))
                .unwrap()
        };
        assert_eq!(get("same-branch"), (1, false));
        assert_eq!(get("other-branch"), (0, false));
        assert_eq!(get("too-late"), (0, false));
        assert_eq!(get("first"), (0, false));
        let h = hygiene_saving(
            &[
                proxied("first", "feat/a", "10:00:00", "10:20:00"),
                proxied("same-branch", "feat/a", "10:20:30", "10:40:00"),
            ],
            &hygiene_prices(),
        );
        assert_eq!((h.fresh_measured, h.fresh_estimated), (0, 1));
    }

    #[test]
    fn the_proxy_is_not_applied_where_the_client_records_a_clear() {
        let mut first = proxied("first", "feat/a", "10:00:00", "10:20:00");
        let mut next = proxied("next", "feat/a", "10:20:30", "10:40:00");
        first.session.source = Source::ClaudeCode;
        next.session.source = Source::ClaudeCode;
        assert_eq!(
            clears_of(&[first.clone(), next.clone()])
                .iter()
                .map(|c| (c.1, c.2))
                .collect::<Vec<_>>(),
            [(0, true), (0, true)],
            "no recorded clear, no proxy"
        );
        next.events.insert(
            0,
            crate::model::Event {
                at: at("10:20:30"),
                kind: EventKind::Clear,
            },
        );
        let h = hygiene_saving(&[first, next], &hygiene_prices());
        let next = h.sessions.iter().find(|s| s.session == "next").unwrap();
        assert_eq!((next.clears, next.clears_measured), (1, true));
        assert_eq!((h.fresh_measured, h.fresh_estimated), (1, 0));
    }

    #[test]
    fn an_estimated_clear_carries_a_tilde() {
        let a = hygiene_advice(&[
            proxied("a", "feat/a", "10:00:00", "10:20:00"),
            proxied("b", "feat/a", "10:20:30", "10:40:00"),
            proxied("c", "feat/c", "12:00:00", "12:20:00"),
        ]);
        let f = find(&a, FindingKind::ContextHygiene);
        let b = f.items.iter().find(|i| i.name == "demo · b").unwrap();
        assert!(
            b.detail.contains("2 compactions, ~1 clear,"),
            "{}",
            b.detail
        );
        assert!(
            f.notes
                .iter()
                .any(|n| n.contains("0 after a recorded /clear (Claude Code), ~1 by the proxy")),
            "{:?}",
            f.notes
        );
    }

    #[test]
    fn fewer_than_three_flagged_sessions_are_no_finding() {
        let a = hygiene_advice(&[compacted("a"), compacted("b")]);
        assert!(find_none(&a, FindingKind::ContextHygiene));
        let a = hygiene_advice(&[compacted("a"), compacted("b"), compacted("c")]);
        assert!(!find_none(&a, FindingKind::ContextHygiene));
    }

    #[test]
    fn a_saving_below_five_percent_of_the_period_cost_is_no_finding() {
        // Three flagged sessions save 360,000 cache read tokens, $0.036; a request of 10M input costs $20.
        // Another model, so that the compactions of the others give it no observed window.
        let mut big = hy("big", Source::ClaudeCode, "claude-h2", &[]);
        big.events.push(crate::model::Event {
            at: None,
            kind: EventKind::Usage(Usage {
                model: Some("claude-h2".into()),
                input: 10_000_000,
                measured: true,
                ..Usage::default()
            }),
        });
        let sessions = [compacted("a"), compacted("b"), compacted("c"), big];
        let h = hygiene_saving(&sessions, &hygiene_prices());
        assert_eq!(h.sessions.len(), 3);
        assert!(h.period_usd > 20.0);
        assert!(find_none(
            &hygiene_advice(&sessions),
            FindingKind::ContextHygiene
        ));
        // Without any price the share is taken of the period's tokens: 360,000 of 1,000,000.
        let unpriced: Vec<SessionEvents> = ["a", "b", "c"]
            .iter()
            .map(|id| {
                let mut se = compacted(id);
                for e in &mut se.events {
                    if let EventKind::Usage(u) = &mut e.kind {
                        u.model = Some("mystery".into());
                    }
                }
                se
            })
            .collect();
        let a = hygiene_advice(&unpriced);
        let f = find(&a, FindingKind::ContextHygiene);
        assert_eq!(f.impact.saving_usd, None);
        assert!(
            f.notes
                .iter()
                .any(|n| n.contains("36.0% of the period's tokens (no price known)"))
        );
        assert!(f.notes.iter().any(|n| n.contains("not in the USD")));
        let mut r = report(vec![]);
        r.totals.tokens = 100_000_000;
        let table = hygiene_prices();
        let a = analyze_with(
            &r,
            &References::new(),
            &[],
            &ContextSaving::default(),
            &RetryLoopUse::default(),
            &ModelMixUse::default(),
            &hygiene_saving(&unpriced, &table),
            &DelegationUse::default(),
            &CacheExpiryUse::default(),
            &Pricing::from_sessions(&unpriced, &table),
        );
        assert!(
            find_none(&a, FindingKind::ContextHygiene),
            "0.36% of the tokens"
        );
    }

    #[test]
    fn context_size_leaves_out_the_requests_of_the_flagged_sessions() {
        use Hy::*;
        // Three flagged sessions with requests of at least 200,000 tokens (the observed compaction window
        // replaces the documented one), and one session that is not flagged.
        let large = |id: &str| {
            hy(
                id,
                Source::ClaudeCode,
                "claude-h1",
                &[
                    Req(50_000),
                    Req(250_000),
                    Compact,
                    Req(60_000),
                    Req(300_000),
                    Compact,
                    Req(70_000),
                ],
            )
        };
        let calm = hy(
            "calm",
            Source::ClaudeCode,
            "claude-h1",
            &[Req(100_000), Req(220_000), Req(240_000)],
        );
        let sessions = [large("a"), large("b"), large("c"), calm.clone()];
        let table = hygiene_prices();
        let hygiene = hygiene_saving(&sessions, &table);
        assert_eq!(flagged(&sessions), ["a", "b", "c"]);
        let all = context_saving(&sessions, &table);
        let only_calm = context_saving(std::slice::from_ref(&calm), &table);
        let in_flagged = context_saving(&sessions[..3], &table);
        assert_eq!(hygiene.overlap, in_flagged);
        assert_eq!(all.requests, only_calm.requests + in_flagged.requests);

        let mut r = report(vec![]);
        r.totals.requests = 18;
        r.totals.tokens = 10_000_000;
        let a = analyze_with(
            &r,
            &References::new(),
            &[],
            &all,
            &RetryLoopUse::default(),
            &ModelMixUse::default(),
            &hygiene,
            &DelegationUse::default(),
            &CacheExpiryUse::default(),
            &Pricing::from_sessions(&sessions, &table),
        );
        let size = find(&a, FindingKind::ContextSize);
        let hyg = find(&a, FindingKind::ContextHygiene);
        // Context size keeps the calm session only: 120,000 + 140,000.
        assert_eq!(size.impact.saving, Tokens::estimated(only_calm.tokens()));
        assert_eq!(only_calm.tokens(), 260_000);
        // ContextHygiene counts every request of a, b and c: 3 × (200,000 + 10,000 + 250,000 + 20,000).
        assert_eq!(hyg.impact.saving, Tokens::estimated(3 * 480_000));
        // No request in both: each session's requests are in exactly one of the two savings.
        let flagged_ids: Vec<&str> = hyg
            .items
            .iter()
            .filter_map(|i| i.session_id.as_deref())
            .collect();
        for se in &sessions {
            let in_hygiene = flagged_ids.contains(&se.session.id.as_str());
            let in_size =
                !in_hygiene && context_saving(std::slice::from_ref(se), &table).requests > 0;
            assert!(in_hygiene != in_size, "{} is in one finding", se.session.id);
        }
        assert!(
            size.notes.iter().any(|n| n.starts_with(
                "6 of those requests lie in the sessions of the Context hygiene finding"
            )),
            "{:?}",
            size.notes
        );
        assert!(close(size.impact.saving_usd, only_calm.usd));

        // Without the finding (two flagged sessions only) Context size keeps all of them.
        let two = [large("a"), large("b"), calm];
        let a = analyze_with(
            &r,
            &References::new(),
            &[],
            &context_saving(&two, &table),
            &RetryLoopUse::default(),
            &ModelMixUse::default(),
            &hygiene_saving(&two, &table),
            &DelegationUse::default(),
            &CacheExpiryUse::default(),
            &Pricing::from_sessions(&two, &table),
        );
        assert!(find_none(&a, FindingKind::ContextHygiene));
        assert_eq!(
            find(&a, FindingKind::ContextSize).impact.saving.count,
            context_saving(&two, &table).tokens()
        );
    }
    // ---- delegation ----

    /// A session of `source` with `large` main-agent requests of 250,000 input tokens, `small` main-agent
    /// requests of 1,000 and `sub` subagent requests of 250,000.
    fn delegating(
        id: &str,
        source: Source,
        large: usize,
        small: usize,
        sub: usize,
    ) -> SessionEvents {
        let mut se = requests(id, &[]);
        se.session.source = source;
        let mut reqs: Vec<Req> = Vec::new();
        reqs.extend((0..large).map(|_| ("test-model", 250_000, 0, 0, false)));
        reqs.extend((0..small).map(|_| ("test-model", 1_000, 0, 0, false)));
        reqs.extend((0..sub).map(|_| ("test-model", 250_000, 0, 0, true)));
        se.events = requests(id, &reqs).events;
        se
    }

    fn delegation_advice(sessions: &[SessionEvents]) -> AdviceReport {
        analyze_with(
            &report(vec![]),
            &References::new(),
            &[],
            &ContextSaving::default(),
            &RetryLoopUse::default(),
            &ModelMixUse::default(),
            &ContextHygieneUse::default(),
            &delegation_use(sessions),
            &CacheExpiryUse::default(),
            &Pricing::default(),
        )
    }

    #[test]
    fn large_context_without_subagents_is_a_delegation_finding_without_a_saving() {
        let a = delegation_advice(&[
            delegating("a", Source::ClaudeCode, 20, 20, 0),
            delegating("b", Source::ClaudeCode, 25, 0, 0),
        ]);
        let f = find(&a, FindingKind::Delegation);
        assert_eq!(f.count, 1, "one client");
        assert_eq!(f.impact, Impact::none());
        assert!(f.explanation.contains("No saving is estimated"));
        assert_eq!(
            f.items[0].detail,
            "65 requests, 45 on a large context, 0 by subagents (0.0%)"
        );
        // Sessions follow the clients, most large-context requests first.
        assert_eq!(f.items[1].session_id.as_deref(), Some("b"));
        assert_eq!(f.items[1].detail, "25 on a large context, 0 by subagents");
        assert_eq!(f.items[2].session_id.as_deref(), Some("a"));
        assert!(f.notes[0].starts_with("69.2% of the period's requests (45 of 65)"));
    }

    #[test]
    fn the_sessions_are_the_top_ten() {
        let sessions: Vec<SessionEvents> = (0..12)
            .map(|i| delegating(&format!("s{i:02}"), Source::OpenCode, 20 + i, 0, 0))
            .collect();
        let f = delegation_advice(&sessions);
        let f = find(&f, FindingKind::Delegation);
        assert_eq!(f.items.len(), 1 + TOP);
        assert_eq!(f.items[1].session_id.as_deref(), Some("s11"));
    }

    #[test]
    fn no_delegation_finding_where_subagents_ran() {
        // 1 of 41 requests by a subagent is 2.4 %: below 5 %, a finding. The fixture-like session
        // with 3 subagent requests of 43 is 7 %: none.
        let low = delegation_advice(&[delegating("a", Source::ClaudeCode, 40, 0, 1)]);
        assert!(!find_none(&low, FindingKind::Delegation));
        let high = delegation_advice(&[delegating("a", Source::ClaudeCode, 40, 0, 3)]);
        assert!(find_none(&high, FindingKind::Delegation));
    }

    #[test]
    fn a_share_just_above_five_percent_is_no_finding() {
        let shown = |small, sub| {
            let a = delegation_advice(&[delegating("a", Source::ClaudeCode, 20, small, sub)]);
            !find_none(&a, FindingKind::Delegation)
        };
        assert!(!shown(17, 2), "2 of 39 is 5.13 %");
        assert!(!shown(18, 2), "2 of 40 is exactly 5 %, which is not below");
        assert!(shown(19, 2), "2 of 41 is 4.88 %");
    }

    #[test]
    fn fewer_than_twenty_large_context_requests_is_no_finding() {
        let a = delegation_advice(&[delegating("a", Source::ClaudeCode, 19, 100, 0)]);
        assert!(find_none(&a, FindingKind::Delegation));
        let a = delegation_advice(&[
            delegating("a", Source::ClaudeCode, 10, 0, 0),
            delegating("b", Source::ClaudeCode, 10, 0, 0),
        ]);
        assert!(
            !find_none(&a, FindingKind::Delegation),
            "the minimum counts per client, not per session"
        );
    }

    #[test]
    fn subagent_requests_do_not_count_as_large_context_main_agent_requests() {
        let a = delegation_advice(&[delegating("a", Source::ClaudeCode, 19, 0, 1)]);
        assert!(find_none(&a, FindingKind::Delegation));
    }

    #[test]
    fn a_client_outside_the_list_is_no_finding() {
        for source in [Source::VsCodeCopilot, Source::Otel] {
            let a = delegation_advice(&[delegating("a", source, 50, 0, 0)]);
            assert!(find_none(&a, FindingKind::Delegation), "{source:?}");
        }
        // Next to a client of the list, it is not shown in the rows.
        let a = delegation_advice(&[
            delegating("a", Source::CopilotCli, 30, 0, 0),
            delegating("b", Source::Otel, 50, 0, 0),
        ]);
        let f = find(&a, FindingKind::Delegation);
        assert_eq!(f.count, 1);
        assert_eq!(f.items.len(), 2);
        assert_eq!(f.items[0].name, "copilot-cli");
    }

    /// $3.75 cache write, $0.30 cache read: the difference is $3.45 per million tokens.
    fn expiry_prices() -> PriceTable {
        PriceTable::parse(
            r#"{"claude-test": {"input": 3.0, "output": 15.0, "cache_write": 3.75, "cache_read": 0.3}}"#,
        )
        .unwrap()
    }

    /// One request: (seconds since the start or `None`, model, cache read, cache write, 1-hour write, subagent).
    type Timed<'a> = (Option<i64>, &'a str, u64, u64, u64, bool);

    fn timed(id: &str, reqs: &[Timed]) -> SessionEvents {
        use crate::model::{Event, Session};
        let start: jiff::Timestamp = "2026-10-03T12:00:00Z".parse().unwrap();
        SessionEvents {
            session: Session::new(id, Source::ClaudeCode),
            events: reqs
                .iter()
                .map(
                    |&(secs, model, cache_read, cache_write, cache_write_1h, subagent)| Event {
                        at: secs.map(|s| start + jiff::SignedDuration::from_secs(s)),
                        kind: EventKind::Usage(Usage {
                            model: Some(model.into()),
                            cache_read,
                            cache_write,
                            cache_write_1h,
                            subagent,
                            measured: true,
                            ..Usage::default()
                        }),
                    },
                )
                .collect(),
            warnings: Vec::new(),
        }
    }

    /// Two main-agent requests of `claude-test`, the second after `gap` seconds with a rewritten context.
    fn after_gap(gap: i64) -> SessionEvents {
        timed(
            "s",
            &[
                (Some(0), "claude-test", 0, 100_000, 0, false),
                (Some(gap), "claude-test", 1_000, 100_000, 0, false),
            ],
        )
    }

    fn expiry_advice(sessions: &[SessionEvents]) -> AdviceReport {
        analyze_with(
            &report(vec![]),
            &References::new(),
            &[],
            &ContextSaving::default(),
            &RetryLoopUse::default(),
            &ModelMixUse::default(),
            &ContextHygieneUse::default(),
            &DelegationUse::default(),
            &cache_expiry_use(sessions, &expiry_prices()),
            &Pricing::default(),
        )
    }

    #[test]
    fn a_pause_longer_than_five_minutes_expires_the_cache_and_the_first_request_never_does() {
        let below = cache_expiry_use(&[after_gap(299)], &expiry_prices());
        let exactly = cache_expiry_use(&[after_gap(300)], &expiry_prices());
        let above = cache_expiry_use(&[after_gap(301)], &expiry_prices());
        assert!(below.sessions.is_empty() && exactly.sessions.is_empty());
        let row = &above.sessions[0];
        assert_eq!((row.requests, row.tokens), (1, 100_000));
        assert_eq!(row.ttl, CACHE_TTL_5M);
        assert_eq!((row.min_gap_secs, row.max_gap_secs), (301, 301));
    }

    #[test]
    fn a_session_that_wrote_one_hour_tokens_has_a_one_hour_lifetime() {
        let session = |gap: i64| {
            timed(
                "s",
                &[
                    (Some(0), "claude-test", 0, 100_000, 100_000, false),
                    (Some(gap), "claude-test", 1_000, 100_000, 100_000, false),
                ],
            )
        };
        let thirty = cache_expiry_use(&[session(30 * 60)], &expiry_prices());
        let seventy = cache_expiry_use(&[session(70 * 60)], &expiry_prices());
        assert!(thirty.sessions.is_empty());
        assert_eq!(seventy.sessions[0].ttl, CACHE_TTL_1H);
        assert_eq!(seventy.sessions[0].requests, 1);
    }

    #[test]
    fn a_request_that_mostly_read_the_cache_is_not_expired() {
        let s = timed(
            "s",
            &[
                (Some(0), "claude-test", 0, 100_000, 0, false),
                (Some(3_600), "claude-test", 90_000, 90_000, 0, false),
            ],
        );
        assert!(cache_expiry_use(&[s], &expiry_prices()).sessions.is_empty());
    }

    #[test]
    fn other_models_and_requests_without_a_timestamp_are_skipped_and_counted() {
        let s = timed(
            "s",
            &[
                (Some(0), "claude-test", 0, 100_000, 0, false),
                (Some(1_000), "gpt-test", 0, 100_000, 0, false),
                (None, "claude-test", 0, 100_000, 0, false),
                (Some(3_000), "claude-test", 0, 100_000, 0, false),
            ],
        );
        let out = cache_expiry_use(&[s], &expiry_prices());
        assert_eq!(out.skipped_models, 1);
        // The request without a timestamp, and the one after it (its predecessor has none).
        assert_eq!(out.skipped_untimed, 2);
        assert!(out.sessions.is_empty());
    }

    #[test]
    fn subagent_requests_are_ignored_and_do_not_reset_the_pause() {
        let s = timed(
            "s",
            &[
                (Some(0), "claude-test", 0, 100_000, 0, false),
                (Some(400), "claude-test", 0, 50_000, 0, true),
                (Some(500), "claude-test", 1_000, 100_000, 0, false),
            ],
        );
        let row = cache_expiry_use(&[s], &expiry_prices()).sessions.remove(0);
        // The main agent paused 500 s; the subagent request at 400 s neither counts nor shortens it.
        assert_eq!(
            (row.requests, row.tokens, row.min_gap_secs),
            (1, 100_000, 500)
        );
    }

    #[test]
    fn the_finding_prices_the_difference_between_write_and_read_of_each_model() {
        let s = timed(
            "sess-1",
            &[
                (Some(0), "claude-test", 0, 200_000, 0, false),
                (Some(420), "claude-test", 0, 200_000, 0, false),
                (Some(420 + 5_700), "claude-test", 0, 100_000, 0, false),
                (Some(420 + 5_700 + 10), "other-model", 0, 100_000, 0, false),
            ],
        );
        let a = expiry_advice(&[s]);
        let f = find(&a, FindingKind::CacheExpiry);
        assert_eq!(f.count, 1);
        assert_eq!(f.impact.saving, Tokens::measured(300_000));
        assert_eq!(f.impact.confidence, Confidence::Inferred);
        // (200,000 + 100,000) × ($3.75 - $0.30) / M = $1.035
        assert!(close(f.impact.saving_usd, 1.035));
        let item = &f.items[0];
        assert_eq!(item.name, "sess-1");
        assert_eq!(item.session_id.as_deref(), Some("sess-1"));
        assert_eq!(item.requests, 2);
        assert_eq!(
            item.detail,
            "2 requests after pauses of 7\u{2013}95 min, 300.0k tokens written again, TTL 5 min"
        );
        assert!(
            f.notes
                .iter()
                .any(|n| n.contains("1 request of models that are not Anthropic"))
        );
        assert!(
            f.notes
                .iter()
                .any(|n| n.contains("assumed to be 5 minutes elsewhere"))
        );
        assert!(!f.notes.iter().any(|n| n.contains("lower bound")));
    }

    #[test]
    fn a_one_hour_session_says_that_its_saving_is_a_lower_bound() {
        let s = timed(
            "s",
            &[
                (Some(0), "claude-test", 0, 100_000, 100_000, false),
                (Some(4_200), "claude-test", 0, 100_000, 100_000, false),
            ],
        );
        let a = expiry_advice(&[s]);
        let f = find(&a, FindingKind::CacheExpiry);
        assert!(f.notes.iter().any(|n| n.contains("lower bound")));
        assert!(f.items[0].detail.ends_with("TTL 60 min"));
    }

    #[test]
    fn no_expired_request_no_cache_expiry_finding() {
        assert!(find_none(
            &expiry_advice(&[after_gap(299)]),
            FindingKind::CacheExpiry
        ));
    }

    // SkillQuality.

    /// A quality skill: loaded four times, two by the model, two by the user, with the given tweak.
    fn quality_skill(name: &str, tweak: impl FnOnce(&mut SkillStats)) -> SkillStats {
        let mut s = skill(name, true, 4, 0, (100, 1000));
        s.triggers = TriggerCounts {
            model: 2,
            user: 2,
            nested: 0,
        };
        tweak(&mut s);
        s
    }

    /// The names listed for SkillQuality, or none when the finding is missing.
    fn quality_names(skills: Vec<SkillStats>) -> Vec<String> {
        analyze(&report(skills))
            .findings
            .iter()
            .find(|f| f.kind == FindingKind::SkillQuality)
            .map(|f| f.items.iter().map(|i| i.name.clone()).collect())
            .unwrap_or_default()
    }

    fn listed(tweak: impl FnOnce(&mut SkillStats)) -> bool {
        !quality_names(vec![quality_skill("s", tweak)]).is_empty()
    }

    #[test]
    fn a_repeat_load_passes_at_one_and_none_is_no_finding() {
        assert!(listed(|s| s.quality.repeat_loads = 1));
        assert!(!listed(|s| s.quality.repeat_loads = 0));
    }

    #[test]
    fn misfires_need_two_and_a_fifth_of_the_model_loads() {
        // 2 of 10 model loads: at the minimum of both.
        assert!(listed(|s| {
            s.triggers.model = 10;
            s.quality.misfires = 2;
        }));
        // 2 of 11: just below 20 %.
        assert!(!listed(|s| {
            s.triggers.model = 11;
            s.quality.misfires = 2;
        }));
        // 1 of 2 is 50 % but a single misfire is below the count.
        assert!(!listed(|s| {
            s.triggers.model = 2;
            s.quality.misfires = 1;
        }));
    }

    #[test]
    fn a_skill_only_the_user_starts_needs_three_starts_no_model_load_and_an_installed_skill() {
        let only = |user: u32, model: u32, installed: bool| {
            listed(|s| {
                s.triggers.user = user;
                s.triggers.model = model;
                s.in_inventory = installed;
            })
        };
        assert!(only(3, 0, true));
        assert!(!only(2, 0, true));
        assert!(!only(3, 1, true));
        assert!(!only(3, 0, false));
    }

    #[test]
    fn errors_after_a_skill_need_five_at_twice_the_rate_of_the_rest() {
        let errors = |fe: u64, fc: u64, be: u64, bc: u64| {
            listed(|s| {
                s.quality.follow_on_errors = fe;
                s.quality.follow_on_calls = fc;
                s.quality.baseline_errors = be;
                s.quality.baseline_calls = bc;
            })
        };
        // 20 % against 10 %: exactly twice.
        assert!(errors(5, 25, 10, 100));
        // 20 % against 10.1 %: just below.
        assert!(!errors(5, 25, 101, 1000));
        // Twice the rate but only 4 errors.
        assert!(!errors(4, 10, 1, 100));
        // A baseline without errors: five errors pass, four do not.
        assert!(errors(5, 100, 0, 200));
        assert!(!errors(4, 100, 0, 200));
        // No baseline calls at all counts as no errors.
        assert!(errors(5, 100, 0, 0));
    }

    #[test]
    fn a_retry_loop_after_a_skill_passes_at_one() {
        assert!(listed(|s| s.quality.follow_on_retry_loops = 1));
        assert!(!listed(|s| s.quality.follow_on_retry_loops = 0));
    }

    #[test]
    fn compactions_soon_after_need_two_and_a_quarter_of_the_loads() {
        // 2 of 8 loads is exactly 25 %.
        assert!(listed(|s| {
            s.invocations = 8;
            s.quality.compactions_soon = 2;
        }));
        // 2 of 9 is below.
        assert!(!listed(|s| {
            s.invocations = 9;
            s.quality.compactions_soon = 2;
        }));
        // 1 of 1 is 100 % but below the count.
        assert!(!listed(|s| {
            s.invocations = 1;
            s.quality.compactions_soon = 1;
        }));
    }

    #[test]
    fn no_skill_passing_no_finding() {
        assert!(quality_names(vec![quality_skill("calm", |_| {})]).is_empty());
    }

    #[test]
    fn the_saving_is_the_wasted_loads_times_load_and_carry_per_use() {
        let mut s = quality_skill("heavy", |s| {
            s.quality.repeat_loads = 1;
            s.triggers.model = 4;
            s.quality.misfires = 2;
            s.load = Tokens::estimated(400);
            s.carry = CarryCost {
                requests: 40,
                tokens: Tokens::estimated(2000),
                cache_read_tokens: 1500,
                uncached_tokens: 500,
            };
        });
        s.triggers.user = 0;
        let r = report(vec![s]);
        let a = analyze_with(
            &r,
            &References::new(),
            &[],
            &ContextSaving::default(),
            &RetryLoopUse::default(),
            &ModelMixUse::default(),
            &ContextHygieneUse::default(),
            &DelegationUse::default(),
            &CacheExpiryUse::default(),
            &pricing(),
        );
        let f = find(&a, FindingKind::SkillQuality);
        // 3 wasted loads x (100 load + 500 carry) = 1800 tokens.
        assert_eq!(f.impact.saving.count, 1800);
        assert_eq!(f.impact.confidence, Confidence::Inferred);
        // 300 cache write at 3.75, 1125 cache read at 0.5, 375 input at 3.
        assert!(close(f.impact.saving_usd, 2812.5 / 1_000_000.0));
        assert_eq!(f.items[0].tokens.count, 1800);
        assert_eq!(
            f.items[0].detail,
            "loaded again into the same context (once); 2 of 4 model loads without any work after"
        );
    }

    #[test]
    fn a_skill_only_the_user_starts_saves_its_listing_as_cache_read() {
        let a = analyze(&report(vec![quality_skill("manual", |s| {
            s.triggers.user = 5;
            s.triggers.model = 0;
        })]));
        let f = find(&a, FindingKind::SkillQuality);
        assert_eq!(f.impact.saving.count, 1000);
        assert_eq!(f.items[0].detail, "only started by /name (5 times)");
    }

    #[test]
    fn signals_without_a_saving_rank_after_skills_with_one_and_have_no_saving() {
        let a = analyze(&report(vec![
            quality_skill("a-loops", |s| s.quality.follow_on_retry_loops = 3),
            quality_skill("z-repeat", |s| {
                s.quality.repeat_loads = 1;
                s.load = Tokens::estimated(40);
            }),
        ]));
        let f = find(&a, FindingKind::SkillQuality);
        let names: Vec<&str> = f.items.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["z-repeat", "a-loops"]);
        assert_eq!(f.items[1].tokens.count, 0);
        assert_eq!(f.items[1].detail, "3 retry loops after it");
        assert_eq!(f.count, 2);
        assert_eq!(f.impact.saving.count, 10);
    }

    #[test]
    fn a_skill_only_with_signals_without_saving_leaves_the_finding_without_impact() {
        let a = analyze(&report(vec![quality_skill("loops", |s| {
            s.quality.follow_on_retry_loops = 1;
        })]));
        let f = find(&a, FindingKind::SkillQuality);
        assert_eq!(f.impact.saving.count, 0);
        assert_eq!(f.impact.saving_usd, None);
    }

    #[test]
    fn the_error_detail_compares_both_rates() {
        let a = analyze(&report(vec![quality_skill("e", |s| {
            s.quality.follow_on_errors = 5;
            s.quality.follow_on_calls = 25;
            s.quality.baseline_errors = 10;
            s.quality.baseline_calls = 100;
        })]));
        assert_eq!(
            find(&a, FindingKind::SkillQuality).items[0].detail,
            "20 % failed calls after it vs 10 % in the rest of its sessions"
        );
    }

    #[test]
    fn ties_in_saving_go_by_signal_count_then_name() {
        let a = analyze(&report(vec![
            quality_skill("b", |s| s.quality.follow_on_retry_loops = 1),
            quality_skill("a", |s| s.quality.follow_on_retry_loops = 1),
            quality_skill("c", |s| {
                s.quality.follow_on_retry_loops = 1;
                s.quality.follow_on_errors = 5;
            }),
        ]));
        let names: Vec<&str> = find(&a, FindingKind::SkillQuality)
            .items
            .iter()
            .map(|i| i.name.as_str())
            .collect();
        assert_eq!(names, ["c", "a", "b"]);
    }
}
