# AgentLens

Analyze the local session data of coding agents — Claude Code, OpenCode and GitHub Copilot — and answer questions such as:

- Which installed skills are actually used, and how many tokens do they cost?
- How much context do tool definitions, MCP servers and instruction files add to every request?
- What do sessions cost per project, branch or issue, and how do the clients compare?

AgentLens only reads files that the clients already write. Nothing is sent anywhere.

## Status

Early development. The plan lives in the GitHub issues and the [AgentLens project](https://github.com/orgs/GravionLabs/projects/13); milestone M0 delivers `agentlens scan` and `agentlens skills` for Claude Code.

## Installation

Download the archive for your platform from the [latest release](https://github.com/GravionLabs/agentlens/releases/latest), unpack it and put `agentlens` on your `PATH`:

| Platform | Archive |
|---|---|
| Linux x86_64 / aarch64 | `agentlens-x86_64-unknown-linux-gnu.tar.gz` / `agentlens-aarch64-unknown-linux-gnu.tar.gz` |
| macOS Apple silicon | `agentlens-aarch64-apple-darwin.tar.gz` |
| Windows x86_64 | `agentlens-x86_64-pc-windows-msvc.zip` |

Verify the download against `SHA256SUMS` from the same release (`sha256sum -c --ignore-missing SHA256SUMS`). The repository is private for now, so use `gh release download --repo GravionLabs/agentlens`.

To build from source, install the Rust toolchain and run `cargo install --path crates/agentlens-cli`.

## Usage

```sh
cargo run -p agentlens-cli -- scan                       # which sources exist, how many sessions
cargo run -p agentlens-cli -- skills --since 30d         # skill usage and token cost
cargo run -p agentlens-cli -- skills --format json       # also: csv
cargo run -p agentlens-cli -- report --html report.html  # self-contained page, works offline
cargo run -p agentlens-cli -- context --kind tools       # static context: tools, MCP, agents, skills, instruction files
cargo run -p agentlens-cli -- cost --since 30d             # cost per model, measured vs. computed
cargo run -p agentlens-cli -- efficiency                   # cache hit rate, subagent share
cargo run -p agentlens-cli -- flow --top 10               # context growth and compactions per session
cargo run -p agentlens-cli -- unused --days 30           # installed skills, agents and MCP servers without use, with their idle cost
```

Global flags: `--since <12h|30d|4w|2026-10-01>`, `--source <id>` (repeatable), `--project <dir>`, `--format table|json|csv`.

Token values with a leading `~` are estimated (see [ADR 0001](docs/adr/0001-token-estimation.md)); values without it were reported by the client. In the skills table:

| Column | Meaning |
|---|---|
| load | tokens the skill content added to the context when it was loaded |
| carry | the loaded content sent again with every later request until the context is compacted |
| baseline | name and description of the skill, sent with every request of the clients that list it, used or not |
| overhead | baseline + load + carry |

`agentlens context` shows static context the client adds to every request, with columns `kind` (tool listing, tool definition, MCP instructions, agent listing, skill listing, instruction file), `owner` (MCP server, `built-in`, or file path), `items` (distinct items, once per session), `tokens` (estimated size of those items), `requests` (main-agent requests that followed them) and `overhead` (tokens x requests, estimated). The measured size of the first request of a session bounds everything static. Only Claude Code reports these items today; other sources are listed as not available.

`agentlens unused --days 30` lists installed skills, agents (`.claude/agents`, `.github/agents`, `.opencode/agents`) and configured MCP servers that were not used in the period (`--all` also shows used ones). `idle cost` is the estimated baseline tokens times the requests that carried the item; for MCP servers it comes from the context items the client reports, so servers of other clients show `unknown`. MCP configuration files are read for server names only.

`agentlens flow` shows how sessions unfold (`--section growth|errors|hooks`; CSV needs a section). The errors section counts failed tool calls per tool (calls without a recorded outcome are `unknown`, never errors) and lists retry loops: three or more adjacent identical calls (same tool and input summary) where the first two failed; calls without an input summary cannot be compared and never form a loop. Shell calls are compared by program name only (arguments are never stored), so a loop of `make` calls may contain different arguments. The hooks section sums hook runs, failures and runtime per hook type and counts permission prompts with the time until the answer (includes time the user was away); it needs events the client records (Copilot CLI today), other sources are listed as not available. The growth section lists, per session, the measured context size (input plus cache) of the first, last and largest main-agent request, the size right before each compaction and whether the session came within 90% of the model's limit. The limit is the documented window of the model family; where a session exceeds it (larger model variants), the largest size at which that model was compacted is used instead (marked `observed-compaction` in JSON).

`agentlens cost` shows requests, tokens and cost per model. Cost the client reported (OpenCode) is shown as is; otherwise it is computed from a bundled price table and marked `~` ([ADR 0002](docs/adr/0002-price-source.md)). The table only contains prices checked against the vendor's price list (with source and date); models without a price are listed as unpriced, never guessed. Add or override prices in `~/.config/agentlens/prices.json` or with `--prices FILE` (per million tokens, keyed by model-id prefix: `{"my-model": {"input": 3, "output": 15, "cache_read": 0.3, "cache_write": 3.75}}`). `--what-if MODEL` recomputes the same tokens with the prices of another model (id prefix) and shows actual cost, what-if cost and the difference per model; requests without an actual price are excluded and counted. The `share` column is the model mix. `--by project|branch|issue` groups the cost instead of per model; the issue number is read from the branch name when a number of 2 to 6 digits starts a `/` segment (`feat/123-title`) or follows `issue-`/`gh-`; other sessions are `(unassigned)`. Only Claude Code and Copilot CLI record the branch. GitHub Copilot premium requests are shown separately and are not money.

`agentlens efficiency` shows the cache hit rate (cache read / all input tokens, measured; `--by session|project|source`) and the share of subagents in requests, tokens and cost. Sources that report no cache tokens show `n/a` instead of 0%; cost shares only cover priced requests.

Supported sources today: Claude Code, OpenCode (SQLite database) and GitHub Copilot CLI. Copilot CLI sessions without per-request rows in `session-store.db` fall back to the session totals from `session.shutdown`, which cannot be split per request.

## Layout

| Path | Purpose |
|---|---|
| `crates/agentlens-core` | normalized event model, adapters, analyzers |
| `crates/agentlens-cli` | the `agentlens` binary |

## Development

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

See [AGENTS.md](AGENTS.md) for the architecture and the working conventions.
