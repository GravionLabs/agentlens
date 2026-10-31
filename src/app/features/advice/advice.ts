import { PageHeader } from "../../ui/page-header";
import { Term } from "../../ui/term";
import {
  afterNextRender,
  ChangeDetectionStrategy,
  Component,
  computed,
  DestroyRef,
  ElementRef,
  inject,
  Injector,
  signal,
} from "@angular/core";
import { analysis } from "../../core/analysis";
import { exactTokens, number, percent, tokens, usd } from "../../core/format";
import type {
  AdviceItem,
  AdviceReport,
  Finding,
  FindingKind,
  Tip,
  TipStep,
  TipsReport,
} from "../../core/types";
import { HelpLink, termTitle } from "../../ui/help-link";
import { Status } from "../../ui/status";
import {
  HxButton,
  HxCell,
  HxColumnHeader,
  HxDataTable,
  type HxColumn,
} from "@gravionlabs/helix-ui";

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
  "model-mix": "Model mix",
  delegation: "Delegation",
  "context-hygiene": "Context hygiene",
  "cache-expiry": "Cache expiry",
  "skill-quality": "Skill quality",
};

/** An advice item with the sortable token count and the estimate flag next to the display text. */
export interface ItemView {
  name: string;
  sessionId: string | null;
  tokens: number;
  tokensText: string;
  estimated: boolean;
  requests: number;
  detail: string;
}

export function itemView(i: AdviceItem): ItemView {
  return {
    name: i.name,
    sessionId: i.session_id ?? null,
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

/** “~$1.23 (4.5%)”; without prices the tokens, marked as an estimate when they are one. */
export function tipSaving(t: Tip): string {
  if (t.saving_usd === null) return `${tokens(t.saving.count, !t.saving.measured)} tokens`;
  const share = t.share === null ? "" : ` (${percent(t.share)})`;
  return `${usd(t.saving_usd, true)}${share}`;
}

/** What the copy button writes: the file, a newline and the change; the change alone without a file. */
export function stepText(step: TipStep): string {
  return step.file ? `${step.file}\n${step.change}` : step.change;
}

/** “No change would save more than 0.5% of the period's cost.” */
export function noTipsText(threshold: number): string {
  return `No change would save more than ${percent(threshold)} of the period's cost.`;
}

const COPIED_MS = 2000;

@Component({
  selector: "app-advice",
  imports: [PageHeader, Term, Status, HxDataTable, HxCell, HxColumnHeader, HelpLink, HxButton],
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
      <app-status
        [loading]="tipsRes.isLoading()"
        [error]="tipsRes.error()"
        [warnings]="0"
        [errors]="tipsRes.hasValue() ? tipsRes.value().errors : []"
      />
      @if (tipsReport(); as t) {
        <section class="tips" aria-label="Top tips">
          <h3>Top tips</h3>
          @for (tip of t.tips; track tip.rank) {
            <article class="card tip">
              <h4>
                <span class="rank">#{{ tip.rank }}</span> {{ tip.headline }}
              </h4>
              <p class="saving">
                <strong [class.est]="tip.saving_usd === null && !tip.saving.measured">{{
                  tipSaving(tip)
                }}</strong>
                <span class="confidence"
                  >{{ tip.confidence }}<app-help-link term="confidence" label="Confidence"
                /></span>
              </p>
              <p class="action">{{ tip.action }}</p>
              @for (target of tip.targets; track $index; let ti = $index) {
                <div class="target">
                  <p>
                    <strong>{{ target.name }}</strong>
                    <span class="muted">{{ target.detail }}</span>
                  </p>
                  @for (step of target.steps; track $index; let si = $index) {
                    <div class="step">
                      <span class="client">{{ step.client }}</span>
                      @if (step.file) {
                        <code class="file">{{ step.file }}</code>
                      }
                      <code class="change">{{ step.change }}</code>
                      <button
                        hx-button
                        type="button"
                        variant="text"
                        size="small"
                        class="copy"
                        (click)="copy(tip.rank + '-' + ti + '-' + si, step)"
                      >
                        {{ copied() === tip.rank + "-" + ti + "-" + si ? "Copied" : "Copy" }}
                      </button>
                      @if (failed() === tip.rank + "-" + ti + "-" + si) {
                        <textarea class="manual" readonly rows="3" [value]="text(step)"></textarea>
                      }
                    </div>
                  }
                </div>
              }
              @if (tip.more > 0) {
                <p class="muted">+{{ tip.more }} more</p>
              }
              <p>
                <a href="#finding-{{ tip.kind }}" class="see" (click)="see($event, tip.kind)"
                  >See the finding</a
                >
              </p>
            </article>
          } @empty {
            <p class="muted none">{{ noTips(t.threshold) }}</p>
          }
          @if (t.below_threshold > 0) {
            <p class="muted below">
              {{ t.below_threshold }} more findings save less than {{ pct(t.threshold) }} each; they
              are listed below.
            </p>
          }
        </section>
      }
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
          <section class="finding" [attr.id]="'finding-' + f.kind">
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
              [columns]="columnsFor(f.kind)"
              emptyMessage="Nothing to show."
              [ariaLabel]="title(f.kind)"
            >
              <ng-template hxColumnHeader="tokens"
                ><app-help-link term="token" label="Tokens"
              /></ng-template>
              <ng-template hxColumnHeader="requests"
                ><app-help-link term="request" label="Requests"
              /></ng-template>
              <ng-template hxCell="name" let-row>
                <span [attr.title]="row.sessionId">{{ row.name }}</span>
              </ng-template>
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
    .tips h3 {
      margin: 12px 0 8px;
    }
    .card {
      background: var(--panel);
      border: 1px solid var(--line);
      border-radius: 10px;
      padding: 12px 16px;
      margin: 0 0 10px;
    }
    .card h4 {
      margin: 0 0 6px;
      font-size: 1rem;
    }
    .confidence {
      margin-left: 8px;
      font-size: 0.8rem;
      color: var(--muted);
    }
    .target {
      margin: 6px 0;
    }
    .target p {
      margin: 2px 0;
    }
    .step {
      display: flex;
      flex-wrap: wrap;
      align-items: center;
      gap: 8px;
      margin: 2px 0 2px 12px;
    }
    .client {
      color: var(--muted);
      font-size: 0.85rem;
    }
    code {
      font-family: ui-monospace, monospace;
      font-size: 0.85rem;
      word-break: break-all;
    }
    .manual {
      width: 100%;
      font-family: ui-monospace, monospace;
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
  protected readonly tipsRes = analysis<TipsReport>("tips");
  protected readonly tipsReport = computed(() =>
    this.tipsRes.hasValue() ? this.tipsRes.value().data : null,
  );
  protected readonly tipSaving = tipSaving;
  protected readonly text = stepText;
  protected readonly noTips = noTipsText;
  protected readonly pct = percent;
  protected readonly copied = signal<string | null>(null);
  protected readonly failed = signal<string | null>(null);

  private readonly host = inject<ElementRef<HTMLElement>>(ElementRef);
  private readonly injector = inject(Injector);
  private timer: ReturnType<typeof setTimeout> | undefined;

  constructor() {
    inject(DestroyRef).onDestroy(() => clearTimeout(this.timer));
  }

  /** Writes the step to the clipboard and shows “Copied” for two seconds; a failed write shows the text selected. */
  protected async copy(key: string, step: TipStep): Promise<void> {
    clearTimeout(this.timer);
    try {
      await navigator.clipboard.writeText(stepText(step));
    } catch {
      this.copied.set(null);
      this.failed.set(key);
      afterNextRender(
        () => this.host.nativeElement.querySelector<HTMLTextAreaElement>(".manual")?.select(),
        { injector: this.injector },
      );
      return;
    }
    this.failed.set(null);
    this.copied.set(key);
    this.timer = setTimeout(() => this.copied.set(null), COPIED_MS);
  }

  protected see(event: Event, kind: FindingKind): void {
    event.preventDefault();
    document.getElementById(`finding-${kind}`)?.scrollIntoView({ block: "start" });
  }
  protected readonly tokens = tokens;
  protected readonly exact = exactTokens;
  protected readonly title = findingTitle;
  protected readonly total = findingTotal;
  protected readonly saving = findingSaving;

  protected readonly n = number;
  protected readonly views = (items: AdviceItem[]): ItemView[] => items.map(itemView);

  protected readonly columnsFor = (kind: FindingKind): HxColumn[] =>
    kind === "context-size" || kind === "context-hygiene" || kind === "cache-expiry"
      ? this.sessionColumns
      : this.skillColumns;

  private readonly sessionColumns: HxColumn[] = this.makeColumns("Session");
  private readonly skillColumns: HxColumn[] = this.makeColumns("Skill");

  private makeColumns(first: string): HxColumn[] {
    return [
      { field: "name", header: first, sortable: true },
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
}
