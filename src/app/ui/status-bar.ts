import { ChangeDetectionStrategy, Component, computed, inject } from "@angular/core";
import { HxButton, HxTag } from "@gravionlabs/helix-ui";
import { SAMPLE_MODE } from "../core/ipc";
import { TokenFormatService } from "../core/token-format.service";
import { ZoomService } from "../core/zoom.service";
import { CliTools } from "./cli-tools";
import { UpdateStatus } from "./update-status";

/**
 * The end of the status bar of the shell (the version and note are set in `App`): the sample badge, the token format switch (full or
 * rounded counts), the interface size controls, the command line tools and the update check.
 */
@Component({
  selector: "app-status-bar",
  imports: [HxButton, HxTag, CliTools, UpdateStatus],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <span class="end">
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
  `,
  styles: `
    :host {
      display: block;
      font-size: 0.85rem;
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
  protected readonly sampleMode = inject(SAMPLE_MODE);
  protected readonly zoom = inject(ZoomService);
  protected readonly percent = computed(() => Math.round(this.zoom.factor() * 100));

  private readonly tokenFormat = inject(TokenFormatService);
  protected readonly rounded = computed(() => this.tokenFormat.mode() === "rounded");

  protected toggleRounded(): void {
    this.tokenFormat.set(this.rounded() ? "full" : "rounded");
  }
}
