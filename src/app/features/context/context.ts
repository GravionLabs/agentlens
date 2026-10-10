import { ChangeDetectionStrategy, Component, computed, signal } from "@angular/core";
import { analysis } from "../../core/analysis";
import { number, sourceLabel, tokens } from "../../core/format";
import type { ContextKind, ContextReport, ContextRow } from "../../core/types";
import { type Column, DataTable } from "../../ui/data-table";
import { Status } from "../../ui/status";

type View = "all" | "tools" | "instructions" | "agents" | "skills";

const VIEWS: { id: View; label: string }[] = [
  { id: "all", label: "All" },
  { id: "tools", label: "Tools and MCP" },
  { id: "instructions", label: "Instruction files" },
  { id: "agents", label: "Agents" },
  { id: "skills", label: "Skills" },
];

export function inView(kind: ContextKind, view: View): boolean {
  switch (view) {
    case "all":
      return true;
    case "tools":
      return kind === "tool-listing" || kind === "tool-definition" || kind === "mcp-instructions";
    case "instructions":
      return kind === "instruction-file";
    case "agents":
      return kind === "agent-listing";
    case "skills":
      return kind === "skill-listing";
  }
}

@Component({
  selector: "app-context",
  imports: [Status, DataTable],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <section class="view">
      <h2>Context</h2>
      <p class="muted">
        Static content the clients add to every request. Overhead = tokens × the requests that
        followed;
        <span class="est">~</span> marks estimates.
      </p>
      <app-status
        [loading]="res.isLoading()"
        [error]="res.error()"
        [warnings]="res.value()?.warnings ?? 0"
        [errors]="res.value()?.errors ?? []"
      />
      <div class="views" role="group" aria-label="Kind of context">
        @for (v of views; track v.id) {
          <button type="button" [class.on]="view() === v.id" (click)="view.set(v.id)">
            {{ v.label }}
          </button>
        }
      </div>
      <app-data-table
        [columns]="columns"
        [data]="rows()"
        [initialSort]="{ key: 'overhead', descending: true }"
        [filterable]="true"
        placeholder="Filter by source"
        empty="No context items found."
      />
      @for (n of notes(); track n) {
        <p class="muted">{{ n }}</p>
      }
    </section>
  `,
  styles: `
    .views {
      display: flex;
      flex-wrap: wrap;
      gap: 6px;
      margin: 8px 0;
    }
    .views button {
      background: var(--panel);
      color: var(--text);
      border: 1px solid var(--line);
      border-radius: 999px;
      padding: 3px 12px;
      cursor: pointer;
    }
    .views button.on {
      border-color: var(--accent);
      color: var(--accent);
      font-weight: 600;
    }
    .est {
      color: var(--warn);
    }
  `,
})
export class Context {
  protected readonly res = analysis<ContextReport>("context");
  protected readonly view = signal<View>("all");
  protected readonly views = VIEWS;

  protected readonly rows = computed(() =>
    (this.res.value()?.data.rows ?? []).filter((r) => inView(r.kind, this.view())),
  );

  protected readonly notes = computed(() => {
    const d = this.res.value()?.data;
    if (!d) return [];
    const out: string[] = [];
    for (const [source, f] of Object.entries(d.first_request)) {
      out.push(
        `${sourceLabel(source)}: the first request of a session holds ${number(
          Math.round(f.total_tokens / Math.max(1, f.sessions)),
        )} tokens on average, at most ${number(f.max_tokens)} (measured; an upper bound of all static context).`,
      );
    }
    for (const [source, n] of Object.entries(d.unavailable)) {
      out.push(
        `${sourceLabel(source)}: not available, the client reports no context items (${n} sessions).`,
      );
    }
    return out;
  });

  protected readonly columns: Column<ContextRow>[] = [
    { key: "source", label: "Source", value: (r) => r.source, text: (r) => sourceLabel(r.source) },
    { key: "project", label: "Project", value: (r) => r.project, text: (r) => r.project ?? "-" },
    { key: "kind", label: "Kind", value: (r) => r.kind },
    { key: "owner", label: "Owner", value: (r) => r.owner },
    { key: "items", label: "Items", value: (r) => r.items, numeric: true },
    {
      key: "tokens",
      label: "Tokens",
      value: (r) => r.tokens,
      text: (r) => tokens(r.tokens, true),
      estimated: () => true,
      numeric: true,
    },
    {
      key: "requests",
      label: "Requests",
      value: (r) => r.requests,
      text: (r) => number(r.requests),
      numeric: true,
    },
    {
      key: "overhead",
      label: "Overhead",
      value: (r) => r.overhead_tokens,
      text: (r) => tokens(r.overhead_tokens, true),
      estimated: () => true,
      numeric: true,
    },
  ];
}
