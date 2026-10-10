# 0002: Source of model prices

- Status: Accepted (decided by the owner on 2026-10-10; the owner chose the recommended option)
- Context: PBI "model price table and cost per request"

## Context

Clients report cost inconsistently: OpenCode stores a cost per message, Claude Code and GitHub Copilot CLI report tokens only (Copilot reports premium requests instead of money). To show cost per model, project or issue, AgentLens needs per-token prices for the other cases.

Constraints:

- The tool is local and read-only and works offline by default.
- Prices change and differ by plan; a wrong number that looks exact is worse than no number.
- Every value must say whether it was measured (reported by the client) or estimated (computed here).

## Options

1. **Bundled price table** in the repository. Offline and reproducible, but it ages and must be maintained.
2. **LiteLLM price list** fetched at run time or vendored. Broad model coverage, but needs a network connection or a vendored third-party file of unknown accuracy.
3. **User configuration only.** Always the user's own prices, but nothing works out of the box.

## Decision

- **Default: option 1 combined with option 3.** A small bundled table (`crates/agentlens-core/assets/prices.json`) plus an optional user file (`<config home>/agentlens/prices.json`, or `--prices FILE`) whose entries override or extend it. No network access.
- The bundled table only contains prices that were checked against the vendor's published price list. Each entry carries the source URL and the date of the check. Models without a known price stay unpriced and are reported as such; prices are never guessed.
- Cost reported by the client is used as is and marked measured. A cost computed from the table is marked estimated. Prompt-size tiers (a higher price above a prompt length) are supported per entry.
- GitHub Copilot premium requests are a count against a plan allowance. They are carried separately and never converted to money.
- Option 2 is not adopted: it adds a runtime dependency on the network or a vendored file whose accuracy we cannot check.

## Consequences

- Cost is only shown for models in the table, the user file, or where the client reports it; the rest is listed as unpriced.
- Cache write prices assume the 5-minute cache. Clients do not report which cache duration was used, so Claude Code costs that involve 1-hour cache writes are slightly underestimated.
- Someone has to refresh the table when vendors change prices; the check date in each entry shows how old it is.
