# Measured and estimated

Every token or cost value in AgentLens is either **measured** or **estimated**, and the output says which.

| You see             | It means                                                                                        |
| ------------------- | ----------------------------------------------------------------------------------------------- |
| `12,345`            | **Measured:** the client reported this number (usage of a request, a cost it computed).          |
| `~12,345`           | **Estimated:** AgentLens computed it, from a file size or from a price table.                    |
| `n/a`               | The client does not record it. It is not zero and it is never guessed.                           |
| `$1.50 + ~$2.00`    | A cost with a measured and an estimated part.                                                    |

## What is estimated, and how

- **The size of text** (a skill, an instruction file, a tool definition) is not reported by any client in isolation. AgentLens estimates it as characters divided by four. [ADR 0001](../adr/0001-token-estimation.md) explains the choice. With [`--exact`](exact-tokens.md) the text of skills is counted by the Anthropic API instead.
- **Cost the client did not report** is computed from a bundled price table and marked `~`. The table only contains prices checked against the vendor's list, with source and date ([ADR 0002](../adr/0002-price-source.md)); a model without a price is listed as unpriced, never guessed.
- **Overhead** is a size times a number of requests, so it is an estimate whenever the size is.

## What is measured

Token usage of a request (input, output, cache read and write), the context size at each request, cost that the client reported, tool outcomes and durations.

## In files and in the MCP server

In the application and on the command line the tilde carries the marking. In `--format json`, a count that can be an estimate is an object with `count` and `measured`; a few plain numbers are estimates by convention (the `tokens` and `overhead_tokens` of context rows, the carry of a skill). The [MCP server](mcp-server.md) names them in the descriptions of its tools.
