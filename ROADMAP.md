# Roadmap

Order of work and release assignment for the open issues. Written 2026-10-29 after the move to apm-hub 3 and `release/next` (Feature #424) and dromos decision 004 (dromos archived; issues are delivered through the `deliver` workflow of [apm-hub](https://github.com/GravionLabs/apm-hub), session state and gates move to [keryx](https://github.com/GravionLabs/keryx)). Last release `v0.11.0` (2026-10-28). Each release has a GitHub milestone named after its version; work runs on `release/next` and the release PR into `main` is the human review point.

## Where we are

| Epic | Open Features | Release |
| --- | --- | --- |
| #255 1.0 readiness | #424 work flow on apm-hub 3 and `release/next`; #416 verified on real use | 0.12; 1.0 |
| #2 Analyses | #332 period comparison; #399 runs: cost per session; #333 what-if; #334 working habits | 0.12; 0.13; 0.14 |
| #3 Interfaces | #331 period with an end and a custom range; #404 Raycast extension | 0.12; 0.15 |
| #294 Flows | #385 flow analysis in the core; #386 flow view | 0.13 (#387, #402, #415) and 0.16 |

## 0.12 — Periods and trends

The first release from `release/next`. The work flow comes first, then the two PBIs that other repositories wait for, then the periods.

| Order | Issue | Title | Depends on |
| --- | --- | --- | --- |
| 1 | #425 | move to `release/next` and remove the dromos runner setup (PR #435, CI green, waits for merge) | #353 (linked; the PR does not need it) |
| 2 | #353 | a release merge bumps the minor version; a chore push creates no release | — |
| 3 | #436 | read the subagents of Claude Code workflows | — |
| 4 | #423 | resolve git worktree paths to their main repository | — |
| 5 | #338 | the CLI builds its filter through `api::Filters` (PR #434) | — |
| 6 | #383 | the filter bar moves into the topbar | — |
| 7 | #337 | a custom period in the application | #383 |
| 8 | #339 | trend analysis: the period against the previous one | — |
| 9 | #340 | `agentlens trend` and the MCP tool | #339 |
| 10 | #341 | deltas against the previous period in the application | #339 |

Release with `github-release`. #353 must be merged before it: `release/0.12` was published as `v0.11.0`, and the next tag has to be `v0.12.0`. Features #424, #331 and #332 close with the release.

## 0.13 — Runs

| Order | Issue | Title | Depends on |
| --- | --- | --- | --- |
| 1 | #437 | `--session <id>` filter for one session in the CLI and the MCP server | — |
| 2 | #400 | `cost --by session` with peak context and subagent share | — |
| 3 | #401 | issue number from commit messages when the branch has none | — |
| 4 | #387 | agent runs with parent, caller and depth in the model | — |
| 5 | #402 | `efficiency --section roles`: cost, requests and cache per agent type | #387 (and #436 for the workflow roles) |
| 6 | #415 | `--agent-type` filter on cost, efficiency, flow and compare | #387, #338 |

Feature #399 closes with the release. #402 and #415 belong to Feature #385 (Flows) and ship early because `deliver` runs need the cost per role.

## 0.14 — What-if and working habits

| Order | Issue | Title | Depends on |
| --- | --- | --- | --- |
| 1 | #342 | `cost --without` estimates the tokens and cost saved by removing an item | — |
| 2 | #343 | what-if removal in the application and the MCP server | #342 |
| 3 | #344 | a RetryLoops finding | — |
| 4 | #345 | a ModelMix finding | — |
| 5 | #346 | a ContextHygiene finding | — |
| 6 | #347 | a Delegation finding | — |

Features #333 and #334 close with the release.

## 0.15 — Raycast

| Order | Issue | Title | Depends on |
| --- | --- | --- | --- |
| 1 | #405 | ADR for the Raycast extension (`needs-decision`) | — |
| 2 | #406 | scaffold under `raycast/` | #405 |
| 3 | #407 | menu bar command with today's cost | #406 |
| 4 | #408 | Sessions by Cost list | #406, #400 |
| 5 | #409 | Skill Usage list | #406 |
| 6 | #410 | guide page, Store assets and the release path | #407, #408, #409 |

Feature #404 closes with the release.

## 0.16 — Flows

Behind what-if and Raycast since 2026-10-28: the orchestrator subagent trees that motivated it ended with apm-hub 3.0.

| Order | Issue | Title | Depends on |
| --- | --- | --- | --- |
| 1 | #388 | `flow --section calls`: per-session call tree and aggregated call graph | #387 |
| 2 | #389 | an AgentCall marker on the timeline | #387 |
| 3 | #390 | ADR 0008 graph library for the flow view (`needs-decision`) | — |
| 4 | #391 | Flow view Calls with the session tree and the aggregated graph | #388, #390 |

Features #385 and #386 and Epic #294 close with the release.

## 1.0 — verified on real use

Feature #416, the criteria of Epic #255: #417 release checklist for the untested paths, #418 client formats checked against real data, #419 code signing ADR (`needs-decision`), #420 outside feedback. No order yet; planned with `planner` after 0.16.

## Decided

| Date | Topic | Decision |
| --- | --- | --- |
| 2026-10-28 | work flow (#424) | integrate on `release/next`; the minor bump comes from the `chore: release minor` commit of `github-release` |
| 2026-10-28 | dromos (#425) | the dromos runner setup is removed instead of tried; the dromos link of #399 (#421, #422) is dropped |
| 2026-10-28 | Flows | 0.16, behind what-if (0.14) and Raycast (0.15) |
| 2026-10-29 | #436 | moved into 0.12, because apm-hub 3.4.0 (#832) and the `deliver` trial need the cost of workflow subagents |
| 2026-10-29 | keryx (#437) | keryx#19 reads the cost of one session through a new `--session` filter, not by filtering `cost --by session` itself |

## Dependencies on other repositories

| Repository | Direction | Status |
| --- | --- | --- |
| apm-hub#826 `deliver` | issues of this repository are delivered through it; #423 and #436 make its runs visible | merged on apm-hub `release/next`, ships with 3.3.0 |
| apm-hub#829 | trial of `/deliver` on one small agentlens PBI; findings become issues here | apm-hub 3.3.0 |
| apm-hub#832 `retro` | reads cost, requests and the role share from agentlens; the role share needs #436 and #402 | apm-hub 3.4.0 |
| keryx#19 | the herdr pane shows the session cost through `agentlens cost --session <id> --format json` (#437) | keryx 0.2.0; moves if 0.13 is late |
| apm-hub `packages/ng` | `apm.yml` requires `>=3.0.0` (#425) | — |

## Risks

- 0.12 holds ten PBIs, most of them `needs-refinement`. Refine each one with `planner` before it starts; #337 to #341 can move to 0.13 if 0.12 should ship early for #436.
- #436 follows a transcript layout that Claude Code does not document (`subagents/workflows/<runId>/`, found in the apm-hub spike #823); a Claude Code update can move it again.
- #437 is called by keryx once a minute per pane. If a single-session call needs a full scan of all transcripts, keryx#19 is too slow; #437 measures this.
- Three decisions are open (#405, #390, #419); each blocks its release.
