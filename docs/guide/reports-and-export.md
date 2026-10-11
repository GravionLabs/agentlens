# Reports and export

## Formats of the CLI

Every analysis command prints a table by default and takes `--format json` or `--format csv`:

```sh
agentlens skills --format json > skills.json
agentlens cost --by project --format csv > cost.csv
```

- **JSON** is the report type of the analysis, with the `measured` flags of its counts and the warnings of reading the sessions.
- **CSV** holds one table with fixed columns: counts are plain integers, money and rates have six decimals, an unknown value is an empty cell, and measured and estimated values are separate columns where a report distinguishes them. `flow` has three tables, so `--format csv` needs `--section growth|errors|hooks`.

The shape of the JSON and the columns of the CSV are guarded; [Compatibility](compatibility.md) says what you can rely on.

## The HTML report

```sh
agentlens report --html report.html
```

One page with the skill analysis. It is self-contained and works offline: no network, no external scripts, nothing to host. The data is embedded in the file, so share it only as far as you would share the numbers.

## Export from the application

Every view of the [application](application.md) has an **Export** menu next to its title. It runs the analysis with the filters of the view and saves the result to a file you choose in the save dialog; nothing is written without that choice.

| Format      | Views                                                                                                   | Same as the CLI                                              |
| ----------- | ------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------ |
| JSON        | all                                                                                                     | `--format json` (flow and cost: the whole data of the view)  |
| CSV         | skills, context, unused, flow (the open tab), cost (its grouping, or the what-if table), efficiency, compare | `--format csv`, same columns, produced by the same code      |
| HTML report | skills                                                                                                  | `agentlens report --html`                                    |
