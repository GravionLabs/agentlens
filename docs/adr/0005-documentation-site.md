# 0005: The documentation site on GitHub Pages

- Status: Accepted (decided by the owner on 2026-10-11)
- Context: Epic "Interfaces" (#3), Feature #213, PBIs #214 to #219; follows the sibling project Ariadne (its ADR 0020 and `.github/workflows/pages.yml`)

## Context

The documentation lives in `README.md` and `docs/` (telemetry, updater, ADRs). People who only want to try AgentLens have to read it on GitHub, and there is no place that explains the terms (overhead, baseline, carry), the sources and the analyses as a guide. The application already contains a glossary and can run on sample data in a browser. A second copy of the text would drift, so the Markdown in the repository has to remain the single source.

## Options

1. **GitHub's rendering of the Markdown** (status quo). No search, no navigation, no demo.
2. **VitePress, reading `docs/` in place.** The solution Ariadne uses: search, dark mode and a sidebar from plain Markdown, dead links fail the build.
3. **MkDocs Material** through the shared `GravionLabs/ci` pages action. Python toolchain in a Rust/Angular repository.
4. **The GitHub wiki.** Separate history, no review, no demo.

## Decision

- **Option 2.** A VitePress project in `site/` (private, not part of the Cargo workspace or the Angular app) whose source directory is `docs/`: nothing is copied. The site contains the guide pages (`docs/guide`), `docs/telemetry.md` and `docs/updater.md`; the ADRs stay on GitHub and are excluded.
- **The sidebar is read from the numbered list of `docs/guide/README.md`**, so a page is added in one place; a test fails when a page is missing from it.
- **Links to files of the repository that the site does not contain** (source files, ADRs, fixtures) are rewritten to `https://github.com/GravionLabs/agentlens/blob/main/<path>` by a small markdown-it plugin; the Markdown stays unchanged and works on GitHub too.
- **Dead links fail the build**, so every pull request that touches the site or the documents finds a broken link before the merge.
- **One public site** at `https://gravionlabs.github.io/agentlens/`, put together as a single artifact: `/` the guide, `/app/` the application built with its sample data as a live demo (`--base-href …/app/`; a notice says that the data is a sample and nothing leaves the browser).
- **The glossary has one source**: the guide's glossary page is generated from `src/app/core/glossary.ts` at build time (PBI #217), so the application and the site cannot disagree.
- **Deployment** by `.github/workflows/pages.yml`: pull requests that touch `site/`, `docs/`, the application or the workflow build everything without deploying; only `main` deploys, and only while the repository variable `DEPLOY_PAGES` is `true` (Settings → Pages → Source: GitHub Actions, an owner task). Actions are pinned to a SHA and the Node setup comes from `GravionLabs/ci`.
- No analytics and no custom domain. Generated text is in US English.

## Consequences

- The text is edited where it is today; the site follows with the next deploy.
- `README.md` becomes shorter and points to the site; `docs/telemetry.md` and `docs/updater.md` are refined as part of PBI #216.
- A new dependency family (VitePress) lives in `site/` only; Renovate can group its updates if they become noisy.
- The application is built twice (installers and demo); the demo build is a normal Angular build and needs no Tauri libraries.
