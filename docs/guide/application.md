# The desktop application

The application shows the same analyses as the CLI with shared filters, and the same numbers with the same marking (`~` estimated, `n/a` not recorded). It reads the same client files and never writes to them. [Installers](download.md) are attached to every release.

## Views

The sidebar lists the views:

| View       | Shows                                                                         |
| ---------- | ----------------------------------------------------------------------------- |
| Sources    | which agents were found, where, and how many sessions and events were read    |
| Skills     | uses and cost per skill, with a cost split chart                              |
| Advice     | what to change to save tokens, with the numbers behind each suggestion        |
| Context    | what the clients add to every request, by owner or by project                 |
| Unused     | installed skills, agents and MCP servers nobody used, and their idle cost     |
| Flow       | context growth, tool errors and retry loops, hooks and permission waits       |
| Cost       | cost per model, project, branch or issue, with a what-if model                |
| Efficiency | cache hit rate and the share of subagents                                     |
| Compare    | the agents side by side                                                       |
| Timeline   | the context size of every request of one session                              |
| Glossary   | the terms (overhead, baseline, carry, …) in one place                         |

The [analyses](analyses.md) page explains what each one means.

## Filters and refresh

The filter bar in the sidebar sets the period, the sources and the project for every view. The application keeps the sessions it has read for a few minutes so that switching views is quick; **Refresh** forgets them and reads the files again.

## Explaining the numbers

Every page has a one-line summary and an **Explain** link that unfolds the details. Terms with a dotted underline and the `?` in table headers lead to the glossary.

## Export

The **Export** menu next to the title of a view saves it as JSON, CSV or (skills) the HTML report to a file you choose; see [Reports and export](reports-and-export.md#export-from-the-application).

## The status bar

The bar at the bottom shows the running version, the token format, the size of the interface and **Check for updates**.

- **Size:** the magnifier buttons (minus and plus; or `Ctrl` `−` and `Ctrl` `+`, `Ctrl` `0` to reset) scale the whole interface from 70% to 160%. The size is remembered.
- **Token format:** the button next to the size controls switches the token counts in tables, cards and sentences between full (`2,114,305`, the default) and rounded (`2.11 M`); while rounded, a table cell shows the exact count as a tooltip. Charts keep their own compact labels (`2.1M`) in both modes. It is a toggle (`aria-pressed`) and the choice is remembered on this machine. Exports (JSON, CSV, copied values) always keep the exact numbers.
- **Updates:** the check makes a network request only when you click it; if a newer release exists the bar offers **Install and restart**. See [Updating the application](../updater.md).

## From source

```sh
pnpm install
pnpm tauri dev        # the application with live reload
pnpm start            # the frontend alone in a browser, on sample data
```

In a browser the application shows sample data (marked in the status bar), because reading the client files needs the desktop shell.
