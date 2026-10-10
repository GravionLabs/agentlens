import { ChangeDetectionStrategy, Component, computed, input, signal } from "@angular/core";

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
  /** A tooltip for the cell, e.g. the full path behind a shortened one. */
  hover?: (row: T) => string | null;
}

/** A sortable table with an optional text filter on the first column. */
@Component({
  selector: "app-data-table",
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
                [title]="c.title ?? ''"
              >
                <button type="button" (click)="sortBy(c.key)">{{ c.label }}</button>
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
                  [attr.title]="c.hover?.(row) ?? null"
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
      overflow-x: auto;
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
      padding: 6px 12px;
      border-bottom: 1px solid var(--line);
      text-align: left;
      white-space: nowrap;
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

  protected cell(c: Column<T>, row: T): string {
    if (c.text) return c.text(row);
    const v = c.value(row);
    return v === null ? "n/a" : String(v);
  }

  protected asInput(e: Event): HTMLInputElement {
    return e.target as HTMLInputElement;
  }
}
