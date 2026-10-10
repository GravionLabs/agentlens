# 0001: Token estimation for static content

- Status: Accepted (decided by the owner on 2026-10-10)
- Context: issue "ADR on token estimation for static content"

## Context

AgentLens must put a token count on text that no client reports a count for: the name and description of every installed skill (sent with every request), the body of a skill, instruction files and tool definitions. Clients only report tokens per request, not per piece of content. We need a number for each piece anyway.

Constraints:

- The tool is local and read-only; it must work offline and without credentials by default.
- Claude, OpenAI and other models use different tokenizers; no single local tokenizer is exact for all.
- Every number must say whether it was measured or estimated (see `AGENTS.md`).

## Options

1. **Heuristic `characters / 4`**, rounded up. Offline, no dependency, fast. Off by tens of percent for code, JSON or non-English text.
2. **BPE tokenizer (e.g. `tiktoken-rs`).** More precise for OpenAI models, but only an approximation for Claude and an extra dependency with embedded vocabularies.
3. **Exact counts through the Anthropic `count_tokens` API.** Exact for Claude models, but needs a network connection and an API key, and sends the content (skill text) to a third party.

## Decision

- **Default: option 1.** The `TokenEstimator` trait in `agentlens-core` has one implementation, `CharsPerToken`, which counts Unicode scalar values and divides by four, rounding up. Results are `Tokens { count, measured: false }`.
- **Optional: option 3**, opt-in per run (planned flag `--exact`, requires `ANTHROPIC_API_KEY`). It is a second `TokenEstimator` implementation whose results are marked as exact; it is not part of the first release and is tracked as its own PBI.
- Option 2 is not adopted: it would add a dependency without making Claude numbers exact.

## Consequences

- Reports show estimated static costs with a visible marker (for example `~`). Costs that clients measure (usage per request) stay marked as measured.
- Estimates are good for ranking and for spotting outliers, not for billing.
- Analyzers depend on the `TokenEstimator` trait, so the exact counter can be added without changing them.
