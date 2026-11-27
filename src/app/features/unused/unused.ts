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
import {
  HxButton,
  HxCell,
  HxColumnHeader,
  HxDataTable,
  HxField,
  HxSwitch,
  type HxColumn,
} from "@gravionlabs/helix-ui";

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
  imports: [
    PageHeader,
    Term,
    Status,
    HxButton,
    HxDataTable,
    HxCell,
    HxColumnHeader,
    HxField,
    HxSwitch,
    HelpLink,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
  templateUrl: "./unused.html",
  styles: `
    .toggle {
      margin: var(--h-space-1) var(--h-space-4) var(--h-space-2) 0;
    }
    .what-if {
      white-space: nowrap;
    }
    .est {
      color: var(--h-tag-warn-color);
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
