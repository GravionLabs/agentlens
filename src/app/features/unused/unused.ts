import { PageHeader } from "../../ui/page-header";
import { Term } from "../../ui/term";
import { ChangeDetectionStrategy, Component, computed, inject, signal } from "@angular/core";
import { analysis } from "../../core/analysis";
import { WithoutService } from "../../core/without.service";
import { exactTokens, number, shortPath, sourceLabel, tokens } from "../../core/format";
import type { UnusedItem, UnusedReport } from "../../core/types";
import { HelpLink, termTitle } from "../../ui/help-link";
import { nullsLast } from "../../ui/sort";
import { Status } from "../../ui/status";
import { HxCell, HxColumnHeader, HxDataTable, type HxColumn } from "@gravionlabs/helix-ui";

/** “agent dev-workflow, command ship”, empty if nothing names the skill. */
export function referencedBy(i: UnusedItem): string {
  return (i.referenced_by ?? []).map((r) => `${r.kind} ${r.name}`).join(", ");
}

/** An unused item with the sortable raw values next to the display text. */
export interface UnusedView {
  kind: string;
  name: string;
  project: string | null;
  clients: string;
  uses: number;
  requests: number;
  /** `null` when the client reports no data (MCP servers). */
  cost: number | null;
  costText: string;
  estimated: boolean;
  referenced: string;
}

export function unusedView(i: UnusedItem): UnusedView {
  return {
    kind: i.kind,
    name: i.name,
    project: i.project,
    clients: i.clients.map(sourceLabel).join(", "),
    uses: i.uses,
    requests: i.requests,
    cost: i.overhead_tokens?.count ?? null,
    costText:
      i.overhead_tokens === null
        ? "unknown"
        : tokens(i.overhead_tokens.count, !i.overhead_tokens.measured),
    estimated: i.overhead_tokens !== null && !i.overhead_tokens.measured,
    referenced: referencedBy(i),
  };
}

@Component({
  selector: "app-unused",
  imports: [PageHeader, Term, Status, HxDataTable, HxCell, HxColumnHeader, HelpLink],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <section class="view">
      <app-page-header
        exportView="unused"
        [exportFormats]="['json', 'csv']"
        [exportOptions]="{ all: all() }"
        title="Unused"
        summary="Installed skills, agents and MCP servers nobody used, and what they cost while idle."
      >
        <p>
          Items that were not used in the period (30 days unless a period is chosen), with the
          <app-term id="idle-cost">idle cost</app-term> they cause. <span class="est">~</span> marks
          estimates; <em>unknown</em> means the client reports no data for MCP servers.
        </p>
      </app-page-header>
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
      <hx-data-table
        [value]="views()"
        [sortFunction]="sort"
        [columns]="columns"
        filterDisplay="row"
        sortField="cost"
        [sortOrder]="-1"
        [paginator]="views().length > 100"
        [rows]="100"
        [rowsPerPageOptions]="[100, 'all']"
        allLabel="Show all"
        emptyMessage="Everything installed was used."
        ariaLabel="Unused items"
      >
        <ng-template hxColumnHeader="requests"
          ><app-help-link term="request" label="Requests"
        /></ng-template>
        <ng-template hxColumnHeader="cost"
          ><app-help-link term="idle-cost" label="Idle cost"
        /></ng-template>
        <ng-template hxCell="project" let-row>
          <span [attr.title]="row.project">{{ shortPath(row.project) }}</span>
        </ng-template>
        <ng-template hxCell="requests" let-value="value">{{ n(value) }}</ng-template>
        <ng-template hxCell="cost" let-row>
          <span [class.est]="row.estimated" [attr.title]="tip(row.cost)">{{ row.costText }}</span>
        </ng-template>
        <ng-template hxCell="action" let-row>
          <button
            type="button"
            class="what-if"
            [disabled]="without.has(row.kind, row.name)"
            (click)="without.add(row.kind, row.name)"
          >
            {{ without.has(row.kind, row.name) ? "Added" : "What if removed" }}
          </button>
        </ng-template>
      </hx-data-table>
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
    .what-if {
      background: var(--panel);
      color: var(--text);
      border: 1px solid var(--line);
      border-radius: 6px;
      padding: 2px 8px;
      cursor: pointer;
      white-space: nowrap;
    }
    .what-if:disabled {
      color: var(--muted);
      cursor: default;
    }
  `,
})
export class Unused {
  protected readonly res = analysis<UnusedReport>("unused");
  protected readonly all = signal(false);
  protected readonly without = inject(WithoutService);

  protected readonly rows = computed(() =>
    (this.res.value()?.data.items ?? []).filter((i) => this.all() || i.uses === 0),
  );

  protected readonly views = computed(() => this.rows().map(unusedView));
  protected readonly sort = nullsLast;
  protected readonly n = number;
  protected readonly shortPath = shortPath;
  protected readonly tip = exactTokens;

  protected readonly columns: HxColumn[] = [
    { field: "kind", header: "Kind", sortable: true, filter: true },
    { field: "name", header: "Name", sortable: true },
    { field: "project", header: "Project", sortable: true },
    { field: "clients", header: "Clients", sortable: true },
    { field: "uses", header: "Uses", align: "right", sortable: true },
    {
      field: "requests",
      header: "Requests",
      title: termTitle("request"),
      align: "right",
      sortable: true,
    },
    {
      field: "cost",
      header: "Idle cost",
      title: "Baseline tokens × the requests that carried the item",
      align: "right",
      sortable: true,
    },
    {
      field: "referenced",
      header: "Referenced by",
      title:
        "Installed agents, commands and skills that name this skill. A mention does not prove use.",
      sortable: true,
    },
    {
      field: "action",
      header: "",
      title: "Add the item to the what-if removal list of the cost view",
    },
  ];
}
