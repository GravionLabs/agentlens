import { ChangeDetectionStrategy, Component, computed, inject } from "@angular/core";
import { HxButton, HxTag } from "@gravionlabs/helix-ui";
import { HelixStatusBar, type HelixStatusBarVersion } from "@gravionlabs/helix-shell";
import { SAMPLE_MODE } from "../core/ipc";
import { TokenFormatService } from "../core/token-format.service";
import { UpdateService } from "../core/update.service";
import { ZoomService } from "../core/zoom.service";
import { CliTools } from "./cli-tools";
import { UpdateStatus } from "./update-status";

/**
 * The bar at the bottom: the running version, the sample badge, the token format switch (full or
 * rounded counts), the interface size controls, the command line tools and the update check.
 */
@Component({
  selector: "app-status-bar",
  imports: [HelixStatusBar, HxButton, HxTag, CliTools, UpdateStatus],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <helix-status-bar
      brand="AgentLens"
      [note]="sampleMode ? 'Nothing here is read from your machine.' : undefined"
      [versions]="versions()"
      height="auto"
    >
      <span hxStatusBarEnd class="end">
        @if (sampleMode) {
          <hx-tag class="sample" severity="warn" value="sample data" rounded />
          <a
            class="demo-link"
            href="https://github.com/GravionLabs/agentlens/releases/latest"
            target="_blank"
            rel="noopener noreferrer"
            >Install the application</a
          >
        }
        <button
          hx-button
          type="button"
          variant="text"
          size="small"
          [title]="
            rounded()
              ? 'Token counts are rounded (2.11 M). Click to show them in full.'
              : 'Token counts are shown in full (2,114,305). Click to round them.'
          "
          aria-label="Round token counts"
          [attr.aria-pressed]="rounded()"
          (click)="toggleRounded()"
        >
          {{ rounded() ? "2.11 M" : "2,114,305" }}
        </button>
        <span class="zoom" role="group" aria-label="Interface size">
          <button
            hx-button
            type="button"
            variant="text"
            size="small"
            title="Smaller (Ctrl -)"
            aria-label="Smaller"
            (click)="zoom.zoomOut()"
          >
            <svg
              class="icon"
              viewBox="0 0 16 16"
              width="16"
              height="16"
              fill="none"
              stroke="currentColor"
              stroke-width="1.5"
              stroke-linecap="round"
              aria-hidden="true"
            >
              <circle cx="7" cy="7" r="4.5" />
              <path d="M10.5 10.5 14 14M5 7h4" />
            </svg>
          </button>
          <button
            hx-button
            type="button"
            variant="text"
            size="small"
            title="Reset (Ctrl 0)"
            aria-label="Reset size"
            (click)="zoom.reset()"
          >
            {{ percent() }}%
          </button>
          <button
            hx-button
            type="button"
            variant="text"
            size="small"
            title="Larger (Ctrl +)"
            aria-label="Larger"
            (click)="zoom.zoomIn()"
          >
            <svg
              class="icon"
              viewBox="0 0 16 16"
              width="16"
              height="16"
              fill="none"
              stroke="currentColor"
              stroke-width="1.5"
              stroke-linecap="round"
              aria-hidden="true"
            >
              <circle cx="7" cy="7" r="4.5" />
              <path d="M10.5 10.5 14 14M5 7h4M7 5v4" />
            </svg>
          </button>
        </span>
        @if (!sampleMode) {
          <app-cli-tools />
          <app-update-status />
        }
      </span>
    </helix-status-bar>
  `,
  styles: `
    :host {
      display: block;
      border-top: 1px solid var(--line);
      background: var(--panel);
      font-size: 0.85rem;
      color: var(--muted);
    }
    .end,
    .zoom {
      display: inline-flex;
      align-items: center;
      gap: 12px;
    }
    .zoom {
      gap: 2px;
    }
    .demo-link {
      color: var(--accent);
      white-space: nowrap;
    }
  `,
})
export class StatusBar {
  protected readonly updates = inject(UpdateService);
  protected readonly sampleMode = inject(SAMPLE_MODE);
  protected readonly zoom = inject(ZoomService);
  protected readonly versions = computed<HelixStatusBarVersion[]>(() => {
    const v = this.updates.version();
    return !this.sampleMode && v ? [{ label: "Version", value: `v${v}` }] : [];
  });
  protected readonly percent = computed(() => Math.round(this.zoom.factor() * 100));

  private readonly tokenFormat = inject(TokenFormatService);
  protected readonly rounded = computed(() => this.tokenFormat.mode() === "rounded");

  protected toggleRounded(): void {
    this.tokenFormat.set(this.rounded() ? "full" : "rounded");
  }

  constructor() {
    void this.updates.loadVersion();
  }
}
