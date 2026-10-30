import { PageHeader } from "../../ui/page-header";
import { Term } from "../../ui/term";
import { ChangeDetectionStrategy, Component, computed } from "@angular/core";
import { analysis } from "../../core/analysis";
import { exactTokens, number, tokens, usd } from "../../core/format";
import type { AdviceItem, AdviceReport, Finding, FindingKind } from "../../core/types";
import { HelpLink, termTitle } from "../../ui/help-link";
import { Status } from "../../ui/status";
import { HxCell, HxColumnHeader, HxDataTable, type HxColumn } from "@gravionlabs/helix-ui";

const TITLES: Record<FindingKind, string> = {
  "unused-skills": "Unused skills",
  "listed-but-loaded-by-hand": "Listed but loaded by hand",
  "costly-descriptions": "Costly descriptions",
  "used-but-not-installed": "Used but not installed",
  "large-bodies": "Large skill bodies",
  "large-context": "Work on a large context",
  "costly-mcp-servers": "Costly MCP servers",
  "context-size": "Context size",
  "retry-loops": "Retry loops",
};

/** An advice item with the sortable token count and the estimate flag next to the display text. */
export interface ItemView {
  name: string;
  tokens: number;
  tokensText: string;
  estimated: boolean;
  requests: number;
  detail: string;
}

export function itemView(i: AdviceItem): ItemView {
  return {
    name: i.name,
    tokens: i.tokens.count,
    tokensText: tokens(i.tokens.count, !i.tokens.measured),
    estimated: !i.tokens.measured,
    requests: i.requests,
    detail: i.detail,
  };
}

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

/** “Could save ~$1.23 (~45,678 tokens, estimated).”, or null for a finding without a saving. */
export function findingSaving(f: Finding): string | null {
  const { saving, saving_usd, confidence } = f.impact;
  if (saving.count === 0) return null;
  const amount = tokens(saving.count, !saving.measured);
  return saving_usd === null
    ? `Could save ${amount} tokens (${confidence}; no price known).`
    : `Could save ${usd(saving_usd, true)} (${amount} tokens, ${confidence}).`;
}

@Component({
  selector: "app-advice",
  imports: [PageHeader, Term, Status, HxDataTable, HxCell, HxColumnHeader, HelpLink],
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
          numbers always give the same findings, ranked by what each could have
          <app-term id="saving">saved</app-term> (its <app-term id="impact">impact</app-term>, with
          its <app-term id="confidence">confidence</app-term>). <span class="est">~</span> marks
          estimates; a finding says what it cannot know.
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
          <strong
            [class.est]="!r.baseline_total.measured"
            [attr.title]="exact(r.baseline_total.count)"
            >{{ tokens(r.baseline_total.count, !r.baseline_total.measured) }}</strong
          >
          tokens in the period.
        </p>
        @for (f of r.findings; track f.kind) {
          <section class="finding">
            <h3>
              <span class="rank">{{ f.rank }}.</span> {{ title(f.kind) }} ({{ f.count }})
            </h3>
            @if (saving(f); as s) {
              <p class="saving" [attr.title]="exact(f.impact.saving.count)">
                <strong>{{ s }}</strong>
              </p>
            }
            <p class="muted">{{ f.explanation }}</p>
            @if (total(f); as t) {
              <p class="total" [attr.title]="exact(f.total?.count)">{{ t }}</p>
            }
            <hx-data-table
              [value]="views(f.items)"
              [columns]="columns"
              emptyMessage="Nothing to show."
              [ariaLabel]="title(f.kind)"
            >
              <ng-template hxColumnHeader="tokens"
                ><app-help-link term="token" label="Tokens"
              /></ng-template>
              <ng-template hxColumnHeader="requests"
                ><app-help-link term="request" label="Requests"
              /></ng-template>
              <ng-template hxCell="tokens" let-row>
                <span [class.est]="row.estimated" [attr.title]="exact(row.tokens)">{{
                  row.tokensText
                }}</span>
              </ng-template>
              <ng-template hxCell="requests" let-value="value">{{ n(value) }}</ng-template>
            </hx-data-table>
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
    .saving,
    .action {
      margin: 6px 0;
    }
    .rank {
      color: var(--muted);
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
  protected readonly exact = exactTokens;
  protected readonly title = findingTitle;
  protected readonly total = findingTotal;
  protected readonly saving = findingSaving;

  protected readonly n = number;
  protected readonly views = (items: AdviceItem[]): ItemView[] => items.map(itemView);

  protected readonly columns: HxColumn[] = [
    { field: "name", header: "Skill", sortable: true },
    {
      field: "tokens",
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
    { field: "detail", header: "Detail", sortable: true },
  ];
}
