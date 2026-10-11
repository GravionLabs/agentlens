import { ChangeDetectionStrategy, Component, computed, inject } from "@angular/core";
import { SAMPLE_MODE } from "../core/ipc";
import { UpdateService } from "../core/update.service";
import { ZoomService } from "../core/zoom.service";
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
        <span class="demo-note">
          Nothing here is read from your machine.
          <a
            href="https://github.com/GravionLabs/agentlens/releases/latest"
            target="_blank"
            rel="noopener noreferrer"
            >Install the application</a
          >
          to analyze your own sessions.
        </span>
      } @else if (updates.version(); as v) {
        <strong>v{{ v }}</strong>
      }
    </span>
    <span class="right">
      <span class="zoom" role="group" aria-label="Interface size">
        <button
          type="button"
          class="btn btn-text btn-sm"
          title="Smaller (Ctrl -)"
          aria-label="Smaller"
          (click)="zoom.zoomOut()"
        >
          A−
        </button>
        <button
          type="button"
          class="btn btn-text btn-sm"
          title="Reset (Ctrl 0)"
          aria-label="Reset size"
          (click)="zoom.reset()"
        >
          {{ percent() }}%
        </button>
        <button
          type="button"
          class="btn btn-text btn-sm"
          title="Larger (Ctrl +)"
          aria-label="Larger"
          (click)="zoom.zoomIn()"
        >
          A+
        </button>
      </span>
      @if (!sampleMode) {
        <app-update-status />
      }
    </span>
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
    .right,
    .zoom {
      display: inline-flex;
      align-items: center;
      gap: 12px;
    }
    .zoom {
      gap: 2px;
    }
    .demo-note a {
      color: var(--accent);
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
  protected readonly zoom = inject(ZoomService);
  protected readonly percent = computed(() => Math.round(this.zoom.factor() * 100));

  constructor() {
    void this.updates.loadVersion();
  }
}
