import { PageHeader } from "../../ui/page-header";
import { ChangeDetectionStrategy, Component, computed } from "@angular/core";
import { analysis } from "../../core/analysis";
import { minutes, number, percent, sourceLabel, tokens } from "../../core/format";
import type { CompareReport, SourceStats } from "../../core/types";
import { Status } from "../../ui/status";

export interface Metric {
  label: string;
  value: (s: SourceStats) => string;
  estimated?: boolean;
}

/** One metric per row, one source per column. Missing values are `n/a`, never 0. */
export const METRICS: Metric[] = [
  { label: "Sessions", value: (s) => number(s.sessions) },
  { label: "Requests", value: (s) => number(s.requests) },
  { label: "Input tokens", value: (s) => number(s.input_tokens) },
  { label: "Output tokens", value: (s) => number(s.output_tokens) },
  { label: "Cache hit rate", value: (s) => percent(s.cache_hit_rate) },
  { label: "Tool calls", value: (s) => number(s.tool_calls) },
  { label: "Tool error rate", value: (s) => percent(s.tool_error_rate) },
  { label: "Skill invocations", value: (s) => number(s.skill_invocations) },
  {
    label: "Skill overhead",
    value: (s) => tokens(s.skill_overhead_tokens, true),
    estimated: true,
  },
  { label: "Mean session (min)", value: (s) => minutes(s.mean_session_ms) },
  { label: "Total time (min)", value: (s) => minutes(s.total_duration_ms) },
];

@Component({
  selector: "app-compare",
  imports: [PageHeader, Status],
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
        <div class="wrap">
          <table>
            <thead>
              <tr>
                <th scope="col">Metric</th>
                @for (s of sources(); track s.source) {
                  <th scope="col" class="num">{{ label(s.source) }}</th>
                }
              </tr>
            </thead>
            <tbody>
              @for (m of metrics; track m.label) {
                <tr>
                  <th scope="row">{{ m.label }}</th>
                  @for (s of sources(); track s.source) {
                    <td class="num" [class.est]="m.estimated && !m.value(s).startsWith('n/a')">
                      {{ m.value(s) }}
                    </td>
                  }
                </tr>
              }
            </tbody>
          </table>
        </div>
      } @else if (!res.isLoading()) {
        <p class="muted">No sessions for the selected filters.</p>
      }
    </section>
  `,
  styles: `
    .wrap {
      overflow-x: auto;
      background: var(--panel);
      border: 1px solid var(--line);
      border-radius: 10px;
      margin-top: 8px;
    }
    table {
      border-collapse: collapse;
      width: 100%;
      font-variant-numeric: tabular-nums;
    }
    th,
    td {
      padding: 6px 14px;
      border-bottom: 1px solid var(--line);
      text-align: left;
      white-space: nowrap;
    }
    thead th {
      color: var(--muted);
      font-size: 0.85rem;
    }
    tbody th {
      font-weight: 500;
    }
    .num {
      text-align: right;
    }
    .est {
      color: var(--warn);
    }
  `,
})
export class Compare {
  protected readonly res = analysis<CompareReport>("compare");
  protected readonly sources = computed(() => this.res.value()?.data.sources ?? []);
  protected readonly metrics = METRICS;
  protected readonly label = sourceLabel;
}
