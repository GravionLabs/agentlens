import { ChangeDetectionStrategy, Component, inject } from "@angular/core";
import { SAMPLE_MODE } from "../core/ipc";
import { UpdateService } from "../core/update.service";
import { UpdateStatus } from "./update-status";

/** The bar at the bottom: the running version, the sample badge and the update check. */
@Component({
  selector: "app-status-bar",
  imports: [UpdateStatus],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <span class="version">
      AgentLens
      @if (sampleMode) {
        <span class="sample">sample data</span>
      } @else if (updates.version(); as v) {
        <strong>v{{ v }}</strong>
      }
    </span>
    @if (!sampleMode) {
      <app-update-status />
    }
  `,
  styles: `
    :host {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 12px;
      min-height: 30px;
      padding: 2px 12px;
      border-top: 1px solid var(--line);
      background: var(--panel);
      font-size: 0.85rem;
      color: var(--muted);
    }
    .version {
      display: inline-flex;
      align-items: center;
      gap: 8px;
    }
    strong {
      color: var(--text);
    }
    .sample {
      border: 1px solid var(--warn);
      color: var(--warn);
      border-radius: 10px;
      padding: 0 8px;
    }
  `,
})
export class StatusBar {
  protected readonly updates = inject(UpdateService);
  protected readonly sampleMode = inject(SAMPLE_MODE);

  constructor() {
    void this.updates.loadVersion();
  }
}
