import { PageHeader } from "../../ui/page-header";
import { Term } from "../../ui/term";
import { ChangeDetectionStrategy, Component, computed, inject, signal } from "@angular/core";
import { analysis, trendAnalysis } from "../../core/analysis";
import { trendNote } from "../../core/delta";
import { FiltersService } from "../../core/filters.service";
import { WithoutService } from "../../core/without.service";
import { exactTokens, number, pathLabel, tokens, usd } from "../../core/format";
import type { CostData, CostRow, WhatIfRow, WithoutReport } from "../../core/types";
import { trendStat } from "../../ui/stat-cards";
import { Status } from "../../ui/status";
import { TrendCards } from "../../ui/trend-cards";
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

/** A row of the without table; the total row has an empty kind. */
export interface WithoutView {
  item: string;
  kind: string;
  installations: number | null;
  requests: number;
  tokens: number;
  cost: number;
  /** `n/a` when no request of the row has a price. */
  costText: string;
}

/** The rows of a without report followed by its total, with the cost as text (`n/a` unpriced). */
export function withoutViews(w: WithoutReport): WithoutView[] {
  const text = (usdValue: number, requests: number, unpriced: number) =>
    requests > 0 && unpriced >= requests ? "n/a" : usd(usdValue, true);
  return [
    ...w.rows.map((r) => ({
      item: r.item,
      kind: r.kind,
      installations: r.installations,
      requests: r.requests,
      tokens: r.tokens.count,
      cost: r.cost_usd,
      costText: text(r.cost_usd, r.requests, r.unpriced_requests),
    })),
    {
      item: "Total",
      kind: "",
      installations: null,
      requests: w.total.requests,
      tokens: w.total.tokens.count,
      cost: w.total.cost_usd,
      costText: text(w.total.cost_usd, w.total.requests, w.total.unpriced_requests),
    },
  ];
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
  imports: [PageHeader, Term, Status, TrendCards, HxDataTable, HxCell],
  changeDetection: ChangeDetectionStrategy.OnPush,
  templateUrl: "./cost.html",
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
    .chips {
      display: flex;
      flex-wrap: wrap;
      gap: 6px;
      list-style: none;
      margin: 8px 0;
      padding: 0;
    }
    .chip {
      display: inline-flex;
      align-items: center;
      gap: 4px;
      background: var(--panel);
      border: 1px solid var(--line);
      border-radius: 999px;
      padding: 2px 4px 2px 10px;
    }
    .chip button {
      background: transparent;
      color: var(--muted);
      border: 0;
      padding: 0 6px;
    }
    .warning {
      color: var(--warn);
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
  protected readonly without = inject(WithoutService);
  protected readonly money = money;
  protected readonly n = number;
  protected readonly tokens = tokens;
  protected readonly tip = exactTokens;

  protected readonly res = analysis<CostData>("cost", () => ({
    by: this.by(),
    whatIf: this.whatIf() || null,
    without: this.without.items(),
  }));

  protected readonly compare = inject(FiltersService).compare;
  protected readonly trend = trendAnalysis();
  protected readonly note = computed(() => {
    const w = this.trend.value()?.data.windows;
    return w ? trendNote(w) : "";
  });
  protected readonly trendStats = computed(() => {
    const t = this.trend.value()?.data;
    if (!t) return [];
    const perDay = t.windows.normalized;
    return [
      trendStat("Measured cost", t.cost.measured_usd, "usd", { good: "down", perDay }),
      trendStat("Estimated cost", t.cost.estimated_usd, "usd", { good: "down", perDay }),
      trendStat("Unpriced requests", t.cost.unpriced_requests, "count", { good: "down", perDay }),
    ];
  });

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

  protected readonly withoutRows = computed(() => {
    const w = this.res.value()?.data.without;
    return w ? withoutViews(w) : [];
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

  protected readonly withoutColumns: HxColumn[] = [
    { field: "item", header: "Item" },
    { field: "kind", header: "Kind" },
    { field: "installations", header: "Installations", align: "right" },
    { field: "requests", header: "Requests", align: "right" },
    { field: "tokens", header: "Tokens", align: "right" },
    { field: "cost", header: "Cost", align: "right" },
  ];

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
