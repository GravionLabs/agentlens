import { PageHeader } from "../../ui/page-header";
import { Term } from "../../ui/term";
import { ChangeDetectionStrategy, Component, computed, signal } from "@angular/core";
import { analysis } from "../../core/analysis";
import { exactTokens, number, pathLabel, shortPath, sourceLabel, tokens } from "../../core/format";
import type { ContextKind, ContextReport, ContextRow, Tokens } from "../../core/types";
import { termTitle } from "../../ui/data-table";
import { Status } from "../../ui/status";
import { HxCell, HxDataTable, type HxColumn } from "@gravionlabs/helix-ui";

export interface Note {
  text: string;
  /** The exact counts while token counts are rounded. */
  title: string | null;
}

/** The sentence about the first request of a session; the exact counts are its tooltip. */
export function firstRequestNote(
  source: string,
  f: { sessions: number; total_tokens: number; max_tokens: number },
): Note {
  const average = Math.round(f.total_tokens / Math.max(1, f.sessions));
  const exactAverage = exactTokens(average);
  const exactMax = exactTokens(f.max_tokens);
  return {
    text: `${sourceLabel(source)}: the first request of a session holds ${tokens(average)} tokens on average, at most ${tokens(f.max_tokens)} (measured; an upper bound of all static context).`,
    title:
      exactAverage || exactMax
        ? `Average ${number(average)} tokens, at most ${number(f.max_tokens)}`
        : null,
  };
}

type View = "all" | "tools" | "instructions" | "agents" | "skills";

const VIEWS: { id: View; label: string }[] = [
  { id: "all", label: "All" },
  { id: "tools", label: "Tools and MCP" },
  { id: "instructions", label: "Instruction files" },
  { id: "agents", label: "Agents" },
  { id: "skills", label: "Skills" },
];

type Grouping = "owner" | "project";

/** Adds two counts; the sum is measured only if both are. */
function add(a: Tokens, b: Tokens): Tokens {
  return { count: a.count + b.count, measured: a.measured && b.measured };
}

/** Sums the rows of all projects: one row per source, kind and owner. */
export function byOwner(rows: ContextRow[]): ContextRow[] {
  const sums = new Map<string, ContextRow>();
  for (const r of rows) {
    const key = `${r.source}|${r.kind}|${r.owner}`;
    const sum = sums.get(key);
    if (!sum) {
      sums.set(key, { ...r, project: null });
    } else {
      sum.items += r.items;
      sum.bytes += r.bytes;
      sum.tokens = add(sum.tokens, r.tokens);
      sum.requests += r.requests;
      sum.overhead_tokens = add(sum.overhead_tokens, r.overhead_tokens);
    }
  }
  return [...sums.values()];
}

/** A context row with sortable raw counts next to the estimate flags of the displayed ones. */
export interface ContextView extends ContextRow {
  tokenCount: number;
  tokenText: string;
  tokensEstimated: boolean;
  overheadCount: number;
  overheadText: string;
  overheadEstimated: boolean;
  ownerLabel: string;
}

export function contextView(r: ContextRow): ContextView {
  return {
    ...r,
    tokenCount: r.tokens.count,
    tokenText: tokens(r.tokens.count, !r.tokens.measured),
    tokensEstimated: !r.tokens.measured,
    overheadCount: r.overhead_tokens.count,
    overheadText: tokens(r.overhead_tokens.count, !r.overhead_tokens.measured),
    overheadEstimated: !r.overhead_tokens.measured,
    ownerLabel: pathLabel(r.owner),
  };
}

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
  imports: [PageHeader, Term, Status, HxDataTable, HxCell],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <section class="view">
      <app-page-header
        exportView="context"
        [exportFormats]="['json', 'csv']"
        title="Context"
        summary="Static content the clients add to every request."
      >
        <p>
          Static content the clients add to every request (<app-term id="context"
            >static context</app-term
          >). <app-term id="overhead">Overhead</app-term> = tokens × the requests that followed;
          <span class="est">~</span> marks estimates.
        </p>
      </app-page-header>
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
      <div class="views" role="group" aria-label="Grouping">
        <button type="button" [class.on]="grouping() === 'owner'" (click)="grouping.set('owner')">
          By owner
        </button>
        <button
          type="button"
          [class.on]="grouping() === 'project'"
          (click)="grouping.set('project')"
        >
          By project
        </button>
      </div>
      <hx-data-table
        [value]="rows()"
        [columns]="columns()"
        filterDisplay="row"
        sortField="overheadCount"
        [sortOrder]="-1"
        [paginator]="rows().length > 100"
        [rows]="100"
        emptyMessage="No context items found."
        ariaLabel="Context"
      >
        <ng-template hxCell="source" let-value="value">{{ source(value) }}</ng-template>
        <ng-template hxCell="project" let-row>
          <span [attr.title]="row.project">{{ shortPath(row.project) }}</span>
        </ng-template>
        <ng-template hxCell="ownerLabel" let-row>
          <span [attr.title]="row.owner">{{ row.ownerLabel }}</span>
        </ng-template>
        <ng-template hxCell="tokenCount" let-row>
          <span [class.est]="row.tokensEstimated" [attr.title]="exact(row.tokenCount)">{{
            row.tokenText
          }}</span>
        </ng-template>
        <ng-template hxCell="requests" let-value="value">{{ n(value) }}</ng-template>
        <ng-template hxCell="overheadCount" let-row>
          <span [class.est]="row.overheadEstimated" [attr.title]="exact(row.overheadCount)">{{
            row.overheadText
          }}</span>
        </ng-template>
      </hx-data-table>
      @for (n of notes(); track n.text) {
        <p class="muted" [attr.title]="n.title">{{ n.text }}</p>
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
      height: auto;
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

  protected readonly grouping = signal<Grouping>("owner");

  protected readonly source = sourceLabel;
  protected readonly shortPath = shortPath;
  protected readonly n = number;
  protected readonly exact = exactTokens;

  protected readonly rows = computed(() => {
    const shown = (this.res.value()?.data.rows ?? []).filter((r) => inView(r.kind, this.view()));
    return (this.grouping() === "owner" ? byOwner(shown) : shown).map(contextView);
  });

  protected readonly notes = computed(() => {
    const d = this.res.value()?.data;
    if (!d) return [];
    const out: Note[] = [];
    for (const [source, f] of Object.entries(d.first_request)) {
      out.push(firstRequestNote(source, f));
    }
    for (const [source, n] of Object.entries(d.unavailable)) {
      out.push({
        text: `${sourceLabel(source)}: not available, the client reports no context items (${n} sessions).`,
        title: null,
      });
    }
    return out;
  });

  protected readonly columns = computed<HxColumn[]>(() => [
    { field: "source", header: "Source", sortable: true, filter: true },
    ...(this.grouping() === "project"
      ? [{ field: "project", header: "Project", sortable: true }]
      : []),
    { field: "kind", header: "Kind", title: termTitle("kind"), sortable: true },
    { field: "ownerLabel", header: "Owner", title: termTitle("owner"), sortable: true },
    { field: "items", header: "Items", align: "right", sortable: true },
    {
      field: "tokenCount",
      header: "Tokens",
      title: termTitle("token"),
      align: "right",
      sortable: true,
    },
    {
      field: "requests",
      header: "Requests",
      title: termTitle("request"),
      align: "right",
      sortable: true,
    },
    {
      field: "overheadCount",
      header: "Overhead",
      title: termTitle("overhead"),
      align: "right",
      sortable: true,
    },
  ]);
}
