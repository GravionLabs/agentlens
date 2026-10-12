# The analyses

Each analysis is a command of the CLI, a view of the [application](application.md) and a tool of the [MCP server](mcp-server.md), and all three show the same numbers (what you can rely on in their output is on the [Compatibility](compatibility.md) page). Every command takes the [filters](getting-started.md#narrowing-down) and `--format table|json|csv`.

## Skills

```sh
agentlens skills --since 30d
```

Which skills are used and what they cost in tokens. A skill costs in three ways:

| Column       | Meaning                                                                                                          |
| ------------ | ---------------------------------------------------------------------------------------------------------------- |
| **load**     | tokens the skill content added to the context when it was loaded                                                 |
| **carry**    | the loaded content sent again with every later request until the context is compacted                            |
| **baseline** | name and description of the skill, sent with every request of the clients that list it, used or not              |
| **overhead** | baseline + load + carry                                                                                          |

**Uses** counts how often the skill was loaded, and the `triggers` in the JSON say by whom: `model` (the client's skill tool), `user` (you typed `/name`; the client then puts the skill text into the conversation without a tool call, and that text is the load) or `nested` (another skill loaded it; only an OpenTelemetry export reports this). A skill is also in use when the model reads its files (the Read, Glob and Grep tools) or runs a script from its folder (`bash <skill folder>/scripts/…`) without loading it, so these tool calls are counted as **file uses** next to the loads. Only the skill name is kept for them, never the command or the path. Looking after a skill does not count: listing, printing, copying, searching or committing its files in a shell, and editing them. A session that edits a skill does not count its file uses either, because what it reads and runs there is part of that work. A folder that is only called `skills` in another project is ignored, because file uses are counted for installed skills only. A skill with file uses is not listed as unused.

This is a signal, not proof: a model that reads a skill's file may not follow it, and a script may be run for another reason.

Two columns are often zero, and that is normal: `user` is zero when you let the model pick the skills and never type `/name`, and `attributed` is only filled from an OpenTelemetry export. Built-in commands such as `/clear` or `/model` are no skills and do not count; a typed command that is not an installed skill is listed under *ignored commands*.

Clients that attribute usage to a skill itself (Claude Code through an [OpenTelemetry export](../telemetry.md)) add an `attributed` column with the measured tokens of those requests. The **follow-on** columns count what happened after the skill until the next user turn, skill or compaction.

## Advice

```sh
agentlens advise --since 30d
```

What to change to save tokens, made from the skill numbers above. No model is involved: the same sessions always give the same findings, and each one has a fixed explanation, a general action and the numbers it rests on, with the tilde for an estimate.

| Finding                    | What it lists                                                                                                               |
| -------------------------- | --------------------------------------------------------------------------------------------------------------------------- |
| **Unused skills**          | installed skills that were neither loaded nor used through their files, what their listing cost, and its share of all listings |
| **Listed but loaded by hand** | installed skills that were never loaded but whose files were read or run (a stack skill that another skill opens), what their listing cost per request and in total, and how often their files were used; that it was another skill who asked is inferred |
| **Costly descriptions**    | the descriptions that take the most tokens per request, and what shortening them to about 40 tokens would have saved        |
| **Used but not installed** | skills that were loaded but are not installed now (renamed, removed, built into the client or supplied by a plugin)         |
| **Large skill bodies**     | loaded bodies that stay in the context and are sent again with every later request                                          |
| **Work on a large context** | the skills after which the work ran on the largest context per request                                                     |
| **Costly MCP servers** | per MCP server, its static context (instructions, tool listing and tool definitions) per request and in total next to the number of its tool calls, ranked by tokens per call; heavily used servers are listed too, the point is the ratio. Only servers whose context the client reports (Claude Code) |
| **Context size**           | the average context of a request, the share of requests above 200,000 tokens, the share of requests that subagents made, and the sessions with the largest context (all measured where the client reports it) |

Every sum is also given as a share of all tokens of the period (`share_of_total_tokens`, input, output and cache together), so that a small share is not read as the main lever (the carry of **Large skill bodies** now has a sum and a share too; the skill report's `totals` holds the period's request, subagent and context numbers): the descriptions of all installed skills are often well under 1% of the tokens, while the size of the context decides the rest. The **Context size** finding is that scale: it ends with the fixed advice to start new work in a fresh session, to clear the context between tasks and to delegate exploration and review to subagents, whose context stays separate.

A finding says what it cannot know: a skill that is needed only now and then may be worth keeping although it shows up as unused, and the work after a skill is activity, not cost the skill caused. The CLI prints the findings as a table (`--format json` and `--format csv` have all of them), the application has an **Advice** view, and the [MCP server](mcp-server.md) has an `advice` tool, so an agent gets the findings and only has to explain and act on them.

## Context

```sh
agentlens context --kind tools      # tools, instructions, agents, skills or all
```

What the clients put into the context besides the conversation. Columns: `kind` (tool listing, tool definition, MCP instructions, agent listing, skill listing, instruction file), `owner` (an MCP server, `built-in`, or a file path), `items` (distinct items, once per session), `tokens` (the estimated size of those items), `requests` (main-agent requests that followed them) and `overhead` (tokens times requests, estimated). The measured size of the first request of a session bounds everything static. Only Claude Code reports these items today; other sources are listed as not available.

### Why the skill listing is larger than the skill baseline

`context --kind skills` shows what the client reported it sent as the skill listing (Claude Code); **Skills** and **Advice** compute a *baseline* from the skill files AgentLens finds. The listing is several times larger, and that is mostly a matter of what is in it, not of a wrong estimate. In one set of about 80 sessions the initial listing held a median of 71 skills and about 24 KB (roughly 6,000 estimated tokens), while the installed skills AgentLens had files for were only around a quarter of those bytes. The rest were skills that no file in the folders AgentLens reads describes: the client's built-in skills (the same eleven in every listing), skills supplied by plugins (long descriptions, about 850 bytes each) or by the account, and skills from places outside the scanned folders. The advice findings that rest on the baseline say this next to their numbers when the client reported a listing. For the exact size of the descriptions of the skills you do have, use `--exact`; the part of the listing without a file cannot be shrunk from AgentLens, only by disabling those skills in the client.

## Unused

```sh
agentlens unused --days 30          # --all also lists items that were used
```

Installed skills (loaded or used through their files), agents (`.claude/agents`, `.github/agents`, `.opencode/agents`) and configured MCP servers that were not used in the period, with what they cost while idle. The idle cost is the estimated baseline tokens times the requests that carried the item; for MCP servers it comes from the context items the client reports, so servers of other clients show `unknown`. MCP configuration files are read for server names only.

**Referenced by.** For an unused skill, the table lists the installed agents, commands (`.claude/commands`, `.opencode/commands`, `.github/prompts`) and other skills whose own files contain its exact name, for example the phases of a workflow agent. Such skills are ranked after the others, and **Advice** marks them the same way. The files are only searched for the name; nothing else is read or stored. A mention does not prove that the skill is used, only that something is meant to use it, and a short, common word as a skill name can be mentioned in ordinary text. Commands shipped by plugins are not searched.

## Flow

```sh
agentlens flow --section growth     # growth, errors or hooks
```

How sessions unfold.

- **Growth** lists, per session, the measured context size (input plus cache) of the first, last and largest main-agent request, the size right before each compaction and whether the session came within 90% of the model's limit. The limit is the documented window of the model family; where a session exceeds it, the largest size at which that model was compacted is used instead (marked `observed-compaction` in JSON).
- **Errors** counts failed tool calls per tool. A call without a recorded outcome is `unknown`, never an error. A **retry loop** is three or more adjacent identical calls (same tool and input summary) where the first two failed; calls without an input summary cannot be compared. Shell calls are compared by program name only, because arguments are never stored.
- **Hooks** sums hook runs, failures and runtime per hook type and counts permission prompts with the time until the answer (including time you were away). It needs events the client records (Copilot CLI today).

CSV holds one table, so it needs `--section`.

## Cost

```sh
agentlens cost --since 30d --by project
agentlens cost --what-if claude-sonnet-5-5
```

Requests, tokens and cost per model. Cost the client reported (OpenCode) is shown as is; otherwise it is computed from the bundled price table and marked `~`. Add or override prices in `~/.config/agentlens/prices.json` or with `--prices FILE` (per million tokens, keyed by model-id prefix):

```json
{ "my-model": { "input": 3, "output": 15, "cache_read": 0.3, "cache_write": 3.75 } }
```

`--by project|branch|issue` groups the cost instead of per model. The issue number is read from the branch name when a number of 2 to 6 digits starts a `/` segment (`feat/123-title`) or follows `issue-` or `gh-`; other sessions are `(unassigned)`. Only Claude Code and Copilot CLI record the branch. `--what-if MODEL` recomputes the same tokens with the prices of another model and shows actual cost, what-if cost and the difference per model; requests without an actual price are excluded and counted. GitHub Copilot premium requests are shown separately; they are a plan allowance, not money, so treat Copilot cost as a comparison figure, not an invoice.

## Efficiency

```sh
agentlens efficiency --by project
```

The cache hit rate (cache read divided by all input tokens, measured) per session, project or source, and the share of subagents in requests, tokens and cost. Sources that report no cache tokens show `n/a` instead of 0%; cost shares only cover priced requests.

## Compare

```sh
agentlens compare --since 30d
```

The clients side by side over the same sessions: sessions, requests, tokens, cache hit rate, tool calls and error rate, skill invocations and their estimated overhead, and session time (first to last event, so idle time is included). A value a client does not record is `n/a`.

## Timeline

The application (and the `timeline` tool of the MCP server) shows the context size of every request of one session, with its compactions. There is no CLI command for it.
