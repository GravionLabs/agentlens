import { PageHeader } from "../../ui/page-header";
import { ChangeDetectionStrategy, Component, computed, signal } from "@angular/core";
import { analysis } from "../../core/analysis";
import { number, percent, seconds, shortId, shortPath, sourceLabel } from "../../core/format";
import type {
  FlowData,
  HookStats,
  PermissionStats,
  RetryLoop,
  SessionGrowth,
  ToolErrors,
} from "../../core/types";
import { type Column, DataTable } from "../../ui/data-table";
import { Status } from "../../ui/status";

type Tab = "growth" | "errors" | "hooks";

const TABS: { id: Tab; label: string }[] = [
  { id: "growth", label: "Context growth" },
  { id: "errors", label: "Tool errors" },
  { id: "hooks", label: "Hooks and permissions" },
];

export function errorRate(t: ToolErrors): number | null {
  return t.calls === 0 ? null : t.errors / t.calls;
}

@Component({
  selector: "app-flow",
  imports: [PageHeader, Status, DataTable],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <section class="view">
      <app-page-header
        exportView="flow"
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
          <app-data-table
            [columns]="growthColumns"
            [data]="growth()"
            [initialSort]="{ key: 'peak', descending: true }"
            [limit]="25"
            [filterable]="true"
            placeholder="Filter by source"
            empty="No sessions with requests."
          />
        }
        @case ("errors") {
          <p class="muted">
            Calls without a recorded outcome are “unknown” and never counted as errors. A retry loop
            is three or more adjacent identical calls whose first two failed.
          </p>
          <app-data-table
            [columns]="errorColumns"
            [data]="errors()"
            [initialSort]="{ key: 'errors', descending: true }"
            [limit]="25"
            [filterable]="true"
            placeholder="Filter by source"
            empty="No tool calls."
          />
          @if (loops().length > 0) {
            <h3>Retry loops</h3>
            <app-data-table
              [columns]="loopColumns"
              [data]="loops()"
              [initialSort]="{ key: 'calls', descending: true }"
              [limit]="25"
            />
          }
        }
        @case ("hooks") {
          <p class="muted">
            Permission wait is the time until the answer, including the time the user was away.
          </p>
          <app-data-table
            [columns]="hookColumns"
            [data]="hooks()"
            [initialSort]="{ key: 'runtime', descending: true }"
            empty="No hook runs recorded."
          />
          @if (permissions().length > 0) {
            <h3>Permission prompts</h3>
            <app-data-table [columns]="permissionColumns" [data]="permissions()" />
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

  protected readonly growth = computed(() => this.res.value()?.data.growth.sessions ?? []);
  protected readonly errors = computed(() => this.res.value()?.data.errors.tools ?? []);
  protected readonly loops = computed(() => this.res.value()?.data.errors.loops ?? []);
  protected readonly hooks = computed(() => this.res.value()?.data.hooks.hooks ?? []);
  protected readonly permissions = computed(() => this.res.value()?.data.hooks.permissions ?? []);
  protected readonly unavailable = computed(() =>
    Object.entries(this.res.value()?.data.hooks.unavailable ?? {}).map(
      ([s, n]) =>
        `${sourceLabel(s)}: not available, the client records neither hooks nor permission prompts (${n} sessions).`,
    ),
  );

  protected readonly growthColumns: Column<SessionGrowth>[] = [
    { key: "source", label: "Source", value: (g) => g.source, text: (g) => sourceLabel(g.source) },
    {
      key: "session",
      label: "Session",
      value: (g) => g.session,
      text: (g) => shortId(g.session),
      hover: (g) => g.session,
    },
    {
      key: "project",
      label: "Project",
      value: (g) => g.project,
      text: (g) => shortPath(g.project),
      hover: (g) => g.project,
    },
    { key: "model", label: "Model", value: (g) => g.model, text: (g) => g.model ?? "-" },
    { key: "requests", label: "Requests", value: (g) => g.requests, numeric: true },
    {
      key: "first",
      label: "First",
      value: (g) => g.first_tokens,
      text: (g) => number(g.first_tokens),
      numeric: true,
    },
    {
      key: "last",
      label: "Last",
      value: (g) => g.last_tokens,
      text: (g) => number(g.last_tokens),
      numeric: true,
    },
    {
      key: "peak",
      label: "Peak",
      term: "peak",
      value: (g) => g.peak_tokens,
      text: (g) => number(g.peak_tokens),
      numeric: true,
    },
    {
      key: "compactions",
      label: "Compactions",
      term: "compaction",
      value: (g) => g.compactions.length,
      text: (g) => (g.compactions.length > 0 ? String(g.compactions.length) : "-"),
      hover: (g) =>
        g.compactions.length > 0
          ? `Size before each compaction: ${g.compactions.map(number).join(", ")}`
          : null,
      numeric: true,
    },
    {
      key: "near",
      label: "Near limit",
      term: "near-limit",
      value: (g) => (g.near_limit ? 1 : 0),
      text: (g) => (g.near_limit ? "yes" : ""),
      title: "At least 90% of the model's window",
    },
  ];

  protected readonly errorColumns: Column<ToolErrors>[] = [
    { key: "source", label: "Source", value: (t) => t.source, text: (t) => sourceLabel(t.source) },
    { key: "tool", label: "Tool", value: (t) => t.tool },
    {
      key: "calls",
      label: "Calls",
      value: (t) => t.calls,
      text: (t) => number(t.calls),
      numeric: true,
    },
    {
      key: "errors",
      label: "Errors",
      value: (t) => t.errors,
      text: (t) => number(t.errors),
      numeric: true,
    },
    {
      key: "rate",
      label: "Error rate",
      value: (t) => errorRate(t),
      text: (t) => percent(errorRate(t)),
      numeric: true,
    },
    {
      key: "unknown",
      label: "Unknown",
      value: (t) => t.unknown,
      text: (t) => number(t.unknown),
      numeric: true,
    },
    { key: "loops", label: "Retry loops", value: (t) => t.loops, numeric: true },
  ];

  protected readonly loopColumns: Column<RetryLoop>[] = [
    { key: "source", label: "Source", value: (l) => l.source, text: (l) => sourceLabel(l.source) },
    { key: "session", label: "Session", value: (l) => l.session, text: (l) => shortId(l.session) },
    { key: "tool", label: "Tool", value: (l) => l.tool },
    { key: "input", label: "Input", value: (l) => l.input, text: (l) => l.input ?? "-" },
    { key: "calls", label: "Calls", value: (l) => l.calls, numeric: true },
    { key: "errors", label: "Errors", value: (l) => l.errors, numeric: true },
  ];

  protected readonly hookColumns: Column<HookStats>[] = [
    { key: "source", label: "Source", value: (h) => h.source, text: (h) => sourceLabel(h.source) },
    { key: "hook", label: "Hook", value: (h) => h.hook },
    {
      key: "runs",
      label: "Runs",
      value: (h) => h.runs,
      text: (h) => number(h.runs),
      numeric: true,
    },
    { key: "failures", label: "Failures", value: (h) => h.failures, numeric: true },
    { key: "unknown", label: "No outcome", value: (h) => h.unknown, numeric: true },
    {
      key: "runtime",
      label: "Runtime (s)",
      value: (h) => h.total_ms,
      text: (h) => seconds(h.total_ms),
      numeric: true,
    },
  ];

  protected readonly permissionColumns: Column<PermissionStats>[] = [
    { key: "source", label: "Source", value: (p) => p.source, text: (p) => sourceLabel(p.source) },
    { key: "prompts", label: "Prompts", value: (p) => p.prompts, numeric: true },
    { key: "answered", label: "Answered", value: (p) => p.answered, numeric: true },
    {
      key: "wait",
      label: "Wait (s)",
      term: "permission-wait",
      value: (p) => p.total_wait_ms,
      text: (p) => seconds(p.total_wait_ms),
      numeric: true,
    },
  ];
}
