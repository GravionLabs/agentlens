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
  template: `
    <section class="view">
      <app-page-header
        exportView="compare"
        [exportFormats]="['json', 'csv']"
        title="Compare"
        summary="The clients side by side over the same sessions."
      >
        <p>
          “n/a”: the client does not record it. <span class="est">~</span> marks estimates; session
          time runs from the first to the last event, so idle time is included.
        </p>
      </app-page-header>
      <app-status
        [loading]="res.isLoading()"
        [error]="res.error()"
        [warnings]="res.value()?.warnings ?? 0"
        [errors]="res.value()?.errors ?? []"
      />
      @if (sources().length > 0) {
        <hx-data-table
          [value]="rows"
          [columns]="columns()"
          emptyMessage="No sessions for the selected filters."
          ariaLabel="Compare"
        >
          <ng-template hxTableBody let-row>
            <th scope="row" class="metric">{{ row.label }}</th>
            @for (s of sources(); track s.source) {
              <td
                class="num"
                [class.est]="row.estimated && !row.value(s).startsWith('n/a')"
                [attr.title]="exact(row, s)"
              >
                {{ row.value(s) }}
              </td>
            }
          </ng-template>
        </hx-data-table>
      } @else if (!res.isLoading()) {
        <p class="muted">No sessions for the selected filters.</p>
      }
    </section>
  `,
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
