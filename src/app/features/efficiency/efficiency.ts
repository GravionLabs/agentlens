import { ChangeDetectionStrategy, Component, computed, signal } from "@angular/core";
import { analysis } from "../../core/analysis";
import { number, pathLabel, percent, shortId, sourceLabel } from "../../core/format";
import type { CacheRow, EfficiencyReport, SubagentShare } from "../../core/types";
import { type Column, DataTable } from "../../ui/data-table";
import { Status } from "../../ui/status";

type By = "source" | "project" | "session";

/** Subagent share of a total; `null` if there is no total to share. */
export function share(part: number, whole: number): number | null {
  return whole > 0 ? part / whole : null;
}

@Component({
  selector: "app-efficiency",
  imports: [Status, DataTable],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <section class="view">
      <h2>Efficiency</h2>
      <p class="muted">
        Cache hit rate = cache read / all input tokens (measured). “n/a”: the client reports no
        cache tokens.
      </p>
      <label class="by">
        <span>Group the cache table by</span>
        <select (change)="by.set(asSelect($event).value)">
          @for (o of options; track o) {
            <option [value]="o" [selected]="o === by()">{{ o }}</option>
          }
        </select>
      </label>
      <app-status
        [loading]="res.isLoading()"
        [error]="res.error()"
        [warnings]="res.value()?.warnings ?? 0"
        [errors]="res.value()?.errors ?? []"
      />
      <app-data-table
        [columns]="cacheColumns"
        [data]="cache()"
        [initialSort]="{ key: 'requests', descending: true }"
        [limit]="25"
        [filterable]="true"
        placeholder="Filter by group"
        empty="No usage found."
      />
      <h3>Subagent share</h3>
      <p class="muted">
        Cost shares only cover requests with a price (reported or from the price table).
      </p>
      <app-data-table [columns]="subagentColumns" [data]="subagents()" empty="No usage found." />
    </section>
  `,
  styles: `
    h3 {
      margin: 20px 0 6px;
      font-size: 1.05rem;
    }
    .by {
      display: inline-flex;
      flex-direction: column;
      gap: 2px;
      color: var(--muted);
      font-size: 0.85rem;
      margin: 8px 0;
    }
    select {
      background: var(--panel);
      color: var(--text);
      border: 1px solid var(--line);
      border-radius: 6px;
      padding: 5px 8px;
    }
  `,
})
export class Efficiency {
  protected readonly by = signal<By>("source");
  protected readonly options: By[] = ["source", "project", "session"];
  protected readonly res = analysis<EfficiencyReport>("efficiency", () => ({ by: this.by() }));
  protected readonly cache = computed(() => this.res.value()?.data.cache ?? []);
  protected readonly subagents = computed(() => this.res.value()?.data.subagents ?? []);

  protected readonly cacheColumns: Column<CacheRow>[] = [
    {
      key: "key",
      label: "Group",
      value: (r) => r.key,
      text: (r) =>
        r.key.length > 30 && !r.key.startsWith("/") ? shortId(r.key) : pathLabel(r.key),
      hover: (r) => r.key,
    },
    { key: "source", label: "Source", value: (r) => r.source, text: (r) => sourceLabel(r.source) },
    {
      key: "requests",
      label: "Requests",
      value: (r) => r.requests,
      text: (r) => number(r.requests),
      numeric: true,
    },
    {
      key: "input",
      label: "Input",
      value: (r) => r.input,
      text: (r) => number(r.input),
      numeric: true,
    },
    {
      key: "read",
      label: "Cache read",
      value: (r) => r.cache_read,
      text: (r) => number(r.cache_read),
      numeric: true,
    },
    {
      key: "write",
      label: "Cache write",
      value: (r) => r.cache_write,
      text: (r) => number(r.cache_write),
      numeric: true,
    },
    {
      key: "rate",
      label: "Hit rate",
      value: (r) => r.hit_rate,
      text: (r) => percent(r.hit_rate),
      numeric: true,
    },
  ];

  protected readonly subagentColumns: Column<SubagentShare>[] = [
    { key: "source", label: "Source", value: (s) => s.source, text: (s) => sourceLabel(s.source) },
    {
      key: "requests",
      label: "Requests",
      value: (s) => s.requests,
      text: (s) => number(s.requests),
      numeric: true,
    },
    {
      key: "sub-requests",
      label: "Subagent requests",
      value: (s) => share(s.subagent_requests, s.requests),
      text: (s) => percent(share(s.subagent_requests, s.requests)),
      numeric: true,
    },
    {
      key: "sub-tokens",
      label: "Subagent tokens",
      value: (s) => share(s.subagent_tokens, s.tokens),
      text: (s) => percent(share(s.subagent_tokens, s.tokens)),
      numeric: true,
    },
    {
      key: "sub-cost",
      label: "Subagent cost",
      value: (s) => share(s.subagent_usd, s.usd),
      text: (s) => percent(share(s.subagent_usd, s.usd)),
      numeric: true,
    },
    {
      key: "unpriced",
      label: "Unpriced requests",
      value: (s) => s.unpriced_requests,
      numeric: true,
    },
  ];

  protected asSelect(e: Event): { value: By } {
    return e.target as unknown as { value: By };
  }
}
