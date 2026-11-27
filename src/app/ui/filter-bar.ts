import {
  ChangeDetectionStrategy,
  Component,
  computed,
  inject,
  resource,
  signal,
} from "@angular/core";
import {
  HxDateTimeRangePicker,
  type HxDateTimeRangePreset,
  type HxDateTimeRangeValue,
  HxInput,
  HxMultiSelect,
  HxSelect,
  HxSwitch,
} from "@gravionlabs/helix-ui";
import { CUSTOM, FiltersService, LAST_UNITS, PERIODS } from "../core/filters.service";
import { Ipc } from "../core/ipc";
import { SOURCES, type Answer, type EfficiencyReport, type SourceId } from "../core/types";

@Component({
  selector: "app-filter-bar",
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [HxSelect, HxMultiSelect, HxInput, HxDateTimeRangePicker, HxSwitch],
  templateUrl: "./filter-bar.html",
  styles: `
    /* A topbar action in the page header: it takes the free width there. */
    :host {
      display: block;
      flex: 1 1 auto;
      min-width: 0;
    }
    /* The page header has a fixed height: one row that shrinks, never wraps. */
    .bar {
      display: flex;
      flex-wrap: nowrap;
      align-items: center;
      gap: 8px;
      /* Where the fields do not fit side by side, the row scrolls inside the header. */
      overflow-x: auto;
      padding: 3px;
      scrollbar-width: thin;
    }
    .field {
      flex: 1 1 11rem;
      min-width: 7rem;
      max-width: 18rem;
    }
    /* The number and the unit stay readable when the row scrolls. */
    .field.last {
      min-width: 11rem;
    }
    .pair {
      display: grid;
      grid-template-columns: 4.5rem 1fr;
      gap: 6px;
    }
    .range {
      min-width: 14rem;
      max-width: 24rem;
    }
    /* Below the page header, so it never changes the header height. */
    .error {
      position: fixed;
      top: calc(var(--helix-topbar-height, 3rem) + 2 * var(--helix-panel-gap, 4px) + 4px);
      right: 1rem;
      z-index: 998;
      margin: 0;
      padding: 4px 8px;
      background: var(--h-surface-overlay);
      border: 1px solid var(--h-border-panel);
      border-radius: 4px;
      color: var(--h-tag-danger-color);
      font-size: var(--h-font-size-sm);
    }
    .compare {
      display: inline-flex;
      flex: 0 0 auto;
      align-items: center;
      gap: 6px;
      font-size: var(--h-font-size-sm);
      white-space: nowrap;
    }
    /* The fields keep their labels for assistive technology; the header has no room to show them. */
    label:not(.compare) {
      position: absolute;
      width: 1px;
      height: 1px;
      overflow: hidden;
      clip-path: inset(50%);
      white-space: nowrap;
    }
  `,
})
export class FilterBar {
  protected readonly filters = inject(FiltersService);
  protected readonly periodOptions = [...PERIODS, { value: CUSTOM, label: "Custom…" }];
  protected readonly custom = CUSTOM;
  protected readonly units = LAST_UNITS;
  protected readonly rangePresets: HxDateTimeRangePreset[] = PERIODS.filter((p) => p.value).map(
    (p) => {
      const n = Number.parseInt(p.value, 10);
      const ms = n * (p.value.endsWith("h") ? 3_600_000 : 86_400_000);
      return { label: p.label, from: () => new Date(Date.now() - ms), to: () => new Date() };
    },
  );

  /** Whether the custom inputs are open; they open with the "Custom…" choice or a custom window. */
  protected readonly customOpen = signal(this.filters.periodChoice() === CUSTOM);
  /** The inputs being edited; the filter only changes when they are valid. */
  private readonly draftLast = signal<number>(this.filters.last()?.last ?? 1);
  protected readonly lastUnit = signal<string>(this.filters.last()?.unit ?? "d");
  protected readonly lastError = signal<string | null>(null);
  protected readonly rangeError = signal<string | null>(null);
  protected readonly lastValue = computed(() => String(this.draftLast()));
  protected readonly rangeValue = computed<HxDateTimeRangeValue>(() => {
    const r = this.filters.range();
    return r ? { from: new Date(r.from), to: new Date(r.to) } : { from: null, to: null };
  });
  protected readonly sources = SOURCES;
  private readonly ipc = inject(Ipc);

  // The roles of the period under the other filters, not under the role filter itself, so the
  // options stay when one is chosen.
  private readonly efficiency = resource({
    params: () => {
      return {
        filters: { ...this.filters.payload(), roles: [] as string[] },
        generation: this.filters.generation(),
      };
    },
    loader: ({ params }) =>
      this.ipc.invoke<Answer<EfficiencyReport>>("efficiency", { filters: params.filters }),
  });
  /** The row keys of the efficiency roles table; empty (and the select hidden) without runs. */
  protected readonly roles = computed(() => {
    const own = this.efficiency.hasValue() ? this.efficiency.value().data.roles : [];
    return [...new Set([...own.map((r) => r.role), ...this.filters.roles()])];
  });

  protected setSince(value: unknown): void {
    // A cleared or non-string value is ignored: the period keeps its last value.
    if (typeof value !== "string") return;
    if (value === CUSTOM) {
      this.customOpen.set(true);
      return;
    }
    this.customOpen.set(false);
    this.lastError.set(null);
    this.rangeError.set(null);
    this.filters.setSince(value);
  }

  protected setLastNumber(event: Event): void {
    const text = (event.target as HTMLInputElement).value.trim();
    this.draftLast.set(text === "" ? NaN : Number(text));
    this.applyLast();
  }

  protected setLastUnit(unit: unknown): void {
    if (typeof unit !== "string") return;
    this.lastUnit.set(unit);
    this.applyLast();
  }

  private applyLast(): void {
    this.rangeError.set(null);
    this.lastError.set(this.filters.setLast(this.draftLast(), this.lastUnit()));
  }

  protected setRange(value: HxDateTimeRangeValue): void {
    this.lastError.set(null);
    // The picker reports an empty side while the user is still choosing; that is not an error yet.
    if (!value.from && !value.to) return;
    this.rangeError.set(this.filters.setRange(value.from, value.to));
  }

  protected setSources(ids: SourceId[]): void {
    const current = this.filters.sources();
    for (const id of new Set([...current, ...ids])) {
      if (current.includes(id) !== ids.includes(id)) this.filters.toggleSource(id);
    }
  }

  protected isChecked(event: Event): boolean {
    return (event.target as HTMLInputElement).checked;
  }

  protected setProject(event: Event): void {
    this.filters.setProject((event.target as HTMLInputElement).value);
  }
}
