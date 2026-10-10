# 0003: Desktop application

- Status: Accepted (decided by the owner on 2026-10-10)
- Context: PBI "decide whether a desktop app is needed"

## Context

AgentLens has a CLI and a self-contained HTML report. The analyses have grown (skills, context, unused, flow, cost, efficiency, compare), and the report only shows the skill analysis. Looking at several analyses with shared filters, sorting through sessions and drilling into a single session is awkward in the terminal and in a static file.

The owner wants a desktop application. Constraints:

- Local and read-only: the application reads the same client files as the CLI and never writes to them.
- The numbers must be identical to the CLI, with the same measured/estimated marking.
- `agentlens-core` has no UI dependencies and must stay that way.
- The sibling project mcp-studio (Tauri 2 + Angular) already shows that a Tauri/Angular stack works on the owner's machines and in CI.

## Options

1. **HTML report only.** No new code, but static, one analysis, no filters or drill-down.
2. **Tauri 2 + Angular application on `agentlens-core`, in this repository.** Reuses the core through thin commands. Needs system libraries for Tauri on Linux and a second toolchain (Node, pnpm).
3. **A module of mcp-studio.** Shares a shell with another tool, but couples two release cycles before either is stable.

## Decision

- **Option 2.** A `src-tauri` crate (`agentlens-app`) joins the Cargo workspace and exposes one thin command per analysis, taking the same filter as the CLI (period, sources, project) and returning the report types of `agentlens-core` as JSON. The frontend is Angular 22 with standalone components and signals, built with pnpm, in `src`.
- The frontend never reads client files; all data comes through the commands. Logic stays in `agentlens-core`; a missing analysis is added there so the CLI benefits as well.
- The CLI and core crates must keep building without Tauri or Node. CI runs the application checks in a separate workflow, and the existing matrix excludes the application crate.
- Option 3 stays open as a later decision: the shared-core idea (one core crate and one adapter source for both tools) is compatible with this layout.

## Consequences

- Contributors need Node and pnpm and, on Linux, the Tauri system libraries to work on the application; they do not for the CLI and the core.
- The report types of the core are a public contract of the application. The TypeScript types mirror them by hand; a change in the core needs a matching change in `src/app` (a test compares the field names of the sample reports).
- Installers for the application are a separate later step; the first milestone delivers a runnable application and CI checks.
