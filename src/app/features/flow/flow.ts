import { PageHeader } from "../../ui/page-header";
import { ChangeDetectionStrategy, Component, computed, signal } from "@angular/core";
import { answerOf, analysis } from "../../core/analysis";
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
import { nullsLast } from "../../ui/sort";
import { Status } from "../../ui/status";
import {
  HxCell,
  HxColumnHeader,
  HxDataTable,
  HxTab,
  HxTabContent,
  HxTabList,
  HxTabPanel,
  HxTabPanels,
  HxTabs,
  type HxColumn,
} from "@gravionlabs/helix-ui";

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
  imports: [
    PageHeader,
    Status,
    HxDataTable,
    HxCell,
    HxColumnHeader,
    HxTabs,
    HxTabList,
    HxTab,
    HxTabPanels,
    HxTabPanel,
    HxTabContent,
    HelpLink,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
  templateUrl: "./flow.html",
  styles: `
    h3 {
      margin: 20px 0 6px;
      font-size: var(--h-font-size-md);
    }
    hx-tabs {
      display: block;
      margin-top: var(--h-space-3);
      /* The panels sit in the page, not in a card: no side padding. */
      --h-tabs-tabpanel-padding: var(--h-space-3) 0 0;
      /* The tab scroller clips what hangs below the tabs: keep the active bar inside. */
      --h-tabs-tab-margin: 0;
      --h-tabs-active-bar-bottom: 0;
      --h-tabs-tab-border-color: transparent;
    }
  `,
})
export class Flow {
  protected readonly res = analysis<FlowData>("flow");
  protected readonly answer = computed(() => answerOf(this.res));
  protected readonly tab = signal<Tab>("growth");
  protected readonly tabs = TABS;

  /** `hx-tabs` reports a plain string; only one of the offered tabs sets the signal. */
  protected selectTab(value: string | null): void {
    const chosen = TABS.find((t) => t.id === value);
    if (chosen) this.tab.set(chosen.id);
  }

  protected readonly sort = nullsLast;
  protected readonly source = sourceLabel;
  protected readonly shortId = shortId;
  protected readonly shortPath = shortPath;
  protected readonly n = number;
  protected readonly percent = percent;
  protected readonly seconds = seconds;
  protected readonly tokens = tokens;
  protected readonly tip = exactTokens;

  protected readonly growth = computed(() =>
    (answerOf(this.res)?.data.growth.sessions ?? []).map(growthView),
  );
  protected readonly errors = computed(() =>
    (answerOf(this.res)?.data.errors.tools ?? []).map(errorView),
  );
  protected readonly loops = computed(() => answerOf(this.res)?.data.errors.loops ?? []);
  protected readonly hooks = computed(() => answerOf(this.res)?.data.hooks.hooks ?? []);
  protected readonly permissions = computed(() => answerOf(this.res)?.data.hooks.permissions ?? []);
  protected readonly unavailable = computed(() =>
    Object.entries(answerOf(this.res)?.data.hooks.unavailable ?? {}).map(
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
    { field: "source", header: "Source", sortable: true },
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
    { field: "source", header: "Source", sortable: true },
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
