import { PageHeader } from "../../ui/page-header";
import { ChangeDetectionStrategy, Component, computed } from "@angular/core";
import { analysis } from "../../core/analysis";
import { number, sourceLabel } from "../../core/format";
import type { SourceOverview } from "../../core/types";
import { Status } from "../../ui/status";

@Component({
  selector: "app-overview",
  imports: [PageHeader, Status],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <section class="view">
      <app-page-header title="Sources" summary="Which coding agents left sessions on this machine.">
        <p>
          Which coding agents left sessions on this machine, for the selected filters. Sources that
          are not found show where AgentLens looked.
        </p>
      </app-page-header>
      <app-status
        [loading]="res.isLoading()"
        [error]="res.error()"
        [warnings]="res.value()?.warnings ?? 0"
        [errors]="res.value()?.errors ?? []"
      />
      <div class="cards">
        @for (s of sources(); track s.source) {
          <article class="card" [class.missing]="!s.found">
            <h3>{{ label(s.source) }}</h3>
            @if (s.found) {
              <p class="big">{{ n(s.sessions) }} <span>sessions</span></p>
              <p class="muted">{{ n(s.events) }} events</p>
              @if (s.warnings > 0) {
                <p class="warn">{{ s.warnings }} warnings</p>
              }
              @for (e of s.errors; track $index) {
                <p class="error">{{ e }}</p>
              }
              @for (r of s.roots; track r) {
                <p class="path">{{ r }}</p>
              }
            } @else {
              <p class="muted">Not found.</p>
              @if (s.note) {
                <p class="path">{{ s.note }}</p>
              }
            }
          </article>
        }
      </div>
    </section>
  `,
  styles: `
    .cards {
      display: grid;
      grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
      gap: 12px;
      margin-top: 12px;
    }
    .card {
      background: var(--panel);
      border: 1px solid var(--line);
      border-radius: 10px;
      padding: 12px 16px;
    }
    .card h3 {
      margin: 0 0 6px;
      font-size: 1rem;
    }
    .missing {
      opacity: 0.65;
    }
    .big {
      font-size: 1.6rem;
      font-weight: 650;
      margin: 0;
      font-variant-numeric: tabular-nums;
    }
    .big span {
      font-size: 0.9rem;
      font-weight: 400;
      color: var(--muted);
    }
    .path {
      font-size: 0.8rem;
      color: var(--muted);
      word-break: break-all;
      margin: 4px 0 0;
    }
    .warn {
      color: var(--warn);
    }
    .error {
      color: var(--bad);
    }
  `,
})
export class Overview {
  protected readonly res = analysis<SourceOverview[]>("overview");
  protected readonly sources = computed(() => this.res.value()?.data ?? []);
  protected readonly n = number;
  protected readonly label = sourceLabel;
}
