export type GlossaryGroup = "Basics" | "Context" | "Skills" | "Cost" | "Efficiency" | "Flow";

export interface GlossaryEntry {
  id: string;
  term: string;
  group: GlossaryGroup;
  /** One sentence, shown as a tooltip. */
  short: string;
  /** Longer explanation for the glossary page. */
  long: string;
}

export const GLOSSARY_GROUPS: GlossaryGroup[] = [
  "Basics",
  "Context",
  "Skills",
  "Cost",
  "Efficiency",
  "Flow",
];

export const GLOSSARY: GlossaryEntry[] = [
  {
    id: "measured",
    term: "Measured and estimated",
    group: "Basics",
    short: "Measured values come from the client; values with a ~ were computed.",
    long: "A measured value was read from the client's own data, for example the token counts of a response. An estimated value was computed from file sizes, heuristics or the price table and is marked with a tilde (~) and the warning color. “n/a” means the client does not record the value at all.",
  },
  {
    id: "session",
    term: "Session",
    group: "Basics",
    short: "One conversation of a coding agent, from its first to its last event.",
    long: "A session is one conversation with a client such as Claude Code, OpenCode, Copilot CLI or VS Code Copilot. Session time runs from the first to the last event, so idle time is included.",
  },
  {
    id: "request",
    term: "Request",
    group: "Basics",
    short: "One call of the client to the model.",
    long: "Every time the client sends the conversation to the model, that is one request. Everything in the static context is sent again with each one, which is why small items add up.",
  },
  {
    id: "token",
    term: "Tokens",
    group: "Basics",
    short: "The unit models read and write; roughly three quarters of a word.",
    long: "Tokens are the unit in which models read input and write output, and in which usage is billed. Where the client reports no count, AgentLens estimates tokens from the size of the content.",
  },
  {
    id: "source",
    term: "Source",
    group: "Basics",
    short: "The coding agent whose data was read.",
    long: "A source is one supported client: Claude Code, OpenCode, Copilot CLI or VS Code Copilot. Use the source filter at the top to include or exclude them.",
  },
  {
    id: "subagent",
    term: "Subagent",
    group: "Basics",
    short: "An agent that the main agent starts for a sub-task.",
    long: "Subagents work in their own context. Their requests are counted separately from the main agent's, which is why context size only covers main-agent requests.",
  },
  {
    id: "subagent-share",
    term: "Subagent share",
    group: "Cost",
    short: "The part of a session's cost that its subagents caused.",
    long: "Cost of the requests made by subagents divided by the cost of all requests of the session (`cost --by session`). It is n/a when the session has no priced request, because an unknown cost is not zero.",
  },
  {
    id: "role",
    term: "Role",
    group: "Efficiency",
    short: "What a run is for: its forked skill, else its workflow phase, else its agent type.",
    long: "`efficiency` groups the runs by role: the name of a forked skill if there is one, otherwise the workflow phase, otherwise the agent type. The main agent is a row of its own (`main`), and a run that names none of the three is `(unknown)`. The free-text description of a run is never used as a role.",
  },
  {
    id: "first-request-context",
    term: "First request context",
    group: "Efficiency",
    short: "Input, cache read and cache write tokens of a run's first request.",
    long: "The static part every run pays: what the first request of a run sends to the model, cached or not. The roles table shows the mean per run. Next to it, *uncached after first* is the uncached input plus cache write of the later requests, which is what the run read. Both are measured when the requests are.",
  },
  {
    id: "context",
    term: "Static context",
    group: "Context",
    short: "Content the client adds to every request, before you type anything.",
    long: "Tool definitions, instruction files (such as CLAUDE.md or AGENTS.md), agent and skill listings and MCP server instructions. The client sends them with every request, so they cost tokens whether they are used or not.",
  },
  {
    id: "context-size",
    term: "Context size",
    group: "Context",
    short: "Input plus cache tokens of a request: how full the window is.",
    long: "The number of tokens the model received for one request (uncached input plus cache reads and writes). It is measured and only counted for main-agent requests. It grows over a session until the client compacts it.",
  },
  {
    id: "overhead",
    term: "Overhead",
    group: "Context",
    short: "Tokens × the requests that carried them: what an item costs in total.",
    long: "A 500-token instruction file that was sent with 200 requests caused 100,000 tokens of overhead. Overhead shows which static content is expensive because it is large, because it is sent often, or both. It is an estimate. For skills it is baseline + load + carry.",
  },
  {
    id: "owner",
    term: "Owner",
    group: "Context",
    short: "Where the context item comes from, e.g. a file, plugin or MCP server.",
    long: "The file, plugin, MCP server or built-in that provides the item. Group by project to see which project adds how much.",
  },
  {
    id: "kind",
    term: "Kind",
    group: "Context",
    short: "The type of context item.",
    long: "Tool listing and tool definition: the tools the model may call. MCP instructions: text an MCP server adds. Instruction file: CLAUDE.md, AGENTS.md and similar. Agent listing and skill listing: names and descriptions of installed agents and skills.",
  },
  {
    id: "first-request",
    term: "First request",
    group: "Context",
    short: "The first request of a session holds all static context at once.",
    long: "Its size is measured, so it is an upper bound of all static context a session starts with.",
  },
  {
    id: "baseline",
    term: "Baseline",
    group: "Skills",
    short: "Name and description of a skill, sent with every request.",
    long: "Clients list installed skills by name and description in each request so the model knows they exist. That is the baseline: the cost of having the skill installed, even if it is never used.",
  },
  {
    id: "load",
    term: "Load",
    group: "Skills",
    short: "Tokens the skill content added to the context when it was used.",
    long: "When a skill is invoked, its full instructions enter the context once. Load is the size of that content.",
  },
  {
    id: "file-use",
    term: "File use",
    group: "Skills",
    short: "A tool call that read files or ran a script in a skill's folder without loading it.",
    long: "A skill is not only loaded by the client's skill tool or by typing its name: a model also reads its files and runs the scripts in its folder. AgentLens counts those tool calls per installed skill (only the skill name is kept, never the command or the path), and a skill with file uses is not listed as unused. Looking after a skill does not count: listing, printing, copying or committing its files, and editing them, and neither do the file accesses of a session that edits the skill. It is a signal, not proof. Loads are shown in the uses column; file uses are listed next to them.",
  },
  {
    id: "carry",
    term: "Carry",
    group: "Skills",
    short: "Loaded content that is sent again with each later request.",
    long: "Once loaded, the skill text stays in the conversation and is re-sent with every following request until a compaction removes it. Carry is that repeated cost and always an estimate.",
  },
  {
    id: "attributed",
    term: "Attributed",
    group: "Skills",
    short: "Tokens of requests the client itself attributes to a skill.",
    long: "Only available from clients that report it (OpenTelemetry). Measured.",
  },
  {
    id: "follow-on",
    term: "Follow-on",
    group: "Skills",
    short: "Tokens used after the skill until the next user turn, skill or compaction.",
    long: "An assumption: everything that happens until something else takes over is attributed to the skill. Use it as a rough guide, not as a measurement.",
  },
  {
    id: "idle-cost",
    term: "Idle cost",
    group: "Context",
    short: "Baseline tokens × requests of an item that was never used.",
    long: "What an installed skill, agent or MCP server costs while it does nothing. “Unknown” means the client reports no data for MCP servers.",
  },
  {
    id: "cost",
    term: "Cost",
    group: "Cost",
    short: "Money as reported by the client, or computed from the price table (~).",
    long: "Reported cost is shown as is. Otherwise the tokens are priced with the price table. Cache writes are priced as 5-minute writes. Add prices for unknown models to ~/.config/agentlens/prices.json.",
  },
  {
    id: "premium",
    term: "Premium requests",
    group: "Cost",
    short: "A plan allowance of Copilot, not money.",
    long: "Copilot counts requests against a monthly allowance instead of billing tokens. They are shown next to the cost, never added to it.",
  },
  {
    id: "unpriced",
    term: "Unpriced",
    group: "Cost",
    short: "Requests without a reported or table price.",
    long: "They are left out of totals and shares instead of being counted as free.",
  },
  {
    id: "what-if",
    term: "What if",
    group: "Cost",
    short: "The same tokens priced with another model.",
    long: "Shows what the period would have cost on a different model. “Excluded” requests have no actual price and are left out of both sides.",
  },
  {
    id: "impact",
    term: "Impact",
    group: "Cost",
    short: "What an advice finding could have saved: tokens, ~USD and a confidence.",
    long: "Every advice finding has an impact: its saving in tokens, the saving in USD (always an estimate, computed from the price table) and the confidence of the saving. The findings are ranked by it: by the USD saving, without any price by the tokens weighted by class (input 1, cache write 1.25, cache read 0.1); findings without a saving come last.",
  },
  {
    id: "saving",
    term: "Saving",
    group: "Cost",
    short: "What a finding's action could have saved in the period, not what the finding is about.",
    long: "For unused skills and costly descriptions the listing tokens that removing or shortening would have saved; for large skill bodies the body above 2,000 tokens times the requests that carried it; for MCP servers their context where none of their tools was called; for the context size the input above each session's first request on requests of at least 200,000 tokens. Findings that know their requests price each with its own model, the others with the period's average price per token class.",
  },
  {
    id: "confidence",
    term: "Confidence",
    group: "Cost",
    short: "How sure a saving is: measured, estimated or inferred.",
    long: "Measured: every number behind the saving came from the client. Estimated: some were computed from file sizes or heuristics. Inferred: the saving rests on an assumption about how the work could have run instead, for example that it could have started in a fresh session.",
  },
  {
    id: "cache-read",
    term: "Cache read",
    group: "Efficiency",
    short: "Input tokens served from the provider's prompt cache, at a lower price.",
    long: "Repeated prefixes of a conversation are cached by the provider. Reading from the cache is much cheaper and faster than sending new input.",
  },
  {
    id: "cache-write",
    term: "Cache write",
    group: "Efficiency",
    short: "Input tokens stored in the cache; priced higher than normal input.",
    long: "The first time content is sent it is written to the cache. Later requests can read it.",
  },
  {
    id: "hit-rate",
    term: "Hit rate",
    group: "Efficiency",
    short: "Cache read ÷ all input tokens. Higher is cheaper.",
    long: "Measured. “n/a” means the client reports no cache tokens.",
  },
  {
    id: "peak",
    term: "Peak",
    group: "Flow",
    short: "The largest context size reached in a session.",
    long: "Measured over the main agent's requests.",
  },
  {
    id: "near-limit",
    term: "Near limit",
    group: "Flow",
    short: "At least 90% of the model's context window.",
    long: "Of the model's documented window, or, if the window does not fit the data, of the largest size at which that model was compacted.",
  },
  {
    id: "compaction",
    term: "Compaction",
    group: "Flow",
    short: "The client summarizes the conversation to free up the context window.",
    long: "After a compaction, content that was carried along (such as loaded skills) is gone from the context. A session compacted two or more times, or one that came near the limit without a compaction, carried its context on after the task was done: the Context hygiene finding lists it, with the average context right before a compaction next to the context at the start.",
  },
  {
    id: "clear",
    term: "Clear",
    group: "Flow",
    short: "The user empties the context and starts afresh (/clear in Claude Code).",
    long: "Claude Code records a /clear and starts a new transcript for it, so the clear is counted as measured at the start of the new session. The other clients record none: there a session counts as a fresh start, estimated, when it began at most 60 seconds after another session of the same client, project and branch ended. Sessions of an unattended queue run on different branches and are not counted.",
  },
  {
    id: "trend",
    term: "Trend",
    group: "Basics",
    short: "The figures of a period next to those of the comparison window, with their change.",
    long: "A trend compares the current period with a comparison window and shows both values and the change between them. A change is “n/a” when either window has no sessions, and the percentage is “n/a” when the earlier value is 0. A change of an estimated value is an estimate.",
  },
  {
    id: "comparison-window",
    term: "Comparison window",
    group: "Basics",
    short:
      "The period a trend compares against, by default the one of the same length right before.",
    long: "Without a choice, the comparison window is the period of the same length right before the current one. If the two periods differ in length, counts are compared per day. The comparison window must not overlap the current period.",
  },
  {
    id: "previous-period",
    term: "Previous period",
    group: "Basics",
    short: "The period of the same length right before the current one.",
    long: "It is the default comparison window of a trend: for the last 7 days, the 7 days before them. A session belongs to the period in which it ends.",
  },
  {
    id: "retry-loop",
    term: "Retry loop",
    group: "Flow",
    short: "Three or more identical tool calls in a row whose first two failed.",
    long: "Each retry sends the whole context again and fails for the same reason until the cause is fixed. The Retry loops finding counts the input of the requests that issued the retries, not the first attempt. Calls without a recorded outcome are “unknown” and never counted as errors.",
  },
  {
    id: "model-mix",
    term: "Model mix",
    group: "Cost",
    short:
      "Small requests that ran on the costliest model although a cheaper one was in use as well.",
    long: "A request is small with fewer than 500 output tokens and fewer than 50,000 tokens of input. Per client, the costliest model is the one with the highest list input price among those that ran, the cheaper one the next lower price. The saving is what the same tokens would have cost on the cheaper model (an estimate, inferred), and the finding appears only with at least 20 small requests and a saving of at least 5% of the period's cost.",
  },
  {
    id: "permission-wait",
    term: "Permission wait",
    group: "Flow",
    short: "Time until a permission prompt was answered.",
    long: "It includes the time the user was away from the keyboard.",
  },
];

export function glossaryEntry(id: string): GlossaryEntry | undefined {
  return GLOSSARY.find((e) => e.id === id);
}

/** Entries whose term or text contains the query; all entries for an empty query. */
export function searchGlossary(query: string): GlossaryEntry[] {
  const q = query.trim().toLowerCase();
  if (!q) return GLOSSARY;
  return GLOSSARY.filter((e) => `${e.term} ${e.short} ${e.long}`.toLowerCase().includes(q));
}
