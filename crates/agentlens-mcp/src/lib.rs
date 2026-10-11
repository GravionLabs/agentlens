//! The MCP server of AgentLens: the analyses of `agentlens-core` as read-only tools.
//!
//! One tool per analysis. Each takes the filter of the CLI (period, sources, project) and the options
//! of its view, and returns what the application shows: the report types of the core as structured
//! JSON with the `measured` flags and the warnings of reading the sessions. The tools only read; they
//! never write, delete or call out. Prompts and file contents are not part of any report.

use std::sync::Arc;

use agentlens_core::api::{self, Answer, Context, Filters};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock};
use rmcp::schemars::{self, JsonSchema};
use rmcp::{ServerHandler, tool, tool_handler, tool_router};
use serde::{Deserialize, Serialize};

/// The filter every analysis takes, like `--since`, `--source` and `--project` of the CLI.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct Query {
    /// A duration (`30d`, `12h`, `4w`), a date (`2026-10-01`) or an RFC 3339 instant. Empty: all sessions.
    pub since: Option<String>,
    /// Source ids: `claude-code`, `opencode`, `copilot-cli`, `vscode-copilot`. Empty: all.
    pub sources: Option<Vec<String>>,
    /// Only sessions that ran in this directory or below it.
    pub project: Option<String>,
}

impl Query {
    fn filters(&self) -> Filters {
        Filters {
            since: self.since.clone(),
            sources: self.sources.clone().unwrap_or_default(),
            project: self
                .project
                .as_ref()
                .filter(|p| !p.trim().is_empty())
                .map(Into::into),
        }
    }
}

/// The cost analysis: the filter, a grouping and an optional what-if model.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct CostQuery {
    #[serde(flatten)]
    pub query: Query,
    /// Grouping: `model` (default), `project`, `branch` or `issue`.
    pub by: Option<String>,
    /// A model id prefix: what the usage would have cost with that model.
    pub what_if: Option<String>,
}

/// The efficiency analysis: the filter and the grouping of the cache table.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct EfficiencyQuery {
    #[serde(flatten)]
    pub query: Query,
    /// Grouping of the cache table: `session` (default), `project` or `source`.
    pub by: Option<String>,
}

/// One session of the timeline.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct TimelineQuery {
    #[serde(flatten)]
    pub query: Query,
    /// The source id of the session, as listed by the `sessions` tool.
    pub source: String,
    /// The session id, as listed by the `sessions` tool.
    pub id: String,
}

/// The server: holds the sessions it has read for a few minutes, like the application.
#[derive(Clone)]
pub struct AgentLensServer {
    ctx: Arc<Context>,
}

impl AgentLensServer {
    pub fn new(ctx: Context) -> Self {
        Self { ctx: Arc::new(ctx) }
    }

    /// Reads the client files of the current user.
    pub fn system() -> Result<Self, String> {
        Ok(Self::new(Context::system()?))
    }

    /// Runs an analysis off the async threads and turns its answer into a tool result. A failure of
    /// the analysis (a bad filter, a missing price) is a tool-level error the agent can read.
    async fn run<T, F>(&self, analysis: F) -> CallToolResult
    where
        T: Serialize + Send + 'static,
        F: FnOnce(&Context) -> Result<Answer<T>, String> + Send + 'static,
    {
        let ctx = Arc::clone(&self.ctx);
        match tokio::task::spawn_blocking(move || analysis(&ctx)).await {
            Ok(Ok(answer)) => match serde_json::to_value(&answer) {
                Ok(value) => CallToolResult::structured(value),
                Err(e) => failure(format!("cannot write the result: {e}")),
            },
            Ok(Err(message)) => failure(message),
            Err(e) => failure(format!("the analysis stopped unexpectedly: {e}")),
        }
    }
}

fn failure(message: String) -> CallToolResult {
    CallToolResult::error(vec![ContentBlock::text(message)])
}

#[tool_router]
impl AgentLensServer {
    #[tool(
        description = "Which coding agents left sessions on this machine: per source whether it was found, where, and how many sessions and events were read.",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    async fn sources(&self, Parameters(q): Parameters<Query>) -> CallToolResult {
        let f = q.filters();
        self.run(move |c| api::overview(c, &f)).await
    }

    #[tool(
        description = "Skills: uses, sessions, load, carry and baseline tokens, and the overhead per skill. `load` and `baseline.tokens` are objects with a `measured` flag; carry and overhead are estimates (characters / 4).",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    async fn skills(&self, Parameters(q): Parameters<Query>) -> CallToolResult {
        let f = q.filters();
        self.run(move |c| api::skills(c, &f)).await
    }

    #[tool(
        description = "Static context the clients add to every request: tool listings and definitions, MCP instructions, instruction files, agent and skill listings, with tokens and the overhead over the requests that followed. `tokens` and `overhead_tokens` of the rows are estimates (characters / 4); `first_request` values were measured by the client.",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    async fn context(&self, Parameters(q): Parameters<Query>) -> CallToolResult {
        let f = q.filters();
        self.run(move |c| api::context(c, &f)).await
    }

    #[tool(
        description = "Installed skills, agents and MCP servers with their use count and the tokens they cost while idle (`overhead_tokens`, an estimate). Items with uses = 0 were not used in the period (default 30 days).",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    async fn unused(&self, Parameters(q): Parameters<Query>) -> CallToolResult {
        let f = q.filters();
        self.run(move |c| api::unused(c, &f)).await
    }

    #[tool(
        description = "How sessions unfold: context growth with compactions, tool errors and retry loops, hooks and permission waits.",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    async fn flow(&self, Parameters(q): Parameters<Query>) -> CallToolResult {
        let f = q.filters();
        self.run(move |c| api::flow(c, &f)).await
    }

    #[tool(
        description = "What the usage costs in USD, grouped by model, project, branch or issue, split into measured (`measured_usd`, reported by the client) and estimated (`estimated_usd`, tokens times a price table) cost, optionally compared with another model.",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    async fn cost(&self, Parameters(q): Parameters<CostQuery>) -> CallToolResult {
        let f = q.query.filters();
        let by = q.by.unwrap_or_else(|| "model".into());
        let what_if = q.what_if;
        self.run(move |c| api::cost(c, &f, &by, what_if.as_deref()))
            .await
    }

    #[tool(
        description = "Prompt cache hit rate per session, project or source, and the share of requests that subagents made.",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    async fn efficiency(&self, Parameters(q): Parameters<EfficiencyQuery>) -> CallToolResult {
        let f = q.query.filters();
        let by = q.by.unwrap_or_else(|| "session".into());
        self.run(move |c| api::efficiency(c, &f, &by)).await
    }

    #[tool(
        description = "The coding agents side by side over the same sessions: requests, tokens, cache hit rate, tool errors, skills and time. A value a client does not record is null; `skill_overhead_tokens` is an estimate.",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    async fn compare(&self, Parameters(q): Parameters<Query>) -> CallToolResult {
        let f = q.filters();
        self.run(move |c| api::compare(c, &f)).await
    }

    #[tool(
        description = "The sessions in the period with source, id, project, model, start and end, so that one can be opened with the timeline tool.",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    async fn sessions(&self, Parameters(q): Parameters<Query>) -> CallToolResult {
        let f = q.filters();
        self.run(move |c| api::sessions(c, &f)).await
    }

    #[tool(
        description = "The context size of every request of one session, with compactions and the events around them.",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    async fn timeline(&self, Parameters(q): Parameters<TimelineQuery>) -> CallToolResult {
        let f = q.query.filters();
        let (source, id) = (q.source, q.id);
        self.run(move |c| api::timeline(c, &f, &source, &id)).await
    }
}

#[tool_handler(
    name = "agentlens",
    instructions = "Read-only analyses of the token use of coding agents (Claude Code, OpenCode, Copilot CLI, VS Code Copilot) from the session files on this machine. Every number says whether the client measured it or AgentLens estimated it."
)]
impl ServerHandler for AgentLensServer {}
