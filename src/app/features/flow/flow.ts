import { PageHeader } from "../../ui/page-header";
import { ChangeDetectionStrategy, Component, computed, signal } from "@angular/core";
import { analysis } from "../../core/analysis";
import {
  exactTokens,
  number,
  percent,
  seconds,
  shortId,
  shortPath,
  sourceLabel,
  tokens,
} from "../../core/format";
import type { FlowData, SessionGrowth, ToolErrors } from "../../core/types";
import { HelpLink, termTitle } from "../../ui/help-link";
import { Status } from "../../ui/status";
import { HxCell, HxColumnHeader, HxDataTable, type HxColumn } from "@gravionlabs/helix-ui";

type Tab = "growth" | "errors" | "hooks";

const TABS: { id: Tab; label: string }[] = [
  { id: "growth", label: "Context growth" },
  { id: "errors", label: "Tool errors" },
  { id: "hooks", label: "Hooks and permissions" },
];

/** An error row with its sortable rate next to it. */
export interface ErrorView extends ToolErrors {
  rate: number | null;
}

export function errorView(t: ToolErrors): ErrorView {
  return { ...t, rate: errorRate(t) };
}

/** A growth row with the number of compactions as a sortable value. */
export interface GrowthView extends SessionGrowth {
  compactionCount: number;
}

export function growthView(g: SessionGrowth): GrowthView {
  return { ...g, compactionCount: g.compactions.length };
}

export function errorRate(t: ToolErrors): number | null {
  return t.calls === 0 ? null : t.errors / t.calls;
}

@Component({
  selector: "app-flow",
  imports: [PageHeader, Status, HxDataTable, HxCell, HxColumnHeader, HelpLink],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <section class="view">
      <app-page-header
        exportView="flow"
        [exportFormats]="['json', 'csv']"
        [exportOptions]="{ section: tab() }"
        title="Flow"
        summary="How sessions unfold: context growth, tool errors and permission waits."
      >
        <p>Values come from what the clients report.</p>
      </app-page-header>
      <app-status
        [loading]="res.isLoading()"
        [error]="res.error()"
        [warnings]="res.value()?.warnings ?? 0"
        [errors]="res.value()?.errors ?? []"
      />
      <div class="tabs" role="tablist">
        @for (t of tabs; track t.id) {
          <button
            type="button"
            role="tab"
            [attr.aria-selected]="tab() === t.id"
            [class.on]="tab() === t.id"
            (click)="tab.set(t.id)"
          >
            {{ t.label }}
          </button>
        }
      </div>

      @switch (tab()) {
        @case ("growth") {
          <p class="muted">
            Context size = input + cache of main-agent requests (measured), sorted by peak. “Near
            limit” means at least 90% of the model's documented window, or of the largest size at
            which that model was compacted if the window does not fit.
          </p>
          <hx-data-table
            [value]="growth()"
            [columns]="growthColumns"
            filterDisplay="row"
            sortField="peak_tokens"
            [sortOrder]="-1"
            [paginator]="growth().length > 25"
            [rows]="25"
            [rowsPerPageOptions]="[25, 'all']"
            allLabel="Show all"
            emptyMessage="No sessions with requests."
            ariaLabel="Context growth"
          >
            <ng-template hxColumnHeader="peak_tokens"
              ><app-help-link term="peak" label="Peak"
            /></ng-template>
            <ng-template hxColumnHeader="compactionCount"
              ><app-help-link term="compaction" label="Compactions"
            /></ng-template>
            <ng-template hxColumnHeader="near_limit"
              ><app-help-link term="near-limit" label="Near limit"
            /></ng-template>
            <ng-template hxCell="source" let-value="value">{{ source(value) }}</ng-template>
            <ng-template hxCell="session" let-row>
              <span [attr.title]="row.session">{{ shortId(row.session) }}</span>
            </ng-template>
            <ng-template hxCell="project" let-row>
              <span [attr.title]="row.project">{{ shortPath(row.project) }}</span>
            </ng-template>
            <ng-template hxCell="model" let-value="value">{{ value ?? "-" }}</ng-template>
            <ng-template hxCell="requests" let-value="value">{{ n(value) }}</ng-template>
            <ng-template hxCell="first_tokens" let-value="value">
              <span [attr.title]="tip(value)">{{ tokens(value) }}</span>
            </ng-template>
            <ng-template hxCell="last_tokens" let-value="value">
              <span [attr.title]="tip(value)">{{ tokens(value) }}</span>
            </ng-template>
            <ng-template hxCell="peak_tokens" let-value="value">
              <span [attr.title]="tip(value)">{{ tokens(value) }}</span>
            </ng-template>
            <ng-template hxCell="compactionCount" let-row>
              <span [attr.title]="compactionTip(row)">{{
                row.compactionCount > 0 ? row.compactionCount : "-"
              }}</span>
            </ng-template>
            <ng-template hxCell="near_limit" let-value="value">{{
              value ? "yes" : ""
            }}</ng-template>
          </hx-data-table>
        }
        @case ("errors") {
          <p class="muted">
            Calls without a recorded outcome are “unknown” and never counted as errors. A retry loop
            is three or more adjacent identical calls whose first two failed.
          </p>
          <hx-data-table
            [value]="errors()"
            [columns]="errorColumns"
            filterDisplay="row"
            sortField="errors"
            [sortOrder]="-1"
            [paginator]="errors().length > 25"
            [rows]="25"
            [rowsPerPageOptions]="[25, 'all']"
            allLabel="Show all"
            emptyMessage="No tool calls."
            ariaLabel="Tool errors"
          >
            <ng-template hxCell="source" let-value="value">{{ source(value) }}</ng-template>
            <ng-template hxCell="calls" let-value="value">{{ n(value) }}</ng-template>
            <ng-template hxCell="errors" let-value="value">{{ n(value) }}</ng-template>
            <ng-template hxCell="rate" let-value="value">{{ percent(value) }}</ng-template>
            <ng-template hxCell="unknown" let-value="value">{{ n(value) }}</ng-template>
          </hx-data-table>
          @if (loops().length > 0) {
            <h3>Retry loops</h3>
            <hx-data-table
              [value]="loops()"
              [columns]="loopColumns"
              sortField="calls"
              [sortOrder]="-1"
              [paginator]="loops().length > 25"
              [rows]="25"
              [rowsPerPageOptions]="[25, 'all']"
              allLabel="Show all"
              ariaLabel="Retry loops"
            >
              <ng-template hxCell="source" let-value="value">{{ source(value) }}</ng-template>
              <ng-template hxCell="session" let-value="value">{{ shortId(value) }}</ng-template>
              <ng-template hxCell="input" let-value="value">{{ value ?? "-" }}</ng-template>
            </hx-data-table>
          }
        }
        @case ("hooks") {
          <p class="muted">
            Permission wait is the time until the answer, including the time the user was away.
          </p>
          <hx-data-table
            [value]="hooks()"
            [columns]="hookColumns"
            sortField="total_ms"
            [sortOrder]="-1"
            emptyMessage="No hook runs recorded."
            ariaLabel="Hooks"
          >
            <ng-template hxCell="source" let-value="value">{{ source(value) }}</ng-template>
            <ng-template hxCell="runs" let-value="value">{{ n(value) }}</ng-template>
            <ng-template hxCell="total_ms" let-value="value">{{ seconds(value) }}</ng-template>
          </hx-data-table>
          @if (permissions().length > 0) {
            <h3>Permission prompts</h3>
            <hx-data-table
              [value]="permissions()"
              [columns]="permissionColumns"
              ariaLabel="Permission prompts"
            >
              <ng-template hxColumnHeader="total_wait_ms"
                ><app-help-link term="permission-wait" label="Wait (s)"
              /></ng-template>
              <ng-template hxCell="source" let-value="value">{{ source(value) }}</ng-template>
              <ng-template hxCell="total_wait_ms" let-value="value">{{
                seconds(value)
              }}</ng-template>
            </hx-data-table>
          }
          @for (n of unavailable(); track n) {
            <p class="muted">{{ n }}</p>
          }
        }
      }
    </section>
  `,
  styles: `
    h3 {
      margin: 20px 0 6px;
      font-size: 1.05rem;
    }
    .tabs {
      display: flex;
      gap: 4px;
      margin: 12px 0;
      border-bottom: 1px solid var(--line);
    }
    .tabs button {
      background: none;
      border: 0;
      border-bottom: 2px solid transparent;
      color: var(--muted);
      padding: 6px 14px;
      cursor: pointer;
      font: inherit;
    }
    .tabs button.on {
      color: var(--accent);
      border-bottom-color: var(--accent);
      font-weight: 600;
    }
  `,
})
export class Flow {
  protected readonly res = analysis<FlowData>("flow");
  protected readonly tab = signal<Tab>("growth");
  protected readonly tabs = TABS;

  protected readonly source = sourceLabel;
  protected readonly shortId = shortId;
  protected readonly shortPath = shortPath;
  protected readonly n = number;
  protected readonly percent = percent;
  protected readonly seconds = seconds;
  protected readonly tokens = tokens;
  protected readonly tip = exactTokens;

  protected readonly growth = computed(() =>
    (this.res.value()?.data.growth.sessions ?? []).map(growthView),
  );
  protected readonly errors = computed(() =>
    (this.res.value()?.data.errors.tools ?? []).map(errorView),
  );
  protected readonly loops = computed(() => this.res.value()?.data.errors.loops ?? []);
  protected readonly hooks = computed(() => this.res.value()?.data.hooks.hooks ?? []);
  protected readonly permissions = computed(() => this.res.value()?.data.hooks.permissions ?? []);
  protected readonly unavailable = computed(() =>
    Object.entries(this.res.value()?.data.hooks.unavailable ?? {}).map(
      ([s, n]) =>
        `${sourceLabel(s)}: not available, the client records neither hooks nor permission prompts (${n} sessions).`,
    ),
  );

  protected compactionTip(g: GrowthView): string | null {
    return g.compactions.length > 0
      ? `Size before each compaction: ${g.compactions.map(number).join(", ")}`
      : null;
  }

  protected readonly growthColumns: HxColumn[] = [
    { field: "source", header: "Source", sortable: true, filter: true },
    { field: "session", header: "Session", sortable: true },
    { field: "project", header: "Project", sortable: true },
    { field: "model", header: "Model", sortable: true },
    { field: "requests", header: "Requests", align: "right", sortable: true },
    { field: "first_tokens", header: "First", align: "right", sortable: true },
    { field: "last_tokens", header: "Last", align: "right", sortable: true },
    {
      field: "peak_tokens",
      header: "Peak",
      title: termTitle("peak"),
      align: "right",
      sortable: true,
    },
    {
      field: "compactionCount",
      header: "Compactions",
      title: termTitle("compaction"),
      align: "right",
      sortable: true,
    },
    {
      field: "near_limit",
      header: "Near limit",
      title: "At least 90% of the model's window",
      sortable: true,
    },
  ];

  protected readonly errorColumns: HxColumn[] = [
    { field: "source", header: "Source", sortable: true, filter: true },
    { field: "tool", header: "Tool", sortable: true },
    { field: "calls", header: "Calls", align: "right", sortable: true },
    { field: "errors", header: "Errors", align: "right", sortable: true },
    { field: "rate", header: "Error rate", align: "right", sortable: true },
    { field: "unknown", header: "Unknown", align: "right", sortable: true },
    { field: "loops", header: "Retry loops", align: "right", sortable: true },
  ];

  protected readonly loopColumns: HxColumn[] = [
    { field: "source", header: "Source", sortable: true },
    { field: "session", header: "Session", sortable: true },
    { field: "tool", header: "Tool", sortable: true },
    { field: "input", header: "Input", sortable: true },
    { field: "calls", header: "Calls", align: "right", sortable: true },
    { field: "errors", header: "Errors", align: "right", sortable: true },
  ];

  protected readonly hookColumns: HxColumn[] = [
    { field: "source", header: "Source", sortable: true },
    { field: "hook", header: "Hook", sortable: true },
    { field: "runs", header: "Runs", align: "right", sortable: true },
    { field: "failures", header: "Failures", align: "right", sortable: true },
    { field: "unknown", header: "No outcome", align: "right", sortable: true },
    { field: "total_ms", header: "Runtime (s)", align: "right", sortable: true },
  ];

  protected readonly permissionColumns: HxColumn[] = [
    { field: "source", header: "Source", sortable: true },
    { field: "prompts", header: "Prompts", align: "right", sortable: true },
    { field: "answered", header: "Answered", align: "right", sortable: true },
    {
      field: "total_wait_ms",
      header: "Wait (s)",
      title: termTitle("permission-wait"),
      align: "right",
      sortable: true,
    },
  ];
}
