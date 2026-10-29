# AGENTS.md

Guidance for humans and coding agents working in this repository. Generated text (docs, issues, commits, code) is written in US English.

## Architecture

```
client files ─► Adapter ─► normalized events ─► Analyzer ─► report ─► CLI / HTML
```

- **Adapter** (`agentlens-core/src/adapters/`): one per source (Claude Code, OpenCode, Copilot CLI, later VS Code Copilot and OpenTelemetry). It knows the client's file format and nothing else, and emits normalized events. Adapters take their root paths from an `Env` value, never directly from the real home directory, so tests run against fixture directories.
- **Analyzer** (`agentlens-core/src/analyzers/`): consumes normalized events (and the skill inventory), never raw client data.
- **CLI** (`agentlens-cli`): argument parsing and output formatting only. Logic belongs in `agentlens-core`.
- `agentlens-core` has no UI or terminal dependencies, so other tools can embed it.

### Rules

- **Measured vs. estimated:** every token or cost value carries a `measured` flag. Values read from client data are measured; anything computed from file sizes or heuristics is estimated and must be shown as such.
- **Stable output:** what the analyses print (the JSON of the CLI, the results of the MCP tools, the commands of the application) is a contract. `crates/agentlens-core/tests/output_contract.rs` has a snapshot of every analysis. A change of one is a decision: review the diff, say in the pull request whether it is compatible (a field was added) or breaking (renamed, removed, retyped), and for a breaking change raise `report::SCHEMA_VERSION`, name it in the changelog and update `src/app/core/types.ts` and the samples (`UPDATE_SAMPLES=1 cargo test -p agentlens-app`).
- **Fail soft:** the client formats are partly undocumented. A malformed or unknown record produces a warning and is skipped; it never aborts a scan or panics.
- **Read-only:** adapters never write to client data. SQLite files are opened read-only. There are two exceptions, both opt-in and undoable: `agentlens mcp-setup --write` (ADR 0004) registers the MCP server in a client's configuration and makes a backup; the application's *Install command line tools* action (ADR 0006) writes only `~/.local/bin/agentlens` and `~/.local/bin/agentlens-mcp`, never overwrites a file it did not create, and *Remove command line tools* undoes it.
- **Timestamps** are `jiff::Timestamp` (UTC instants) everywhere in the model.
- **Privacy:** tests and fixtures use synthetic data only (no real prompts, file contents, paths of real projects or secrets).

## Desktop application

- `src-tauri` (crate `agentlens-app`) is a thin shell: the plain function per analysis (taking the CLI's filter and returning the core's report types in an `Answer { data, warnings, errors }`) lives in `agentlens-core::api` and is shared with the MCP server; `src/api.rs` only re-exports it and `src/lib.rs` wraps the functions as Tauri commands. New logic belongs in `agentlens-core` so the CLI benefits too.
- `src` is Angular 22 (standalone components, signals, pnpm). Views call commands through `analysis()` in `src/app/core`; types in `types.ts` mirror the JSON, and the samples in `src/app/core/samples` come from the Rust tests (`UPDATE_SAMPLES=1`). A change in a core report type needs a matching change in `types.ts`.
- helix-ui and helix-shell are the adopted UI libraries, and chart.js via `HxChart` is the chosen stack for a chart built on a library ([ADR 0007](docs/adr/0007-helix-ui.md)). The views were migrated to helix in #351; both charts (stacked bars, timeline) stay SVG, and `chart.js` was removed from the dependencies in #413 (add it back with the first `HxChart`). The components the ADR lists stay hand-written, and a new UI or chart dependency beyond the ADR needs a decision.
- The CLI workflow (`ci.yml`) excludes the app crate and needs no system libraries; `app.yml` checks the frontend and the crate.
- Estimates carry a tilde and missing data is `n/a`, as in the CLI. Prettier covers the frontend only.

## Documentation site

`docs/` is the single source of the documentation; `site/` (VitePress, `pnpm docs:dev`, `pnpm docs:build`) renders it and copies nothing. A guide page is a Markdown file in `docs/guide` and a line in the numbered list of `docs/guide/README.md` (that list is the sidebar, and a test fails when a page is missing). `docs/guide/glossary.md` is generated from `src/app/core/glossary.ts`: run `pnpm docs:glossary` after changing a term. Dead links fail the site build. `.github/workflows/pages.yml` builds the site and the application demo (`pnpm build:demo`); a push to `main` deploys it to GitHub Pages only while the repository variable `DEPLOY_PAGES` is `true`. Decisions: [ADR 0005](docs/adr/0005-documentation-site.md).

## Verification

```sh
cargo fmt --all --check
cargo clippy --workspace --exclude agentlens-app --all-targets -- -D warnings
cargo test --workspace --exclude agentlens-app
```

For the application: `pnpm lint && pnpm format:check && pnpm test && pnpm build` and `cargo test -p agentlens-app`. Run `scripts/stage-sidecars.sh` (`.ps1` on Windows) once before `cargo test -p agentlens-app` or `pnpm tauri build`: tauri-build needs the staged `externalBin` files in `src-tauri/binaries`.

## Issue and PR conventions

- Hierarchy: Epic → Feature → PBI → Task, linked as native sub-issues; dependencies as "blocked by". Titles carry the level prefix (`[PBI] feat: …`). The shared `issues.yml` workflow of `GravionLabs/ci` sets it from the level label.
- One branch and one PR per PBI; the PR body says `Closes #<PBI>` and lists the task numbers. Conventional commit messages.
- Work is integrated on `release/next`: every PBI branch is cut from it and its PR targets it; `main` only receives a finished `release/next` (the `github-release` skill cuts and finishes it). Milestones are named after the version the release gets.
- The role skills (`planner`, `implementation`, `code-review`, `retro`) come from apm-hub 3; `apm.yml` installs the Angular and Rust stack skills. Worktrees go to the git-ignored `.worktrees/`.
- Decisions get the `needs-decision` label. Write the ADR in `docs/adr/` with the next free number and Status: Proposed, then wait for the owner.

## Releases

A push to `main` runs `.github/workflows/release.yml`: GitVersion (`GitVersion.yml`, through the shared `GravionLabs/ci` action like the other repositories) determines the version, a plain matrix build (no cargo-dist) on native runners builds the CLI and the MCP server (`agentlens-mcp`, in the same archive) for Linux x86_64/aarch64, macOS aarch64 (Apple silicon only) and Windows x86_64 with that version, and a GitHub release `v<version>` is created with the packaged binaries (each with a SHA-256 file), a combined `SHA256SUMS` and notes from the conventional commits (`cliff.toml`). The version numbers in `Cargo.toml`, `package.json` and `tauri.conf.json` are placeholders; the workflow sets the Cargo version for the build. Pull requests that change the release files run the build as a dry run without publishing. The jobs live in the shared reusable workflow `GravionLabs/ci/.github/workflows/tauri-app.yml` (`release.yml` only calls it): `determine-version` runs first and every job reads the version from its outputs. The same run builds the application installers (Linux x86_64, macOS aarch64, Windows x86_64): one job creates the draft release, all installer jobs upload into it, and the publish job adds the CLI binaries and publishes it once everything is attached. Do not pin `next-version` in `GitVersion.yml`; the minor bump comes from the `chore: release minor` commit (`+semver: minor`) that the `github-release` skill adds when it finishes `release/next`. GitVersion bumps from the last tag, so the version is the next minor of the last release (the release branch `release/0.12` was published as v0.11.0). The installers are not signed.
