import { ChangeDetectionStrategy, Component, computed, input, signal } from "@angular/core";
import { RouterLink } from "@angular/router";
import { exactTokens } from "../core/format";
import { glossaryEntry } from "../core/glossary";

export interface Column<T> {
  key: string;
  label: string;
  /** The value used for sorting and, without `text`, for display. */
  value: (row: T) => string | number | null;
  /** The displayed text; defaults to the value. */
  text?: (row: T) => string;
  numeric?: boolean;
  /** `true` marks the displayed value as an estimate (shown with a tilde by `text`). */
  estimated?: (row: T) => boolean;
  title?: string;
  /** A glossary id: the header gets its explanation as a tooltip and a link to the glossary. */
  term?: string;
  /** A tooltip for the cell, e.g. the full path behind a shortened one. */
  hover?: (row: T) => string | null;
  /** The value is a token count: while counts are rounded, the cell's tooltip has the exact one. */
  tokens?: boolean;
}

/** A sortable table with an optional text filter on the first column. */
@Component({
  selector: "app-data-table",
  imports: [RouterLink],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    @if (filterable()) {
      <input
        class="filter"
        type="search"
        [placeholder]="placeholder()"
        [attr.aria-label]="placeholder()"
        (input)="query.set(asInput($event).value)"
      />
    }
    <div class="wrap">
      <table>
        <thead>
          <tr>
            @for (c of columns(); track c.key) {
              <th
                scope="col"
                [class.num]="c.numeric"
                [attr.aria-sort]="ariaSort(c.key)"
                [title]="header(c)"
              >
                <button type="button" (click)="sortBy(c.key)">{{ c.label }}</button>
                @if (c.term) {
                  <a
                    class="help"
                    routerLink="/glossary"
                    [queryParams]="{ term: c.term }"
                    [attr.aria-label]="'Explain ' + c.label"
                    >?</a
                  >
                }
              </th>
            }
          </tr>
        </thead>
        <tbody>
          @for (row of rows(); track $index) {
            <tr [class.dim]="dim()(row)">
              @for (c of columns(); track c.key) {
                <td
                  [class.num]="c.numeric"
                  [class.est]="c.estimated?.(row)"
                  [attr.title]="tip(c, row)"
                >
                  {{ cell(c, row) }}
                </td>
              }
            </tr>
          } @empty {
            <tr>
              <td class="empty" [attr.colspan]="columns().length">{{ empty() }}</td>
            </tr>
          }
        </tbody>
      </table>
    </div>
    @if (hidden() > 0) {
      <p class="more">
        {{ hidden() }} more rows.
        <button type="button" class="link" (click)="showAll.set(true)">Show all</button>
      </p>
    }
  `,
  styles: `
    .wrap {
      /* Scrolls on its own, so the header stays visible and the page does not grow with the rows. */
      overflow: auto;
      max-height: var(--table-max, max(240px, calc(100dvh - 220px)));
      background: var(--panel);
      border: 1px solid var(--line);
      border-radius: 10px;
    }
    table {
      border-collapse: collapse;
      width: 100%;
      font-variant-numeric: tabular-nums;
    }
    th,
    td {
      padding: 4px 12px;
      border-bottom: 1px solid var(--line);
      text-align: left;
      white-space: nowrap;
    }
    thead th {
      position: sticky;
      top: 0;
      z-index: 1;
      background: var(--panel);
      box-shadow: 0 1px 0 var(--line);
    }
    .help {
      margin-left: 4px;
      font-size: 0.7rem;
      color: var(--muted);
      border: 1px solid var(--line);
      border-radius: 50%;
      width: 14px;
      height: 14px;
      display: inline-flex;
      align-items: center;
      justify-content: center;
      text-decoration: none;
      vertical-align: middle;
    }
    .help:hover {
      color: var(--accent);
      border-color: var(--accent);
    }
    th button {
      all: unset;
      cursor: pointer;
      color: var(--muted);
      font-weight: 600;
      font-size: 0.85rem;
    }
    th button:focus-visible {
      outline: 2px solid var(--accent);
    }
    th[aria-sort="ascending"] button::after {
      content: " \\25B2";
    }
    th[aria-sort="descending"] button::after {
      content: " \\25BC";
    }
    .num {
      text-align: right;
    }
    .est {
      color: var(--warn);
    }
    .dim td {
      color: var(--muted);
    }
    .empty {
      color: var(--muted);
      text-align: center;
    }
    .filter {
      margin-bottom: 8px;
      background: var(--panel);
      color: var(--text);
      border: 1px solid var(--line);
      border-radius: 6px;
      padding: 5px 10px;
      width: min(320px, 100%);
    }
    .more {
      color: var(--muted);
    }
    .link {
      all: unset;
      cursor: pointer;
      color: var(--accent);
      text-decoration: underline;
    }
  `,
})
export class DataTable<T> {
  readonly columns = input.required<Column<T>[]>();
  readonly data = input.required<T[]>();
  readonly initialSort = input<{ key: string; descending: boolean } | null>(null);
  readonly filterable = input(false);
  readonly placeholder = input("Filter");
  readonly empty = input("Nothing to show.");
  /** Rows shown before "Show all"; 0 shows everything. */
  readonly limit = input(100);
  readonly dim = input<(row: T) => boolean>(() => false);

  protected readonly query = signal("");
  protected readonly showAll = signal(false);
  private readonly sort = signal<{ key: string; descending: boolean } | null | undefined>(
    undefined,
  );

  private readonly effectiveSort = computed(() =>
    this.sort() === undefined ? this.initialSort() : this.sort(),
  );

  private readonly sorted = computed(() => {
    const q = this.query().trim().toLowerCase();
    const cols = this.columns();
    let rows = this.data();
    if (q && cols.length > 0) {
      rows = rows.filter((r) =>
        String(cols[0].value(r) ?? "")
          .toLowerCase()
          .includes(q),
      );
    }
    const s = this.effectiveSort();
    const col = s ? cols.find((c) => c.key === s.key) : undefined;
    if (!s || !col) return rows;
    const dir = s.descending ? -1 : 1;
    return [...rows].sort((a, b) => {
      const x = col.value(a);
      const y = col.value(b);
      if (x === null && y === null) return 0;
      if (x === null) return 1;
      if (y === null) return -1;
      if (typeof x === "number" && typeof y === "number") return (x - y) * dir;
      return String(x).localeCompare(String(y)) * dir;
    });
  });

  protected readonly rows = computed(() => {
    const all = this.sorted();
    const limit = this.limit();
    return this.showAll() || limit === 0 ? all : all.slice(0, limit);
  });

  protected readonly hidden = computed(() => this.sorted().length - this.rows().length);

  protected sortBy(key: string): void {
    const cur = this.effectiveSort();
    const numeric = this.columns().find((c) => c.key === key)?.numeric ?? false;
    this.sort.set(
      cur?.key === key ? { key, descending: !cur.descending } : { key, descending: numeric },
    );
  }

  protected ariaSort(key: string): "ascending" | "descending" | null {
    const s = this.effectiveSort();
    if (!s || s.key !== key) return null;
    return s.descending ? "descending" : "ascending";
  }

  protected header(c: Column<T>): string {
    return c.title ?? (c.term ? (glossaryEntry(c.term)?.short ?? "") : "");
  }

  protected tip(c: Column<T>, row: T): string | null {
    const hover = c.hover?.(row);
    if (hover) return hover;
    if (!c.tokens) return null;
    const v = c.value(row);
    return typeof v === "number" ? exactTokens(v) : null;
  }

  protected cell(c: Column<T>, row: T): string {
    if (c.text) return c.text(row);
    const v = c.value(row);
    return v === null ? "n/a" : String(v);
  }

  protected asInput(e: Event): HTMLInputElement {
    return e.target as HTMLInputElement;
  }
}
