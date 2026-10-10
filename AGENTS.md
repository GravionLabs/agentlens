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
- **Fail soft:** the client formats are partly undocumented. A malformed or unknown record produces a warning and is skipped; it never aborts a scan or panics.
- **Read-only:** adapters never write to client data. SQLite files are opened read-only.
- **Timestamps** are `jiff::Timestamp` (UTC instants) everywhere in the model.
- **Privacy:** tests and fixtures use synthetic data only (no real prompts, file contents, paths of real projects or secrets).

## Verification

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Issue and PR conventions

- Hierarchy: Epic → Feature → PBI → Task, linked as native sub-issues; dependencies as "blocked by". Titles carry the level prefix (`[PBI] feat: …`).
- One branch and one PR per PBI; the PR body says `Closes #<PBI>` and lists the task numbers. Conventional commit messages.
- Milestone work is integrated on `release/<milestone>` branches; `main` only receives a finished release branch.
- Decisions get the `needs-decision` label. Write the ADR in `docs/adr/` with the next free number and Status: Proposed, then wait for the owner.

## Releases

Pushing a tag `v*` runs `.github/workflows/release.yml`: a plain matrix build (no cargo-dist) on native runners for Linux x86_64/aarch64, macOS aarch64 (Apple silicon only) and Windows x86_64, which packages each binary with a SHA-256 file and attaches everything, plus a combined `SHA256SUMS`, to a GitHub release. Pull requests that change the workflow run the build as a dry run without publishing.
