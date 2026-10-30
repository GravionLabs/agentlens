import { ChangeDetectionStrategy, Component, computed, inject, signal } from "@angular/core";
import {
  HxDateTimeRangePicker,
  type HxDateTimeRangePreset,
  type HxDateTimeRangeValue,
  HxInput,
  HxMultiSelect,
  HxSelect,
} from "@gravionlabs/helix-ui";
import { CUSTOM, FiltersService, LAST_UNITS, PERIODS } from "../core/filters.service";
import { SOURCES, type SourceId } from "../core/types";

@Component({
  selector: "app-filter-bar",
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [HxSelect, HxMultiSelect, HxInput, HxDateTimeRangePicker],
  template: `
    <form class="bar" (submit)="$event.preventDefault()" aria-label="Filters">
      <div class="field">
        <label for="filter-period">Period</label>
        <hx-select
          inputId="filter-period"
          fluid
          [options]="periodOptions"
          optionLabel="label"
          optionValue="value"
          [value]="customOpen() ? custom : filters.periodChoice()"
          (valueChange)="setSince($event)"
        />
      </div>
      @if (customOpen()) {
        <div class="field last">
          <label for="filter-last">Last</label>
          <div class="pair">
            <input
              hx-input
              fluid
              id="filter-last"
              type="number"
              min="1"
              step="1"
              [attr.aria-invalid]="lastError() ? 'true' : null"
              [value]="lastValue()"
              (change)="setLastNumber($event)"
            />
            <hx-select
              inputId="filter-last-unit"
              ariaLabel="Unit"
              fluid
              [options]="units"
              optionLabel="label"
              optionValue="value"
              [value]="lastUnit()"
              (valueChange)="setLastUnit($event)"
            />
          </div>
        </div>
        <div class="field range">
          <label for="filter-window">Date range</label>
          <hx-date-time-range-picker
            inputId="filter-window"
            ariaLabel="Date range"
            showTime
            [presets]="rangePresets"
            [invalid]="!!rangeError()"
            [value]="rangeValue()"
            (valueChange)="setRange($event)"
          />
        </div>
        @if (lastError() || rangeError()) {
          <p class="error" role="alert">{{ lastError() || rangeError() }}</p>
        }
      }
      <div class="field">
        <label for="filter-sources">Sources</label>
        <hx-multi-select
          inputId="filter-sources"
          fluid
          [options]="sources"
          optionLabel="label"
          optionValue="id"
          placeholder="All sources"
          showClear
          [value]="filters.sources()"
          (valueChange)="setSources($event)"
        />
      </div>
      <div class="field">
        <label for="filter-project">Project</label>
        <input
          hx-input
          fluid
          id="filter-project"
          type="text"
          placeholder="Directory, or empty for all"
          [value]="filters.project()"
          (change)="setProject($event)"
        />
      </div>
    </form>
  `,
  styles: `
    .bar {
      display: flex;
      flex-flow: row wrap;
      align-items: flex-end;
      gap: 10px 16px;
    }
    .field {
      flex: 1 1 11rem;
      max-width: 18rem;
    }
    .field {
      display: flex;
      flex-direction: column;
      gap: 2px;
    }
    .pair {
      display: grid;
      grid-template-columns: 5rem 1fr;
      gap: 6px;
    }
    .range {
      max-width: 24rem;
    }
    .error {
      flex: 1 1 100%;
      margin: 0;
      color: var(--danger, #b00020);
      font-size: 0.85rem;
    }
    label {
      font-size: 0.85rem;
      color: var(--muted);
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
  private readonly draftLast = signal<unknown>(this.filters.last()?.last ?? 1);
  protected readonly lastUnit = signal<string>(this.filters.last()?.unit ?? "d");
  protected readonly lastError = signal<string | null>(null);
  protected readonly rangeError = signal<string | null>(null);
  protected readonly lastValue = computed(() => String(this.draftLast() ?? ""));
  protected readonly rangeValue = computed<HxDateTimeRangeValue>(() => {
    const r = this.filters.range();
    return r ? { from: new Date(r.from), to: new Date(r.to) } : { from: null, to: null };
  });
  protected readonly sources = SOURCES;

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

  protected setProject(event: Event): void {
    this.filters.setProject((event.target as HTMLInputElement).value);
  }
}
