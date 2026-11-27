# Compatibility

What AgentLens prints is meant to be read by other programs: the JSON and CSV of the command line, the results of the [MCP server](mcp-server.md), the files of the [export](reports-and-export.md). This page says what you can rely on and what may change.

AgentLens is **before 1.0**: the rules below hold, but a breaking change is allowed. It is then announced in the changelog and raises the schema version, never silent. From 1.0 on, a breaking change needs a new major version.

## What is stable

| What                               | The promise                                                                                                                                                                                         |
| ---------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Commands and flags**             | The names of the commands (`scan`, `skills`, `advise`, `context`, `unused`, `flow`, `cost`, `efficiency`, `compare`, `report`, `import-otel`, `mcp-setup`) and of their flags keep their meaning.                      |
| **JSON of the analyses**           | Field names and types of the reports (`--format json`, the `data` of an MCP result, the JSON export). A field is not renamed, removed or retyped without raising the schema version.                 |
| **The `schema_version`**           | The envelope of the MCP results and of the application's commands says which layout of `data` it has. A new field does **not** raise it, a renamed, removed or retyped one does.                     |
| **CSV**                            | The columns that exist keep their name and their position. A new column is appended at the end of the table.                                                                                        |
| **MCP**                            | The names and parameters of the tools, the names and arguments of the prompts `optimize-skills` and `tips` and the address `agentlens://glossary` of the resource.                                                |
| **Measured or estimated**          | A count that can be an estimate carries a `measured` flag, or sits in a field that is measured by definition (the usage a client reports). The [rule](measured-and-estimated.md) does not change.     |

The layout is guarded by snapshots of the JSON of every analysis (`crates/agentlens-core/tests/output_contract.rs`): a change of shape fails the tests until someone decides what kind of change it is.

## What a consumer has to expect

- **New fields appear.** Ignore the fields you do not know. A field that is added is a compatible change (for example `file_uses` and the `schema_version` itself).
- **Values of an enumeration can grow:** the `kind` of a finding of `advise`, the kind of a context item, a source id. Handle an unknown one gracefully.
- **A missing value is `null` or `n/a`, not 0.** A client that does not record something has no number for it.

## What is not promised

- The text of the tables of the command line, their column widths and the order of their rows.
- The wording of explanations, notes and actions (`explanation`, `action`, `notes` of the findings of `advise`, the descriptions of the MCP tools). The structure around them is stable, the sentences are not.
- The **estimates** themselves: how a size is estimated (characters divided by four today, [exact counts](exact-tokens.md) when asked) may be improved, and an estimate is marked as one.
- The layout and the script of the HTML report; its data is the skill report with its own schema version.
- The files of the agents that AgentLens reads. The clients change their formats without notice; AgentLens skips what it does not understand and says so in a warning.

## The schema version

| Schema version | Releases            | What changed                                                                                                                                                                  |
| -------------- | ------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1              | up to 0.7           | The first layout.                                                                                                                                                             |
| 2              | 0.8 and later       | The tokens of context rows, the idle cost of unused items, the carry of a skill and the skill overhead of `compare` became `{ count, measured }` instead of a bare number. |

The version is in the envelope `schema_version` of the MCP results and of the application's commands, and in the `meta` of the HTML report. The JSON of the command line is the report itself, without an envelope: it has the schema version of the release that printed it, which this table gives.
