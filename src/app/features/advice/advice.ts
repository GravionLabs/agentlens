import { PageHeader } from "../../ui/page-header";
import { Term } from "../../ui/term";
import { ChangeDetectionStrategy, Component, computed } from "@angular/core";
import { analysis } from "../../core/analysis";
import { number, tokens } from "../../core/format";
import type { AdviceItem, AdviceReport, Finding, FindingKind } from "../../core/types";
import { type Column, DataTable } from "../../ui/data-table";
import { Status } from "../../ui/status";

const TITLES: Record<FindingKind, string> = {
  "unused-skills": "Unused skills",
  "listed-but-loaded-by-hand": "Listed but loaded by hand",
  "costly-descriptions": "Costly descriptions",
  "used-but-not-installed": "Used but not installed",
  "large-bodies": "Large skill bodies",
  "large-context": "Work on a large context",
  "costly-mcp-servers": "Costly MCP servers",
  "context-size": "Context size",
};

export function findingTitle(kind: FindingKind): string {
  return TITLES[kind];
}

/** “Together they cost ~12,235,734 tokens, 30% of the baseline.” */
export function findingTotal(f: Finding): string | null {
  if (!f.total) return null;
  const what = f.kind === "costly-descriptions" ? "could save" : "cost";
  const share =
    f.share_of_baseline === null
      ? ""
      : `, ${Math.round(f.share_of_baseline * 100)}% of the baseline`;
  const overall =
    f.share_of_total_tokens === null
      ? ""
      : `, ${f.total.measured ? "" : "~"}${(f.share_of_total_tokens * 100).toFixed(2)}% of all tokens`;
  return `Together they ${what} ${tokens(f.total.count, !f.total.measured)} tokens${share}${overall}.`;
}

@Component({
  selector: "app-advice",
  imports: [PageHeader, Term, Status, DataTable],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <section class="view">
      <app-page-header
        exportView="advice"
        [exportFormats]="['json', 'csv']"
        title="Advice"
        summary="What to change to save tokens, with the numbers behind each suggestion."
      >
        <p>
          Findings made from the <app-term id="baseline">baseline</app-term>, load and
          <app-term id="carry">carry</app-term> of your skills. No model is involved: the same
          numbers always give the same findings. <span class="est">~</span> marks estimates; a
          finding says what it cannot know.
        </p>
      </app-page-header>
      <app-status
        [loading]="res.isLoading()"
        [error]="res.error()"
        [warnings]="res.value()?.warnings ?? 0"
        [errors]="res.value()?.errors ?? []"
      />
      @if (report(); as r) {
        <p class="lead">
          {{ r.sessions }} sessions, {{ r.installed_skills }} installed skills; their names and
          descriptions cost
          <strong [class.est]="!r.baseline_total.measured">{{
            tokens(r.baseline_total.count, !r.baseline_total.measured)
          }}</strong>
          tokens in the period.
        </p>
        @for (f of r.findings; track f.kind) {
          <section class="finding">
            <h3>{{ title(f.kind) }} ({{ f.count }})</h3>
            <p class="muted">{{ f.explanation }}</p>
            @if (total(f); as t) {
              <p class="total">{{ t }}</p>
            }
            <app-data-table [columns]="columns" [data]="f.items" empty="Nothing to show." />
            @if (f.items.length < f.count) {
              <p class="muted">
                {{ f.count - f.items.length }} more; the export lists the top ones only.
              </p>
            }
            <p class="action"><strong>What to do:</strong> {{ f.action }}</p>
            @for (n of f.notes; track n) {
              <p class="muted note">{{ n }}</p>
            }
          </section>
        } @empty {
          <p class="muted">Nothing to suggest for these sessions.</p>
        }
      }
    </section>
  `,
  styles: `
    .lead {
      margin: 8px 0 12px;
    }
    .finding {
      margin: 18px 0 24px;
    }
    h3 {
      margin: 0 0 4px;
    }
    .total,
    .action {
      margin: 6px 0;
    }
    .note {
      margin: 2px 0;
      font-size: 0.85rem;
    }
    .est {
      color: var(--warn);
    }
  `,
})
export class Advice {
  protected readonly res = analysis<AdviceReport>("advice");
  protected readonly report = computed(() => this.res.value()?.data ?? null);
  protected readonly tokens = tokens;
  protected readonly title = findingTitle;
  protected readonly total = findingTotal;

  protected readonly columns: Column<AdviceItem>[] = [
    { key: "name", label: "Skill", value: (i) => i.name },
    {
      key: "tokens",
      label: "Tokens",
      term: "token",
      value: (i) => i.tokens.count,
      text: (i) => tokens(i.tokens.count, !i.tokens.measured),
      estimated: (i) => !i.tokens.measured,
      numeric: true,
    },
    {
      key: "requests",
      label: "Requests",
      term: "request",
      value: (i) => i.requests,
      text: (i) => number(i.requests),
      numeric: true,
    },
    { key: "detail", label: "Detail", value: (i) => i.detail },
  ];
}
