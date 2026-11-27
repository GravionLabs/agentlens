import { RouterLink } from "@angular/router";
import { tipSaving } from "../advice/advice";
import { PageHeader } from "../../ui/page-header";
import { ChangeDetectionStrategy, Component, computed, inject } from "@angular/core";
import { answerOf, analysis, trendAnalysis } from "../../core/analysis";
import { FiltersService } from "../../core/filters.service";
import { trendNote } from "../../core/delta";
import { number, sourceLabel } from "../../core/format";
import type { SourceOverview, TipsReport } from "../../core/types";
import { trendStat } from "../../ui/stat-cards";
import { TrendCards } from "../../ui/trend-cards";
import { Status } from "../../ui/status";

@Component({
  selector: "app-overview",
  imports: [PageHeader, Status, TrendCards, RouterLink],
  changeDetection: ChangeDetectionStrategy.OnPush,
  templateUrl: "./overview.html",
  styles: `
    .cards {
      display: grid;
      grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
      gap: 12px;
      margin-top: 12px;
    }
    .card {
      background: var(--h-surface-card);
      border: 1px solid var(--h-border-panel);
      border-radius: 10px;
      padding: 12px 16px;
    }
    .card h3 {
      margin: 0 0 6px;
      font-size: var(--h-font-size-md);
    }
    .top-tip {
      margin-top: 12px;
    }
    .top-tip p {
      margin: 4px 0;
    }
    .missing {
      opacity: 0.65;
    }
    .big {
      font-size: var(--h-font-size-2xl);
      font-weight: 650;
      margin: 0;
      font-variant-numeric: tabular-nums;
    }
    .big span {
      font-size: var(--h-font-size-sm);
      font-weight: 400;
      color: var(--h-text-muted-color);
    }
    .path {
      font-size: var(--h-font-size-xs);
      color: var(--h-text-muted-color);
      word-break: break-all;
      margin: 4px 0 0;
    }
    .warn {
      color: var(--h-tag-warn-color);
    }
    .error {
      color: var(--h-tag-danger-color);
    }
  `,
})
export class Overview {
  protected readonly res = analysis<SourceOverview[]>("overview");
  protected readonly answer = computed(() => answerOf(this.res));
  // The Overview stays silent when the tips fail; the Advice view shows the error.
  private readonly tips = analysis<TipsReport>("tips", () => ({ limit: 1 }));
  protected readonly topTip = computed(() =>
    this.tips.hasValue() ? (this.tips.value().data.tips[0] ?? null) : null,
  );
  protected readonly saving = tipSaving;
  protected readonly sources = computed(() => answerOf(this.res)?.data ?? []);
  protected readonly compare = inject(FiltersService).compare;
  protected readonly trend = trendAnalysis();
  protected readonly note = computed(() => {
    const w = answerOf(this.trend)?.data.windows;
    return w ? trendNote(w) : "";
  });
  protected readonly trendStats = computed(() => {
    const t = answerOf(this.trend)?.data;
    if (!t) return [];
    const perDay = t.windows.normalized;
    return [
      trendStat("Sessions", t.volume.sessions, "count", { perDay }),
      trendStat("Requests", t.volume.requests, "count", { perDay }),
      trendStat("Tokens", t.volume.tokens, "tokens", { perDay }),
    ];
  });
  protected readonly n = number;
  protected readonly label = sourceLabel;
}
