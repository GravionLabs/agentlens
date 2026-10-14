import { ChangeDetectionStrategy, Component, inject, signal } from "@angular/core";
import { HxDateTimeRangePicker, HxMultiSelect, HxSelect } from "@gravionlabs/helix-ui";
import { FiltersService, PERIODS } from "../core/filters.service";
import { SOURCES, type SourceId } from "../core/types";

@Component({
  selector: "app-filter-bar",
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [HxSelect, HxMultiSelect, HxDateTimeRangePicker],
  template: `
    <form class="bar" (submit)="$event.preventDefault()" aria-label="Filters">
      <div class="field">
        <label for="filter-period">Period</label>
        <hx-select
          inputId="filter-period"
          fluid
          [options]="periods"
          optionLabel="label"
          optionValue="value"
          [value]="filters.since()"
          (valueChange)="setSince($event)"
        />
      </div>
      <div class="field">
        <label for="filter-window">Custom window (preview, not applied)</label>
        <hx-date-time-range-picker
          inputId="filter-window"
          ariaLabel="Custom window (preview, not applied)"
          showButtonBar
          [stepMinute]="15"
          [value]="window()"
          (valueChange)="window.set($event)"
        />
      </div>
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
      <label>
        <span>Project</span>
        <input
          type="text"
          placeholder="Directory, or empty for all"
          [value]="filters.project()"
          (change)="setProject($event)"
        />
      </label>
    </form>
  `,
  styles: `
    .bar {
      display: flex;
      flex-flow: row wrap;
      align-items: flex-end;
      gap: 10px 16px;
    }
    .field,
    .bar > label {
      flex: 1 1 11rem;
      max-width: 18rem;
    }
    .field {
      display: flex;
      flex-direction: column;
      gap: 2px;
    }
    label {
      display: flex;
      flex-direction: column;
      gap: 2px;
      font-size: 0.85rem;
      color: var(--muted);
    }
    input[type="text"] {
      background: var(--bg);
      color: var(--text);
      border: 1px solid var(--line);
      border-radius: 6px;
      padding: 5px 8px;
      width: 100%;
    }
  `,
})
export class FilterBar {
  protected readonly filters = inject(FiltersService);
  protected readonly periods = PERIODS;
  protected readonly sources = SOURCES;

  /**
   * A custom window. Local only: the core has no `until` yet, so it does not reach
   * FiltersService (out of scope of the spike, see #331).
   */
  protected readonly window = signal<{ from: Date | null; to: Date | null }>({
    from: null,
    to: null,
  });

  protected setSince(value: unknown): void {
    // A cleared or non-string value is ignored: the period keeps its last value.
    if (typeof value === "string") this.filters.setSince(value);
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
