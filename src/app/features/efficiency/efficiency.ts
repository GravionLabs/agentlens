import { PageHeader } from "../../ui/page-header";
import { Term } from "../../ui/term";
import { ChangeDetectionStrategy, Component, computed, inject, signal } from "@angular/core";
import { analysis, trendAnalysis } from "../../core/analysis";
import { trendNote } from "../../core/delta";
import { FiltersService } from "../../core/filters.service";
import {
  exactTokens,
  number,
  pathLabel,
  percent,
  shortId,
  sourceLabel,
  tokens,
} from "../../core/format";
import type { EfficiencyReport, SubagentShare } from "../../core/types";
import { HelpLink, termTitle } from "../../ui/help-link";
import { nullsLast } from "../../ui/sort";
import { trendStat } from "../../ui/stat-cards";
import { Status } from "../../ui/status";
import { TrendCards } from "../../ui/trend-cards";
import {
  HxCell,
  HxColumnHeader,
  HxDataTable,
  HxField,
  HxSelect,
  type HxColumn,
} from "@gravionlabs/helix-ui";

type By = "source" | "project" | "session";

/** Subagent share of a total; `null` if there is no total to share. */
export function share(part: number, whole: number): number | null {
  return whole > 0 ? part / whole : null;
}

/** The label of a cache group: long ids are shortened, paths keep their last segments. */
export function groupLabel(key: string): string {
  return key.length > 30 && !key.startsWith("/") ? shortId(key) : pathLabel(key);
}

/** A subagent row with the shares precomputed, so the table can sort them (`null`: no total). */
export interface SubagentView {
  source: string;
  requests: number;
  subRequests: number | null;
  subTokens: number | null;
  subCost: number | null;
  unpriced: number;
}

export function subagentView(s: SubagentShare): SubagentView {
  return {
    source: s.source,
    requests: s.requests,
    subRequests: share(s.subagent_requests, s.requests),
    subTokens: share(s.subagent_tokens, s.tokens),
    subCost: share(s.subagent_usd, s.usd),
    unpriced: s.unpriced_requests,
  };
}

@Component({
  selector: "app-efficiency",
  imports: [
    PageHeader,
    Term,
    Status,
    TrendCards,
    HxDataTable,
    HxCell,
    HxColumnHeader,
    HxField,
    HxSelect,
    HelpLink,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
  templateUrl: "./efficiency.html",
  styles: `
    h3 {
      margin: 20px 0 6px;
      font-size: 1.05rem;
    }
    .by {
      width: fit-content;
      min-width: 16rem;
      max-width: 100%;
      margin: var(--h-space-2) 0;
    }
  `,
})
export class Efficiency {
  protected readonly by = signal<By>("source");
  protected readonly options: By[] = ["source", "project", "session"];
  protected readonly res = analysis<EfficiencyReport>("efficiency", () => ({ by: this.by() }));
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
      trendStat("Cache hit rate", t.efficiency.cache_hit_rate, "ratio", { good: "up" }),
      trendStat("Subagent share", t.efficiency.subagent_share, "ratio"),
      trendStat("Failed tool calls", t.errors.failed_calls, "count", { good: "down", perDay }),
      trendStat("Retry loops", t.errors.retry_loops, "count", { good: "down", perDay }),
    ];
  });
  protected readonly cache = computed(() => this.res.value()?.data.cache ?? []);
  protected readonly subagents = computed(() => this.res.value()?.data.subagents ?? []);

  protected readonly sort = nullsLast;
  protected readonly n = number;
  protected readonly tokens = tokens;
  protected readonly tip = exactTokens;
  protected readonly percent = percent;
  protected readonly label = sourceLabel;
  /** Cache rows with the group as shown, so the column filter matches the text the user sees. */
  protected readonly cacheViews = computed(() =>
    this.cache().map((c) => ({ ...c, groupName: groupLabel(c.key) })),
  );
  protected readonly subagentViews = computed(() => this.subagents().map(subagentView));

  protected readonly cacheColumns: HxColumn[] = [
    { field: "groupName", header: "Group", sortable: true, filter: true },
    { field: "source", header: "Source", sortable: true },
    {
      field: "requests",
      header: "Requests",
      title: termTitle("request"),
      align: "right",
      sortable: true,
    },
    { field: "input", header: "Input", align: "right", sortable: true },
    {
      field: "cache_read",
      header: "Cache read",
      title: termTitle("cache-read"),
      align: "right",
      sortable: true,
    },
    {
      field: "cache_write",
      header: "Cache write",
      title: termTitle("cache-write"),
      align: "right",
      sortable: true,
    },
    {
      field: "hit_rate",
      header: "Hit rate",
      title: termTitle("hit-rate"),
      align: "right",
      sortable: true,
    },
  ];

  protected readonly subagentColumns: HxColumn[] = [
    { field: "source", header: "Source", sortable: true },
    {
      field: "requests",
      header: "Requests",
      title: termTitle("request"),
      align: "right",
      sortable: true,
    },
    { field: "subRequests", header: "Subagent requests", align: "right", sortable: true },
    { field: "subTokens", header: "Subagent tokens", align: "right", sortable: true },
    { field: "subCost", header: "Subagent cost", align: "right", sortable: true },
    {
      field: "unpriced",
      header: "Unpriced requests",
      title: termTitle("unpriced"),
      align: "right",
      sortable: true,
    },
  ];

  protected setBy(value: By | null): void {
    // A cleared value is ignored: the table always has a grouping.
    if (value) this.by.set(value);
  }
}
