import { ChangeDetectionStrategy, Component, inject } from "@angular/core";
import { FiltersService, PERIODS } from "../core/filters.service";
import { SOURCES, type SourceId } from "../core/types";

@Component({
  selector: "app-filter-bar",
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <form class="bar" (submit)="$event.preventDefault()" aria-label="Filters">
      <label>
        <span>Period</span>
        <select [value]="filters.since()" (change)="setSince($event)">
          @for (p of periods; track p.value) {
            <option [value]="p.value" [selected]="p.value === filters.since()">
              {{ p.label }}
            </option>
          }
        </select>
      </label>
      <fieldset>
        <legend>Sources</legend>
        @for (s of sources; track s.id) {
          <label class="check">
            <input
              type="checkbox"
              [checked]="filters.sources().includes(s.id)"
              (change)="filters.toggleSource(s.id)"
            />
            {{ s.label }}
          </label>
        }
      </fieldset>
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
      flex-direction: column;
      gap: 10px;
    }
    label,
    fieldset {
      display: flex;
      flex-direction: column;
      gap: 2px;
      font-size: 0.85rem;
      color: var(--muted);
    }
    fieldset {
      flex-direction: column;
      align-items: flex-start;
      gap: 2px;
      border: 0;
      margin: 0;
      padding: 0;
    }
    legend {
      padding: 0;
    }
    .check {
      flex-direction: row;
      align-items: center;
      gap: 4px;
      color: var(--text);
    }
    select,
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

  protected setSince(event: Event): void {
    this.filters.setSince((event.target as HTMLSelectElement).value);
  }

  protected setProject(event: Event): void {
    this.filters.setProject((event.target as HTMLInputElement).value);
  }

  protected isSource(id: SourceId): boolean {
    return this.filters.sources().includes(id);
  }
}
