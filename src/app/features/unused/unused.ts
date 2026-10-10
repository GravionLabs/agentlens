import { ChangeDetectionStrategy, Component, computed, signal } from "@angular/core";
import { analysis } from "../../core/analysis";
import { number, shortPath, sourceLabel, tokens } from "../../core/format";
import type { UnusedItem, UnusedReport } from "../../core/types";
import { type Column, DataTable } from "../../ui/data-table";
import { Status } from "../../ui/status";

@Component({
  selector: "app-unused",
  imports: [Status, DataTable],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <section class="view">
      <h2>Unused</h2>
      <p class="muted">
        Installed skills, agents and MCP servers that were not used in the period (30 days unless a
        period is chosen), with the cost they cause while idle. <span class="est">~</span> marks
        estimates; <em>unknown</em> means the client reports no data for MCP servers.
      </p>
      <app-status
        [loading]="res.isLoading()"
        [error]="res.error()"
        [warnings]="res.value()?.warnings ?? 0"
        [errors]="res.value()?.errors ?? []"
      />
      <label class="check">
        <input type="checkbox" [checked]="all()" (change)="all.set(!all())" />
        Also show items that were used
      </label>
      <app-data-table
        [columns]="columns"
        [data]="rows()"
        [initialSort]="{ key: 'cost', descending: true }"
        [filterable]="true"
        placeholder="Filter by kind"
        empty="Everything installed was used."
      />
    </section>
  `,
  styles: `
    .check {
      display: inline-flex;
      gap: 6px;
      margin: 4px 16px 8px 0;
      color: var(--muted);
      align-items: center;
    }
    .est {
      color: var(--warn);
    }
  `,
})
export class Unused {
  protected readonly res = analysis<UnusedReport>("unused");
  protected readonly all = signal(false);

  protected readonly rows = computed(() =>
    (this.res.value()?.data.items ?? []).filter((i) => this.all() || i.uses === 0),
  );

  protected readonly columns: Column<UnusedItem>[] = [
    { key: "kind", label: "Kind", value: (i) => i.kind },
    { key: "name", label: "Name", value: (i) => i.name },
    {
      key: "project",
      label: "Project",
      value: (i) => i.project,
      text: (i) => shortPath(i.project),
      hover: (i) => i.project,
    },
    {
      key: "clients",
      label: "Clients",
      value: (i) => i.clients.join(" "),
      text: (i) => i.clients.map(sourceLabel).join(", "),
    },
    { key: "uses", label: "Uses", value: (i) => i.uses, numeric: true },
    {
      key: "requests",
      label: "Requests",
      value: (i) => i.requests,
      text: (i) => number(i.requests),
      numeric: true,
    },
    {
      key: "cost",
      label: "Idle cost",
      value: (i) => i.overhead_tokens,
      text: (i) => (i.overhead_tokens === null ? "unknown" : tokens(i.overhead_tokens, true)),
      estimated: (i) => i.overhead_tokens !== null,
      numeric: true,
      title: "Baseline tokens × the requests that carried the item",
    },
  ];
}
