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
import { answerOf, analysis } from "../../core/analysis";
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
  HxInput,
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
  imports: [
    PageHeader,
    Term,
    Status,
    HxDataTable,
    HxCell,
    HxColumnHeader,
    HelpLink,
    HxButton,
    HxInput,
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
  templateUrl: "./advice.html",
  styles: `
    .lead {
      margin: 8px 0 12px;
    }
    .tips h3 {
      margin: 12px 0 8px;
    }
    .card {
      background: var(--h-surface-card);
      border: 1px solid var(--h-border-panel);
      border-radius: 10px;
      padding: 12px 16px;
      margin: 0 0 10px;
    }
    .card h4 {
      margin: 0 0 6px;
      font-size: var(--h-font-size-md);
    }
    .confidence {
      margin-left: 8px;
      font-size: var(--h-font-size-xs);
      color: var(--h-text-muted-color);
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
      color: var(--h-text-muted-color);
      font-size: var(--h-font-size-sm);
    }
    code {
      font-family: ui-monospace, monospace;
      font-size: var(--h-font-size-sm);
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
      color: var(--h-text-muted-color);
    }
    .note {
      margin: 2px 0;
      font-size: var(--h-font-size-sm);
    }
    .est {
      color: var(--h-tag-warn-color);
    }
  `,
})
export class Advice {
  protected readonly res = analysis<AdviceReport>("advice");
  protected readonly answer = computed(() => answerOf(this.res));
  protected readonly report = computed(() => answerOf(this.res)?.data ?? null);
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
