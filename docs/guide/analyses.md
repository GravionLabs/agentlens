# The analyses

Each analysis is a command of the CLI, a view of the [application](application.md) and a tool of the [MCP server](mcp-server.md), and all three show the same numbers (what you can rely on in their output is on the [Compatibility](compatibility.md) page). Every command takes the [filters](getting-started.md#narrowing-down) (`--since`, `--until`, `--source`, `--project`, `--session`) and `--format table|json|csv`.

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

The `quality` object of every skill holds signals of how the work went after its loads. They count loads of the main agent only (loads inside a subagent get no signals until every subagent run has its own identity) and are signals, not proof: they show where to look, not that a skill is bad.

| Signal | Meaning |
| :-- | :-- |
| `repeat_loads` | loads of a skill that the main agent had already loaded in the same session with no compaction in between (any trigger: the body is in the context twice) |
| `misfires` | model loads whose follow-on window ends at a user turn and holds no tool call and no file use of the skill; a window that ends at another skill load (a dispatch) or at a compaction is work, and a window that ends with the session is undecidable and not counted |
| `compactions_soon` | loads whose follow-on window ends at a compaction |
| `follow_on_calls`, `follow_on_errors` | all tool calls in the follow-on windows and those that failed (an unknown outcome is no error) |
| `baseline_calls`, `baseline_errors` | the tool calls of the same sessions outside the skill's follow-on windows, to compare the error rate after the skill with the rest of its sessions |
| `follow_on_retry_loops` | retry loops (see **Retry loops** below) whose first call lies inside a follow-on window |

A skill that only the user started is one with `triggers.user` above zero and `triggers.model` zero.

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
| **Retry loops**            | per client and tool, the retry loops (three or more identical calls in a row whose first two failed), their calls and sessions, the input most often repeated, and the input tokens of the requests that issued the retries (not the first attempt); names a failing hook run inside a loop |
| **Model mix**              | per client, the small requests (fewer than 500 output tokens and fewer than 50,000 tokens of input) on the costliest model (highest list input price among the priced models that ran), next to the next cheaper model that ran in the same client: requests, small requests, the share by subagents, tokens and the estimated saving. Not shown below 20 small requests, below a saving of 5% of the period's cost, or when the client ran one priced model only |
| **Context hygiene**        | the sessions that carried their context on after the task was done: near the limit of the context window without a compaction, or compacted at least twice. Per session its main-agent requests, the peak context, the compactions, the clears (a `/clear` that Claude Code recorded, measured; for the other clients a session that began at most 60 seconds after another of the same client, project and branch ended, estimated with a tilde; sessions of an unattended queue run on other branches and are not counted), whether it came near the limit, and the average context before a compaction against the context at the start. Not shown below 3 sessions or below a saving of 5% of the period's cost (of the period's tokens when no price is known) |
| **Delegation**             | per client (Claude Code, OpenCode, Copilot CLI) with a subagent share below 5% of its requests and at least 20 main-agent requests of 200,000 tokens of input or more: its requests, large-context requests, subagent requests and subagent share, then the 10 sessions with the most large-context requests with their subagent requests, and the share of the period's requests that ran on a large context. No saving is estimated |
| **Cache expiry**           | per session, the main-agent requests of Anthropic models that came more than the session's cache lifetime after the previous main-agent request and wrote more tokens than they read: the context was written again at the write price instead of being read. The 10 sessions with the largest saving, with the number of such requests, the range of the pauses, the tokens written again and the lifetime. Requests of other models and requests without a timestamp (or right after one) are skipped and counted in a note |

Every sum is also given as a share of all tokens of the period (`share_of_total_tokens`, input, output and cache together), so that a small share is not read as the main lever (the carry of **Large skill bodies** now has a sum and a share too; the skill report's `totals` holds the period's request, subagent and context numbers): the descriptions of all installed skills are often well under 1% of the tokens, while the size of the context decides the rest. The **Context size** finding is that scale: it ends with the fixed advice to start new work in a fresh session, to clear the context between tasks and to delegate exploration and review to subagents, whose context stays separate.

### Ranking by impact

Every finding has an *impact*: what its action could have saved in the period (`saving`, in tokens), the same in USD (`saving_usd`) and a `confidence` (`measured`, `estimated` or `inferred`). The saving is what the action saves, not what the finding is about: the **Context size** finding is about all the context of the period, its saving only the part a fresh session would have avoided. The findings come sorted by `saving_usd`, largest first, and `rank` numbers them from 1; equal savings keep the order of the table above. Findings without a saving come last, in that order.

| Kind | Saving | Priced as | Confidence |
| :-- | :-- | :-- | :-- |
| **Unused skills** | the listing tokens of all unused skills (= `total`) | cache read | measured if all listings were measured, else estimated |
| **Listed but loaded by hand** | the listing tokens (= `total`) | cache read | as above |
| **Costly descriptions** | what shortening to 40 tokens would have saved (= `total`) | cache read | estimated |
| **Large skill bodies** | per skill, the body above 2,000 tokens (its load divided by its invocations) times the requests that carried it | split like the skill's own carry into cache read and uncached (priced as input) | estimated |
| **Costly MCP servers** | per server, its static context in the (client, project) groups in which none of its tools was called; a group with a call saves nothing | cache read | measured if the sizes were, else estimated |
| **Context size** | per session, on every main-agent request of at least 200,000 tokens, the input above the size of the session's first main-agent request (never below 0); the sessions of **Context hygiene** are left out when that finding is shown | each request in its own input : cache read : cache write ratio, with its own model (a model without a price at the period's average) | inferred |
| **Retry loops** | per client and tool, the input of the requests strictly between the first and the last call of each loop (the first attempt is not counted; a subagent request inside the span is) | each request in its own input : cache read : cache write split, with its own model (a model without a price at the period's average) | inferred (it assumes that fixing the cause avoids the retries) |
| **Model mix** | per client, for each small request on the costliest model, its cost on that model minus its cost on the next cheaper model that ran in the client (both from the price table, on the request's own tokens) | none: it saves cost, not tokens (`saving` 0); requests of a model without a price are not in it and a note counts them | inferred (it assumes the cheaper model could have answered) |
| **Context hygiene** | per listed session, on every main-agent request, the input above the size of the session's first main-agent request (the quantity of **Context size** without its 200,000-token threshold); **Context size** then leaves out the requests of these sessions, so nothing is counted twice | each request in its own input : cache read : cache write ratio, with its own model (a model without a price at the period's average) | inferred (it assumes the work could have started fresh at that size) |
| **Delegation** | none (`Impact::none()`: `saving` 0, `saving_usd` null) | – | – |
| **Cache expiry** | per expired request, its cache-write tokens | its own model: cache-write tokens times (write price minus read price); a model without a price is left out of the USD and a note counts it | inferred (the action shrinks the rewrite, it does not prove that it would not have happened) |
| **Used but not installed**, **Work on a large context** | none (`saving` 0, `saving_usd` null) | – | – |

`saving_usd` is always an estimate, computed from the [price table](#cost) and never reported by a client, so the CLI and the application show it with a tilde. Where a finding knows its requests (**Context size**, **Retry loops**, **Context hygiene**), each request is priced with its own model; a request whose model has no price is priced with the period's average price per token class, split in its own ratio, and a note says how many were. The others are priced with the period's average price per token class (input, cache read, cache write), each class weighted with its tokens over all priced requests. Requests without a price are left out of those averages and a note says how many. So `saving_usd` is null only when the period has no price at all, and then for every finding. Without any price `saving_usd` is null and the order falls back to the tokens weighted by class: input 1, cache write 1.25, cache read 0.1.

An `inferred` saving rests on an assumption: for **Context size**, that the work on the large requests could have run in a fresh session that started at the size of the session's first request. The constants are `DESCRIPTION_TARGET_TOKENS` (40), `BODY_TARGET_TOKENS` (2,000), `LARGE_CONTEXT_TOKENS` (200,000), `MANY_COMPACTIONS` (2), `MIN_HYGIENE_SESSIONS` (3), `MIN_HYGIENE_SHARE` (5%), `CLEAR_PROXY_SECONDS` (60), `DELEGATING_SOURCES` (Claude Code, OpenCode, Copilot CLI), `DELEGATION_MAX_SHARE` (5%), `MIN_DELEGATION_REQUESTS` (20), `CACHE_TTL_5M` (5 minutes) and `CACHE_TTL_1H` (60 minutes) and the weights `INPUT_WEIGHT`, `CACHE_WRITE_WEIGHT` and `CACHE_READ_WEIGHT` (1, 1.25 and 0.1). The CLI table puts the rank before each finding and its saving below the title (`Could save ~$1.23 (~45,678 tokens, estimated).`); `--format csv` repeats `rank`, `saving_tokens`, `saving_usd` and `confidence` on every row.

**Cache expiry** rests on the cache lifetime (TTL) of the session: `CACHE_TTL_1H` (60 minutes) if any of its requests wrote tokens with the 1-hour lifetime (`Usage.cache_write_1h`, which Claude Code reports as `cache_creation.ephemeral_1h_input_tokens`), else `CACHE_TTL_5M` (5 minutes). The lifetime is read from the client's own report where it gives one (Claude Code) and assumed to be 5 minutes elsewhere. A request is expired when it is not the first main-agent request of its session, its model starts with `claude`, it and the request before it have a timestamp, the pause is longer than the lifetime, and `cache_write` is larger than `cache_read`; subagent requests are ignored and a compaction in between does not matter. The price table has one cache-write price (the 5-minute one) and writes with the 1-hour lifetime cost more, so the saving of a 1-hour session is a lower bound. **Context size** counts the volume of a large context, **Cache expiry** the price difference of writing it again: both can name the same session, and their savings are not additive.

A finding says what it cannot know: a skill that is needed only now and then may be worth keeping although it shows up as unused, and the work after a skill is activity, not cost the skill caused. The CLI prints the findings as a table (`--format json` and `--format csv` have all of them), the application has an **Advice** view, and the [MCP server](mcp-server.md) has an `advice` tool, so an agent gets the findings and only has to explain and act on them.

## Tips

```sh
agentlens tips --limit 5
```

The few [advice](#advice) findings that pay most, said in one line each. A tip is a view over a finding, computed from the same sessions, never written by a model: the same sessions give the same tips.

- **Threshold.** A finding with a saving becomes a tip only if its saving is at least `MIN_TIP_SHARE` (0.5 %) of the period's cost. Without any price the share is taken of the tokens of the period weighted by class (input 1, cache write 1.25, cache read 0.1). The findings below are counted in `below_threshold`; findings without a saving are no tips. `--limit` (default 5) caps the list; the tips keep the order of the findings.
- **Targets.** A tip lists the first `TIP_TARGETS` (3) items of its finding, and `more` counts the rest. The headline is a fixed sentence per kind with the numbers; estimates carry a tilde, and the USD saving is always an estimate.
- **Steps.** Where the client documents the syntax, a target carries a ready step (`client`, `file`, `change`); every other finding and client has the general `action` only. Today only Claude Code has steps: for an unused skill or one that is listed but loaded by hand, add `disable-model-invocation: true` to the front matter of its `SKILL.md` ([documentation](https://code.claude.com/docs/en/skills); the skill stays usable as `/name` and leaves the listing; skills supplied by plugins get no step); for a costly MCP server, move it from the user scope (`~/.claude.json`) to the `.mcp.json` of the projects that call it ([documentation](https://code.claude.com/docs/en/mcp)).

`--format json` prints the report (`period_cost_usd`, `threshold`, `tips`, `below_threshold`), `--format csv` one row per target (`rank`, `kind`, `target`, `saving_tokens`, `saving_usd`, `confidence`, `step`). The [MCP server](mcp-server.md) has a `tips` tool and a `tips` prompt.

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

`--days` is ignored when `--since` is given; with `--until` the period is counted back from that point instead of from now (`agentlens unused --days 30 --until 2026-10-01`).

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
agentlens cost --without skill:grill-me --without mcp-server:github
```

Requests, tokens and cost per model. Cost the client reported (OpenCode) is shown as is; otherwise it is computed from the bundled price table and marked `~`. Add or override prices in `~/.config/agentlens/prices.json` or with `--prices FILE` (per million tokens, keyed by model-id prefix):

```json
{ "my-model": { "input": 3, "output": 15, "cache_read": 0.3, "cache_write": 3.75 } }
```

`--by project|branch|issue|session` groups the cost instead of per model. `--by session` has one row per session: source, project, start, duration, issue, models, requests (and the main-thread ones), peak context (the largest `input + cache read + cache write` of a main-agent request, as in `flow`), the token counts, the cost and the *subagent share* of that cost. A session without a priced request shows `n/a`, never 0. `--top <n>` (default 25, `0` = all) limits the text table; JSON and CSV carry every row (`report.sessions`). The issue number of a session comes from two sources, tried in this order, and the first match wins: the branch name, when a number of 2 to 6 digits starts a `/` segment (`feat/123-title`) or follows `issue-` or `gh-`; then the messages of the `git commit -m` commands the session ran (Claude Code, OpenCode and Copilot CLI), where the first `(#123)`, `Closes #123` or `Refs #123` (or another GitHub closing keyword) counts. A message from a file (`-F`) is not read, and only the number is kept, never the message. Sessions that match neither are `(unassigned)`. Only Claude Code and Copilot CLI record the branch. The JSON says which source matched: `issue_source` (`branch` or `commit`) on each row of `--by issue` (for a row with sessions of both sources, `commit`) and on each session of `--by session`. `--what-if MODEL` recomputes the same tokens with the prices of another model and shows actual cost, what-if cost and the difference per model; requests without an actual price are excluded and counted. `--without KIND:NAME` (repeatable; `KIND` is `skill`, `agent` or `mcp-server`) estimates the tokens and cost that removing an item would have saved in the period: what its listing adds to the requests, and for a skill also its load and what it carried afterwards, each counted once. Each token is priced at the effective input price per token of the request it sits in (the input cost of the request divided by its input plus cache tokens), so the cache mix of that request is assumed to stay; a request without a price is counted apart (`unpriced_requests`) and never priced as zero. Everything is an estimate: the text output has a tilde, the JSON (`without`, next to `report` and `what_if`) carries estimated `Tokens`. A name matches every installation of it, across projects and clients, and the numbers are summed (`installations` and the requests behind them are in the row); MCP server names compare after the same normalization as `unused` (`claude.ai Docs` equals `claude_ai_Docs`), and a name that matches nothing is a warning that names it, not an error. `--without` can be combined with `--what-if`; both reports are in the JSON, and `--format csv` writes the table of `--without` when it is set (otherwise the table of `--what-if`, otherwise the default table). The limits are in the report's `notes`: it assumes the same requests and the same cache mix, counts the listing only, and does not remove the results of MCP tool calls or the work of subagent runs that the item caused. The MCP tool `cost` takes the same list as `without`.

GitHub Copilot premium requests are shown separately; they are a plan allowance, not money, so treat Copilot cost as a comparison figure, not an invoice.

## Efficiency

```sh
agentlens efficiency --by project
```

The cache hit rate (cache read divided by all input tokens, measured) per session, project or source, and the share of subagents in requests, tokens and cost. Sources that report no cache tokens show `n/a` instead of 0%; cost shares only cover priced requests.

The *roles* table has a row per role over the period: runs, requests per run, tokens, cost per run (with a `~` when estimated), the models, the *first request context* (`input + cache read + cache write` of a run's first request, the static part every run pays), the uncached tokens after it (`input + cache write` of the later requests, what the run read) and the hit rate, sorted by total cost. A role is the name of a forked skill, else the workflow phase, else the agent type; the main agent is the row `main` and a run without any of the three is `(unknown)`. The context columns are means per run over the runs with requests; a run without requests (a workflow container, say) counts in `runs` and adds no cost. `--section all|cache|roles` (default `all`) filters the text, and `--format csv` needs `--section cache|roles` because it holds one table.

## Compare

```sh
agentlens compare --since 30d
```

The clients side by side over the same sessions: sessions, requests, tokens, cache hit rate, tool calls and error rate, skill invocations and their estimated overhead, and session time (first to last event, so idle time is included). A value a client does not record is `n/a`.

## Trend

The current period next to a comparison window, with the change of every figure. `agentlens trend [--against SINCE..UNTIL]` prints it (the MCP tool `trend` returns the same data).

```sh
agentlens trend --since 7d                              # the last 7 days against the 7 before
agentlens trend --since 2026-10-01 --against 2026-09-01..2026-09-15
agentlens trend --format csv                            # one row per figure, two per skill
```

The table has the columns group, figure, before, after, change and change %; the skill rows come last. An estimate carries a tilde (`~`), a missing change is `n/a`, and a note line says when the values are per day. `SINCE` and `UNTIL` take the same forms as `--since`; a malformed `--against` or one that overlaps the period is a usage error. The global filters (`--since`, `--until`, `--source`, `--project`, `--session`) apply to both windows.

- **Windows.** The current window is `[since, until)` of the filters; without `since` it is the last 30 days, without `until` it ends now. The comparison window is, by default, the window of the same length right before (`[since - length, since)`); it can be given explicitly. Overlapping windows are an error.
- **Boundary rule.** A session belongs to the window in which it ends (its start if it has no end), the same rule as the period filter: a session that ends exactly at `since` is in the current window, one that ends exactly at `until` is in the next one.
- **Normalization.** When the two windows differ in length, `normalized` is `true` and every count and amount is compared per day (figure divided by the length of the window in days). Rates (cache hit rate, subagent share) are compared as rates and never normalized. With equal lengths the values are absolute.
- **Delta.** Each figure has `before` (comparison window), `after` (current window), `change` (`after - before`), `change_pct` (in percent of `before`) and `measured`. `change` is `n/a` when either window has no sessions, never a 100 % drop; `change_pct` is `n/a` when `before` is 0; `measured` is true only if both values were measured, so a change of an estimate is an estimate.
- **Figures.** Volume (sessions, requests, subagent requests, tokens, context tokens, large-context requests), cost (measured and estimated USD kept apart, unpriced requests), efficiency (cache hit rate, subagent request share), errors (tool calls, failed calls, retry loops) and, per skill, loads and overhead tokens. A skill loaded in one window only has the status `new` or `gone`; the skills are sorted by the size of the change of their overhead tokens, then by name.

## Timeline

The application (and the `timeline` tool of the MCP server) shows the context size of every request of one session, with its compactions, tool errors, failed hooks, permission prompts and model switches. Every point carries the model of its request (the request's model, else the session's), and a model switch is a marker where two consecutive main-agent requests ran on different models. There is no CLI command for it.
