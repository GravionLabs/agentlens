import { PageHeader } from "../../ui/page-header";
import { ChangeDetectionStrategy, Component, computed } from "@angular/core";
import { analysis } from "../../core/analysis";
import { exactTokens, minutes, number, percent, sourceLabel, tokens } from "../../core/format";
import type { CompareReport, SourceStats } from "../../core/types";
import { Status } from "../../ui/status";
import { HxDataTable, HxTableBody, type HxColumn } from "@gravionlabs/helix-ui";

export interface Metric {
  label: string;
  value: (s: SourceStats) => string;
  estimated?: boolean;
  /** The exact token count behind `value`, shown as tooltip while counts are rounded. */
  exact?: (s: SourceStats) => number | null;
}

/** One metric per row, one source per column. Missing values are `n/a`, never 0. */
export const METRICS: Metric[] = [
  { label: "Sessions", value: (s) => number(s.sessions) },
  { label: "Requests", value: (s) => number(s.requests) },
  {
    label: "Input tokens",
    value: (s) => tokens(s.input_tokens),
    exact: (s) => s.input_tokens,
  },
  {
    label: "Output tokens",
    value: (s) => tokens(s.output_tokens),
    exact: (s) => s.output_tokens,
  },
  { label: "Cache hit rate", value: (s) => percent(s.cache_hit_rate) },
  { label: "Tool calls", value: (s) => number(s.tool_calls) },
  { label: "Tool error rate", value: (s) => percent(s.tool_error_rate) },
  { label: "Skill invocations", value: (s) => number(s.skill_invocations) },
  {
    label: "Skill overhead",
    value: (s) =>
      s.skill_overhead_tokens
        ? tokens(s.skill_overhead_tokens.count, !s.skill_overhead_tokens.measured)
        : "n/a",
    estimated: true,
    exact: (s) => s.skill_overhead_tokens?.count ?? null,
  },
  { label: "Mean session (min)", value: (s) => minutes(s.mean_session_ms) },
  { label: "Total time (min)", value: (s) => minutes(s.total_duration_ms) },
];

@Component({
  selector: "app-compare",
  imports: [PageHeader, Status, HxDataTable, HxTableBody],
  changeDetection: ChangeDetectionStrategy.OnPush,
  templateUrl: "./compare.html",
  styles: `
    hx-data-table {
      display: block;
      margin-top: 8px;
    }
    .metric {
      font-weight: 500;
      text-align: left;
    }
    .num {
      text-align: right;
      font-variant-numeric: tabular-nums;
    }
    .est {
      color: var(--warn);
    }
  `,
})
export class Compare {
  protected exact(m: Metric, s: SourceStats): string | null {
    return exactTokens(m.exact?.(s));
  }

  protected readonly res = analysis<CompareReport>("compare");
  protected readonly sources = computed(() => this.res.value()?.data.sources ?? []);
  protected readonly rows = METRICS;
  protected readonly columns = computed<HxColumn[]>(() => [
    { field: "label", header: "Metric" },
    ...this.sources().map((s) => ({
      field: s.source,
      header: sourceLabel(s.source),
      align: "right" as const,
    })),
  ]);
  protected readonly label = sourceLabel;
}
