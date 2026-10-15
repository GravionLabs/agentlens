import { PageHeader } from "../../ui/page-header";
import { Term } from "../../ui/term";
import { ChangeDetectionStrategy, Component, computed, signal } from "@angular/core";
import { analysis } from "../../core/analysis";
import { exactTokens, number, pathLabel, tokens, usd } from "../../core/format";
import type { CostData, CostRow, WhatIfRow } from "../../core/types";
import { Status } from "../../ui/status";
import { HxCell, HxDataTable, type HxColumn } from "@gravionlabs/helix-ui";

/** A cost row prepared for the helix table: sortable raw values next to the display text. */
export interface CostView {
  key: string;
  label: string;
  requests: number;
  input: number;
  output: number;
  cost: number;
  costText: string;
  /** Only computed prices: shown with the estimate colour (the tilde is in `costText`). */
  estimated: boolean;
  premium: number;
}

export function costView(r: CostRow): CostView {
  return {
    key: r.key,
    label: pathLabel(r.key),
    requests: r.requests,
    input: r.input + r.cache_read + r.cache_write,
    output: r.output,
    cost: r.measured_usd + r.estimated_usd,
    costText: money(r),
    estimated: r.estimated_usd > 0 && r.measured_usd === 0,
    premium: r.premium_requests,
  };
}

/** A what-if row with the difference computed, so the table can sort it. */
export interface WhatIfView extends WhatIfRow {
  diff: number;
}

export function whatIfView(r: WhatIfRow): WhatIfView {
  return { ...r, diff: r.what_if_usd - r.actual_usd };
}

/** `+$1.20` or `-$0.40`. */
export function signedUsd(d: number): string {
  return `${d < 0 ? "-" : "+"}${usd(Math.abs(d))}`;
}

export type By = "model" | "project" | "branch" | "issue";

const GROUPS: { id: By; label: string }[] = [
  { id: "model", label: "Model" },
  { id: "project", label: "Project" },
  { id: "branch", label: "Branch" },
  { id: "issue", label: "Issue" },
];

/** `$1.50`, `~$1.50` for computed values, `$1.50 + ~$2.00` for a mix, `unpriced` without any price. */
export function money(r: CostRow): string {
  const measured = r.measured_usd > 0;
  const estimated = r.estimated_usd > 0;
  if (measured && estimated) return `${usd(r.measured_usd)} + ${usd(r.estimated_usd, true)}`;
  if (estimated) return usd(r.estimated_usd, true);
  if (measured) return usd(r.measured_usd);
  return r.requests > 0 && r.unpriced_requests === r.requests ? "unpriced" : usd(0);
}

@Component({
  selector: "app-cost",
  imports: [PageHeader, Term, Status, HxDataTable, HxCell],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <section class="view">
      <app-page-header
        exportView="cost"
        [exportFormats]="['json', 'csv']"
        [exportOptions]="{ by: by(), whatIf: whatIf() || null }"
        title="Cost"
        summary="What the usage costs, grouped by model, project, branch or issue."
      >
        <p>
          Cost the client reported is shown as is; <span class="est">~</span> was computed from the
          price table. Cache writes are priced as 5-minute writes.
          <app-term id="premium">Premium requests</app-term> are a plan allowance, not money.
        </p>
      </app-page-header>
      <form class="controls" (submit)="apply($event)">
        <label>
          <span>Group by</span>
          <select (change)="by.set(asSelect($event).value)">
            @for (g of groups; track g.id) {
              <option [value]="g.id" [selected]="g.id === by()">{{ g.label }}</option>
            }
          </select>
        </label>
        <label>
          <span>What if everything ran on…</span>
          <input
            type="text"
            placeholder="model id prefix, e.g. claude-haiku-4-5"
            [value]="draft()"
            (input)="draft.set(asInput($event).value)"
          />
        </label>
        <button type="submit">Compare</button>
        @if (whatIf()) {
          <button type="button" (click)="clearWhatIf()">Clear</button>
        }
      </form>
      <app-status
        [loading]="res.isLoading()"
        [error]="res.error()"
        [warnings]="res.value()?.warnings ?? 0"
        [errors]="res.value()?.errors ?? []"
      />
      <hx-data-table
        [value]="views()"
        [columns]="columns()"
        dataKey="key"
        [sortField]="'cost'"
        [sortOrder]="-1"
        [paginator]="views().length > 50"
        [rows]="50"
        emptyMessage="No usage found."
        ariaLabel="Cost"
      >
        <ng-template hxCell="key" let-row>
          <span [attr.title]="row.key">{{ row.label }}</span>
        </ng-template>
        <ng-template hxCell="requests" let-value="value">{{ n(value) }}</ng-template>
        <ng-template hxCell="input" let-value="value">
          <span [attr.title]="tip(value)">{{ tokens(value) }}</span>
        </ng-template>
        <ng-template hxCell="output" let-value="value">
          <span [attr.title]="tip(value)">{{ tokens(value) }}</span>
        </ng-template>
        <ng-template hxCell="cost" let-row>
          <span [class.est]="row.estimated">{{ row.costText }}</span>
        </ng-template>
        <ng-template hxCell="premium" let-value="value">{{ value > 0 ? value : "" }}</ng-template>
      </hx-data-table>
      @if (total(); as t) {
        <p class="total">
          <strong>Total</strong>: {{ n(t.requests) }} requests, {{ money(t) }}
          @if (t.premium_requests > 0) {
            , {{ t.premium_requests }} premium requests
          }
        </p>
      }
      @if (unpriced().length > 0) {
        <p class="muted">
          No price for: {{ unpriced().join(", ") }}. Add prices to ~/.config/agentlens/prices.json
          (per million tokens, keyed by model-id prefix).
        </p>
      }
      @if (res.value()?.data?.what_if; as w) {
        <h3>What if everything ran on {{ w.target }}?</h3>
        <hx-data-table
          [value]="whatIfRows()"
          [columns]="whatIfColumns"
          emptyMessage="Nothing to compare."
          [ariaLabel]="'What if everything ran on ' + w.target"
        >
          <ng-template hxCell="actual_usd" let-value="value">{{ usd(value, true) }}</ng-template>
          <ng-template hxCell="what_if_usd" let-value="value">{{ usd(value, true) }}</ng-template>
          <ng-template hxCell="diff" let-value="value">{{ signedUsd(value) }}</ng-template>
        </hx-data-table>
        <p class="muted">
          Same tokens priced with the prices of {{ w.target }}. “Excluded” requests have no actual
          price and are left out of both sides.
        </p>
      }
    </section>
  `,
  styles: `
    h3 {
      margin: 20px 0 6px;
      font-size: 1.05rem;
    }
    .controls {
      display: flex;
      flex-wrap: wrap;
      gap: 12px;
      align-items: end;
      margin: 12px 0;
    }
    label {
      display: flex;
      flex-direction: column;
      gap: 2px;
      color: var(--muted);
      font-size: 0.85rem;
    }
    select,
    input {
      background: var(--panel);
      color: var(--text);
      border: 1px solid var(--line);
      border-radius: 6px;
      padding: 5px 8px;
      min-width: 200px;
    }
    button {
      background: var(--accent);
      color: #fff;
      border: 0;
      border-radius: 6px;
      padding: 6px 14px;
      cursor: pointer;
    }
    button[type="button"] {
      background: var(--panel);
      color: var(--text);
      border: 1px solid var(--line);
    }
    .total {
      margin: 10px 0;
    }
    .est {
      color: var(--warn);
    }
  `,
})
export class Cost {
  protected readonly by = signal<By>("model");
  protected readonly whatIf = signal("");
  protected readonly draft = signal("");
  protected readonly groups = GROUPS;
  protected readonly money = money;
  protected readonly n = number;
  protected readonly tokens = tokens;
  protected readonly tip = exactTokens;

  protected readonly res = analysis<CostData>("cost", () => ({
    by: this.by(),
    whatIf: this.whatIf() || null,
  }));

  protected readonly rows = computed(() => this.res.value()?.data.report.rows ?? []);
  protected readonly total = computed(() => this.res.value()?.data.report.total ?? null);
  protected readonly unpriced = computed(() =>
    Object.entries(this.res.value()?.data.report.unpriced_models ?? {}).map(
      ([m, n]) => `${m} (${n})`,
    ),
  );
  protected readonly whatIfRows = computed(() => {
    const w = this.res.value()?.data.what_if;
    return w ? [...w.rows, w.total].map(whatIfView) : [];
  });

  protected readonly views = computed(() => this.rows().map(costView));

  protected readonly columns = computed<HxColumn[]>(() => [
    {
      field: "key",
      header: GROUPS.find((g) => g.id === this.by())?.label ?? "Group",
      sortable: true,
    },
    { field: "requests", header: "Requests", align: "right", sortable: true },
    { field: "input", header: "Input", align: "right", sortable: true },
    { field: "output", header: "Output", align: "right", sortable: true },
    { field: "cost", header: "Cost", align: "right", sortable: true },
    ...(this.rows().some((r) => r.premium_requests > 0)
      ? [{ field: "premium", header: "Premium requests", align: "right" as const, sortable: true }]
      : []),
  ]);

  protected readonly usd = usd;
  protected readonly signedUsd = signedUsd;

  protected readonly whatIfColumns: HxColumn[] = [
    { field: "model", header: "Model", sortable: true },
    { field: "requests", header: "Requests", align: "right", sortable: true },
    { field: "actual_usd", header: "Actual", align: "right", sortable: true },
    { field: "what_if_usd", header: "What-if", align: "right", sortable: true },
    { field: "diff", header: "Difference", align: "right", sortable: true },
    { field: "excluded_requests", header: "Excluded", align: "right", sortable: true },
  ];

  protected apply(event: Event): void {
    event.preventDefault();
    this.whatIf.set(this.draft().trim());
  }

  protected clearWhatIf(): void {
    this.draft.set("");
    this.whatIf.set("");
  }

  protected asSelect(e: Event): { value: By } {
    return e.target as unknown as { value: By };
  }

  protected asInput(e: Event): HTMLInputElement {
    return e.target as HTMLInputElement;
  }
}
