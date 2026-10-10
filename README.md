# AgentLens

Analyze the local session data of coding agents — Claude Code, OpenCode and GitHub Copilot — and answer questions such as:

- Which installed skills are actually used, and how many tokens do they cost?
- How much context do tool definitions, MCP servers and instruction files add to every request?
- What do sessions cost per project, branch or issue, and how do the clients compare?

AgentLens only reads files that the clients already write. Nothing is sent anywhere.

## Status

Early development. The plan lives in the GitHub issues and the [AgentLens project](https://github.com/orgs/GravionLabs/projects/13); milestone M0 delivers `agentlens scan` and `agentlens skills` for Claude Code.

## Installation

Every push to `main` publishes a release. Download the archive for your platform from the [latest release](https://github.com/GravionLabs/agentlens/releases/latest), unpack it and put `agentlens` on your `PATH`:

| Platform | Archive |
|---|---|
| Linux x86_64 / aarch64 | `agentlens-x86_64-unknown-linux-gnu.tar.gz` / `agentlens-aarch64-unknown-linux-gnu.tar.gz` |
| macOS Apple silicon | `agentlens-aarch64-apple-darwin.tar.gz` |
| Windows x86_64 | `agentlens-x86_64-pc-windows-msvc.zip` |

Verify the download against `SHA256SUMS` from the same release (`sha256sum -c --ignore-missing SHA256SUMS`). The repository is private for now, so use `gh release download --repo GravionLabs/agentlens`.

The desktop application comes as installers in the same release: a `.deb`, `.rpm` or AppImage for Linux x86_64, a `.dmg` for macOS on Apple silicon and an `.msi` or setup `.exe` for Windows x86_64. They are not signed yet, so the systems warn on first start: on macOS open the app with a right click and choose Open (or allow it in System Settings, Privacy and Security), on Windows choose More info and Run anyway in SmartScreen, and make an AppImage executable (`chmod +x`) before running it.

The application checks for a newer release only when you click **Check for updates**, and installs it after you confirm; see [docs/updater.md](docs/updater.md).

To build from source, install the Rust toolchain and run `cargo install --path crates/agentlens-cli`.

## Usage

```sh
cargo run -p agentlens-cli -- scan                       # which sources exist, how many sessions
cargo run -p agentlens-cli -- skills --since 30d         # skill usage and token cost
cargo run -p agentlens-cli -- skills --format json       # also: csv
cargo run -p agentlens-cli -- report --html report.html  # self-contained page, works offline
cargo run -p agentlens-cli -- context --kind tools       # static context: tools, MCP, agents, skills, instruction files
cargo run -p agentlens-cli -- compare --since 30d          # clients side by side
cargo run -p agentlens-cli -- cost --since 30d             # cost per model, measured vs. computed
cargo run -p agentlens-cli -- efficiency                   # cache hit rate, subagent share
cargo run -p agentlens-cli -- flow --top 10               # context growth and compactions per session
cargo run -p agentlens-cli -- unused --days 30           # installed skills, agents and MCP servers without use, with their idle cost
```

Global flags: `--since <12h|30d|4w|2026-10-01>`, `--source <id>` (repeatable), `--project <dir>`, `--format table|json|csv`, `--exact` (see below).

Token values with a leading `~` are estimated (see [ADR 0001](docs/adr/0001-token-estimation.md)); values without it were reported by the client. In the skills table:

| Column | Meaning |
|---|---|
| load | tokens the skill content added to the context when it was loaded |
| carry | the loaded content sent again with every later request until the context is compacted |
| baseline | name and description of the skill, sent with every request of the clients that list it, used or not |
| overhead | baseline + load + carry |

### Exact token counts (opt-in)

By default the size of skill texts is estimated as characters / 4 and nothing leaves your machine. With `--exact` the name and description of every installed skill (the baseline listing) and the body of each skill are counted by the Anthropic `count_tokens` API instead:

```sh
export ANTHROPIC_API_KEY=...
agentlens --exact skills
agentlens --exact --exact-model claude-opus-5-5 unused
```

- `--exact` works for `skills`, `context`, `unused`, `compare` and `report`. Without `ANTHROPIC_API_KEY` the command stops before reading anything. `--exact-model` picks the tokenizer (default `claude-sonnet-5-5`); Claude models do not all tokenize alike.
- **What is sent:** the text of each installed skill (name plus description, and the body), one request per distinct text, to `https://api.anthropic.com/v1/messages/count_tokens` (or to `ANTHROPIC_BASE_URL` if set). Session contents are never sent. Skill files can contain private instructions; do not use `--exact` if they must not leave the machine. The counting endpoint is free of charge but rate limited.
- **Cache:** counts are stored by model and content hash in `$XDG_CACHE_HOME/agentlens/count_tokens/<model>.json` (`~/.cache/...`), so repeated runs send nothing for unchanged skills. Delete the file to start over.
- **Marking:** counted values are shown without `~`. Values that are still computed stay estimates: carry, and the sizes of instruction files and tool definitions, which are known only by their file size.
- **Failure:** if the key is rejected, the network is down or the rate limit is hit, the run continues with the estimate, prints one warning on stderr and sends no further requests.
- The count includes the few tokens the API adds around a message, so it is slightly higher than the bare text.

`agentlens context` shows static context the client adds to every request, with columns `kind` (tool listing, tool definition, MCP instructions, agent listing, skill listing, instruction file), `owner` (MCP server, `built-in`, or file path), `items` (distinct items, once per session), `tokens` (estimated size of those items), `requests` (main-agent requests that followed them) and `overhead` (tokens x requests, estimated). The measured size of the first request of a session bounds everything static. Only Claude Code reports these items today; other sources are listed as not available.

`agentlens unused --days 30` lists installed skills, agents (`.claude/agents`, `.github/agents`, `.opencode/agents`) and configured MCP servers that were not used in the period (`--all` also shows used ones). `idle cost` is the estimated baseline tokens times the requests that carried the item; for MCP servers it comes from the context items the client reports, so servers of other clients show `unknown`. MCP configuration files are read for server names only.

`agentlens flow` shows how sessions unfold (`--section growth|errors|hooks`; CSV needs a section). The errors section counts failed tool calls per tool (calls without a recorded outcome are `unknown`, never errors) and lists retry loops: three or more adjacent identical calls (same tool and input summary) where the first two failed; calls without an input summary cannot be compared and never form a loop. Shell calls are compared by program name only (arguments are never stored), so a loop of `make` calls may contain different arguments. The hooks section sums hook runs, failures and runtime per hook type and counts permission prompts with the time until the answer (includes time the user was away); it needs events the client records (Copilot CLI today), other sources are listed as not available. The growth section lists, per session, the measured context size (input plus cache) of the first, last and largest main-agent request, the size right before each compaction and whether the session came within 90% of the model's limit. The limit is the documented window of the model family; where a session exceeds it (larger model variants), the largest size at which that model was compacted is used instead (marked `observed-compaction` in JSON).

`agentlens cost` shows requests, tokens and cost per model. Cost the client reported (OpenCode) is shown as is; otherwise it is computed from a bundled price table and marked `~` ([ADR 0002](docs/adr/0002-price-source.md)). The table only contains prices checked against the vendor's price list (with source and date); models without a price are listed as unpriced, never guessed. Add or override prices in `~/.config/agentlens/prices.json` or with `--prices FILE` (per million tokens, keyed by model-id prefix: `{"my-model": {"input": 3, "output": 15, "cache_read": 0.3, "cache_write": 3.75}}`). `--what-if MODEL` recomputes the same tokens with the prices of another model (id prefix) and shows actual cost, what-if cost and the difference per model; requests without an actual price are excluded and counted. The `share` column is the model mix. `--by project|branch|issue` groups the cost instead of per model; the issue number is read from the branch name when a number of 2 to 6 digits starts a `/` segment (`feat/123-title`) or follows `issue-`/`gh-`; other sessions are `(unassigned)`. Only Claude Code and Copilot CLI record the branch. GitHub Copilot premium requests are shown separately and are not money.

`agentlens efficiency` shows the cache hit rate (cache read / all input tokens, measured; `--by session|project|source`) and the share of subagents in requests, tokens and cost. Sources that report no cache tokens show `n/a` instead of 0%; cost shares only cover priced requests.

`agentlens compare` puts the clients side by side over the same sessions: sessions, requests, tokens, cache hit rate, tool calls and error rate, skill invocations and their estimated overhead, and session time (first to last event, so idle time is included). A value a client does not record is `n/a`, never 0.

OpenTelemetry exports of Claude Code can be added with `--otel FILE` or inspected with `agentlens import-otel FILE`; see [docs/telemetry.md](docs/telemetry.md) for how to produce them. The measured per-skill attribution from the export shows up as the `attributed` column of `agentlens skills`.

Supported sources today: Claude Code, OpenCode (SQLite database), GitHub Copilot CLI and GitHub Copilot Chat in VS Code and VS Code Insiders (`chatSessions` files, including the `.jsonl` operation log). Copilot CLI sessions without per-request rows in `session-store.db` fall back to the session totals from `session.shutdown`, which cannot be split per request. VS Code reports prompt and completion tokens per request without a cache split, plus the share of system instructions and tool definitions in the prompt, which `agentlens context` uses as reported token counts. Cost for Copilot sources is computed with the vendor's API list prices when the model is in the price table; Copilot itself bills premium requests, so treat it as a comparison figure, not an invoice.

## Application

The desktop application shows the same analyses as the CLI with shared filters (period, sources, project): sources, skills, context, unused, flow, cost, efficiency, compare and a timeline of single sessions. It reads the same client files and never writes to them, and it shows the same numbers with the same marking (`~` estimated, `n/a` not recorded).

Prerequisites: Node 24 with pnpm, the Rust toolchain from `rust-toolchain.toml`, and the Tauri system libraries (Linux: `libwebkit2gtk-4.1-dev build-essential libssl-dev libxdo-dev libayatana-appindicator3-dev librsvg2-dev patchelf`; macOS: Xcode command line tools; Windows: the WebView2 runtime and the MSVC build tools).

```sh
pnpm install
pnpm tauri dev        # the application with live reload
pnpm tauri build      # a release build and installers
pnpm start            # only the frontend in a browser, with sample data (no Tauri)
pnpm lint && pnpm format:check && pnpm test && pnpm build
cargo test -p agentlens-app
```

The frontend only calls commands of the `agentlens-app` crate; it never reads client files. The JSON samples in `src/app/core/samples` are written by a Rust test (`UPDATE_SAMPLES=1 cargo test -p agentlens-app`) and the TypeScript types and tests are built on them.

## Layout

| Path | Purpose |
|---|---|
| `crates/agentlens-core` | normalized event model, adapters, analyzers |
| `crates/agentlens-cli` | the `agentlens` binary |
| `src-tauri` | the desktop application (Tauri 2), a thin shell over the core |
| `src` | the application frontend (Angular 22) |

## Development

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

See [AGENTS.md](AGENTS.md) for the architecture and the working conventions.

## License

MIT, see [LICENSE](LICENSE).
